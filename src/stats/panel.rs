//! Agent usage panel — layout matched to midudev's stats mockup (egui Painter only).

use super::charts;
use super::format::{format_hours_short, format_pct_delta, format_tokens, format_with_commas};
use super::loader::{StatsLoader, StatsRange};
use super::{tables, AgentFilter, StatsSnapshot};
use crate::config::Config;
use crate::logos::LogoAtlas;
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Margin, Rect, RichText, Sense, Stroke,
    Ui, Vec2,
};
use std::sync::{Arc, Mutex};

const BG: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
const CARD_BG: Color32 = Color32::from_rgb(0x0C, 0x0C, 0x0C);
pub(super) const CARD_BORDER: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
pub(super) const MUTED: Color32 = Color32::from_rgb(0x83, 0x83, 0x83);
pub(super) const TEXT: Color32 = Color32::from_rgb(0xE7, 0xE7, 0xE7);
const SEG_BG: Color32 = Color32::from_rgb(0x10, 0x10, 0x10);
const SEG_ACTIVE: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);
const ESC_BG: Color32 = Color32::from_rgb(0x0E, 0x0E, 0x0E);
const ESC_BORDER: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
const BADGE_POS_FG: Color32 = Color32::from_rgb(0x5B, 0xAF, 0x78);
const BADGE_POS_BG: Color32 = Color32::from_rgb(0x0D, 0x2D, 0x15);
const BADGE_NEG_FG: Color32 = Color32::from_rgb(0xE5, 0x69, 0x5B);
const BADGE_NEG_BG: Color32 = Color32::from_rgb(0x2A, 0x0F, 0x0F);
const SPARK_BAR: Color32 = Color32::from_rgb(0x3A, 0x3A, 0x3A);
pub(super) const BAR_IN: Color32 = Color32::from_rgb(0x3E, 0x3E, 0x3E);
pub(super) const BAR_OUT: Color32 = Color32::from_rgb(0x4A, 0x9F, 0xFF);
pub struct StatsPanel {
    pub open: bool,
    pub range: StatsRange,
    loader: StatsLoader,
    snapshot: Option<StatsSnapshot>,
    filter_agent: Option<AgentFilter>,
    /// Opened this frame: the app moves the pane to the active tab and closes the other panes.
    pub just_opened: bool,
    /// Last click landed in the pane: Esc closes it instead of reaching the terminal.
    pub focused: bool,
}

