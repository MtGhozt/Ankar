//! Footer under a plain shell (Warp look): a cwd chip that opens the in-app "cd" picker, a git
//! branch chip, and a changed-files chip with +N/-N lines that opens the changes pane. The git
//! facts come from the app's [`crate::changes::GitWatch`].

use crate::changes::GitSnapshot;
use crate::dir_picker::DirPicker;
use crate::ui_theme::{self, Icon};
use egui::text::LayoutJob;
use egui::{Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, TextFormat, Ui, Vec2};
use std::path::{Path, PathBuf};

pub const FOOTER_H: f32 = 26.0;
const CHIP_H: f32 = 18.0;

#[derive(Default)]
pub struct ShellFooter {
    picker: DirPicker,
}

pub enum FooterAction {
    Cd(PathBuf),
    OpenChanges,
}

#[derive(Clone, Copy)]
enum ChipIcon {
    Glyph(Icon),
    Branch,
}

/// `C:\Users\Matias\x` -> `~\x`.
pub fn display_cwd(cwd: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| cwd.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~\\{}", rest.display()),
        None => cwd.display().to_string(),
    }
}

fn plain(text: &str, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    append(&mut job, text, color);
    job
}

fn append(job: &mut LayoutJob, text: &str, color: Color32) {
    let format = TextFormat::simple(FontId::new(ui_theme::SIZE_UI_SMALL, FontFamily::Proportional), color);
    job.append(text, 0.0, format);
}

impl ShellFooter {
    /// Paints the footer into `rect`; reports a cd from the picker or a click on the changes chip.
    pub fn draw(&mut self, ui: &mut Ui, rect: Rect, cwd: Option<&Path>, git: &GitSnapshot) -> Option<FooterAction> {
        let (branch, totals) = (git.branch.clone(), git.totals());
        let mut x = rect.left() + 10.0;
        let mut action = None;
        if let Some(cwd) = cwd {
            let text = display_cwd(cwd, dirs::home_dir().as_deref());
            let job = plain(&text, ui_theme::TEXT_MUTED);
            let resp = chip(ui, &mut x, rect, Some(ChipIcon::Glyph(Icon::Folder)), job, "cwd");
            if resp.clicked() {
                self.picker.toggle(cwd);
            }
            if let Some(dir) = self.picker.show(ui.ctx(), resp.rect.left_top() - Vec2::new(0.0, 4.0), resp.rect, cwd) {
                action = Some(FooterAction::Cd(dir));
            }
        }
        if let Some(branch) = branch {
            chip(ui, &mut x, rect, Some(ChipIcon::Branch), plain(&branch, ui_theme::TEXT_MUTED), "branch");
        }
        if let Some((files, added, removed)) = totals.filter(|t| t.0 > 0) {
            let mut job = plain(&files.to_string(), ui_theme::TEXT_MUTED);
            if added > 0 || removed > 0 {
                append(&mut job, "  \u{2022}  ", ui_theme::TEXT_FAINT);
            }
            if added > 0 {
                append(&mut job, &format!("+{added}"), ui_theme::DIFF_ADD);
            }
            if removed > 0 {
                append(&mut job, &format!("{}-{removed}", if added > 0 { " " } else { "" }), ui_theme::DIFF_DEL);
            }
            let resp = chip(ui, &mut x, rect, Some(ChipIcon::Glyph(Icon::File)), job, "changes")
                .on_hover_text("Show changes");
            if resp.clicked() {
                action = Some(FooterAction::OpenChanges);
            }
        }
        action
    }
}

/// Rounded chip (#1a1a1a fill, 1 px #333 border, 11 px text); clickable, advances `x` past it.
fn chip(ui: &mut Ui, x: &mut f32, row: Rect, icon: Option<ChipIcon>, job: LayoutJob, id: &str) -> egui::Response {
    let galley = ui.painter().layout_job(job);
    let icon_w = if icon.is_some() { 16.0 } else { 0.0 };
    let w = (8.0 + icon_w + galley.size().x + 8.0).min((row.right() - *x - 8.0).max(0.0));
    let rect = Rect::from_center_size(egui::pos2(*x + w / 2.0, row.center().y), Vec2::new(w, CHIP_H));
    *x = rect.right() + 6.0;
    let resp = ui.interact(rect, ui.scope_id().with(("footer_chip", id)), Sense::click());
    let painter = ui.painter().with_clip_rect(rect);
    let fill = if resp.hovered() { ui_theme::SURFACE_SEL } else { ui_theme::CHIP_BG };
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, ui_theme::CHIP_BORDER), egui::StrokeKind::Inside);
    let icon_center = egui::pos2(rect.left() + 8.0 + 5.5, rect.center().y);
    match icon {
        Some(ChipIcon::Glyph(icon)) => {
            painter.text(
                icon_center,
                egui::Align2::CENTER_CENTER,
                icon.codepoint().to_string(),
                FontId::new(11.0, FontFamily::Name(ui_theme::FONT_ICONS.into())),
                ui_theme::TEXT_MUTED,
            );
        }
        Some(ChipIcon::Branch) => paint_branch_icon(&painter, icon_center, ui_theme::TEXT_MUTED),
        None => {}
    }
    let text_pos = egui::pos2(rect.left() + 8.0 + icon_w, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, ui_theme::TEXT_MUTED);
    resp
}

/// Git-branch mark in an 11 px box: a trunk with a commit at its foot, a side commit, and the
/// curve that joins them.
fn paint_branch_icon(painter: &egui::Painter, center: Pos2, color: Color32) {
    let p = |x: f32, y: f32| center + Vec2::new(x, y);
    let stroke = Stroke::new(1.1, color);
    painter.line_segment([p(-2.5, -5.0), p(-2.5, 1.6)], stroke);
    painter.circle_stroke(p(-2.5, 3.2), 1.6, stroke);
    painter.circle_stroke(p(3.0, -2.6), 1.6, stroke);
    painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
        [p(3.0, -1.0), p(3.0, 2.6), p(-0.9, 3.2)],
        false,
        Color32::TRANSPARENT,
        stroke,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_prefix_becomes_a_tilde() {
        let home = Path::new(r"C:\Users\Matias");
        assert_eq!(display_cwd(Path::new(r"C:\Users\Matias\Programacion"), Some(home)), r"~\Programacion");
        assert_eq!(display_cwd(home, Some(home)), "~");
        assert_eq!(display_cwd(Path::new(r"D:\work"), Some(home)), r"D:\work");
        assert_eq!(display_cwd(Path::new(r"C:\Users\Matiasx"), Some(home)), r"C:\Users\Matiasx");
    }
}
