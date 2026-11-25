//! Agent usage panel — layout matched to midudev's stats mockup (egui Painter only).

use super::format::{format_hours_short, format_pct_delta, format_tokens, format_with_commas};
use super::loader::{StatsLoader, StatsRange};
use super::{Source, StatsSnapshot};
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Key, Margin, Rect, RichText, Sense, Stroke,
    Ui, Vec2,
};
use std::sync::{Arc, Mutex};

const BG: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
const CARD_BORDER: Color32 = Color32::from_rgb(0x22, 0x22, 0x22);
const MUTED: Color32 = Color32::from_rgb(0x88, 0x88, 0x88);
const TEXT: Color32 = Color32::from_rgb(0xE8, 0xE8, 0xE8);
const CHIP_ACTIVE: Color32 = Color32::from_rgb(0x2A, 0x2A, 0x2A);
const BADGE_OK: Color32 = Color32::from_rgb(0x3D, 0xA8, 0x6A);
const BAR_IN: Color32 = Color32::from_rgb(0x6A, 0x6A, 0x6A);
const BAR_OUT: Color32 = Color32::from_rgb(0x3A, 0x7C, 0xA5);
const PROVIDER_ANTHROPIC: Color32 = Color32::from_rgb(0xD4, 0x7B, 0x5A);
const PROVIDER_GOOGLE: Color32 = Color32::from_rgb(0x5A, 0x9B, 0xE0);
const PROVIDER_OPENAI: Color32 = Color32::from_rgb(0x6F, 0xA8, 0x7A);
const PROVIDER_OTHER: Color32 = Color32::from_rgb(0x88, 0x88, 0x88);

pub struct StatsPanel {
    pub open: bool,
    pub range: StatsRange,
    loader: StatsLoader,
    snapshot: Option<StatsSnapshot>,
    filter_agent: Option<String>,
}

