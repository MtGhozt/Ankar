use super::backend::{CellMetrics, TerminalSession, VisibleCell};
use super::bindings::{map_key, KeyAction};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use egui::epaint::RectShape;
use egui::text::{LayoutJob, TextFormat};
use egui::{
    Color32, CornerRadius, FontId, Frame, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2,
};
use std::collections::BTreeMap;

const FONT_SIZE: f32 = 14.0;
const FONT_FAMILY: egui::FontFamily = egui::FontFamily::Monospace;

pub struct TerminalView;

impl TerminalView {
    pub fn show(ui: &mut Ui, session: &mut TerminalSession, focused: bool) -> egui::Response {
        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());

        let font_id = FontId::new(FONT_SIZE, FONT_FAMILY);
        let metrics = measure_cell(ui, &font_id);
        session.resize(response.rect.width(), response.rect.height(), metrics);

        if focused || response.clicked() || response.dragged() {
            response.request_focus();
        }

        if focused {
            handle_input(ui, &response, session);
        }
        paint_grid(ui, &response, &painter, session, &font_id, metrics);
        response
    }
}

fn measure_cell(ui: &Ui, font_id: &FontId) -> CellMetrics {
    let galley = ui.fonts_mut(|f| f.layout_no_wrap("W".to_owned(), font_id.clone(), Color32::WHITE));
    CellMetrics {
        width: galley.size().x.max(1.0),
        height: galley.size().y.max(1.0),
    }
}

fn handle_input(ui: &Ui, response: &egui::Response, session: &mut TerminalSession) {
    if !response.has_focus() {
        return;
    }

    let modifiers = ui.input(|i| i.modifiers);
    let mode = session.snapshot().mode;
    let events: Vec<egui::Event> = ui.input(|i| i.events.clone());

    for event in events {
        match event {
            egui::Event::Text(text) => {
                // Skip when Ctrl is held — handled via Key events.
                if modifiers.ctrl || modifiers.command {
                    continue;
                }
                session.write(text.into_bytes());
            }
            egui::Event::Paste(text) => {
                if mode.contains(TermMode::BRACKETED_PASTE) {
                    let mut buf = b"\x1b[200~".to_vec();
                    buf.extend(text.as_bytes());
                    buf.extend_from_slice(b"\x1b[201~");
                    session.write(buf);
                } else {
                    session.write(text.into_bytes());
                }
            }
            egui::Event::Copy => {
                let text = session.selected_text();
                if !text.is_empty() {
                    ui.ctx().copy_text(text);
                }
            }
            egui::Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } => match map_key(key, modifiers, mode, pressed) {
                KeyAction::Bytes(bytes) => session.write(bytes),
                KeyAction::Copy => {
                    let text = session.selected_text();
                    if !text.is_empty() {
                        ui.ctx().copy_text(text);
                    }
                }
                KeyAction::Paste => {}
                KeyAction::None => {}
            },
            egui::Event::MouseWheel { unit, delta, .. } => {
                let lines = match unit {
                    egui::MouseWheelUnit::Line => delta.y.round() as i32,
                    egui::MouseWheelUnit::Point => (delta.y / FONT_SIZE).round() as i32,
                    egui::MouseWheelUnit::Page => {
                        if delta.y > 0.0 {
                            10
                        } else if delta.y < 0.0 {
                            -10
                        } else {
                            0
                        }
                    }
                };
                if lines != 0 {
                    session.scroll(lines);
                }
            }
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } if response.rect.contains(pos) => {
                let local = pos - response.rect.min;
                session.start_selection(local.x, local.y);
            }
            egui::Event::PointerMoved(pos) if response.dragged() => {
                let local = pos - response.rect.min;
                session.update_selection(local.x, local.y);
            }
            _ => {}
        }
    }
}