impl StatsPanel {
    pub fn new(ankar: Arc<Mutex<super::AnkarStatsStore>>) -> Self {
        Self {
            open: false,
            range: StatsRange::Days30,
            loader: StatsLoader::new(ankar),
            snapshot: None,
            filter_agent: None,
            just_opened: false,
            focused: false,
        }
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open = true;
            self.just_opened = true;
            self.focused = true;
            self.loader.request(self.range, None, false);
        }
    }

    /// Close and free the snapshot and loader cache.
    pub fn close(&mut self) {
        self.open = false;
        self.just_opened = false;
        self.focused = false;
        self.snapshot = None;
        self.loader.clear_cache();
        self.filter_agent = None;
    }

    /// Esc: clears the agent filter first, else closes. Returns true when it closed.
    pub fn esc(&mut self) -> bool {
        if self.filter_agent.take().is_some() {
            self.loader.request(self.range, None, false);
            return false;
        }
        self.close();
        true
    }

    /// Draws the pane into `rect`. Returns true when the user closed it (`esc` chip).
    pub fn draw_pane(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        config: &Config,
        logos: &mut LogoAtlas,
    ) -> bool {
        if !self.open {
            return false;
        }
        if let Some(snap) = self.loader.poll() {
            self.snapshot = Some(snap);
        }
        if self.snapshot.is_none() {
            self.snapshot = self.loader.cached().cloned();
        }
        if self.loader.is_loading() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }

        // A focusable background keeps egui's keyboard focus off the terminal while we have it.
        let id = ui.scope_id().with("stats_pane");
        ui.interact(rect, id, Sense::click().union(Sense::focusable_noninteractive()));
        if let Some(p) = ui.input(|i| i.pointer.any_pressed().then(|| i.pointer.interact_pos()).flatten()) {
            self.focused = rect.contains(p);
        }
        if self.focused {
            ui.memory_mut(|m| m.request_focus(id));
        }
        ui.painter().rect_filled(rect, 0.0, BG);

        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(22.0)));
        inner.set_clip_rect(rect.intersect(ui.clip_rect()));
        let mut close = false;
        egui::ScrollArea::vertical()
            .id_salt("stats_page")
            .auto_shrink([false, false])
            .show(&mut inner, |ui| {
                close = self.draw_contents(ui, config, logos);
            });
        if close {
            self.close();
        }
        close
    }

    /// Page content. Returns true when the `esc` chip was clicked.
    fn draw_contents(&mut self, ui: &mut Ui, config: &Config, logos: &mut LogoAtlas) -> bool {
        let snap = self.snapshot.as_ref();
        let mut close = false;

        let outer_w = ui.available_width();
        let content_w = outer_w.min(CONTENT_MAX_W);
        let tier = layout_tier(content_w);
        let stack_header = content_w < BREAK_MEDIUM;

        // Header
        let mut draw_header_controls = |ui: &mut Ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (esc_rect, esc_resp) =
                    ui.allocate_exact_size(Vec2::new(29.0, 17.0), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(esc_rect, CornerRadius::same(4), ESC_BG);
                painter.rect_stroke(
                    esc_rect,
                    CornerRadius::same(4),
                    Stroke::new(1.0, ESC_BORDER),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    esc_rect.center(),
                    Align2::CENTER_CENTER,
                    "esc",
                    FontId::proportional(11.0),
                    MUTED,
                );
                close = esc_resp.clicked();
                if esc_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                ui.add_space(18.0);
                Frame::NONE
                    .fill(SEG_BG)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.set_min_height(22.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            for r in StatsRange::all().into_iter().rev() {
                                let active = self.range == r;
                                let resp = Frame::NONE
                                    .fill(if active {
                                        SEG_ACTIVE
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(Margin::symmetric(8, 2))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(r.label())
                                                .size(12.0)
                                                .color(if active { TEXT } else { MUTED }),
                                        );
                                    })
                                    .response
                                    .interact(Sense::click());
                                if resp.clicked() && self.range != r {
                                    self.range = r;
                                    self.loader.request(r, self.filter_agent.clone(), false);
                                }
                            }
                        });
                    });
                ui.add_space(6.0);
                if ui
                    .add(
                        egui::Button::new(RichText::new("↻").size(14.0).color(MUTED))
                            .frame(false),
                    )
                    .on_hover_text("Refresh")
                    .clicked()
                {
                    self.loader.request(self.range, self.filter_agent.clone(), true);
                }
                if self.filter_agent.is_some() {
                    ui.add_space(8.0);
                    let clear = egui::Button::new(RichText::new("× clear filter").size(11.0).color(MUTED))
                        .frame(false);
                    if ui.add(clear).clicked() {
                        self.filter_agent = None;
                        self.loader.request(self.range, None, false);
                    }
                }
            });
        };
        if stack_header {
            ui.vertical(|ui| {
                ui.label(RichText::new("Agent usage").size(14.0).color(TEXT));
                let sub = snap.map(|s| s.range_label.as_str()).unwrap_or("…");
                ui.label(RichText::new(sub).size(12.0).color(MUTED));
                ui.add_space(6.0);
                draw_header_controls(ui);
            });
        } else {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Agent usage").size(14.0).color(TEXT));
                    let sub = snap.map(|s| s.range_label.as_str()).unwrap_or("…");
                    ui.label(RichText::new(sub).size(12.0).color(MUTED));
                });
                draw_header_controls(ui);
            });
        }

        ui.add_space(12.0);

        let mut draw_page = |ui: &mut Ui| {
            draw_kpi_row(ui, snap, self.range, content_w, tier);
            ui.add_space(14.0);

            // No data yet: the same boxes, empty (a skeleton, never a blank page).
            let skeleton = StatsSnapshot { range: self.range, ..StatsSnapshot::default() };
            let snap = snap.unwrap_or(&skeleton);

            let side_by_side = tier == LayoutTier::Wide;
            // Middle: tokens by day + heatmap
            boxes(
                ui,
                side_by_side,
                200.0,
                content_w,
                |ui, w| charts::draw_tokens_by_day(ui, snap, w),
                |ui, w| charts::draw_heatmap(ui, snap, w),
            );

            ui.add_space(14.0);

            // Bottom: models + agents. A click on an agent row refilters the whole page.
            let filter_before = self.filter_agent.clone();
            let filter = &mut self.filter_agent;
            boxes(
                ui,
                side_by_side,
                180.0,
                content_w,
                |ui, w| tables::draw_models(ui, snap, w),
                |ui, w| tables::draw_agents(ui, snap, filter, config, logos, w),
            );
            if self.filter_agent != filter_before {
                self.loader.request(self.range, self.filter_agent.clone(), false);
            }

            ui.add_space(8.0);
            for w in &snap.warnings {
                ui.label(RichText::new(w).size(11.0).color(MUTED));
            }
            let ago = snap.computed_at.elapsed().map_or(0, |d| d.as_secs());
            ui.label(
                RichText::new("Everything is read locally. Ankar sends nothing anywhere.")
                    .size(11.0)
                    .color(MUTED),
            )
            .on_hover_text(format!("Updated {ago}s ago"));
            ui.add_space(24.0);
        };

        if outer_w > content_w + 1.0 {
            let side = (outer_w - content_w) * 0.5;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(content_w);
                    draw_page(ui);
                });
            });
        } else {
            draw_page(ui);
        }
        close
    }
}

