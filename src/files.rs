use crate::ui_theme::{self, Icon};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Sense, Ui, Vec2};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SynStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_FILE_LINES: usize = 5000;
const CONTENT_FONT_SIZE: f32 = 12.0;

#[derive(Clone)]
struct DirEntryInfo {
    path: PathBuf,
    name: String,
    is_dir: bool,
    /// Gitignored or conventionally ignored: still listed, painted italic and muted.
    ignored: bool,
}

struct Syntect {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl Syntect {
    fn load() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let mut ts = ThemeSet::load_defaults();
        let theme = ts
            .themes
            .remove("base16-ocean.dark")
            .or_else(|| ts.themes.into_values().next())
            .expect("syntect theme");
        Self { syntax_set, theme }
    }
}

struct HighlightedText {
    lines: Vec<Arc<LayoutJob>>,
    max_cols: usize,
}

#[derive(Default)]
enum FileContent {
    #[default]
    Empty,
    Loading,
    Ready(HighlightedText),
    Error(String),
}

struct LoadResult {
    generation: u64,
    syntect: Arc<Syntect>,
    content: FileContent,
}

#[derive(Default)]
pub struct FileViewer {
    pub root: Option<PathBuf>,
    /// Last shell cwd applied to `root`; manual Parent/Home navigation sticks until it changes.
    synced_cwd: Option<PathBuf>,
    pub open_file: Option<PathBuf>,
    content: FileContent,
    /// Heavy (>10 MB); dropped whenever the panel is hidden.
    syntect: Option<Arc<Syntect>>,
    /// Bumped on every load so results for a file the user already left are dropped.
    generation: u64,
    load_rx: Option<Receiver<LoadResult>>,
    dir_cache: HashMap<PathBuf, Vec<DirEntryInfo>>,
    expanded: HashSet<PathBuf>,
}

impl FileViewer {
    /// Follow the shell cwd, but only when it actually changes.
    pub fn sync_shell_cwd(&mut self, cwd: Option<PathBuf>) {
        if self.synced_cwd != cwd {
            self.synced_cwd = cwd.clone();
            self.set_root(cwd);
        }
    }

    fn set_root(&mut self, root: Option<PathBuf>) {
        let root = root.or_else(dirs::home_dir);
        if self.root != root {
            self.root = root;
            self.open_file = None;
            self.cancel_load();
            self.content = FileContent::Empty;
            self.dir_cache.clear();
        }
    }

    /// Free syntect and the highlighted lines while the panel is hidden.
    pub fn release(&mut self) {
        if self.syntect.is_none()
            && self.load_rx.is_none()
            && matches!(self.content, FileContent::Empty)
            && self.dir_cache.is_empty()
        {
            return;
        }
        // Re-read directories when the panel opens again.
        self.dir_cache.clear();
        self.cancel_load();
        self.syntect = None;
        if let FileContent::Ready(text) = &mut self.content {
            text.lines.clear();
            text.lines.shrink_to_fit();
        }
        self.content = FileContent::Empty;
    }

    fn cancel_load(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.load_rx = None;
    }

