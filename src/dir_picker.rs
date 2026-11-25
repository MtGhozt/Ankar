//! The footer's "cd" popup (Warp look): opens above the cwd chip with a search field, a
//! ".. (Parent Directory)" row, then the folders and greyed-out files of the shell's directory.
//! Arrow keys or the mouse move, Enter or a click cds, typing filters, Esc closes.

use crate::ui_theme::{self, Icon};
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use std::path::{Path, PathBuf};
use std::time::Duration;

const WIDTH: f32 = 360.0;
const ROW_H: f32 = 24.0;
const MAX_ROWS: usize = 10;
const SELECTED: Color32 = Color32::from_rgb(0x1E, 0x9F, 0xD6);

/// Upper bound on footer cd popup height (`LEFT_BOTTOM` pivot): search row + divider + scroll cap.
fn footer_popup_max_height() -> f32 {
    4.0 * 2.0 + 6.0 * 2.0 + ui_theme::SIZE_UI + 4.0 + 5.0 + ROW_H * MAX_ROWS as f32
}

/// Keep a footer popup fully on screen when it would cross the top edge.
pub(crate) fn clamp_footer_popup_anchor(ctx: &egui::Context, anchor: Pos2) -> Pos2 {
    let top = ctx.content_rect().top();
    let y = anchor.y.max(top + footer_popup_max_height());
    Pos2::new(anchor.x, y)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    name: String,
    is_dir: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Parent,
    Entry(usize),
}

#[derive(Default)]
pub struct DirPicker {
    dir: Option<PathBuf>,
    entries: Vec<Entry>,
    query: String,
    /// Index into the visible rows.
    selected: usize,
    just_opened: bool,
    /// Lists folders only (new session in folder); footer still shows greyed files.
    folders_only: bool,
    /// Parent and folder rows navigate in the listing; Enter picks the current directory.
    new_session: bool,
}