impl StatsPanel {
    pub fn new(ankar: Arc<Mutex<super::AnkarStatsStore>>) -> Self {
        Self {
            open: false,
            range: StatsRange::Days30,
            loader: StatsLoader::new(ankar),
            snapshot: None,
            filter_agent: None,
        }
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open = true;
            self.loader.request(self.range, false);
        }
    }

    /// Close and free the snapshot and loader cache.
    pub fn close(&mut self) {
        self.open = false;
        self.snapshot = None;
        self.loader.clear_cache();
        self.filter_agent = None;
    }

    pub fn draw(&mut self, ui: &mut Ui) {
        if !self.open {
            return;
        }

        if let Some(snap) = self.loader.poll() {
            self.snapshot = Some(snap);
        }
        if self.loader.is_loading() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }

        if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.close();
            return;
        }

        let screen = ui.ctx().viewport_rect();
        let size = Vec2::new(
            (screen.width() * 0.92).clamp(720.0, 1100.0),
            (screen.height() * 0.90).clamp(480.0, 780.0),
        );
        let pos = egui::pos2(
            screen.center().x - size.x * 0.5,
            screen.center().y - size.y * 0.5,
        );

        // Dim backdrop.
        ui.painter()
            .rect_filled(screen, 0.0, Color32::from_black_alpha(160));

        egui::Area::new(egui::Id::unique("ankar_stats_panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ui.ctx(), |ui| {
                Frame::NONE
                    .fill(BG)
                    .stroke(Stroke::new(1.0, CARD_BORDER))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_min_size(size);
                        ui.set_max_size(size);
                        self.draw_contents(ui);
                    });
            });
    }

    fn draw_contents(&mut self, ui: &mut Ui) {
        let snap = self
            .snapshot
            .clone()
            .or_else(|| self.loader.cached().cloned());

        // Header
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Agent usage")
                        .size(20.0)
                        .color(TEXT)
                        .strong(),
                );
                let sub = snap
                    .as_ref()
                    .map(|s| s.range_label.clone())
                    .unwrap_or_else(|| "…".into());
                ui.label(RichText::new(sub).size(12.0).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("esc").size(11.0).color(MUTED));
                ui.add_space(8.0);
                for r in StatsRange::all().into_iter().rev() {
                    let active = self.range == r;
                    let fill = if active {
                        CHIP_ACTIVE
                    } else {
                        Color32::TRANSPARENT
                    };
                    let resp = Frame::NONE
                        .fill(fill)
                        .stroke(Stroke::new(1.0, CARD_BORDER))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(10, 4))
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
                        self.loader.request(r, false);
                    }
                    ui.add_space(4.0);
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("↻").size(14.0).color(MUTED))
                            .frame(false),
                    )
                    .on_hover_text("Refresh")
                    .clicked()
                {
                    self.loader.request(self.range, true);
                }
            });
        });

        ui.add_space(12.0);

        if self.loader.is_loading() && snap.is_none() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Loading local stats…").color(MUTED));
            });
            return;
        }

        let Some(snap) = snap else {
            ui.label(RichText::new("No data yet.").color(MUTED));
            return;
        };

        // Metric cards
        let card_w = ((ui.available_width() - 36.0) / 4.0).max(140.0);
        ui.horizontal(|ui| {
            metric_card(
                ui,
                card_w,
                &format_tokens(snap.tokens.input),
                snap.delta_input_pct,
                &cache_subtext(&snap),
                &spark_from_days(&snap, true),
                "Input tokens",
                &format!(
                    "reasoning {} · cache read {} · write {}",
                    format_tokens(snap.tokens.reasoning),
                    format_tokens(snap.tokens.cache_read),
                    format_tokens(snap.tokens.cache_write)
                ),
            );
            ui.add_space(12.0);
            let per_turn = snap.tokens.output.checked_div(snap.steps).unwrap_or(0);
            metric_card(
                ui,
                card_w,
                &format_tokens(snap.tokens.output),
                snap.delta_output_pct,
                &format!("{} per turn", format_tokens(per_turn)),
                &spark_from_days(&snap, false),
                "Output tokens",
                "Assistant output tokens (OpenCode)",
            );
            ui.add_space(12.0);
            let tasks = snap.sessions.saturating_add(snap.ankar_starts);
            metric_card(
                ui,
                card_w,
                &format_with_commas(tasks),
                snap.delta_tasks_pct,
                &format!("{} turns", format_with_commas(snap.steps)),
                &spark_tasks(&snap),
                "Tasks",
                &format!(
                    "OpenCode+cursor sessions {} · Ankar starts {}",
                    snap.sessions, snap.ankar_starts
                ),
            );
            ui.add_space(12.0);
            let hours_label = if snap.agent_hours >= 10.0 {
                format!("{:.0} h", snap.agent_hours)
            } else {
                format!("{:.1} h", snap.agent_hours)
            };
            metric_card(
                ui,
                card_w,
                &hours_label,
                snap.delta_hours_pct,
                "agent time",
                &spark_hours(&snap),
                "Agent time",
                &format!(
                    "Ankar busy {} · OpenCode {:.1} h · cursor-agent {:.1} h",
                    format_hours_short(snap.ankar_busy_secs),
                    snap.opencode_hours,
                    snap.cursor_hours
                ),
            );
        });

        ui.add_space(14.0);

        // Middle: tokens by day + heatmap
        let mid_h = 200.0;
        let left_w = ui.available_width() * 0.58;
        ui.horizontal(|ui| {
            Frame::NONE
                .stroke(Stroke::new(1.0, CARD_BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new(left_w - 8.0, mid_h));
                    ui.set_max_width(left_w - 8.0);
                    draw_tokens_by_day(ui, &snap);
                });
            ui.add_space(12.0);
            Frame::NONE
                .stroke(Stroke::new(1.0, CARD_BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_min_height(mid_h);
                    ui.set_width(ui.available_width());
                    draw_heatmap(ui, &snap);
                });
        });

        ui.add_space(14.0);

        // Bottom: models + agents
        let bot_h = ui.available_height() - 36.0;
        let half = ui.available_width() * 0.5 - 8.0;
        ui.horizontal(|ui| {
            Frame::NONE
                .stroke(Stroke::new(1.0, CARD_BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_min_size(Vec2::new(half, bot_h.max(180.0)));
                    ui.set_max_width(half);
                    draw_models(ui, &snap);
                });
            ui.add_space(12.0);
            Frame::NONE
                .stroke(Stroke::new(1.0, CARD_BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_min_height(bot_h.max(180.0));
                    ui.set_width(ui.available_width());
                    self.draw_agents(ui, &snap);
                });
        });

        ui.add_space(8.0);
        if !snap.warnings.is_empty() {
            for w in &snap.warnings {
                ui.label(RichText::new(w).size(11.0).color(MUTED));
            }
        }
        let ago = snap
            .computed_at
            .elapsed()
            .ok()
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ui.label(
            RichText::new(format!(
                "Updated {ago}s ago · Everything is read locally. Ankar sends nothing anywhere."
            ))
            .size(11.0)
            .color(MUTED),
        );
    }

    fn draw_agents(&mut self, ui: &mut Ui, snap: &StatsSnapshot) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("AGENTS")
                    .size(11.0)
                    .color(MUTED)
                    .strong(),
            );
            ui.label(
                RichText::new("click one to see only its usage")
                    .size(11.0)
                    .color(MUTED),
            );
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(ui.available_width() - 180.0);
            ui.label(RichText::new("input").size(10.0).color(MUTED));
            ui.add_space(28.0);
            ui.label(RichText::new("output").size(10.0).color(MUTED));
            ui.add_space(28.0);
            ui.label(RichText::new("time").size(10.0).color(MUTED));
        });

        let max_tok = snap
            .agents
            .iter()
            .map(|a| a.tokens.as_ref().map(|t| t.total()).unwrap_or(a.busy_secs))
            .max()
            .unwrap_or(1)
            .max(1);

        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 8.0)
            .show(ui, |ui| {
                for a in &snap.agents {
                    if let Some(f) = &self.filter_agent {
                        if &a.name != f {
                            continue;
                        }
                    }
                    let row = ui.horizontal(|ui| {
                        let color = match a.source {
                            Source::OpenCode => PROVIDER_ANTHROPIC,
                            Source::CursorAgent => PROVIDER_OPENAI,
                            Source::Ankar => PROVIDER_GOOGLE,
                        };
                        let (icon_r, _) =
                            ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        ui.painter()
                            .circle_filled(icon_r.center(), 5.0, color);
                        let name = truncate(&a.name, 22);
                        ui.label(RichText::new(name).size(12.0).color(TEXT));
                        let bar_w = ui.available_width() - 200.0;
                        let (bar_r, _) =
                            ui.allocate_exact_size(Vec2::new(bar_w.max(40.0), 8.0), Sense::hover());
                        ui.painter().rect_filled(
                            bar_r,
                            CornerRadius::same(2),
                            Color32::from_rgb(0x18, 0x18, 0x18),
                        );
                        let busy_w = a.busy_secs as f32;
                        let wait_w = a.waiting_secs as f32;
                        let total_t = (busy_w + wait_w).max(1.0);
                        let scale = if a.tokens.as_ref().map(|t| t.total()).unwrap_or(0) > 0 {
                            a.tokens.as_ref().map(|t| t.total()).unwrap_or(0) as f32 / max_tok as f32
                        } else {
                            (a.busy_secs as f32 / max_tok as f32).clamp(0.0, 1.0)
                        }
                        .clamp(0.05, 1.0);
                        let busy_frac = (busy_w / total_t) * scale;
                        let wait_frac = (wait_w / total_t) * scale;
                        let busy_r = Rect::from_min_size(
                            bar_r.min,
                            Vec2::new(bar_r.width() * busy_frac, bar_r.height()),
                        );
                        let wait_r = Rect::from_min_size(
                            egui::pos2(busy_r.right(), bar_r.top()),
                            Vec2::new(bar_r.width() * wait_frac, bar_r.height()),
                        );
                        ui.painter().rect_filled(busy_r, CornerRadius::same(2), color);
                        ui.painter().rect_filled(
                            wait_r,
                            CornerRadius::same(2),
                            Color32::from_rgb(0x3A, 0x3A, 0x3A),
                        );

                        let tin = a
                            .tokens
                            .as_ref()
                            .map(|t| format_tokens(t.input))
                            .unwrap_or_else(|| "—".into());
                        let tout = a
                            .tokens
                            .as_ref()
                            .map(|t| format_tokens(t.output))
                            .unwrap_or_else(|| "—".into());
                        ui.label(RichText::new(tin).size(11.0).color(MUTED));
                        ui.add_space(12.0);
                        ui.label(RichText::new(tout).size(11.0).color(MUTED));
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(format_hours_short(a.busy_secs))
                                .size(11.0)
                                .color(MUTED),
                        );
                    });
                    let resp = row.response.interact(Sense::click());
                    if resp.clicked() {
                        if self.filter_agent.as_deref() == Some(a.name.as_str()) {
                            self.filter_agent = None;
                        } else {
                            self.filter_agent = Some(a.name.clone());
                        }
                    }
                    if a.tokens.is_none() {
                        resp.on_hover_text("cursor-agent does not store tokens locally");
                    }
                }
            });
    }
}