fn paint_grid(
    ui: &Ui,
    response: &egui::Response,
    painter: &egui::Painter,
    session: &mut TerminalSession,
    font_id: &FontId,
    metrics: CellMetrics,
) {
    let colors = session.colors.clone();
    let content = session.sync();
    let origin = response.rect.min;
    let cell_w = metrics.width;
    let cell_h = metrics.height;

    let mut shapes = vec![Shape::Rect(RectShape::filled(
        response.rect,
        CornerRadius::ZERO,
        colors.background,
    ))];

    let display_offset = content.display_offset as i32;
    let cursor_point = content.cursor;
    let app_cursor = content.mode.contains(TermMode::APP_CURSOR);
    let selection = content.selection;

    // Group non-skipped cells by viewport row.
    let mut by_line: BTreeMap<i32, Vec<&VisibleCell>> = BTreeMap::new();
    for cell in &content.cells {
        let row = cell.line + display_offset;
        by_line.entry(row).or_default().push(cell);
    }

    for (row, cells) in &by_line {
        let y = origin.y + cell_h * (*row as f32);
        paint_row_backgrounds(
            &mut shapes,
            origin.x,
            y,
            cell_w,
            cell_h,
            cells,
            selection,
            &colors,
            cursor_point,
        );
        paint_row_text(
            ui,
            &mut shapes,
            origin.x,
            y,
            cell_w,
            cells,
            selection,
            &colors,
            font_id,
            app_cursor,
            cursor_point,
        );
        paint_row_underlines(&mut shapes, origin.x, y, cell_w, cell_h, cells, &colors);
    }

    // Cursor when the cell was skipped (empty).
    let cursor_row = cursor_point.line.0 + display_offset;
    let cursor_on_skipped = !content.cells.iter().any(|c| {
        c.line == cursor_point.line.0 && c.col as usize == cursor_point.column.0
    });
    if cursor_on_skipped {
        let x = origin.x + cell_w * cursor_point.column.0 as f32;
        let y = origin.y + cell_h * cursor_row as f32;
        shapes.push(Shape::Rect(RectShape::filled(
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h)),
            CornerRadius::ZERO,
            colors.cursor,
        )));
    }

    painter.extend(shapes);

    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect,
            CornerRadius::ZERO,
            Stroke::new(1.0, colors.accent.gamma_multiply(0.5)),
            egui::StrokeKind::Inside,
        );
    }

    let _ = Frame::NONE;
}

fn cell_colors(
    cell: &VisibleCell,
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    app_cursor: bool,
    cursor_point: Point,
) -> (Color32, Color32, f32) {
    let wide = cell.flags.contains(Flags::WIDE_CHAR);
    let inverse = cell.flags.contains(Flags::INVERSE);
    let dim = cell.flags.intersects(Flags::DIM | Flags::DIM_BOLD);
    let point = Point::new(Line(cell.line), Column(cell.col as usize));
    let selected = selection.is_some_and(|r| r.contains(point));

    let mut fg = colors.resolve(cell.fg);
    let mut bg = colors.resolve(cell.bg);
    let width_cells = if wide { 2.0 } else { 1.0 };

    if dim {
        fg = fg.linear_multiply(0.7);
    }
    if inverse || selected {
        std::mem::swap(&mut fg, &mut bg);
    }

    let at_cursor =
        cursor_point.line.0 == cell.line && cursor_point.column.0 == cell.col as usize;
    if at_cursor {
        if app_cursor {
            std::mem::swap(&mut fg, &mut bg);
        } else {
            fg = colors.background;
        }
    }

    (fg, bg, width_cells)
}

