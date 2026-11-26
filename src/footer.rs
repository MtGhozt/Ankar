//! Footer under a plain shell (Warp look): a cwd chip that opens the in-app "cd" picker, a git
//! branch chip, and a changed-files chip with +N/-N lines that opens the changes pane. The git
//! facts come from the app's [`crate::changes::GitWatch`].

use crate::changes::{GitSnapshot, GitWatch};
use crate::dir_picker::{self, DirPicker};
use crate::gitinfo;
use crate::ui_theme::{self, Icon};
use egui::text::LayoutJob;
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Key, Modifiers, Pos2, Rect, Sense, Stroke, TextFormat, Ui, Vec2};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

const FOOTER_BOTTOM_MARGIN: f32 = 8.0;
const FOOTER_BLOCK_PAD: f32 = 6.0;
const FOOTER_BLOCK_BORDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
const CHIP_H: f32 = 18.0;
const FOOTER_BLOCK_H: f32 = 1.0 + FOOTER_BLOCK_PAD + CHIP_H + FOOTER_BLOCK_PAD;

/// Bottom panel: margin under the chip block, then the block (top rule + padded chips).
pub const FOOTER_H: f32 = FOOTER_BOTTOM_MARGIN + FOOTER_BLOCK_H;

#[derive(Default)]
pub struct ShellFooter {
    picker: DirPicker,
    branches: BranchMenu,
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
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cwd: Option<&Path>,
        git: &GitSnapshot,
        watch: &GitWatch,
    ) -> Option<FooterAction> {
        let block_bottom = rect.bottom() - FOOTER_BOTTOM_MARGIN;
        let block_top = block_bottom - FOOTER_BLOCK_H;
        let block = Rect::from_min_max(egui::pos2(rect.left(), block_top), egui::pos2(rect.right(), block_bottom));
        ui.painter()
            .hline(block.x_range(), block_top, Stroke::new(1.0, FOOTER_BLOCK_BORDER));
        let row_top = block_top + 1.0 + FOOTER_BLOCK_PAD;
        let row = Rect::from_min_max(
            egui::pos2(block.left() + FOOTER_BLOCK_PAD, row_top),
            egui::pos2(block.right() - FOOTER_BLOCK_PAD, row_top + CHIP_H),
        );

        let (branch, totals) = (git.branch.clone(), git.totals());
        let mut x = row.left();
        let mut action = None;
        self.branches.poll(watch);
        let mut cd = None;
        if let Some(cwd) = cwd {
            let text = display_cwd(cwd, dirs::home_dir().as_deref());
            let job = plain(&text, ui_theme::TEXT_MUTED);
            let resp = chip(ui, &mut x, rect, Some(ChipIcon::Glyph(Icon::Folder)), job, "cwd");
            if resp.clicked() {
                self.branches.dismiss();
                self.picker.toggle(cwd);
            }
            cd = self
                .picker
                .show(ui.ctx(), resp.rect.left_top() - Vec2::new(0.0, 4.0), resp.rect, Some(cwd));
        }
        if let Some(branch) = branch {
            let resp = chip(ui, &mut x, rect, Some(ChipIcon::Branch), plain(&branch, ui_theme::TEXT_MUTED), "branch");
            if resp.clicked() {
                // The cd popup can cover this chip; that click is not a directory choice.
                self.picker.close();
                cd = None;
                if let Some(root) = git.root.as_deref() {
                    self.branches.toggle(ui.ctx(), root);
                }
            }
            let anchor = resp.rect.left_top() - Vec2::new(0.0, 4.0);
            self.branches.show(ui.ctx(), anchor, resp.rect, git.root.as_deref(), &branch);
        }
        if let Some(dir) = cd {
            action = Some(FooterAction::Cd(dir));
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

const MENU_W: f32 = 360.0;
const MENU_ROW_H: f32 = 24.0;
const MENU_ROWS: usize = 10;

/// Local-branch list above the footer chip. Listing and `git switch` run on `ankar-git-job`.
#[derive(Default)]
struct BranchMenu {
    open: bool,
    root: Option<PathBuf>,
    query: String,
    names: Vec<String>,
    list: Option<Receiver<Vec<String>>>,
    job: Option<Receiver<Result<(), String>>>,
    error: Option<String>,
    just_opened: bool,
}

fn spawn_list(ctx: &egui::Context, root: PathBuf) -> Option<Receiver<Vec<String>>> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("ankar-git-job".into()).spawn(move || {
        let names = gitinfo::run(
            &root,
            &["for-each-ref", "--sort=-committerdate", "refs/heads", "--format=%(refname:short)"],
        )
        .map(|s| s.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect())
        .unwrap_or_default();
        let _ = tx.send(names);
        ctx.request_repaint();
    });
    spawned.ok()?;
    Some(rx)
}

fn spawn_switch(ctx: &egui::Context, root: PathBuf, branch: String) -> Option<Receiver<Result<(), String>>> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("ankar-git-job".into()).spawn(move || {
        let _ = tx.send(switch_branch(&root, &branch));
        ctx.request_repaint();
    });
    spawned.ok()?;
    Some(rx)
}