/// Content caps at this width and centers in a wider pane.
const CONTENT_MAX_W: f32 = 1100.0;
/// Four KPI columns and paired chart/table columns at or above this width.
const BREAK_WIDE: f32 = 900.0;
/// Two KPI columns; header title and range selector stack below this width.
const BREAK_MEDIUM: f32 = 600.0;
/// Minimum stats pane width when dragging the split (see `app.rs`).
pub(crate) const STATS_PANE_MIN_W: f32 = 420.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum LayoutTier {
    Wide,
    Medium,
    Narrow,
}

fn layout_tier(content_w: f32) -> LayoutTier {
    if content_w >= BREAK_WIDE {
        LayoutTier::Wide
    } else if content_w >= BREAK_MEDIUM {
        LayoutTier::Medium
    } else {
        LayoutTier::Narrow
    }
}
const GAP: f32 = 12.0;
const KPI_GAP: f32 = 11.0;
const KPI_H: f32 = 105.0;
/// Inner margin (2 x 12) plus the 1 px stroke on both sides of a box.
const BOX_PAD: f32 = 26.0;

fn draw_kpi_row(
    ui: &mut Ui,
    snap: Option<&StatsSnapshot>,
    range: StatsRange,
    avail: f32,
    tier: LayoutTier,
) {
    let cols = match tier {
        LayoutTier::Wide => 4,
        LayoutTier::Medium => 2,
        LayoutTier::Narrow => 1,
    };
    let card_w = ((avail - KPI_GAP * (cols - 1) as f32) / cols as f32).max(80.0);
    for row in 0..4 / cols {
        if row > 0 {
            ui.add_space(KPI_GAP);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for i in row * cols..(row + 1) * cols {
                if i % cols > 0 {
                    ui.add_space(KPI_GAP);
                }
                draw_metric(ui, snap, range, i, card_w);
            }
        });
    }
}

