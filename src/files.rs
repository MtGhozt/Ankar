use crate::ui_theme::{self, Icon};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Sense, Ui, Vec2};
use std::collections::HashMap;
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
    pub open_file: Option<PathBuf>,
    content: FileContent,
    /// Heavy (>10 MB); dropped whenever the panel is hidden.
    syntect: Option<Arc<Syntect>>,
    /// Bumped on every load so results for a file the user already left are dropped.
    generation: u64,
    load_rx: Option<Receiver<LoadResult>>,
    dir_cache: HashMap<PathBuf, Vec<DirEntryInfo>>,
    pub show: bool,
}

impl FileViewer {
    /// Sync viewer root from the shell cwd. Does not clear navigation if the
    /// user already browsed elsewhere under the same project — only when the
    /// shell cwd itself changes.
    pub fn set_root(&mut self, root: Option<PathBuf>) {
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
        {
            return;
        }
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

    pub fn draw(&mut self, ui: &mut Ui) {
        self.poll_load();
        if matches!(self.content, FileContent::Empty) && self.load_rx.is_none() {
            if let Some(path) = self.open_file.clone() {
                self.open_path(ui.ctx(), path);
            }
        }

        ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            if let Some(root) = self.root.clone() {
                ui.label(
                    ui_theme::text_sub(root.display().to_string()),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::same(ui_theme::RADIUS_KBD),
                        ui_theme::HOVER,
                    );
                }
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    Icon::X.codepoint().to_string(),
                    egui::FontId::new(
                        12.0,
                        egui::FontFamily::Name(ui_theme::FONT_ICONS.into()),
                    ),
                    ui_theme::TEXT_MUTED,
                );
                if resp.clicked() {
                    self.show = false;
                }
            });
        });
        let (sep, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(sep, 0.0, ui_theme::BORDER);
        ui.add_space(4.0);

        if self.root.is_none() {
            self.root = dirs::home_dir();
        }
        let Some(root) = self.root.clone() else {
            ui.label(ui_theme::text_sub("No hay directorio home."));
            return;
        };

        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(ui_theme::icon(Icon::ChevronRight, 12.0, ui_theme::TEXT_MUTED)).frame(false))
                .on_hover_text("Parent")
                .clicked()
            {
                // Use folder-up via go_parent — chevron as affordance
                self.go_parent();
            }
            if ui
                .add(egui::Button::new(ui_theme::icon(Icon::Folder, 14.0, ui_theme::TEXT_MUTED)).frame(false))
                .on_hover_text("Home")
                .clicked()
            {
                self.go_home();
            }
            let _ = root;
        });

        let root = self.root.clone().unwrap_or(root);
        let mut clicked: Option<PathBuf> = None;
        egui::ScrollArea::vertical()
            .id_salt("file_tree")
            .max_height(ui.available_height() * 0.35)
            .show(ui, |ui| {
                draw_tree(ui, &root, self.open_file.as_ref(), &mut clicked, &mut self.dir_cache);
            });

        if let Some(path) = clicked {
            self.open_path(ui.ctx(), path);
        }

        if let Some(path) = self.open_file.clone() {
            ui.add_space(4.0);
            let (sep2, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(sep2, 0.0, ui_theme::BORDER);
            ui.label(ui_theme::text_sub(path.display().to_string()));
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
    }
}

fn read_file_capped(path: &Path) -> std::io::Result<(String, bool)> {
    let mut file = File::open(path)?;
    let mut buf = Vec::new();
    file.by_ref().take(MAX_FILE_BYTES).read_to_end(&mut buf)?;
    let truncated = buf.len() as u64 == MAX_FILE_BYTES;
    let probe = &buf[..buf.len().min(8 * 1024)];
    if probe.contains(&0) {
        return Ok(("Archivo binario, no se muestra".into(), false));
    }
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

fn list_dir_cached(dir: &Path, cache: &mut HashMap<PathBuf, Vec<DirEntryInfo>>) -> Vec<DirEntryInfo> {
    if let Some(items) = cache.get(dir) {
        return items.clone();
    }
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            let is_dir = path.is_dir();
            items.push(DirEntryInfo { path, name, is_dir });
        }
        items.sort_by_key(|e| (!e.is_dir, e.name.to_lowercase()));
    }
    cache.insert(dir.to_path_buf(), items.clone());
    items
}

fn draw_tree(
    ui: &mut Ui,
    dir: &Path,
    selected: Option<&PathBuf>,
    clicked: &mut Option<PathBuf>,
    cache: &mut HashMap<PathBuf, Vec<DirEntryInfo>>,
) {
    let items = list_dir_cached(dir, cache);

    for entry in items {
        if entry.is_dir {
            let label = format!("{}  {}", Icon::Folder.codepoint(), entry.name);
            egui::CollapsingHeader::new(
                egui::RichText::new(label)
                    .size(ui_theme::SIZE_TITLE)
                    .color(ui_theme::TEXT),
            )
            .id_salt(entry.path.as_os_str())
            .show(ui, |ui| {
                draw_tree(ui, &entry.path, selected, clicked, cache);
            });
        } else {
            let is_sel = selected == Some(&entry.path);
            let fill = if is_sel {
                ui_theme::SURFACE_SEL
            } else {
                Color32::TRANSPARENT
            };
            let row = egui::Frame::NONE
                .fill(fill)
                .corner_radius(egui::CornerRadius::same(ui_theme::RADIUS_ROW))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.set_min_height(22.0);
                    ui.horizontal(|ui| {
                        ui.label(ui_theme::icon(Icon::File, 14.0, ui_theme::TEXT_MUTED));
                        ui.label(ui_theme::text_title(entry.name.clone()));
                    });
                });
            let resp = row.response.interact(Sense::click());
            if resp.hovered() && !is_sel {
                ui.painter().rect_filled(
                    resp.rect,
                    egui::CornerRadius::same(ui_theme::RADIUS_ROW),
                    ui_theme::HOVER,
                );
            }
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