#[allow(clippy::too_many_arguments)]
fn metric_card(
    ui: &mut Ui,
    width: f32,
    value: &str,
    delta: Option<f64>,
    sub: &str,
    spark: &[f32],
    _title: &str,
    tip: &str,
) {
    Frame::NONE
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.set_min_height(108.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(value).size(28.0).color(TEXT).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if let Some(p) = format_pct_delta(delta) {
                        Frame::NONE
                            .fill(Color32::from_rgb(0x1A, 0x3A, 0x28))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(RichText::new(p).size(11.0).color(BADGE_OK));
                            });
                    }
                });
            });
            ui.label(RichText::new(sub).size(12.0).color(MUTED));
            ui.add_space(8.0);
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), 28.0),
                Sense::hover(),
            );
            paint_sparkline(ui, rect, spark);
        })
        .response
        .on_hover_text(tip);
}

fn paint_sparkline(ui: &Ui, rect: Rect, values: &[f32]) {
    if values.is_empty() {
        return;
    }
    let max = values.iter().cloned().fold(0.0_f32, f32::max).max(1.0);
    let n = values.len();
    let gap = 2.0;
    let bar_w = ((rect.width() - gap * (n as f32 - 1.0)) / n as f32).max(1.5);
    let painter = ui.painter();
    for (i, v) in values.iter().enumerate() {
        let h = (v / max) * rect.height();
        let x = rect.left() + i as f32 * (bar_w + gap);
        let r = Rect::from_min_max(
            egui::pos2(x, rect.bottom() - h),
            egui::pos2(x + bar_w, rect.bottom()),
        );
        painter.rect_filled(r, CornerRadius::same(1), Color32::from_rgb(0x3A, 0x3A, 0x3A));
    }
}

