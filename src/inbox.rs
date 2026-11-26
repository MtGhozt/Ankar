//! Notifications inbox (Warp look): the tab bar's tray icon opens a small panel listing past
//! notifications (agent done, sessions that exited with an error), newest first, at most 100,
//! in memory only. Clicking one focuses its tab.

use crate::ui_theme::{self, Icon};
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Key, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::VecDeque;

pub const MAX_ITEMS: usize = 100;
const WIDTH: f32 = 340.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Done,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    pub tab_id: u64,
    pub tab_name: String,
    pub text: String,
    /// Local wall-clock "HH:MM" when it happened.
    pub time: String,
}

#[derive(Default)]
pub struct Inbox {
    items: VecDeque<Item>,
    open: bool,
    /// Filter chip: every tab, or only the active one.
    this_tab_only: bool,
}

impl Inbox {
    pub fn push(&mut self, kind: Kind, tab_id: u64, tab_name: &str, text: String) {
        let time = jiff::ZonedDateTime::now().strftime("%H:%M").to_string();
        self.items.push_front(Item { kind, tab_id, tab_name: tab_name.to_owned(), text, time });
        self.items.truncate(MAX_ITEMS);
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Newest first; only `active_tab`'s when the filter says so.
    fn visible(&self, active_tab: Option<u64>) -> impl Iterator<Item = &Item> {
        let only = self.this_tab_only.then_some(active_tab).flatten();
        self.items.iter().filter(move |i| only.is_none_or(|id| i.tab_id == id))
    }

    /// Draws the panel under `icon` (its right edge aligned); returns the tab to focus.
    pub fn show(&mut self, ctx: &egui::Context, icon: Rect, active_tab: Option<u64>) -> Option<u64> {
        if !self.open {
            return None;
        }
        let mut focus = None;
        let mut close = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        let area = egui::Area::new(egui::Id::unique("notifications_inbox"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(Pos2::new(icon.right(), icon.bottom() + 6.0))
            .show(ctx, |ui| {
                Frame::NONE
                    .fill(ui_theme::SURFACE_2)
                    .stroke(Stroke::new(1.0, ui_theme::CHIP_BORDER))
                    .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
                    .shadow(ui_theme::popup_shadow())
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Notifications").size(13.0).color(ui_theme::TEXT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let x = ui.add(egui::Button::new(ui_theme::icon(Icon::X, 13.0, ui_theme::TEXT_MUTED)).frame(false));
                                if x.on_hover_text("Close").clicked() {
                                    close = true;
                                }
                            });
                        });
                        let label = if self.this_tab_only { "This tab" } else { "All tabs" };
                        let chip = egui::Button::new(egui::RichText::new(label).size(ui_theme::SIZE_UI_SMALL).color(ui_theme::TEXT_MUTED))
                            .fill(ui_theme::CHIP_BG)
                            .stroke(Stroke::new(1.0, ui_theme::CHIP_BORDER))
                            .corner_radius(CornerRadius::same(4));
                        if ui.add(chip).on_hover_text("Show notifications from all tabs or only this one").clicked() {
                            self.this_tab_only = !self.this_tab_only;
                        }
                        let items: Vec<Item> = self.visible(active_tab).cloned().collect();
                        if items.is_empty() {
                            ui.add_space(18.0);
                            ui.vertical_centered(|ui| ui.label(ui_theme::text_sub("No notifications")));
                            ui.add_space(18.0);
                            return;
                        }
                        egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, true]).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            for item in &items {
                                if row(ui, item).clicked() {
                                    focus = Some(item.tab_id);
                                }
                            }
                        });
                    });
            });
        let clicked_away = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !icon.contains(p))
        });
        if close || clicked_away || focus.is_some() {
            self.open = false;
        }
        focus
    }
}

fn row(ui: &mut egui::Ui, item: &Item) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(WIDTH, 42.0), Sense::click());
    let p = ui.painter().with_clip_rect(rect);
    if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(4), ui_theme::HOVER);
    }
    let dot = match item.kind {
        Kind::Done => ui_theme::STATUS_DOT_OK,
        Kind::Error => ui_theme::ERROR,
    };
    p.circle_filled(Pos2::new(rect.left() + 10.0, rect.top() + 14.0), 3.0, dot);
    let font = |size| FontId::new(size, FontFamily::Proportional);
    p.text(Pos2::new(rect.left() + 22.0, rect.top() + 14.0), Align2::LEFT_CENTER, &item.text, font(ui_theme::SIZE_UI), ui_theme::TEXT);
    p.text(
        Pos2::new(rect.left() + 22.0, rect.top() + 30.0),
        Align2::LEFT_CENTER,
        format!("{} · {}", item.time, item.tab_name),
        font(ui_theme::SIZE_UI_SMALL),
        ui_theme::TEXT_FAINT,
    );
    resp.on_hover_text("Go to tab")
}

/// Inbox tray drawn as strokes (the icon font is a subset without one).
pub fn paint_inbox_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.3, color);
    let pt = |x: f32, y: f32| c + Vec2::new(x, y);
    p.add(egui::Shape::closed_line(vec![pt(-7.0, 1.0), pt(-4.5, -6.0), pt(4.5, -6.0), pt(7.0, 1.0), pt(7.0, 6.0), pt(-7.0, 6.0)], s));
    p.add(egui::Shape::line(vec![pt(-7.0, 1.0), pt(-3.0, 1.0), pt(-1.8, 3.2), pt(1.8, 3.2), pt(3.0, 1.0), pt(7.0, 1.0)], s));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_capped_at_100_and_filtered_by_tab() {
        let mut inbox = Inbox::default();
        for i in 0..105u64 {
            inbox.push(Kind::Done, i % 2, "tab", format!("n{i}"));
        }
        assert_eq!(inbox.items.len(), MAX_ITEMS);
        assert_eq!(inbox.visible(Some(1)).next().unwrap().text, "n104", "newest first, all tabs");
        assert_eq!(inbox.items.back().unwrap().text, "n5", "oldest dropped");
        inbox.this_tab_only = true;
        assert!(inbox.visible(Some(1)).all(|i| i.tab_id == 1));
        assert_eq!(inbox.visible(Some(1)).count(), 50);
        assert_eq!(inbox.visible(None).count(), 100, "no active tab: nothing to filter by");
    }

    #[test]
    fn the_panel_draws_empty_and_full_and_esc_closes_it() {
        let ctx = egui::Context::default();
        ui_theme::apply(&ctx);
        let icon = Rect::from_min_size(Pos2::new(900.0, 8.0), Vec2::splat(22.0));
        let mut inbox = Inbox::default();
        let frame = |inbox: &mut Inbox, events: Vec<egui::Event>| {
            let raw = egui::RawInput { events, ..Default::default() };
            let mut focus = None;
            let mut out = ctx.run_ui(raw, |ui| focus = inbox.show(ui.ctx(), icon, Some(1)));
            out.textures_delta.clear();
            focus
        };
        inbox.toggle();
        assert_eq!(frame(&mut inbox, Vec::new()), None);
        inbox.push(Kind::Error, 1, "pwsh", "pwsh exited with code 1".into());
        inbox.push(Kind::Done, 2, "Claude", "Claude finished working".into());
        assert_eq!(frame(&mut inbox, Vec::new()), None);
        assert!(inbox.is_open());
        let esc = egui::Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        frame(&mut inbox, vec![esc]);
        assert!(!inbox.is_open());
    }
}