    fn open_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.cancel_load();
        let generation = self.generation;
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        self.content = FileContent::Loading;
        self.open_file = Some(path.clone());
        let syntect = self.syntect.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let syntect = syntect.unwrap_or_else(|| Arc::new(Syntect::load()));
            let content = match read_file_capped(&path) {
                Ok((text, truncated)) => {
                    FileContent::Ready(highlight_lines(&syntect, &path, &text, truncated))
                }
                Err(e) => FileContent::Error(format!("Failed to read file: {e}")),
            };
            if tx
                .send(LoadResult {
                    generation,
                    syntect,
                    content,
                })
                .is_ok()
            {
                ctx.request_repaint();
            }
        });
    }

    fn poll_load(&mut self) {
        let Some(rx) = &self.load_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.load_rx = None;
                if result.generation == self.generation {
                    self.syntect = Some(result.syntect);
                    self.content = result.content;
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.load_rx = None;
                self.content = FileContent::Error("Failed to read file.".into());
            }
        }
    }

    pub fn go_parent(&mut self) {
        if let Some(root) = self.root.clone() {
            if let Some(parent) = root.parent() {
                self.set_root(Some(parent.to_path_buf()));
            }
        }
    }

    pub fn go_home(&mut self) {
        self.set_root(dirs::home_dir());
    }

    /// Draw the tree. Returns a folder that asked for a new tab ("New tab here").
    pub fn draw(&mut self, ui: &mut Ui) -> Option<PathBuf> {
        self.poll_load();
        if matches!(self.content, FileContent::Empty) && self.load_rx.is_none() {
            if let Some(path) = self.open_file.clone() {
                self.open_path(ui.ctx(), path);
            }
        }

        // Just the path, truncated; the close X lives in the sidebar's header row.
        let (header, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
        if let Some(root) = &self.root {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(header)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.add(
                        egui::Label::new(ui_theme::text_sub(root.display().to_string()))
                            .truncate(),
                    );
                },
            );
        }
        let (sep, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(sep, 0.0, ui_theme::BORDER);
        ui.add_space(4.0);

        if self.root.is_none() {
            self.root = dirs::home_dir();
        }
        let Some(root) = self.root.clone() else {
            ui.label(ui_theme::text_sub("No home folder."));
            return None;
        };

        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("↑")
                            .size(ui_theme::SIZE_TITLE)
                            .color(ui_theme::TEXT_MUTED),
                    )
                    .frame(false),
                )
                .on_hover_text("Parent")
                .clicked()
            {
                self.go_parent();
            }
            if ui
                .add(egui::Button::new(ui_theme::icon(Icon::Folder, 14.0, ui_theme::TEXT_MUTED)).frame(false))
                .on_hover_text("Home")
                .clicked()
            {
                self.go_home();
            }
            // Inter has no ↻; Hack (monospace) does.
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("↻")
                            .font(FontId::monospace(ui_theme::SIZE_TITLE))
                            .color(ui_theme::TEXT_MUTED),
                    )
                    .frame(false),
                )
                .on_hover_text("Refresh")
                .clicked()
            {
                self.dir_cache.clear();
            }
        });

        let root = self.root.clone().unwrap_or(root);
        let mut clicked: Option<PathBuf> = None;
        let mut new_tab: Option<PathBuf> = None;
        // The viewer keeps room under the tree; with no file open the tree takes the pane.
        let tree_h = if self.open_file.is_some() {
            ui.available_height() * 0.35
        } else {
            ui.available_height()
        };
        egui::ScrollArea::vertical()
            .id_salt("file_tree")
            .max_height(tree_h)
            .show(ui, |ui| {
                // Highlighted root row reads as the tree header.
                let root_name = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.display().to_string());
                tree_row(ui, None, Icon::Folder, &root_name, true, false);
                ui.indent(("tree_root", &root), |ui| {
                    draw_tree(
                        ui,
                        &root,
                        self.open_file.as_ref(),
                        &mut clicked,
                        &mut new_tab,
                        &mut self.dir_cache,
                        &mut self.expanded,
                    );
                });
            });

        if let Some(path) = clicked {
            self.open_path(ui.ctx(), path);
        }

        if let Some(path) = self.open_file.clone() {
            ui.add_space(4.0);
            let (sep2, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(sep2, 0.0, ui_theme::BORDER);
            ui.add(egui::Label::new(ui_theme::text_sub(path.display().to_string())).truncate());
            match &self.content {
                FileContent::Empty | FileContent::Loading => {
                    ui.label(ui_theme::text_sub("Loading…"));
                }
                FileContent::Error(msg) => {
                    ui.label(ui_theme::text_sub(msg.clone()));
                }
                FileContent::Ready(text) => {
                    let font = FontId::monospace(CONTENT_FONT_SIZE);
                    let (row_h, char_w) =
                        ui.fonts_mut(|f| (f.row_height(&font), f.glyph_width(&font, 'M')));
                    egui::ScrollArea::both().id_salt("file_content").show_rows(
                        ui,
                        row_h,
                        text.lines.len(),
                        |ui, rows| {
                            ui.set_min_width(text.max_cols as f32 * char_w);
                            for line in &text.lines[rows] {
                                ui.add(egui::Label::new(line.clone()).extend());
                            }
                        },
                    );
                }
            }
        }
        new_tab
    }
}