fn spark_from_days(snap: &StatsSnapshot, input: bool) -> Vec<f32> {
    snap.days
        .iter()
        .map(|d| if input { d.input as f32 } else { d.output as f32 })
        .collect()
}

fn spark_tasks(snap: &StatsSnapshot) -> Vec<f32> {
    snap.days.iter().map(|d| d.tasks as f32).collect()
}

fn spark_hours(snap: &StatsSnapshot) -> Vec<f32> {
    snap.days.iter().map(|d| d.busy_secs as f32).collect()
}

fn cache_subtext(snap: &StatsSnapshot) -> String {
    let total = snap.tokens.input.saturating_add(snap.tokens.cache_read);
    if total == 0 {
        return "no cache data".into();
    }
    let pct = (snap.tokens.cache_read as f64 / total as f64) * 100.0;
    format!("{:.0}% from cache", pct)
}

fn draw_tokens_by_day(ui: &mut Ui, snap: &StatsSnapshot) {
    let days_with = snap.days.iter().filter(|d| d.input + d.output > 0).count();
    let n_days = match snap.range.days() {
        Some(d) => d as usize,
        None => snap.days.len().max(1),
    };
    let avg = if days_with > 0 {
        (snap.tokens.input + snap.tokens.output) / days_with as u64
    } else {
        0
    };
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("TOKENS BY DAY")
                .size(11.0)
                .color(MUTED)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!(
                    "{days_with} of {n_days} days with agents · {}/day",
                    format_tokens(avg)
                ))
                .size(11.0)
                .color(MUTED),
            );
        });
    });
    ui.add_space(8.0);

    let chart_h = 140.0;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), chart_h),
        Sense::hover(),
    );
    let mid_y = rect.center().y;
    ui.painter()
        .hline(rect.x_range(), mid_y, Stroke::new(1.0, CARD_BORDER));

    let days = &snap.days;
    if days.is_empty() {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No daily data",
            FontId::proportional(12.0),
            MUTED,
        );
        return;
    }
    let max_in = days.iter().map(|d| d.input).max().unwrap_or(1).max(1) as f32;
    let max_out = days.iter().map(|d| d.output).max().unwrap_or(1).max(1) as f32;
    let n = days.len();
    let gap = 2.0;
    let bar_w = ((rect.width() - gap * (n as f32 - 1.0)) / n as f32).clamp(2.0, 18.0);
    let half_h = chart_h * 0.42;

    for (i, d) in days.iter().enumerate() {
        let x = rect.left() + i as f32 * (bar_w + gap);
        let in_h = (d.input as f32 / max_in) * half_h;
        let out_h = (d.output as f32 / max_out) * half_h;
        let up = Rect::from_min_max(
            egui::pos2(x, mid_y - in_h),
            egui::pos2(x + bar_w, mid_y),
        );
        let down = Rect::from_min_max(
            egui::pos2(x, mid_y),
            egui::pos2(x + bar_w, mid_y + out_h),
        );
        ui.painter().rect_filled(up, CornerRadius::same(1), BAR_IN);
        ui.painter()
            .rect_filled(down, CornerRadius::same(1), BAR_OUT);
    }

    // Axis labels: first, mid, last
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if let Some(d) = days.first() {
            ui.label(RichText::new(short_day(&d.day)).size(10.0).color(MUTED));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(d) = days.last() {
                let label = if d.day == today_str() {
                    "today".into()
                } else {
                    short_day(&d.day)
                };
                ui.label(RichText::new(label).size(10.0).color(MUTED));
            }
        });
        if days.len() > 2 {
            let mid = &days[days.len() / 2];
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(short_day(&mid.day)).size(10.0).color(MUTED));
            });
        }
    });

    // Legend values for last day
    if let Some(last) = days.last() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("↑ {} input", format_tokens(last.input)))
                    .size(11.0)
                    .color(MUTED),
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!("↓ {} output", format_tokens(last.output)))
                    .size(11.0)
                    .color(BAR_OUT),
            );
        });
    }
}