enum SparkKind {
    Input,
    Output,
    Tasks,
    Hours,
}

/// Card `i` of the top row.
fn draw_metric(ui: &mut Ui, snap: Option<&StatsSnapshot>, range: StatsRange, i: usize, w: f32) {
    let kind = match i {
        0 => SparkKind::Input,
        1 => SparkKind::Output,
        2 => SparkKind::Tasks,
        _ => SparkKind::Hours,
    };
    let spark = spark_series(snap, range, kind);
    let (title, value, delta, sub, tip) = match (i, snap) {
        (0, Some(snap)) => (
            "INPUT TOKENS",
            format_tokens(snap.tokens.input),
            snap.delta_input_pct,
            cache_subtext(snap),
            format!(
                "reasoning {} · cache read {} · write {}",
                format_tokens(snap.tokens.reasoning),
                format_tokens(snap.tokens.cache_read),
                format_tokens(snap.tokens.cache_write)
            ),
        ),
        (1, Some(snap)) => (
            "OUTPUT TOKENS",
            format_tokens(snap.tokens.output),
            snap.delta_output_pct,
            format!(
                "{} per turn",
                format_tokens(snap.tokens.output.checked_div(snap.steps).unwrap_or(0))
            ),
            "Assistant output tokens (OpenCode)".into(),
        ),
        (2, Some(snap)) => (
            "TASKS",
            format_with_commas(snap.sessions.saturating_add(snap.ankar_starts)),
            snap.delta_tasks_pct,
            format!("{} turns", format_with_commas(snap.steps)),
            format!(
                "OpenCode+cursor sessions {} · Ankar starts {}",
                snap.sessions, snap.ankar_starts
            ),
        ),
        (_, Some(snap)) => (
            "AGENT TIME",
            {
                let secs = super::summed_busy_secs(&snap.agents);
                if secs < 3600 {
                    format_hours_short(secs)
                } else if secs >= 10 * 3600 {
                    format!("{:.0} h", secs as f64 / 3600.0)
                } else {
                    format!("{:.1} h", secs as f64 / 3600.0)
                }
            },
            snap.delta_hours_pct,
            // Concurrency is not tracked, so say only what is known.
            match snap.agents.len() {
                1 => "1 agent".to_string(),
                n => format!("{n} agents"),
            },
            format!(
                "Ankar busy {} · OpenCode {:.1} h · cursor-agent {:.1} h",
                format_hours_short(snap.ankar_busy_secs),
                snap.opencode_hours,
                snap.cursor_hours
            ),
        ),
        _ => (
            match i {
                0 => "INPUT TOKENS",
                1 => "OUTPUT TOKENS",
                2 => "TASKS",
                _ => "AGENT TIME",
            },
            "—".into(),
            None,
            "…".into(),
            String::new(),
        ),
    };
    metric_card(ui, w, &value, delta, &sub, &spark, title, &tip);
}

/// Two bordered boxes: side by side (58/42) or stacked when not `side_by_side`.
fn boxes(
    ui: &mut Ui,
    side_by_side: bool,
    min_h: f32,
    content_w: f32,
    left: impl FnOnce(&mut Ui, f32),
    right: impl FnOnce(&mut Ui, f32),
) {
    if side_by_side {
        let left_outer = (content_w - GAP) * 0.58;
        let right_outer = content_w - GAP - left_outer;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            boxed(ui, left_outer, min_h, |ui, inner| left(ui, inner));
            ui.add_space(GAP);
            boxed(ui, right_outer, min_h, |ui, inner| right(ui, inner));
        });
        return;
    }
    boxed(ui, content_w, min_h, |ui, inner| left(ui, inner));
    ui.add_space(GAP);
    boxed(ui, content_w, min_h, |ui, inner| right(ui, inner));
}