fn read_file_capped(path: &Path) -> std::io::Result<(String, bool)> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.by_ref().take(MAX_FILE_BYTES).read_to_end(&mut buf)?;
    let truncated = buf.len() as u64 == MAX_FILE_BYTES;
    let probe = &buf[..buf.len().min(8 * 1024)];
    if probe.contains(&0) {
        return Ok(("Binary file, not shown".into(), false));
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

/// Names that are ignored even without a `.gitignore` (kept visible, painted dim).
const COMMON_IGNORED: &[&str] = &["node_modules", "dist", "target", "build", "__pycache__"];

fn list_dir_cached(dir: &Path, cache: &mut HashMap<PathBuf, Vec<DirEntryInfo>>) -> Vec<DirEntryInfo> {
    if let Some(items) = cache.get(dir) {
        return items.clone();
    }
    let rules = read_gitignore(dir);
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            // `.git` is noisy; hidden files and gitignored entries stay visible but dimmed.
            if name == ".git" {
                continue;
            }
            let is_dir = path.is_dir();
            let ignored = name.starts_with('.') || is_dir_ignored(&rules, &name);
            items.push(DirEntryInfo { path, name, is_dir, ignored });
        }
        items.sort_by_key(|e| (!e.is_dir, e.name.to_lowercase()));
    }
    cache.insert(dir.to_path_buf(), items.clone());
    items
}

fn read_gitignore(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join(".gitignore"))
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| l.trim_start_matches('/').trim_end_matches('/').to_lowercase())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// shortcut: only the listed directory's own `.gitignore`; patterns containing `/`
/// (sub-paths) and negations are not applied. Enough to dim common build output.
fn is_dir_ignored(rules: &[String], name: &str) -> bool {
    let lower = name.to_lowercase();
    COMMON_IGNORED.contains(&lower.as_str()) || rules.iter().any(|rule| rule_matches(rule, &lower))
}

fn rule_matches(rule: &str, name: &str) -> bool {
    let mut pat = rule;
    for prefix in ["**/", "*/"] {
        if let Some(rest) = pat.strip_prefix(prefix) {
            pat = rest;
        }
    }
    // Negations re-include; sub-path patterns cannot be applied to a single name.
    if pat.starts_with('!') || pat.contains('/') {
        return false;
    }
    glob_match(pat, name)
}