fn draw_heatmap(ui: &mut Ui, snap: &StatsSnapshot) {
    let (best_wd, best_h, best_n) = peak_activity(&snap.activity);
    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("WHEN THEY WORK")
                .size(11.0)
                .color(MUTED)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if best_n > 0 {
                ui.label(
                    RichText::new(format!(
                        "most active {} {:02} h",
                        day_names.get(best_wd).unwrap_or(&"?"),
                        best_h
                    ))
                    .size(11.0)
                    .color(MUTED),
                );
            }
        });
    });
    ui.add_space(8.0);

    let labels = ["M", "T", "W", "T", "F", "S", "S"];
    let max = snap
        .activity
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);

    let cell = 12.0;
    let gap = 2.0;
    let label_w = 14.0;
    let grid_w: f32 = label_w + 24.0 * (cell + gap);
    let grid_h: f32 = 7.0 * (cell + gap) + 20.0;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(grid_w.min(ui.available_width()), grid_h),
        Sense::hover(),
    );

    // Hour labels
    for (i, h) in [0usize, 6, 12, 18].iter().enumerate() {
        let x = rect.left() + label_w + (*h as f32) * (cell + gap);
        ui.painter().text(
            egui::pos2(x, rect.top()),
            Align2::LEFT_TOP,
            format!("{h}"),
            FontId::proportional(9.0),
            MUTED,
        );
        let _ = i;
    }

    let top = rect.top() + 14.0;
    for (wd, row) in snap.activity.iter().enumerate() {
        let y = top + wd as f32 * (cell + gap);
        ui.painter().text(
            egui::pos2(rect.left(), y + cell * 0.5),
            Align2::LEFT_CENTER,
            labels[wd],
            FontId::proportional(10.0),
            MUTED,
        );
        for (h, &n) in row.iter().enumerate() {
            let x = rect.left() + label_w + h as f32 * (cell + gap);
            let level = ((n as f32 / max as f32) * 4.0).round() as i32;
            let c = heat_color(level);
            let r = Rect::from_min_size(egui::pos2(x, y), Vec2::splat(cell));
            ui.painter().rect_filled(r, CornerRadius::same(2), c);
            let id = ui.scope_id().with(("heat", wd, h));
            let resp = ui.interact(r, id, Sense::hover());
            if resp.hovered() {
                resp.on_hover_text(format!(
                    "{} {:02}:00 — {n} events",
                    day_names[wd], h
                ));
            }
        }
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new("more").size(10.0).color(MUTED));
            for lvl in (0..=3).rev() {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter()
                    .rect_filled(r, CornerRadius::same(2), heat_color(lvl));
                ui.add_space(2.0);
            }
            ui.label(RichText::new("less").size(10.0).color(MUTED));
        });
    });
}

fn heat_color(level: i32) -> Color32 {
    match level.clamp(0, 4) {
        0 => Color32::from_rgb(0x14, 0x14, 0x14),
        1 => Color32::from_rgb(0x2A, 0x2A, 0x2A),
        2 => Color32::from_rgb(0x4A, 0x4A, 0x4A),
        3 => Color32::from_rgb(0x8A, 0x8A, 0x8A),
        _ => Color32::from_rgb(0xE0, 0xE0, 0xE0),
    }
}

