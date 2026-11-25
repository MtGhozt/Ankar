//! Stats page charts: TOKENS BY DAY and WHEN THEY WORK. Painter rects and short texts only.

use super::format::format_tokens;
use super::panel::{ordered_range_days, BAR_IN, BAR_OUT, CARD_BORDER, MUTED};
use super::{DayBucket, StatsSnapshot};
use egui::{pos2, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, Ui, Vec2};

const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const DAY_INITIALS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// Tokens chart: plot height, the label column on the left, the date row below.
const PLOT_H: f32 = 118.0;
const LABEL_W: f32 = 36.0;
const DATES_H: f32 = 16.0;
/// The zero line sits at 60 % of the plot: input goes up, output goes down.
const ZERO_FRAC: f32 = 0.6;

const HEAT_LABEL_W: f32 = 14.0;
const CELL_H: f32 = 12.0;
const CELL_GAP: f32 = 2.0;
const HOURS_H: f32 = 14.0;
const LEGEND_H: f32 = 16.0;

/// One `(day, input, output)` per slot of `ordered`; days without data are zeros.
fn day_slots(ordered: &[String], days: &[DayBucket]) -> Vec<(String, u64, u64)> {
    let map: std::collections::HashMap<&str, &DayBucket> =
        days.iter().map(|d| (d.day.as_str(), d)).collect();
    ordered
        .iter()
        .map(|d| match map.get(d.as_str()) {
            Some(b) => (d.clone(), b.input, b.output),
            None => (d.clone(), 0, 0),
        })
        .collect()
}

/// Slot under `x` when `n` equal slots span `[left, left + width)`.
fn slot_at(x: f32, left: f32, width: f32, n: usize) -> Option<usize> {
    if n == 0 || width <= 0.0 || x < left || x >= left + width {
        return None;
    }
    Some((((x - left) / width * n as f32) as usize).min(n - 1))
}

/// Heatmap step 0..=4: 0 only for no events; otherwise four equal buckets of `max`.
fn heat_level(n: u32, max: u32) -> u8 {
    if n == 0 || max == 0 {
        return 0;
    }
    1 + ((n as u64 * 4 - 1) / max as u64).min(3) as u8
}

fn heat_color(level: u8) -> Color32 {
    match level {
        0 => Color32::from_rgb(0x21, 0x21, 0x21),
        1 => Color32::from_rgb(0x5A, 0x5A, 0x5A),
        2 => Color32::from_rgb(0x8E, 0x8E, 0x8E),
        3 => Color32::from_rgb(0xAB, 0xAB, 0xAB),
        _ => Color32::from_rgb(0xC2, 0xC2, 0xC2),
    }
}

/// Busiest `(weekday Mon=0, hour, events)`; the first one wins a tie.
pub(super) fn peak_activity(a: &[[u32; 24]; 7]) -> (usize, usize, u32) {
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

/// Cell `(weekday, hour)` under `(x, y)` relative to the first cell's top-left corner.
fn heat_cell_at(x: f32, y: f32, cell_w: f32, cell_h: f32, gap: f32) -> Option<(usize, usize)> {
    if x < 0.0 || y < 0.0 {
        return None;
    }
    let (px, py) = (cell_w + gap, cell_h + gap);
    let (h, wd) = ((x / px) as usize, (y / py) as usize);
    let inside = x - h as f32 * px < cell_w && y - wd as f32 * py < cell_h;
    (inside && h < 24 && wd < 7).then_some((wd, h))
}

fn section_title(ui: &mut Ui, title: &str, right: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(11.0).color(MUTED).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(right).size(11.0).color(MUTED));
        });
    });
    ui.add_space(8.0);
}

