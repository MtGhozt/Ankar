//! Footer under a plain shell (Warp idea): cwd chip (click picks a folder and `cd`s there),
//! git branch chip, changed-file count. Git runs off the UI thread, at most every few seconds.

use crate::gitinfo;
use crate::ui_theme::{self, Icon};
use egui::{Align2, CornerRadius, FontFamily, FontId, Rect, Sense, Stroke, Ui, Vec2};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const FOOTER_H: f32 = 26.0;
const GIT_EVERY: Duration = Duration::from_secs(5);

#[derive(Default)]
struct GitState {
    root: Option<PathBuf>,
    branch: Option<String>,
    changes: Option<usize>,
    checked: Option<Instant>,
    running: bool,
}

#[derive(Default)]
pub struct ShellFooter {
    git: Arc<Mutex<GitState>>,
}

/// `C:\Users\Matias\x` -> `~\x`.
pub fn display_cwd(cwd: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| cwd.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~\\{}", rest.display()),
        None => cwd.display().to_string(),
    }
}

impl ShellFooter {
    /// Paints the footer into `rect`; true when the cwd chip was clicked.
    pub fn draw(&mut self, ui: &mut Ui, rect: Rect, cwd: Option<&Path>, project_key: &str) -> bool {
        let root = Some(PathBuf::from(project_key)).filter(|r| r.join(".git").exists());
        self.refresh(ui.ctx(), root);
        let (branch, changes) = self
            .git
            .lock()
            .map(|g| (g.branch.clone(), g.changes))
            .unwrap_or((None, None));

        let mut x = rect.left() + 10.0;
        let mut clicked = false;
        if let Some(cwd) = cwd {
            let text = display_cwd(cwd, dirs::home_dir().as_deref());
            let resp = chip(ui, &mut x, rect, Some(Icon::Folder), &text, true).on_hover_text("Change directory…");
            clicked = resp.clicked();
        }
        if let Some(branch) = branch {
            chip(ui, &mut x, rect, None, &branch, false);
        }
        if let Some(n) = changes.filter(|&n| n > 0) {
            let label = if n == 1 { "1 file changed".to_owned() } else { format!("{n} files changed") };
            ui.painter().text(
                egui::pos2(x + 2.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::new(ui_theme::SIZE_UI_SMALL, FontFamily::Proportional),
                ui_theme::TEXT_MUTED,
            );
        }
        clicked
    }

    fn refresh(&mut self, ctx: &egui::Context, root: Option<PathBuf>) {
        let Ok(mut g) = self.git.lock() else {
            return;
        };
        let stale = g.root != root || g.checked.is_none_or(|t| t.elapsed() >= GIT_EVERY);
        if !stale || g.running {
            return;
        }
        if g.root != root {
            *g = GitState { root: root.clone(), ..GitState::default() };
        }
        g.checked = Some(Instant::now());
        let Some(root) = root else {
            return;
        };
        g.running = true;
        let (state, ctx) = (self.git.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("ankar-git".into()).spawn(move || {
            let branch = gitinfo::branch(&root);
            let changes = gitinfo::status_porcelain(&root).map(|s| gitinfo::count_changes(&s));
            if let Ok(mut g) = state.lock() {
                if g.root.as_deref() == Some(root.as_path()) {
                    g.branch = branch;
                    g.changes = changes;
                }
                g.running = false;
            }
            ctx.request_repaint();
        });
        if spawned.is_err() {
            g.running = false;
        }
    }
}

/// Subtle grey pill; advances `x` past it.
fn chip(ui: &mut Ui, x: &mut f32, row: Rect, icon: Option<Icon>, text: &str, clickable: bool) -> egui::Response {
    let font = FontId::new(ui_theme::SIZE_UI_SMALL, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, ui_theme::TEXT_MUTED);
    let icon_w = if icon.is_some() { 16.0 } else { 0.0 };
    let w = (8.0 + icon_w + galley.size().x + 8.0).min((row.right() - *x - 8.0).max(0.0));
    let rect = Rect::from_center_size(egui::pos2(*x + w / 2.0, row.center().y), Vec2::new(w, 18.0));
    *x = rect.right() + 6.0;
    let sense = if clickable { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, ui.scope_id().with(("footer_chip", text)), sense);
    let painter = ui.painter().with_clip_rect(rect);
    if clickable && resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(4), ui_theme::ROW_HOVER);
    }
    painter.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, ui_theme::CHROME_DIVIDER), egui::StrokeKind::Inside);
    if let Some(icon) = icon {
        painter.text(
            egui::pos2(rect.left() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            icon.codepoint().to_string(),
            FontId::new(11.0, FontFamily::Name(ui_theme::FONT_ICONS.into())),
            ui_theme::TEXT_MUTED,
        );
    }
    let text_pos = egui::pos2(rect.left() + 8.0 + icon_w, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, ui_theme::TEXT_MUTED);
    resp
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