#[allow(clippy::too_many_arguments)]
fn paint_row_backgrounds(
    shapes: &mut Vec<Shape>,
    origin_x: f32,
    y: f32,
    cell_w: f32,
    cell_h: f32,
    cells: &[&VisibleCell],
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    cursor_point: Point,
) {
    let mut sorted: Vec<&VisibleCell> = cells.to_vec();
    sorted.sort_by_key(|c| c.col);

    let mut run_bg: Option<Color32> = None;
    let mut run_start_x = 0.0;
    let mut run_end_x = 0.0;

    let flush = |shapes: &mut Vec<Shape>, bg: Color32, x0: f32, x1: f32| {
        if bg != colors.background && x1 > x0 {
            shapes.push(Shape::Rect(RectShape::filled(
                Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1 + 1.0, y + cell_h + 1.0)),
                CornerRadius::ZERO,
                bg,
            )));
        }
    };

    for cell in &sorted {
        let (_fg, bg, width_cells) = cell_colors(cell, selection, colors, false, cursor_point);
        let x = origin_x + cell_w * cell.col as f32;
        let w = cell_w * width_cells;

        if cursor_point.line.0 == cell.line && cursor_point.column.0 == cell.col as usize {
            shapes.push(Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h)),
                CornerRadius::ZERO,
                colors.cursor,
            )));
        }

        match run_bg {
            Some(prev) if prev == bg && (x - run_end_x).abs() < 0.5 => {
                run_end_x = x + w;
            }
            Some(prev) => {
                flush(shapes, prev, run_start_x, run_end_x);
                run_bg = Some(bg);
                run_start_x = x;
                run_end_x = x + w;
            }
            None => {
                run_bg = Some(bg);
                run_start_x = x;
                run_end_x = x + w;
            }
        }
    }
    if let Some(bg) = run_bg {
        flush(shapes, bg, run_start_x, run_end_x);
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_row_text(
    ui: &Ui,
    shapes: &mut Vec<Shape>,
    origin_x: f32,
    y: f32,
    cell_w: f32,
    cells: &[&VisibleCell],
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    font_id: &FontId,
    app_cursor: bool,
    cursor_point: Point,
) {
    let mut sorted: Vec<&VisibleCell> = cells
        .iter()
        .copied()
        .filter(|c| c.c != ' ' && c.c != '\t' && !c.flags.contains(Flags::WIDE_CHAR_SPACER))
        .collect();
    sorted.sort_by_key(|c| c.col);

    if sorted.is_empty() {
        return;
    }

    // One LayoutJob per contiguous same-fg run; absolute x keeps grid alignment
    // across gaps from skipped default cells.
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let (fg0, _bg, _) = cell_colors(start, selection, colors, app_cursor, cursor_point);
        let mut job = LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        job.wrap.break_anywhere = false;

        let run_x = origin_x + cell_w * start.col as f32;
        let mut expected_col = start.col;
        let mut text = String::new();

        while i < sorted.len() {
            let cell = sorted[i];
            if cell.col != expected_col {
                break;
            }
            let (fg, _, width_cells) =
                cell_colors(cell, selection, colors, app_cursor, cursor_point);
            if fg != fg0 {
                break;
            }
            text.push(cell.c);
            expected_col = cell.col.saturating_add(width_cells as u16);
            i += 1;
        }

        if text.is_empty() {
            i += 1;
            continue;
        }

        job.append(
            &text,
            0.0,
            TextFormat {
                font_id: font_id.clone(),
                color: fg0,
                ..Default::default()
            },
        );

        let galley = ui.fonts_mut(|f| f.layout_job(job));
        shapes.push(Shape::galley(Pos2::new(run_x, y), galley, fg0));
    }
}

fn paint_row_underlines(
    shapes: &mut Vec<Shape>,
    origin_x: f32,
    y: f32,
    cell_w: f32,
    cell_h: f32,
    cells: &[&VisibleCell],
    colors: &super::theme::TermColors,
) {
    for cell in cells {
        if !cell.flags.contains(Flags::UNDERLINE) {
            continue;
        }
        let wide = cell.flags.contains(Flags::WIDE_CHAR);
        let width = if wide { cell_w * 2.0 } else { cell_w };
        let x = origin_x + cell_w * cell.col as f32;
        let y2 = y + cell_h - 1.0;
        let fg = colors.resolve(cell.fg);
        shapes.push(Shape::LineSegment {
            points: [Pos2::new(x, y2), Pos2::new(x + width, y2)],
            stroke: Stroke::new(1.0, fg),
        });
    }
}