/// Folders first, then files, each sorted case-insensitively; dot entries hidden like Warp.
fn list(dir: &Path, folders_only: bool) -> Vec<Entry> {
    let mut entries: Vec<Entry> = std::fs::read_dir(dir)
        .map(|it| {
            it.filter_map(Result::ok)
                .map(|e| Entry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    is_dir: e.file_type().is_ok_and(|t| t.is_dir()),
                })
                .filter(|e| !e.name.starts_with('.') && (!folders_only || e.is_dir))
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by_key(|e| (!e.is_dir, e.name.to_lowercase()));
    entries
}

/// ".." while there is a parent and nothing typed, then every entry matching the query.
fn visible_rows(entries: &[Entry], has_parent: bool, query: &str) -> Vec<Row> {
    let q = query.trim().to_lowercase();
    let parent = (has_parent && q.is_empty()).then_some(Row::Parent);
    parent
        .into_iter()
        .chain(entries.iter().enumerate().filter(|(_, e)| e.name.to_lowercase().contains(&q)).map(|(i, _)| Row::Entry(i)))
        .collect()
}

/// Path to show, whether a picker click may type another `cd`, and whether to drop `pending`.
/// `pending` is (target, directory from before the cd, how long ago). The second only re-enables
/// clicks; the target stays shown until the shell reports it or some other directory.
pub fn cwd_follow(reported: &Path, pending: Option<(&Path, &Path, Duration)>) -> (PathBuf, bool, bool) {
    let Some((target, origin, elapsed)) = pending else {
        return (reported.to_path_buf(), true, false);
    };
    if reported == target {
        return (target.to_path_buf(), true, true);
    }
    if reported != origin {
        return (reported.to_path_buf(), true, true);
    }
    if elapsed < Duration::from_secs(1) {
        return (target.to_path_buf(), false, false);
    }
    (target.to_path_buf(), true, false)
}

fn selectable(row: Row, entries: &[Entry]) -> bool {
    match row {
        Row::Parent => true,
        Row::Entry(i) => entries[i].is_dir,
    }
}

/// Moves the selection by `delta` rows, skipping files; stays put at either end.
fn step(rows: &[Row], entries: &[Entry], selected: usize, delta: isize) -> usize {
    let mut i = selected as isize;
    loop {
        i += delta;
        if i < 0 || i >= rows.len() as isize {
            return selected;
        }
        if selectable(rows[i as usize], entries) {
            return i as usize;
        }
    }
}

/// First selectable row, so Enter right after typing picks the best match.
fn first_selectable(rows: &[Row], entries: &[Entry]) -> usize {
    rows.iter().position(|&r| selectable(r, entries)).unwrap_or(0)
}

impl DirPicker {
    pub fn is_open(&self) -> bool {
        self.dir.is_some()
    }

    pub fn close(&mut self) {
        self.dir = None;
        self.new_session = false;
        self.folders_only = false;
    }

    pub fn toggle(&mut self, cwd: &Path) {
        if self.is_open() {
            self.dir = None;
            self.new_session = false;
            self.folders_only = false;
            return;
        }
        self.new_session = false;
        self.folders_only = false;
        self.entries = list(cwd, false);
        self.dir = Some(cwd.to_path_buf());
        self.query.clear();
        self.just_opened = true;
        let rows = visible_rows(&self.entries, cwd.parent().is_some(), "");
        self.selected = first_selectable(&rows, &self.entries);
    }

    /// Opens below the sidebar folder button; toggles closed when clicked again.
    pub fn toggle_new_session(&mut self) {
        if self.is_open() && self.new_session {
            self.dir = None;
            self.new_session = false;
            self.folders_only = false;
            return;
        }
        self.new_session = true;
        self.folders_only = true;
        let start = PathBuf::from(r"C:\");
        self.entries = list(&start, true);
        self.dir = Some(start);
        self.query.clear();
        self.just_opened = true;
        let has_parent = self.dir.as_ref().and_then(|d| d.parent()).is_some();
        let rows = visible_rows(&self.entries, has_parent, "");
        self.selected = first_selectable(&rows, &self.entries);
    }

    /// Draws the popup at `anchor` (footer: bottom-left above the chip; new session: top-left below the button).
    /// Returns a directory to cd into, or for new session mode the folder to start in.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        anchor: egui::Pos2,
        dismiss_rect: Rect,
        external_cwd: Option<&Path>,
    ) -> Option<PathBuf> {
        if !self.new_session {
            if let Some(cwd) = external_cwd {
                if self.dir.as_deref().is_some_and(|d| d != cwd) {
                    self.entries = list(cwd, false);
                    self.dir = Some(cwd.to_path_buf());
                    self.query.clear();
                    let rows = visible_rows(&self.entries, cwd.parent().is_some(), "");
                    self.selected = first_selectable(&rows, &self.entries);
                }
            }
        }
        let dir = self.dir.clone()?;
        let rows = visible_rows(&self.entries, dir.parent().is_some(), &self.query);
        let (up, down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if up {
            self.selected = step(&rows, &self.entries, self.selected, -1);
        }
        if down {
            self.selected = step(&rows, &self.entries, self.selected, 1);
        }
        let mut chosen = None;
        let mut confirm = None;
        if enter {
            if self.new_session {
                confirm = Some(dir.clone());
            } else {
                chosen = rows.get(self.selected).copied().filter(|&r| selectable(r, &self.entries));
            }
        }

        let (pivot, area_id) = if self.new_session {
            (Align2::LEFT_TOP, egui::Id::unique("new_session_dir_picker"))
        } else {
            (Align2::LEFT_BOTTOM, egui::Id::unique("footer_dir_picker"))
        };
        let anchor = if self.new_session {
            anchor
        } else {
            clamp_footer_popup_anchor(ctx, anchor)
        };
        let area = egui::Area::new(area_id)
            .order(egui::Order::Foreground)
            .pivot(pivot)
            .fixed_pos(anchor)
            .show(ctx, |ui| {
                Frame::NONE
                    .fill(ui_theme::SURFACE_2)
                    .stroke(Stroke::new(1.0, ui_theme::CHIP_BORDER))
                    .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
                    .shadow(ui_theme::popup_shadow())
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        let before = self.query.clone();
                        let edit = egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Search directories...")
                            .font(FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional))
                            .text_color(ui_theme::TEXT)
                            .frame(Frame::NONE)
                            .margin(Vec2::new(8.0, 6.0))
                            .desired_width(f32::INFINITY);
                        let resp = ui.add(edit);
                        if std::mem::take(&mut self.just_opened) {
                            resp.request_focus();
                        }
                        if self.query != before {
                            let rows = visible_rows(&self.entries, dir.parent().is_some(), &self.query);
                            self.selected = first_selectable(&rows, &self.entries);
                        }
                        let rows = visible_rows(&self.entries, dir.parent().is_some(), &self.query);
                        let (line, _) = ui.allocate_exact_size(Vec2::new(WIDTH, 5.0), Sense::hover());
                        ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, ui_theme::CHIP_BORDER));
                        egui::ScrollArea::vertical()
                            .max_height(ROW_H * MAX_ROWS as f32)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (i, &row) in rows.iter().enumerate() {
                                    if let Some(hit) = self.row(ui, i, row, &rows) {
                                        chosen = Some(hit);
                                    }
                                }
                            });
                    });
            });

        let clicked_away = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !dismiss_rect.contains(p))
        });
        if esc || clicked_away {
            self.dir = None;
            self.new_session = false;
            self.folders_only = false;
        }
        if let Some(path) = confirm {
            self.dir = None;
            self.new_session = false;
            self.folders_only = false;
            return Some(path);
        }
        if self.new_session {
            if let Some(row) = chosen {
                let next = match row {
                    Row::Parent => dir.parent().unwrap_or(&dir).to_path_buf(),
                    Row::Entry(i) => dir.join(&self.entries[i].name),
                };
                self.entries = list(&next, true);
                self.dir = Some(next);
                self.query.clear();
                let rows = visible_rows(&self.entries, self.dir.as_ref().and_then(|d| d.parent()).is_some(), "");
                self.selected = first_selectable(&rows, &self.entries);
                return None;
            }
            return None;
        }
        chosen.map(|row| match row {
            Row::Parent => dir.parent().unwrap_or(&dir).to_path_buf(),
            Row::Entry(i) => dir.join(&self.entries[i].name),
        })
    }

    /// One row; returns it when clicked.
    fn row(&mut self, ui: &mut egui::Ui, i: usize, row: Row, rows: &[Row]) -> Option<Row> {
        let can_pick = selectable(row, &self.entries);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(WIDTH, ROW_H), Sense::click());
        if can_pick && resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
            self.selected = i;
        }
        let on = can_pick && i == self.selected;
        if on {
            ui.painter().rect_filled(rect, CornerRadius::same(ui_theme::RADIUS_KBD), SELECTED);
            if i < rows.len() && ui.input(|inp| inp.key_down(Key::ArrowUp) || inp.key_down(Key::ArrowDown)) {
                resp.scroll_to_me(None);
            }
        }
        let (icon, name, color) = match row {
            Row::Parent => ("\u{2191}".to_owned(), ".. (Parent Directory)".to_owned(), ui_theme::TEXT_MUTED),
            Row::Entry(e) => {
                let entry = &self.entries[e];
                let icon = if entry.is_dir { Icon::Folder } else { Icon::File };
                let color = if entry.is_dir { ui_theme::TEXT } else { ui_theme::TEXT_FAINT };
                (icon.codepoint().to_string(), entry.name.clone(), color)
            }
        };
        let color = if on { Color32::WHITE } else { color };
        let icon_font = match row {
            Row::Parent => FontId::new(12.0, FontFamily::Proportional),
            Row::Entry(_) => FontId::new(12.0, FontFamily::Name(ui_theme::FONT_ICONS.into())),
        };
        let painter = ui.painter().with_clip_rect(rect);
        painter.text(egui::pos2(rect.left() + 14.0, rect.center().y), Align2::CENTER_CENTER, icon, icon_font, color);
        painter.text(
            egui::pos2(rect.left() + 28.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional),
            color,
        );
        (can_pick && resp.clicked()).then_some(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, is_dir: bool) -> Entry {
        Entry { name: name.into(), is_dir }
    }

    #[test]
    fn pending_cd_stays_on_the_target_until_the_shell_reports() {
        let here = Path::new(r"C:\Programacion\Terminal");
        let parent = Path::new(r"C:\Programacion");
        let elsewhere = Path::new(r"C:\Other");
        let pending = |elapsed| Some((parent, here, elapsed));
        assert_eq!(cwd_follow(here, None), (here.into(), true, false));
        assert_eq!(cwd_follow(parent, pending(Duration::from_millis(10))), (parent.into(), true, true));
        assert_eq!(cwd_follow(elsewhere, pending(Duration::from_millis(10))), (elsewhere.into(), true, true));
        assert_eq!(cwd_follow(here, pending(Duration::from_millis(999))), (parent.into(), false, false));
        assert_eq!(cwd_follow(here, pending(Duration::from_secs(1))), (parent.into(), true, false));
    }

    #[test]
    fn parent_first_then_matches_and_typing_hides_the_parent() {
        let entries = [e("assets", true), e("docs", true), e("AGENTS.md", false)];
        assert_eq!(
            visible_rows(&entries, true, ""),
            vec![Row::Parent, Row::Entry(0), Row::Entry(1), Row::Entry(2)]
        );
        assert_eq!(visible_rows(&entries, true, "DO"), vec![Row::Entry(1)], "case-insensitive filter");
        assert_eq!(visible_rows(&entries, false, ""), vec![Row::Entry(0), Row::Entry(1), Row::Entry(2)], "drive root");
    }

    #[test]
    fn arrows_skip_files_and_stop_at_the_ends() {
        let entries = [e("src", true), e("Cargo.toml", false), e("README.md", false), e("zeta", true)];
        let rows = [Row::Parent, Row::Entry(1), Row::Entry(0), Row::Entry(2), Row::Entry(3)];
        assert_eq!(step(&rows, &entries, 0, 1), 2, "file row skipped");
        assert_eq!(step(&rows, &entries, 2, 1), 4);
        assert_eq!(step(&rows, &entries, 4, 1), 4, "end");
        assert_eq!(step(&rows, &entries, 0, -1), 0, "start");
        assert_eq!(first_selectable(&[Row::Entry(1), Row::Entry(3)], &entries), 1);
    }

    #[test]
    fn listing_puts_folders_first_and_hides_dot_entries() {
        let dir = std::env::temp_dir().join(format!("ankar-dirpick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["b", "A", ".git"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        for f in ["a.txt", ".env"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        assert_eq!(list(&dir, false), vec![e("A", true), e("b", true), e("a.txt", false)]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