fn peak_activity(a: &[[u32; 24]; 7]) -> (usize, usize, u32) {
    let mut best = (0usize, 0usize, 0u32);
    for (wd, row) in a.iter().enumerate() {
        for (h, &n) in row.iter().enumerate() {
            if n > best.2 {
                best = (wd, h, n);
            }
        }
    }
    best
}

fn draw_models(ui: &mut Ui, snap: &StatsSnapshot) {
    ui.label(
        RichText::new("MODELS")
            .size(11.0)
            .color(MUTED)
            .strong(),
    );
    ui.add_space(6.0);

    // Provider stacked bar
    let mut by_prov: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for m in &snap.models {
        *by_prov.entry(m.provider.clone()).or_default() += m.tokens.total();
    }
    let total: u64 = by_prov.values().sum::<u64>().max(1);
    let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 10.0), Sense::hover());
    let mut x = bar.left();
    let mut parts: Vec<(String, u64)> = by_prov.into_iter().collect();
    parts.sort_by_key(|a| std::cmp::Reverse(a.1));
    for (prov, n) in &parts {
        let w = (*n as f32 / total as f32) * bar.width();
        let r = Rect::from_min_size(egui::pos2(x, bar.top()), Vec2::new(w.max(1.0), bar.height()));
        ui.painter()
            .rect_filled(r, CornerRadius::ZERO, provider_color(prov));
        x += w;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (prov, n) in parts.iter().take(4) {
            let pct = (*n as f64 / total as f64) * 100.0;
            ui.colored_label(provider_color(prov), "●");
            ui.label(
                RichText::new(format!("{prov} {:.0}%", pct))
                    .size(11.0)
                    .color(MUTED),
            );
            ui.add_space(8.0);
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(ui.available_width() - 200.0);
        ui.label(RichText::new("input").size(10.0).color(MUTED));
        ui.add_space(28.0);
        ui.label(RichText::new("output").size(10.0).color(MUTED));
        ui.add_space(28.0);
        ui.label(RichText::new("tasks").size(10.0).color(MUTED));
    });

    let max_tok = snap
        .models
        .iter()
        .map(|m| m.tokens.total())
        .max()
        .unwrap_or(1)
        .max(1);

    egui::ScrollArea::vertical()
        .max_height(ui.available_height() - 8.0)
        .show(ui, |ui| {
            for m in &snap.models {
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        4.0,
                        provider_color(&m.provider),
                    );
                    let label = if m.provider == "other" {
                        m.model.clone()
                    } else {
                        truncate(&m.model, 28)
                    };
                    ui.label(RichText::new(label).size(12.0).color(TEXT));
                    let bar_w = ui.available_width() - 200.0;
                    let (bar_r, _) =
                        ui.allocate_exact_size(Vec2::new(bar_w.max(40.0), 8.0), Sense::hover());
                    ui.painter().rect_filled(
                        bar_r,
                        CornerRadius::same(2),
                        Color32::from_rgb(0x18, 0x18, 0x18),
                    );
                    let frac = m.tokens.total() as f32 / max_tok as f32;
                    let fill = Rect::from_min_size(
                        bar_r.min,
                        Vec2::new(bar_r.width() * frac, bar_r.height()),
                    );
                    ui.painter().rect_filled(
                        fill,
                        CornerRadius::same(2),
                        provider_color(&m.provider),
                    );
                    ui.label(
                        RichText::new(format_tokens(m.tokens.input))
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format_tokens(m.tokens.output))
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format_with_commas(m.steps))
                            .size(11.0)
                            .color(MUTED),
                    );
                });
            }
        });
}

fn provider_color(p: &str) -> Color32 {
    let l = p.to_lowercase();
    if l.contains("anthropic") || l.contains("claude") {
        PROVIDER_ANTHROPIC
    } else if l.contains("google") || l.contains("gemini") {
        PROVIDER_GOOGLE
    } else if l.contains("openai") || l.contains("gpt") {
        PROVIDER_OPENAI
    } else {
        PROVIDER_OTHER
    }
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn short_day(ymd: &str) -> String {
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

fn today_str() -> String {
    jiff::ZonedDateTime::now()
        .strftime("%Y-%m-%d")
        .to_string()
}
