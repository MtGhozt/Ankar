use super::backend::{CellMetrics, ResizeState, TerminalSession, VisibleCell};
use super::resize::DEBOUNCE;
use super::bindings::{map_key, KeyAction};
use crate::ui_theme::{self, CELL_H, CELL_W, PANE_PAD_X, PANE_PAD_Y};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::{SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use egui::epaint::RectShape;
use egui::text::{LayoutJob, TextFormat};
use egui::{
    Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2,
};
use std::collections::BTreeMap;

const FONT_FAMILY: egui::FontFamily = egui::FontFamily::Monospace;

pub struct TerminalView;

impl TerminalView {
    pub fn show(
        ui: &mut Ui,
        session: &mut TerminalSession,
        focused: bool,
        font_size: f32,
    ) -> egui::Response {
        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        if let Some(text) = session.take_clipboard() {
            ui.ctx().copy_text(text);
        }

        // Midterm: Hack at 14.33 with fixed cell metrics (config size is for scroll only).
        let paint_size = ui_theme::TERM_FONT_SIZE;
        let font_id = FontId::new(paint_size, FONT_FAMILY);
        let metrics = CellMetrics {
            width: CELL_W,
            height: CELL_H,
        };
        let content = content_rect(response.rect, ppp);
        match session.resize(content.width(), content.height(), metrics) {
            ResizeState::Idle => {}
            ResizeState::Pending => ui.ctx().request_repaint_after(DEBOUNCE),
            ResizeState::Applied => ui.ctx().request_repaint(),
        }

        // Only take focus on click/drag — never every frame, or sidebar search
        // and other TextEdits can never keep the keyboard.
        if response.clicked() || response.dragged() {
            response.request_focus();
        } else if focused
            && !response.has_focus()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.ctx().memory(|m| m.focused().is_none())
        {
            // Active pane with nothing else focused → claim keys once.
            response.request_focus();
        }

        if focused {
            if response.has_focus() {
                ui.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        response.id,
                        egui::EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: true,
                        },
                    );
                });
            }
            // Keys only when the terminal widget has focus (sidebar search can keep it).
            handle_input(
                ui,
                &response,
                session,
                font_size,
                response.has_focus(),
                content,
            );
        }
        paint_grid(ui, &response, &painter, session, &font_id, metrics, content);
        response
    }
}

/// Pane content area after midterm padding, snapped to physical pixels.
fn content_rect(pane: Rect, ppp: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(
            ui_theme::snap(pane.min.x + PANE_PAD_X, ppp),
            ui_theme::snap(pane.min.y + PANE_PAD_Y, ppp),
        ),
        pane.max,
    )
}

/// Convert a screen position to content-local coordinates (padding subtracted).
fn to_content_local(pos: Pos2, content: Rect) -> Vec2 {
    pos - content.min
}

fn handle_input(
    ui: &Ui,
    response: &egui::Response,
    session: &mut TerminalSession,
    font_size: f32,
    keyboard_focus: bool,
    content: Rect,
) {
    let modifiers = ui.input(|i| i.modifiers);
    let mode = session.snapshot().mode;
    let mouse_reporting = mode.intersects(TermMode::MOUSE_MODE) && !modifiers.shift;

    // Pointer always when this pane is the interaction target.
    if mouse_reporting {
        handle_mouse_report(ui, response, session, content);
    } else if keyboard_focus || response.clicked() || response.dragged() {
        handle_selection_pointer(ui, response, session, font_size, content);
    }

    if !keyboard_focus {
        return;
    }

    let events: Vec<egui::Event> = ui.input(|i| i.events.clone());

    for event in events {
        match event {
            egui::Event::Text(text) => {
                if modifiers.ctrl || modifiers.command {
                    continue;
                }
                if modifiers.alt {
                    let mut buf = vec![0x1b];
                    buf.extend(text.as_bytes());
                    session.write(buf);
                } else {
                    session.write(text.into_bytes());
                }
            }
            egui::Event::Paste(text) => {
                if mode.contains(TermMode::BRACKETED_PASTE) {
                    let mut buf = b"\x1b[200~".to_vec();
                    buf.extend(text.as_bytes());
                    buf.extend_from_slice(b"\x1b[201~");
                    session.write(buf);
                } else {
                    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
                    session.write(normalized.into_bytes());
                }
            }
            egui::Event::Copy => {
                let text = session.selected_text();
                if !text.is_empty() {
                    ui.ctx().copy_text(text);
                    session.clear_selection();
                } else {
                    session.write(vec![0x03]);
                }
            }
            egui::Event::Cut => {
                session.write(vec![0x18]);
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
                KeyAction::Paste => {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                }
                KeyAction::None => {}
            },
            egui::Event::MouseWheel { unit, delta, .. } if !mouse_reporting => {
                handle_wheel(session, mode, unit, delta, font_size);
            }
            _ => {}
        }
    }
}