pub(super) fn draw_tokens_by_day(ui: &mut Ui, snap: &StatsSnapshot) {
    let days_with = snap.days.iter().filter(|d| d.input + d.output > 0).count();
    let ordered: Vec<String> = match snap.range.days() {
        Some(d) => ordered_range_days(d as usize),
        None => snap.days.iter().map(|d| d.day.clone()).collect(),
    };
    let avg = (snap.tokens.input + snap.tokens.output)
        .checked_div(days_with as u64)
        .unwrap_or(0);
    section_title(
        ui,
        "TOKENS BY DAY",
        &format!(
            "{days_with} of {} days with agents · {}/day",
            ordered.len().max(days_with),
            format_tokens(avg)
        ),
    );

    let slots = day_slots(&ordered, &snap.days);
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), PLOT_H + DATES_H),
        Sense::hover(),
    );
    let plot = Rect::from_min_size(
        pos2(rect.left() + LABEL_W, rect.top()),
        Vec2::new((rect.width() - LABEL_W).max(1.0), PLOT_H),
    );
    let zero_y = plot.top() + PLOT_H * ZERO_FRAC;
    let max_in = slots.iter().map(|s| s.1).max().unwrap_or(0);
    let max_out = slots.iter().map(|s| s.2).max().unwrap_or(0);
    let n = slots.len();
    let hovered = resp
        .hover_pos()
        .and_then(|p| slot_at(p.x, plot.left(), plot.width(), n));

    let painter = ui.painter();
    painter.hline(plot.x_range(), zero_y, Stroke::new(1.0, CARD_BORDER));
    let small = FontId::proportional(10.0);
    painter.text(rect.left_top(), Align2::LEFT_TOP, format_tokens(max_in), small.clone(), MUTED);
    painter.text(
        rect.left_top() + Vec2::new(0.0, 12.0),
        Align2::LEFT_TOP,
        "input",
        FontId::proportional(9.0),
        MUTED,
    );
    let bottom = plot.bottom();
    painter.text(
        pos2(rect.left(), bottom - 11.0),
        Align2::LEFT_BOTTOM,
        format_tokens(max_out),
        small.clone(),
        MUTED,
    );
    painter.text(
        pos2(rect.left(), bottom),
        Align2::LEFT_BOTTOM,
        "output",
        FontId::proportional(9.0),
        BAR_OUT,
    );

    let pitch = plot.width() / n.max(1) as f32;
    let bar_w = (pitch - if pitch > 4.0 { 2.0 } else { 1.0 }).max(1.0);
    let (up_h, down_h) = (zero_y - plot.top() - 2.0, bottom - zero_y - 2.0);
    for (i, (_, input, output)) in slots.iter().enumerate() {
        let x = plot.left() + i as f32 * pitch;
        if hovered == Some(i) {
            let col = Rect::from_min_max(pos2(x, plot.top()), pos2(x + pitch, bottom));
            painter.rect_filled(col, CornerRadius::ZERO, Color32::from_white_alpha(8));
        }
        let up = if max_in > 0 { *input as f32 / max_in as f32 * up_h } else { 0.0 };
        let down = if max_out > 0 { *output as f32 / max_out as f32 * down_h } else { 0.0 };
        let (up, down) = (if *input > 0 { up.max(1.0) } else { 0.0 }, if *output > 0 { down.max(1.0) } else { 0.0 });
        painter.rect_filled(
            Rect::from_min_max(pos2(x, zero_y - up), pos2(x + bar_w, zero_y)),
            CornerRadius::ZERO,
            BAR_IN,
        );
        painter.rect_filled(
            Rect::from_min_max(pos2(x, zero_y), pos2(x + bar_w, zero_y + down)),
            CornerRadius::ZERO,
            BAR_OUT,
        );
    }

    // Dates: first, middle, today.
    let y = bottom + 4.0;
    let date_font = FontId::proportional(10.0);
    if let Some(first) = slots.first() {
        painter.text(pos2(plot.left(), y), Align2::LEFT_TOP, super::panel::short_day(&first.0), date_font.clone(), MUTED);
    }
    if n > 2 {
        painter.text(pos2(plot.center().x, y), Align2::CENTER_TOP, super::panel::short_day(&slots[n / 2].0), date_font.clone(), MUTED);
    }
    if let Some(last) = slots.last().filter(|_| n > 1) {
        let label = if last.0 == super::panel::today_str() { "today".into() } else { super::panel::short_day(&last.0) };
        painter.text(pos2(plot.right(), y), Align2::RIGHT_TOP, label, date_font, MUTED);
    }

    if let Some((day, input, output)) = hovered.and_then(|i| slots.get(i)) {
        resp.on_hover_text_at_pointer(format!(
            "{}\ninput {}\noutput {}",
            super::panel::short_day(day),
            format_tokens(*input),
            format_tokens(*output)
        ));
    }
}

