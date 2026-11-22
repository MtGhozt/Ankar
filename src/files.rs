use crate::ui_theme::{self, Icon};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Sense, Ui, Vec2};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SynStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

const MAX_FILE_BYTES: u64 = 512 * 1024;

#[derive(Clone)]
struct DirEntryInfo {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

#[derive(Default)]
pub struct FileViewer {
    pub root: Option<PathBuf>,
    pub open_file: Option<PathBuf>,
    pub content: String,
    /// Cached highlighted layout; rebuilt only when a file is opened.
    pub highlighted_job: Option<LayoutJob>,
    syntax_set: Option<SyntaxSet>,
    theme: Option<Theme>,
    dir_cache: HashMap<PathBuf, Vec<DirEntryInfo>>,
    pub show: bool,
    loaded_path: Option<PathBuf>,
}

impl FileViewer {
    fn ensure_syntect(&mut self) {
        if self.syntax_set.is_some() {
            return;
        }
        self.syntax_set = Some(SyntaxSet::load_defaults_newlines());
        let mut ts = ThemeSet::load_defaults();
        let theme = ts
            .themes
            .remove("base16-ocean.dark")
            .or_else(|| ts.themes.into_values().next())
            .expect("syntect theme");
        self.theme = Some(theme);
    }

    /// Sync viewer root from the shell cwd. Does not clear navigation if the
    /// user already browsed elsewhere under the same project — only when the
    /// shell cwd itself changes.
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        let root = root.or_else(dirs::home_dir);
        if self.root != root {
            self.root = root;
            self.open_file = None;
            self.loaded_path = None;
            self.content.clear();
            self.highlighted_job = None;
            self.dir_cache.clear();
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

    pub fn open_path(&mut self, path: PathBuf) {
        match read_file_capped(&path) {
            Ok((text, truncated)) => {
                let mut clipped = text;
                if truncated {
                    clipped.push_str("\n\n… truncated …");
                }
                self.ensure_syntect();
                let job = highlight_job(
                    self.syntax_set.as_ref().unwrap(),
                    self.theme.as_ref().unwrap(),
                    &path,
                    &clipped,
                );
                self.highlighted_job = Some(job);
                self.content = clipped;
                self.loaded_path = Some(path.clone());
                self.open_file = Some(path);
            }
            Err(e) => {
                self.content = format!("Failed to read file: {e}");
                self.highlighted_job = None;
                self.loaded_path = Some(path.clone());
                self.open_file = Some(path);
            }
        }
    }

    pub fn draw(&mut self, ui: &mut Ui) {
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
            self.open_path(path);
        }

        if let Some(path) = self.open_file.clone() {
            ui.add_space(4.0);
            let (sep2, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(sep2, 0.0, ui_theme::BORDER);
            ui.label(ui_theme::text_sub(path.display().to_string()));
            egui::ScrollArea::both().id_salt("file_content").show(ui, |ui| {
                if let Some(job) = self.highlighted_job.clone() {
                    ui.label(job);
                } else {
                    ui.monospace(&self.content);
                }
            });
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

fn highlight_job(ss: &SyntaxSet, theme: &Theme, path: &Path, text: &str) -> LayoutJob {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);
    let mut job = LayoutJob::default();
    let font = FontId::monospace(12.0);
    for line in LinesWithEndings::from(text) {
        match h.highlight_line(line, ss) {
            Ok(ranges) => {
                for (style, piece) in ranges {
                    job.append(
                        piece,
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: syn_color(style),
                            ..Default::default()
                        },
                    );
                }
            }
            Err(_) => {
                job.append(
                    line,
                    0.0,
                    TextFormat {
                        font_id: font.clone(),
                        color: ui_theme::TEXT,
                        ..Default::default()
                    },
                );
            }
        }
    }
    job
}

fn syn_color(style: SynStyle) -> Color32 {
    // Dynamic colors from syntect theme — routed through ui_theme.
    ui_theme::rgb(style.foreground.r, style.foreground.g, style.foreground.b)
}