/// A bordered box `outer_w` wide in total; `add` receives the inner content width.
fn boxed(ui: &mut Ui, outer_w: f32, min_h: f32, add: impl FnOnce(&mut Ui, f32)) {
    let inner_w = (outer_w - BOX_PAD).max(0.0);
    Frame::NONE
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(inner_w, min_h));
            ui.set_max_width(inner_w);
            add(ui, inner_w);
        });
}

#[allow(clippy::too_many_arguments)]
fn metric_card(
    ui: &mut Ui,
    width: f32,
    value: &str,
    delta: Option<f64>,
    sub: &str,
    spark: &[f32],
    title: &str,
    tip: &str,
) {
    let (card, response) = ui.allocate_exact_size(Vec2::new(width, KPI_H), Sense::hover());
    let mut title_right = card.right() - 12.0;
    let badge = format_pct_delta(delta).zip(delta_badge_colors(delta));
    let badge_rect = badge.as_ref().map(|(label, _)| {
        let pad_x = 6.0;
        let bw = label.chars().count() as f32 * 6.5 + pad_x * 2.0;
        title_right -= bw + 4.0;
        let bh = 15.0 + 4.0;
        Rect::from_min_max(
            egui::pos2(card.right() - 12.0 - bw, card.top() + 12.0),
            egui::pos2(card.right() - 12.0, card.top() + 12.0 + bh),
        )
    });
    {
        let painter = ui.painter();
        painter.rect_filled(card, CornerRadius::same(8), CARD_BG);
        painter.rect_stroke(
            card,
            CornerRadius::same(8),
            Stroke::new(1.0, CARD_BORDER),
            egui::StrokeKind::Inside,
        );
        if let (Some((label, (bg, fg))), Some(badge_rect)) = (&badge, badge_rect) {
            painter.rect_filled(badge_rect, CornerRadius::same(4), *bg);
            painter.text(
                badge_rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(11.0),
                *fg,
            );
        }
    }
    let title_rect = Rect::from_min_max(
        egui::pos2(card.left() + 12.0, card.top() + 12.0),
        egui::pos2(title_right, card.top() + 28.0),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(title_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.add(
                egui::Label::new(RichText::new(title).size(11.0).color(MUTED)).truncate(),
            );
        },
    );
    let painter = ui.painter();
    painter.text(
        egui::pos2(card.left() + 12.0, card.top() + 32.0),
        Align2::LEFT_TOP,
        value,
        FontId::proportional(26.0),
        TEXT,
    );
    painter.text(
        egui::pos2(card.left() + 12.0, card.top() + 60.0),
        Align2::LEFT_TOP,
        sub,
        FontId::proportional(11.0),
        MUTED,
    );
    let spark_rect = Rect::from_min_max(
        egui::pos2(card.left() + 12.0, card.top() + 73.0),
        egui::pos2(card.right() - 12.0, card.top() + 94.0),
    );
    paint_sparkline(ui, spark_rect, spark);
    if !tip.is_empty() {
        response.on_hover_text(tip);
    }
}

fn paint_sparkline(ui: &Ui, rect: Rect, values: &[f32]) {
    if values.is_empty() {
        return;
    }
    let max = values.iter().cloned().fold(0.0_f32, f32::max).max(1.0);
    let n = values.len();
    let gap = 2.0;
    let bar_w = ((rect.width() - gap * (n as f32 - 1.0)) / n as f32).max(1.0);
    let painter = ui.painter();
    for (i, v) in values.iter().enumerate() {
        let h = if *v > 0.0 {
            (v / max) * rect.height()
        } else {
            0.0
        };
        let x = rect.left() + i as f32 * (bar_w + gap);
        let r = Rect::from_min_max(
            egui::pos2(x, rect.bottom() - h),
            egui::pos2(x + bar_w, rect.bottom()),
        );
        painter.rect_filled(r, CornerRadius::same(1), SPARK_BAR);
    }
}

fn spark_slot_count(range: StatsRange) -> usize {
    range.days().map(|d| d as usize).unwrap_or(30).min(30)
}