fn switch_branch(root: &Path, branch: &str) -> Result<(), String> {
    let out = gitinfo::command(root, &["switch", "--", branch])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let text = err.trim();
    Err(if text.is_empty() { "git failed".into() } else { text.to_owned() })
}

impl BranchMenu {
    fn dismiss(&mut self) {
        self.open = false;
    }

    fn toggle(&mut self, ctx: &egui::Context, root: &Path) {
        if self.open {
            self.open = false;
            return;
        }
        self.open = true;
        self.root = Some(root.to_path_buf());
        self.query.clear();
        self.names.clear();
        self.error = None;
        self.just_opened = true;
        self.list = spawn_list(ctx, root.to_path_buf());
    }

    /// Applies a finished `git switch` and drops the wait on [`GitWatch::refresh`].
    fn poll(&mut self, watch: &GitWatch) {
        if let Some(Ok(names)) = self.list.as_ref().map(|rx| rx.try_recv()) {
            self.list = None;
            self.names = names;
        }
        if let Some(Ok(result)) = self.job.as_ref().map(|rx| rx.try_recv()) {
            self.job = None;
            watch.invalidate();
            if self.open {
                match result {
                    Ok(()) => self.open = false,
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }

    fn show(&mut self, ctx: &egui::Context, anchor: Pos2, dismiss: Rect, root: Option<&Path>, current: &str) {
        if self.open && self.root.as_deref() != root {
            self.open = false;
        }
        if !self.open {
            return;
        }
        let esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let anchor = dir_picker::clamp_footer_popup_anchor(ctx, anchor);
        let mut pick = None;
        let area = egui::Area::new(egui::Id::unique("footer_branch_menu"))
            .order(egui::Order::Foreground)
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(anchor)
            .show(ctx, |ui| {
                Frame::NONE
                    .fill(ui_theme::SURFACE_2)
                    .stroke(Stroke::new(1.0, ui_theme::CHIP_BORDER))
                    .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
                    .shadow(ui_theme::popup_shadow())
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.set_width(MENU_W);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        let edit = egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Search branches...")
                            .font(FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional))
                            .text_color(ui_theme::TEXT)
                            .frame(Frame::NONE)
                            .margin(Vec2::new(8.0, 6.0))
                            .desired_width(f32::INFINITY);
                        let resp = ui.add(edit);
                        if std::mem::take(&mut self.just_opened) {
                            resp.request_focus();
                        }
                        let (line, _) = ui.allocate_exact_size(Vec2::new(MENU_W, 5.0), Sense::hover());
                        ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, ui_theme::CHIP_BORDER));
                        let q = self.query.trim().to_lowercase();
                        let names: Vec<String> = self
                            .names
                            .iter()
                            .filter(|n| q.is_empty() || n.to_lowercase().contains(&q))
                            .cloned()
                            .collect();
                        let error = self.error.clone();
                        let busy = self.job.is_some();
                        egui::ScrollArea::vertical()
                            .max_height(MENU_ROW_H * MENU_ROWS as f32)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                if let Some(err) = error {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(err).size(ui_theme::SIZE_UI_SMALL).color(ui_theme::ERROR),
                                        )
                                        .wrap(),
                                    );
                                }
                                for name in names {
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(MENU_W, MENU_ROW_H), Sense::click());
                                    let on = name == current;
                                    if resp.hovered() {
                                        ui.painter().rect_filled(rect, CornerRadius::same(4), ui_theme::ROW_HOVER);
                                    }
                                    let color = if on { ui_theme::TEXT } else { ui_theme::TEXT_MUTED };
                                    let label = if on { format!("\u{2713} {name}") } else { format!("   {name}") };
                                    ui.painter().text(
                                        egui::pos2(rect.left() + 10.0, rect.center().y),
                                        Align2::LEFT_CENTER,
                                        label,
                                        FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional),
                                        color,
                                    );
                                    if resp.clicked() && !busy {
                                        pick = Some(name);
                                    }
                                }
                            });
                    });
            });
        let clicked_away = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !dismiss.contains(p))
        });
        if esc || clicked_away {
            self.open = false;
            return;
        }
        if self.job.is_none() {
            if let Some((name, root)) = pick.zip(self.root.clone()) {
                self.error = None;
                self.job = spawn_switch(ctx, root, name);
            }
        }
    }
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