fn handle_wheel(
    session: &mut TerminalSession,
    mode: TermMode,
    unit: egui::MouseWheelUnit,
    delta: Vec2,
    font_size: f32,
) {
    let lines = match unit {
        egui::MouseWheelUnit::Line => delta.y.round() as i32,
        egui::MouseWheelUnit::Point => (delta.y / font_size).round() as i32,
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
    if lines == 0 {
        return;
    }

    // Alternate screen + alternate scroll → send arrows instead of scrollback.
    if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
        let arrow = if lines > 0 {
            b"\x1b[A".as_slice()
        } else {
            b"\x1b[B".as_slice()
        };
        let notches = lines.unsigned_abs().saturating_mul(3);
        let mut buf = Vec::with_capacity(notches as usize * arrow.len());
        for _ in 0..notches {
            buf.extend_from_slice(arrow);
        }
        session.write(buf);
    } else {
        session.scroll(lines);
    }
}

fn handle_mouse_report(
    ui: &Ui,
    response: &egui::Response,
    session: &mut TerminalSession,
    content: Rect,
) {
    let mode = session.snapshot().mode;
    let events: Vec<egui::Event> = ui.input(|i| i.events.clone());
    for event in events {
        match event {
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                ..
            } if response.rect.contains(pos) => {
                let local = to_content_local(pos, content);
                let (col, row) = session.cell_at(local.x, local.y);
                let btn = match button {
                    egui::PointerButton::Primary => 0,
                    egui::PointerButton::Middle => 1,
                    egui::PointerButton::Secondary => 2,
                    egui::PointerButton::Extra1 | egui::PointerButton::Extra2 => continue,
                };
                session.report_mouse(btn, col, row, pressed, false);
            }
            egui::Event::PointerMoved(pos)
                if response.rect.contains(pos)
                    && mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) =>
            {
                let local = to_content_local(pos, content);
                let (col, row) = session.cell_at(local.x, local.y);
                let pressed = ui.input(|i| i.pointer.any_down());
                let btn = if ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
                    0
                } else if ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle)) {
                    1
                } else if ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary)) {
                    2
                } else if mode.contains(TermMode::MOUSE_MOTION) {
                    3 // motion with no button (rare)
                } else {
                    continue;
                };
                if pressed || mode.contains(TermMode::MOUSE_MOTION) {
                    session.report_mouse(btn, col, row, true, true);
                }
            }
            egui::Event::MouseWheel { delta, .. } if response.hovered() => {
                let Some(pos) = ui.input(|i| i.pointer.hover_pos()) else {
                    continue;
                };
                if !response.rect.contains(pos) {
                    continue;
                }
                let local = to_content_local(pos, content);
                let (col, row) = session.cell_at(local.x, local.y);
                let btn = if delta.y > 0.0 { 64 } else { 65 };
                session.report_mouse(btn, col, row, true, false);
            }
            _ => {}
        }
    }
}