pub(super) fn draw_heatmap(ui: &mut Ui, snap: &StatsSnapshot) {
    let (best_wd, best_h, best_n) = peak_activity(&snap.activity);
    let right = if best_n > 0 {
        format!("most active {} {best_h:02} h", DAY_NAMES[best_wd])
    } else {
        String::new()
    };
    section_title(ui, "WHEN THEY WORK", &right);

    let avail = ui.available_width();
    let cell_w = ((avail - HEAT_LABEL_W - 23.0 * CELL_GAP) / 24.0).clamp(3.0, 15.5);
    let grid_h = 7.0 * (CELL_H + CELL_GAP) - CELL_GAP;
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(avail, grid_h + HOURS_H + LEGEND_H),
        Sense::hover(),
    );
    let origin = pos2(rect.left() + HEAT_LABEL_W, rect.top());
    let max = snap.activity.iter().flat_map(|r| r.iter()).copied().max().unwrap_or(0);
    let hovered = resp
        .hover_pos()
        .and_then(|p| heat_cell_at(p.x - origin.x, p.y - origin.y, cell_w, CELL_H, CELL_GAP));

    let painter = ui.painter();
    for (wd, row) in snap.activity.iter().enumerate() {
        let y = origin.y + wd as f32 * (CELL_H + CELL_GAP);
        painter.text(
            pos2(rect.left(), y + CELL_H * 0.5),
            Align2::LEFT_CENTER,
            DAY_INITIALS[wd],
            FontId::proportional(10.0),
            MUTED,
        );
        for (h, &n) in row.iter().enumerate() {
            let x = origin.x + h as f32 * (cell_w + CELL_GAP);
            let r = Rect::from_min_size(pos2(x, y), Vec2::new(cell_w, CELL_H));
            painter.rect_filled(r, CornerRadius::same(2), heat_color(heat_level(n, max)));
            if hovered == Some((wd, h)) {
                painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.0, MUTED), egui::StrokeKind::Inside);
            }
        }
    }
    for h in [0usize, 6, 12, 18] {
        painter.text(
            pos2(origin.x + h as f32 * (cell_w + CELL_GAP), origin.y + grid_h + 2.0),
            Align2::LEFT_TOP,
            h.to_string(),
            FontId::proportional(9.0),
            MUTED,
        );
    }

    // Legend: less ■■■■■ more
    let ly = rect.bottom() - LEGEND_H * 0.5;
    let font = FontId::proportional(10.0);
    let mut x = painter.text(pos2(rect.right(), ly), Align2::RIGHT_CENTER, "more", font.clone(), MUTED).left() - 4.0;
    for level in (0..5u8).rev() {
        let sq = Rect::from_center_size(pos2(x - 5.0, ly), Vec2::splat(10.0));
        painter.rect_filled(sq, CornerRadius::same(2), heat_color(level));
        x -= 12.0;
    }
    painter.text(pos2(x - 2.0, ly), Align2::RIGHT_CENTER, "less", font, MUTED);

    if let Some((wd, h)) = hovered {
        resp.on_hover_text_at_pointer(format!("{} {h:02}:00 · {} events", DAY_NAMES[wd], snap.activity[wd][h]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_level_buckets() {
        assert_eq!(heat_level(0, 100), 0);
        assert_eq!(heat_level(5, 0), 0);
        assert_eq!(heat_level(1, 100), 1);
        assert_eq!(heat_level(25, 100), 1);
        assert_eq!(heat_level(26, 100), 2);
        assert_eq!(heat_level(51, 100), 3);
        assert_eq!(heat_level(76, 100), 4);
        assert_eq!(heat_level(100, 100), 4);
    }

    #[test]
    fn peak_activity_finds_the_busiest_cell_first_wins_ties() {
        let mut a = [[0u32; 24]; 7];
        assert_eq!(peak_activity(&a).2, 0);
        a[5][18] = 9;
        a[1][3] = 9;
        a[2][2] = 4;
        assert_eq!(peak_activity(&a), (1, 3, 9));
    }

    #[test]
    fn slots_and_cells_map_pointers() {
        assert_eq!(slot_at(0.0, 0.0, 100.0, 10), Some(0));
        assert_eq!(slot_at(99.9, 0.0, 100.0, 10), Some(9));
        assert_eq!(slot_at(100.0, 0.0, 100.0, 10), None);
        assert_eq!(heat_cell_at(0.0, 0.0, 10.0, 12.0, 2.0), Some((0, 0)));
        assert_eq!(heat_cell_at(11.0, 0.0, 10.0, 12.0, 2.0), None);
        assert_eq!(heat_cell_at(12.0 * 18.0 + 1.0, 14.0 * 5.0 + 1.0, 10.0, 12.0, 2.0), Some((5, 18)));
    }

    #[test]
    fn day_slots_fill_missing_days_with_zeros() {
        let days = vec![DayBucket { day: "2025-01-02".into(), input: 7, output: 3, ..DayBucket::default() }];
        let ordered = vec!["2025-01-01".to_string(), "2025-01-02".to_string()];
        assert_eq!(
            day_slots(&ordered, &days),
            vec![("2025-01-01".into(), 0, 0), ("2025-01-02".into(), 7, 3)]
        );
    }
}