pub(super) fn ordered_range_days(n: usize) -> Vec<String> {
    let now = jiff::ZonedDateTime::now();
    let mut out = Vec::with_capacity(n);
    for i in (0..n).rev() {
        let days_back = i as i64;
        if let Ok(d) = now.checked_sub(jiff::Span::new().days(days_back)) {
            let dt = d.datetime();
            out.push(format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day()));
        } else {
            out.push(String::new());
        }
    }
    out
}

fn spark_series(snap: Option<&StatsSnapshot>, range: StatsRange, kind: SparkKind) -> Vec<f32> {
    let n = spark_slot_count(range);
    let ordered = ordered_range_days(n);
    let pairs: Vec<(String, f32)> = snap
        .map(|s| {
            s.days
                .iter()
                .map(|d| {
                    let v = match kind {
                        SparkKind::Input => d.input as f32,
                        SparkKind::Output => d.output as f32,
                        SparkKind::Tasks => d.tasks as f32,
                        SparkKind::Hours => d.busy_secs as f32,
                    };
                    (d.day.clone(), v)
                })
                .collect()
        })
        .unwrap_or_default();
    bucket_daily_spark(&ordered, &pairs)
}

/// Badge colours from a percent delta (`None` when there is no prior window).
fn delta_badge_colors(pct: Option<f64>) -> Option<(Color32, Color32)> {
    let p = pct?;
    if !p.is_finite() {
        return None;
    }
    if p >= 0.0 {
        Some((BADGE_POS_BG, BADGE_POS_FG))
    } else {
        Some((BADGE_NEG_BG, BADGE_NEG_FG))
    }
}

/// One spark value per calendar day in `ordered_days`; missing days → `0.0`.
fn bucket_daily_spark(ordered_days: &[String], values: &[(String, f32)]) -> Vec<f32> {
    use std::collections::HashMap;
    let map: HashMap<&str, f32> = values.iter().map(|(d, v)| (d.as_str(), *v)).collect();
    ordered_days
        .iter()
        .map(|d| map.get(d.as_str()).copied().unwrap_or(0.0))
        .collect()
}

fn cache_subtext(snap: &StatsSnapshot) -> String {
    let total = snap.tokens.input.saturating_add(snap.tokens.cache_read);
    if total == 0 {
        return "no cache data".into();
    }
    let pct = (snap.tokens.cache_read as f64 / total as f64) * 100.0;
    format!("{:.0}% from cache", pct)
}

pub(super) fn short_day(ymd: &str) -> String {
    // YYYY-MM-DD → "Oct 23"
    let Ok(ts) = jiff::civil::Date::strptime("%Y-%m-%d", ymd) else {
        return ymd.to_string();
    };
    let dt = ts
        .at(12, 0, 0, 0)
        .to_zoned_datetime(jiff::tz::TimeZone::system())
        .ok();
    if let Some(z) = dt {
        return z.strftime("%b %d").to_string();
    }
    ymd.to_string()
}

pub(super) fn today_str() -> String {
    jiff::ZonedDateTime::now()
        .strftime("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_badge_colors_signed() {
        assert_eq!(
            delta_badge_colors(Some(12.0)),
            Some((BADGE_POS_BG, BADGE_POS_FG))
        );
        assert_eq!(
            delta_badge_colors(Some(-3.0)),
            Some((BADGE_NEG_BG, BADGE_NEG_FG))
        );
        assert_eq!(delta_badge_colors(None), None);
    }

    #[test]
    fn bucket_daily_spark_fills_gaps() {
        let days = vec![
            "2025-01-01".into(),
            "2025-01-02".into(),
            "2025-01-03".into(),
        ];
        let vals = vec![("2025-01-01".into(), 10.0), ("2025-01-03".into(), 7.0)];
        assert_eq!(bucket_daily_spark(&days, &vals), vec![10.0, 0.0, 7.0]);
    }
}