fn handle_selection_pointer(
    ui: &Ui,
    response: &egui::Response,
    session: &mut TerminalSession,
    font_size: f32,
    content: Rect,
) {
    let modifiers = ui.input(|i| i.modifiers);

    if response.secondary_clicked() {
        let text = session.selected_text();
        if !text.is_empty() {
            ui.ctx().copy_text(text);
            session.clear_selection();
        } else {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
        return;
    }

    if response.triple_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let local = to_content_local(pos, content);
            session.start_selection(local.x, local.y, SelectionType::Lines);
        }
        return;
    }
    if response.double_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let local = to_content_local(pos, content);
            session.start_selection(local.x, local.y, SelectionType::Semantic);
        }
        return;
    }

    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            let local = to_content_local(pos, content);
            if modifiers.shift && session.has_selection() {
                session.update_selection(local.x, local.y);
            } else {
                session.start_selection(local.x, local.y, SelectionType::Simple);
            }
        }
    } else if response.dragged() {
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let local = to_content_local(pos, content);
            // Autoscroll when dragging past the top/bottom edge.
            if pos.y < response.rect.min.y {
                let steps = ((response.rect.min.y - pos.y) / font_size).ceil() as i32;
                session.scroll(steps.max(1));
            } else if pos.y > response.rect.max.y {
                let steps = ((pos.y - response.rect.max.y) / font_size).ceil() as i32;
                session.scroll(-steps.max(1));
            }
            session.update_selection(local.x, local.y);
        }
    } else if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let local = to_content_local(pos, content);
            if modifiers.shift {
                if session.has_selection() {
                    session.update_selection(local.x, local.y);
                } else {
                    session.start_selection(local.x, local.y, SelectionType::Simple);
                }
            } else {
                session.clear_selection();
            }
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
    content_area: Rect,
) {
    let colors = session.colors.clone();
    let grid = session.sync();
    let origin = content_area.min;
    let cell_w = metrics.width;
    let cell_h = metrics.height;
    let ppp = ui.ctx().pixels_per_point();

    let mut shapes = vec![Shape::Rect(RectShape::filled(
        response.rect,
        CornerRadius::ZERO,
        colors.background,
    ))];

    let display_offset = grid.display_offset as i32;
    let cursor_point = grid.cursor;
    let app_cursor = grid.mode.contains(TermMode::APP_CURSOR);
    // TUIs (OpenCode, etc.) draw their own cursor on the alternate screen.
    let alt_screen = grid.mode.contains(TermMode::ALT_SCREEN);
    let selection = grid.selection;

    let mut by_line: BTreeMap<i32, Vec<&VisibleCell>> = BTreeMap::new();
    for cell in &grid.cells {
        let row = cell.line + display_offset;
        by_line.entry(row).or_default().push(cell);
    }

    for (row, cells) in &by_line {
        // row_top(n) snapped; bottom = next row's top (no seams).
        let y = ui_theme::term_row_top(origin.y, *row, ppp);
        let y_next = ui_theme::term_row_top(origin.y, *row + 1, ppp);
        let row_h = (y_next - y).max(cell_h);
        paint_row_backgrounds(
            &mut shapes,
            origin.x,
            y,
            cell_w,
            row_h,
            cells,
            selection,
            &colors,
            cursor_point,
            ppp,
            !alt_screen,
        );
        paint_row_box_drawing(
            &mut shapes,
            origin.x,
            y,
            cell_w,
            cell_h,
            cells,
            selection,
            &colors,
            ppp,
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
            !alt_screen,
            ppp,
        );
        paint_row_underlines(
            &mut shapes,
            origin.x,
            y,
            cell_w,
            cell_h,
            cells,
            &colors,
            ppp,
        );
    }

    if !alt_screen {
        let cursor_row = cursor_point.line.0 + display_offset;
        let cursor_on_skipped = !grid.cells.iter().any(|c| {
            c.line == cursor_point.line.0 && c.col as usize == cursor_point.column.0
        });
        let window_focused = ui.input(|i| i.focused);
        if cursor_on_skipped {
            let x = ui_theme::term_col_x(origin.x, cursor_point.column.0 as u16, ppp);
            let y = ui_theme::term_row_top(origin.y, cursor_row, ppp);
            let y_next = ui_theme::term_row_top(origin.y, cursor_row + 1, ppp);
            paint_cursor(
                &mut shapes,
                x,
                y,
                cell_w,
                (y_next - y).max(cell_h),
                colors.cursor,
                window_focused,
            );
        }
    }

    painter.extend(shapes);
}