/// `*` and `?` on a single name (already lowercased).
fn glob_match(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<usize> = None;
    let mut mark = 0;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            mark = ni;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// A full-width row that is one click target. Its text is painted, not added as labels:
/// selectable labels sense clicks themselves and would swallow the row's click.
fn tree_row(
    ui: &mut Ui,
    chevron: Option<Icon>,
    icon: Icon,
    name: &str,
    selected: bool,
    dim: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
    let fill = if selected {
        ui_theme::SURFACE_SEL
    } else if resp.hovered() {
        ui_theme::HOVER
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(ui_theme::RADIUS_ROW), fill);
    let icons = |size| FontId::new(size, egui::FontFamily::Name(ui_theme::FONT_ICONS.into()));
    let y = rect.center().y;
    let color = if dim { ui_theme::TEXT_FAINT } else { ui_theme::TEXT_MUTED };
    let mut x = rect.left() + 6.0;
    if let Some(chevron) = chevron {
        painter.text(
            egui::pos2(x + 6.0, y),
            egui::Align2::CENTER_CENTER,
            chevron.codepoint().to_string(),
            icons(12.0),
            color,
        );
        x += 20.0;
    }
    painter.text(
        egui::pos2(x + 7.0, y),
        egui::Align2::CENTER_CENTER,
        icon.codepoint().to_string(),
        icons(14.0),
        color,
    );
    let text_color = if dim { ui_theme::TEXT_FAINT } else { ui_theme::TEXT };
    let job = LayoutJob::single_section(
        name.to_owned(),
        TextFormat {
            font_id: FontId::proportional(ui_theme::SIZE_TITLE),
            color: text_color,
            italics: dim,
            ..Default::default()
        },
    );
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    painter.galley(
        egui::pos2(x + 22.0, y - galley.size().y * 0.5),
        galley,
        text_color,
    );
    resp
}

fn draw_tree(
    ui: &mut Ui,
    dir: &Path,
    selected: Option<&PathBuf>,
    clicked: &mut Option<PathBuf>,
    new_tab: &mut Option<PathBuf>,
    cache: &mut HashMap<PathBuf, Vec<DirEntryInfo>>,
    expanded: &mut HashSet<PathBuf>,
) {
    let items = list_dir_cached(dir, cache);

    for entry in items {
        if entry.is_dir {
            let open = expanded.contains(&entry.path);
            let chevron = if open { Icon::ChevronDown } else { Icon::ChevronRight };
            let resp = tree_row(ui, Some(chevron), Icon::Folder, &entry.name, false, entry.ignored);
            resp.context_menu(|ui| {
                if ui.button("New tab here").clicked() {
                    *new_tab = Some(entry.path.clone());
                    ui.close();
                }
            });
            if resp.clicked() {
                if open {
                    expanded.remove(&entry.path);
                } else {
                    expanded.insert(entry.path.clone());
                }
            }
            if open {
                ui.indent(entry.path.as_os_str(), |ui| {
                    draw_tree(ui, &entry.path, selected, clicked, new_tab, cache, expanded);
                });
            }
        } else {
            let is_sel = selected == Some(&entry.path);
            let resp = tree_row(ui, None, Icon::File, &entry.name, is_sel, entry.ignored);
            if resp.clicked() {
                *clicked = Some(entry.path);
            }
        }
    }
}

fn highlight_lines(syntect: &Syntect, path: &Path, text: &str, truncated: bool) -> HighlightedText {
    let ss = &syntect.syntax_set;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, &syntect.theme);
    let font = FontId::monospace(CONTENT_FONT_SIZE);
    let format = |color| TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };
    let mut lines = Vec::new();
    let mut max_cols = 0;
    let mut truncated = truncated;
    for (n, line) in LinesWithEndings::from(text).enumerate() {
        if n >= MAX_FILE_LINES {
            truncated = true;
            break;
        }
        let mut job = LayoutJob::default();
        match h.highlight_line(line, ss) {
            Ok(ranges) => {
                for (style, piece) in ranges {
                    let piece = piece.trim_end_matches(['\n', '\r']);
                    if !piece.is_empty() {
                        job.append(piece, 0.0, format(syn_color(style)));
                    }
                }
            }
            Err(_) => {
                job.append(line.trim_end_matches(['\n', '\r']), 0.0, format(ui_theme::TEXT));
            }
        }
        // Empty jobs lay out with zero height and would break the fixed row pitch.
        if job.is_empty() {
            job.append(" ", 0.0, format(ui_theme::TEXT));
        }
        max_cols = max_cols.max(job.text.chars().count());
        lines.push(Arc::new(job));
    }
    if truncated {
        let mut job = LayoutJob::default();
        job.append("… truncated …", 0.0, format(ui_theme::TEXT_MUTED));
        lines.push(Arc::new(job));
    }
    HighlightedText { lines, max_cols }
}

fn syn_color(style: SynStyle) -> Color32 {
    // Dynamic colors from syntect theme — routed through ui_theme.
    ui_theme::rgb(style.foreground.r, style.foreground.g, style.foreground.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_rules_dim_names_without_hiding_them() {
        let rules = vec![
            "node_modules/".to_string(),
            "*.log".to_string(),
            "**/dist".to_string(),
            "# comment".to_string(),
            "src/gen".to_string(),
        ];
        assert!(is_dir_ignored(&rules, "node_modules"));
        assert!(is_dir_ignored(&rules, "debug.log"));
        assert!(is_dir_ignored(&rules, "dist"));
        assert!(is_dir_ignored(&rules, "target")); // common fallback
        assert!(!is_dir_ignored(&rules, "src")); // sub-path pattern is not a name match
        assert!(!is_dir_ignored(&rules, "main.rs"));
        assert!(glob_match("*.ts", "main.ts") && !glob_match("*.ts", "main.rs"));
    }
}
