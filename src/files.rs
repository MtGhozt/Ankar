use crate::ui_theme;
use egui::{Color32, RichText, Ui};
use std::fs;
use std::path::{Path, PathBuf};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SynStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct FileViewer {
    pub root: Option<PathBuf>,
    pub open_file: Option<PathBuf>,
    pub content: String,
    pub highlighted: Vec<(Color32, String)>,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    pub show: bool,
    loaded_path: Option<PathBuf>,
}

impl Default for FileViewer {
    fn default() -> Self {
        Self {
            root: None,
            open_file: None,
            content: String::new(),
            highlighted: Vec::new(),
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            show: false,
            loaded_path: None,
        }
    }
}

impl FileViewer {
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        if self.root != root {
            self.root = root;
            self.open_file = None;
            self.loaded_path = None;
            self.content.clear();
            self.highlighted.clear();
        }
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match fs::read_to_string(&path) {
            Ok(text) => {
                let clipped = if text.len() > 512 * 1024 {
                    let mut t = text[..512 * 1024].to_string();
                    t.push_str("\n\n… truncated …");
                    t
                } else {
                    text
                };
                self.highlighted = highlight(&self.syntax_set, &self.theme_set, &path, &clipped);
                self.content = clipped;
                self.loaded_path = Some(path.clone());
                self.open_file = Some(path);
            }
            Err(e) => {
                self.content = format!("Failed to read file: {e}");
                self.highlighted.clear();
                self.loaded_path = Some(path.clone());
                self.open_file = Some(path);
            }
        }
    }

    pub fn draw(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Files").strong());
            if ui.small_button("Close").clicked() {
                self.show = false;
            }
        });
        ui.separator();

        let Some(root) = self.root.clone() else {
            ui.label("No project folder for the active session.");
            return;
        };

        let mut clicked: Option<PathBuf> = None;
        egui::ScrollArea::vertical()
            .id_salt("file_tree")
            .max_height(ui.available_height() * 0.35)
            .show(ui, |ui| {
                draw_tree(ui, &root, self.open_file.as_ref(), &mut clicked);
            });

        if let Some(path) = clicked {
            self.open_path(path);
        }

        if let Some(path) = self.open_file.clone() {
            ui.separator();
            ui.label(
                RichText::new(path.display().to_string())
                    .small()
                    .color(ui_theme::TEXT_MUTED),
            );
            egui::ScrollArea::both().id_salt("file_content").show(ui, |ui| {
                if self.highlighted.is_empty() {
                    ui.monospace(&self.content);
                } else {
                    // Render as wrapped runs; group by line breaks already in pieces.
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        for (color, text) in &self.highlighted {
                            if text.contains('\n') {
                                let parts: Vec<&str> = text.split_inclusive('\n').collect();
                                for part in parts {
                                    ui.label(
                                        RichText::new(part).monospace().color(*color).size(12.0),
                                    );
                                    if part.ends_with('\n') {
                                        ui.end_row();
                                    }
                                }
                            } else {
                                ui.label(RichText::new(text).monospace().color(*color).size(12.0));
                            }
                        }
                    });
                }
            });
        }
    }
}

fn draw_tree(
    ui: &mut Ui,
    dir: &Path,
    selected: Option<&PathBuf>,
    clicked: &mut Option<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    items.sort_by_key(|e| {
        (
            !e.path().is_dir(),
            e.file_name().to_string_lossy().to_lowercase(),
        )
    });

    for entry in items {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            egui::CollapsingHeader::new(name).show(ui, |ui| {
                draw_tree(ui, &path, selected, clicked);
            });
        } else if ui
            .selectable_label(selected == Some(&path), name)
            .clicked()
        {
            *clicked = Some(path);
        }
    }
}

fn highlight(
    ss: &SyntaxSet,
    ts: &ThemeSet,
    path: &Path,
    text: &str,
) -> Vec<(Color32, String)> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let syntax = ss
        .find_syntax_by_extension(ext)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let theme = ts
        .themes
        .get("base16-ocean.dark")
        .or_else(|| ts.themes.values().next())
        .expect("theme");
    let mut h = HighlightLines::new(syntax, theme);
    let mut out = Vec::new();
    for line in LinesWithEndings::from(text) {
        match h.highlight_line(line, ss) {
            Ok(ranges) => {
                for (style, piece) in ranges {
                    out.push((syn_color(style), piece.to_string()));
                }
            }
            Err(_) => out.push((ui_theme::TEXT, line.to_string())),
        }
    }
    out
}

fn syn_color(style: SynStyle) -> Color32 {
    Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b)
}