fn paint_cursor(
    shapes: &mut Vec<Shape>,
    x: f32,
    y: f32,
    cell_w: f32,
    cell_h: f32,
    color: Color32,
    window_focused: bool,
) {
    // Midterm: focused = 2 px bar, height cell − 3; unfocused = 1 px outline box.
    if window_focused {
        let h = (cell_h - 3.0).max(1.0);
        let bar = Rect::from_min_size(Pos2::new(x, y + 1.5), Vec2::new(2.0, h));
        shapes.push(Shape::Rect(RectShape::filled(bar, CornerRadius::ZERO, color)));
    } else {
        shapes.push(Shape::Rect(RectShape::stroke(
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h)),
            CornerRadius::ZERO,
            Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        )));
    }
}

fn is_box_drawing(c: char) -> bool {
    matches!(
        c,
        '\u{2500}'..='\u{257F}' | '\u{2580}'..='\u{259F}'
    )
}

#[allow(clippy::too_many_arguments)]
fn paint_row_box_drawing(
    shapes: &mut Vec<Shape>,
    origin_x: f32,
    y: f32,
    cell_w: f32,
    cell_h: f32,
    cells: &[&VisibleCell],
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    ppp: f32,
) {
    for cell in cells {
        if !is_box_drawing(cell.c) {
            continue;
        }
        let (fg, _bg, width_cells) = cell_colors(
            cell,
            selection,
            colors,
            false,
            Point::new(Line(0), Column(0)),
            false,
        );
        let x = ui_theme::term_col_x(origin_x, cell.col, ppp);
        let x_end =
            ui_theme::term_col_x(origin_x, cell.col.saturating_add(width_cells as u16), ppp);
        let w = (x_end - x).max(cell_w * width_cells);
        let cx = x + w * 0.5;
        let cy = y + cell_h * 0.5;
        let stroke = Stroke::new(1.25, fg);
        let hline = |shapes: &mut Vec<Shape>, x0: f32, x1: f32| {
            shapes.push(Shape::LineSegment {
                points: [Pos2::new(x0, cy), Pos2::new(x1, cy)],
                stroke,
            });
        };
        let vline = |shapes: &mut Vec<Shape>, y0: f32, y1: f32| {
            shapes.push(Shape::LineSegment {
                points: [Pos2::new(cx, y0), Pos2::new(cx, y1)],
                stroke,
            });
        };
        match cell.c {
            '─' | '━' => hline(shapes, x, x + w),
            '│' | '┃' => vline(shapes, y, y + cell_h),
            '┌' | '╭' => {
                hline(shapes, cx, x + w);
                vline(shapes, cy, y + cell_h);
            }
            '┐' | '╮' => {
                hline(shapes, x, cx);
                vline(shapes, cy, y + cell_h);
            }
            '└' | '╰' => {
                hline(shapes, cx, x + w);
                vline(shapes, y, cy);
            }
            '┘' | '╯' => {
                hline(shapes, x, cx);
                vline(shapes, y, cy);
            }
            '├' => {
                hline(shapes, cx, x + w);
                vline(shapes, y, y + cell_h);
            }
            '┤' => {
                hline(shapes, x, cx);
                vline(shapes, y, y + cell_h);
            }
            '▀' => {
                shapes.push(Shape::Rect(RectShape::filled(
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, cell_h * 0.5)),
                    CornerRadius::ZERO,
                    fg,
                )));
            }
            '▄' => {
                shapes.push(Shape::Rect(RectShape::filled(
                    Rect::from_min_size(
                        Pos2::new(x, y + cell_h * 0.5),
                        Vec2::new(w, cell_h * 0.5),
                    ),
                    CornerRadius::ZERO,
                    fg,
                )));
            }
            '█' => {
                shapes.push(Shape::Rect(RectShape::filled(
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, cell_h)),
                    CornerRadius::ZERO,
                    fg,
                )));
            }
            _ => {
                // Fallback: full cell block so unknown box glyphs still join.
                shapes.push(Shape::Rect(RectShape::filled(
                    Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, cell_h)),
                    CornerRadius::ZERO,
                    fg.gamma_multiply(0.35),
                )));
            }
        }
    }
}

fn cell_colors(
    cell: &VisibleCell,
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    app_cursor: bool,
    cursor_point: Point,
    apply_host_cursor: bool,
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

    // Only invert/blank for our host cursor on the primary screen.
    // On ALT_SCREEN the TUI already painted the cursor via cell attrs.
    if apply_host_cursor {
        let at_cursor =
            cursor_point.line.0 == cell.line && cursor_point.column.0 == cell.col as usize;
        if at_cursor {
            if app_cursor {
                std::mem::swap(&mut fg, &mut bg);
            } else {
                fg = colors.background;
            }
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
    ppp: f32,
    paint_host_cursor: bool,
) {
    let mut sorted: Vec<&VisibleCell> = cells.to_vec();
    sorted.sort_by_key(|c| c.col);

    let mut run_bg: Option<Color32> = None;
    let mut run_start_x = 0.0;
    let mut run_end_x = 0.0;

    // Overlap by 1 physical px so adjacent rows/cols have no hairline gaps.
    let overlap = 1.0 / ppp;
    let flush = |shapes: &mut Vec<Shape>, bg: Color32, x0: f32, x1: f32| {
        if bg != colors.background && x1 > x0 {
            shapes.push(Shape::Rect(RectShape::filled(
                Rect::from_min_max(
                    Pos2::new(ui_theme::snap(x0, ppp), y),
                    Pos2::new(ui_theme::snap(x1, ppp) + overlap, y + cell_h + overlap),
                ),
                CornerRadius::ZERO,
                bg,
            )));
        }
    };

    for cell in &sorted {
        let (_fg, bg, width_cells) =
            cell_colors(cell, selection, colors, false, cursor_point, paint_host_cursor);
        let x = ui_theme::term_col_x(origin_x, cell.col, ppp);
        let x_end = ui_theme::term_col_x(origin_x, cell.col.saturating_add(width_cells as u16), ppp);
        let w = x_end - x;

        if paint_host_cursor
            && cursor_point.line.0 == cell.line
            && cursor_point.column.0 == cell.col as usize
        {
            paint_cursor(shapes, x, y, cell_w, cell_h, colors.cursor, true);
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
    _cell_w: f32,
    cells: &[&VisibleCell],
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    font_id: &FontId,
    app_cursor: bool,
    cursor_point: Point,
    apply_host_cursor: bool,
    ppp: f32,
) {
    let mut sorted: Vec<&VisibleCell> = cells
        .iter()
        .copied()
        .filter(|c| {
            c.c != ' '
                && c.c != '\t'
                && !c.flags.contains(Flags::WIDE_CHAR_SPACER)
                && !is_box_drawing(c.c)
        })
        .collect();
    sorted.sort_by_key(|c| c.col);

    if sorted.is_empty() {
        return;
    }

    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let (fg0, _bg, _) = cell_colors(
            start,
            selection,
            colors,
            app_cursor,
            cursor_point,
            apply_host_cursor,
        );
        let mut job = LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        job.wrap.break_anywhere = false;

        let run_x = ui_theme::term_col_x(origin_x, start.col, ppp);
        let mut expected_col = start.col;
        let mut text = String::new();

        while i < sorted.len() {
            let cell = sorted[i];
            if cell.col != expected_col {
                break;
            }
            let (fg, _, width_cells) = cell_colors(
                cell,
                selection,
                colors,
                app_cursor,
                cursor_point,
                apply_host_cursor,
            );
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

#[allow(clippy::too_many_arguments)]
fn paint_row_underlines(
    shapes: &mut Vec<Shape>,
    origin_x: f32,
    y: f32,
    cell_w: f32,
    cell_h: f32,
    cells: &[&VisibleCell],
    colors: &super::theme::TermColors,
    ppp: f32,
) {
    for cell in cells {
        if !cell.flags.contains(Flags::UNDERLINE) {
            continue;
        }
        let wide = cell.flags.contains(Flags::WIDE_CHAR);
        let cells_w = if wide { 2u16 } else { 1 };
        let x = ui_theme::term_col_x(origin_x, cell.col, ppp);
        let x_end = ui_theme::term_col_x(origin_x, cell.col.saturating_add(cells_w), ppp);
        let width = (x_end - x).max(if wide { cell_w * 2.0 } else { cell_w });
        let y2 = y + cell_h - 1.0;
        let fg = colors.resolve(cell.fg);
        shapes.push(Shape::LineSegment {
            points: [Pos2::new(x, y2), Pos2::new(x + width, y2)],
            stroke: Stroke::new(1.0, fg),
        });
    }
}
