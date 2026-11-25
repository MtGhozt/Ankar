use super::backend::{CellMetrics, ResizeState, TerminalSession, VisibleCell};
use super::resize::DEBOUNCE;
use super::bindings::{map_key, KeyAction};
use crate::ui_theme::{self, CELL_H, CELL_W, PANE_PAD_X, PANE_PAD_Y};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::{SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::CursorShape;
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

        if focused && response.has_focus() {
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
        // Pointer reporting must work while hovering, even if another widget owns
        // the keyboard. Keys stay gated on this pane being active and focused.
        handle_input(
            ui,
            &response,
            session,
            font_size,
            focused && response.has_focus(),
            content,
        );
        paint_grid(ui, &response, &painter, session, &font_id, metrics, content, focused);
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

/// Protocol modifier bits: shift 4, alt 8, ctrl 16.
fn mouse_modifiers(m: egui::Modifiers) -> u8 {
    (if m.shift { 4 } else { 0 })
        | (if m.alt { 8 } else { 0 })
        | (if m.ctrl || m.command { 16 } else { 0 })
}

fn handle_mouse_report(
    ui: &Ui,
    response: &egui::Response,
    session: &mut TerminalSession,
    content: Rect,
) {
    if !response.hovered() {
        session.clear_mouse_motion();
        return;
    }
    let mode = session.snapshot().mode;
    let modifiers = ui.input(|i| i.modifiers);
    let events: Vec<egui::Event> = ui.input(|i| i.events.clone());
    for event in events {
        match event {
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers,
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
                session.report_mouse(
                    btn,
                    col,
                    row,
                    pressed,
                    false,
                    mouse_modifiers(modifiers),
                );
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
                    3 // motion with no button held (DECSET 1003)
                } else {
                    continue;
                };
                // 1002 only reports drags; 1003 also reports plain hover.
                if pressed || mode.contains(TermMode::MOUSE_MOTION) {
                    session.report_mouse(btn, col, row, true, true, mouse_modifiers(modifiers));
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
                session.report_mouse(btn, col, row, true, false, mouse_modifiers(modifiers));
            }
            egui::Event::PointerGone => session.clear_mouse_motion(),
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

#[allow(clippy::too_many_arguments)]
fn paint_grid(
    ui: &Ui,
    response: &egui::Response,
    painter: &egui::Painter,
    session: &mut TerminalSession,
    font_id: &FontId,
    metrics: CellMetrics,
    content_area: Rect,
    focused: bool,
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
    let selection = grid.selection;
    // row_top(n) snapped; bottom = next row's top (no seams).
    let row_span = |row: i32| {
        let y = ui_theme::term_row_top(origin.y, row, ppp);
        (y, ui_theme::term_row_top(origin.y, row + 1, ppp) - y)
    };

    let mut by_line: BTreeMap<i32, Vec<&VisibleCell>> = BTreeMap::new();
    for cell in &grid.cells {
        let row = cell.line + display_offset;
        by_line.entry(row).or_default().push(cell);
    }

    let cursor = host_cursor(grid, &colors, focused && ui.input(|i| i.focused));
    let block_cursor = cursor
        .filter(|c| c.shape == CursorShape::Block)
        .map(|c| c.point);

    for (row, cells) in &by_line {
        let (y, row_h) = row_span(*row);
        paint_row_backgrounds(
            &mut shapes,
            origin.x,
            y,
            row_h,
            cells,
            selection,
            &colors,
            ppp,
        );
    }

    // Above backgrounds, below glyphs: a block cursor shows the text in the cell's bg.
    if let Some(cursor) = cursor {
        let row = cursor.point.line.0 + display_offset;
        if (0..grid.size.num_lines as i32).contains(&row) {
            let (y, row_h) = row_span(row);
            let col = cursor.point.column.0 as u16;
            let x = ui_theme::term_col_x(origin.x, col, ppp);
            let x_end = ui_theme::term_col_x(origin.x, col.saturating_add(cursor.width), ppp);
            paint_cursor(
                &mut shapes,
                Rect::from_min_max(Pos2::new(x, y), Pos2::new(x_end, y + row_h)),
                cursor.shape,
                cursor.color,
                ppp,
            );
        }
    }

    for (row, cells) in &by_line {
        let (y, row_h) = row_span(*row);
        paint_row_box_drawing(
            &mut shapes,
            origin.x,
            y,
            cell_w,
            cell_h,
            row_h,
            cells,
            selection,
            &colors,
            block_cursor,
            ppp,
        );
        paint_row_text(
            ui,
            &mut shapes,
            origin.x,
            y,
            cells,
            selection,
            &colors,
            font_id,
            block_cursor,
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

    painter.extend(shapes);
}

#[derive(Clone, Copy, Debug)]
struct HostCursor {
    point: Point,
    shape: CursorShape,
    color: Color32,
    /// Cells covered: 2 on a wide glyph.
    width: u16,
}

/// The cursor the app asked for (DECSCUSR shape, OSC 12 colour, DECTCEM visibility).
/// An unfocused pane always shows the hollow box.
fn host_cursor(
    grid: &super::backend::Snapshot,
    colors: &super::theme::TermColors,
    focused: bool,
) -> Option<HostCursor> {
    if grid.cursor_shape == CursorShape::Hidden {
        return None;
    }
    let point = grid.cursor;
    let wide = grid.cells.iter().any(|c| {
        c.line == point.line.0
            && c.col as usize == point.column.0
            && c.flags.contains(Flags::WIDE_CHAR)
    });
    Some(HostCursor {
        point,
        shape: if focused {
            grid.cursor_shape
        } else {
            CursorShape::HollowBlock
        },
        color: grid
            .cursor_color
            .map(|c| Color32::from_rgb(c.r, c.g, c.b))
            .unwrap_or(colors.cursor),
        width: if wide { 2 } else { 1 },
    })
}

/// `cell` is the snapped cell rect; every part is a whole-pixel solid rect.
fn paint_cursor(
    shapes: &mut Vec<Shape>,
    cell: Rect,
    shape: CursorShape,
    color: Color32,
    ppp: f32,
) {
    let px = 1.0 / ppp.max(f32::EPSILON);
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let (x0, x1) = px_span(x0, x1, ppp);
        let (y0, y1) = px_span(y0, y1, ppp);
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    };
    let Rect { min, max } = cell;
    match shape {
        CursorShape::Block => push_solid_rect(shapes, cell, color),
        // Midterm bar: 2 px wide, inset 1.5 px top and bottom.
        CursorShape::Beam => {
            push_solid_rect(shapes, rect(min.x, min.y + 1.5, min.x + 2.0, max.y - 1.5), color)
        }
        CursorShape::Underline => {
            push_solid_rect(shapes, rect(min.x, max.y - 2.0, max.x, max.y), color)
        }
        CursorShape::HollowBlock => {
            for edge in [
                rect(min.x, min.y, max.x, min.y + px),
                rect(min.x, max.y - px, max.x, max.y),
                rect(min.x, min.y, min.x + px, max.y),
                rect(max.x - px, min.y, max.x, max.y),
            ] {
                push_solid_rect(shapes, edge, color);
            }
        }
        CursorShape::Hidden => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Weight {
    Light,
    Heavy,
}

impl Weight {
    /// Light strokes are ~1/8 of the cell in whole device pixels; heavy ones are
    /// clearly double. Returned in logical points.
    fn px(self, cell_w: f32, cell_h: f32, ppp: f32) -> f32 {
        let ppp = ppp.max(f32::EPSILON);
        let light = (cell_w.min(cell_h) * ppp / 8.0).round().max(1.0);
        let device = match self {
            Self::Light => light,
            Self::Heavy => (light * 2.0).max(light + 1.0),
        };
        device / ppp
    }
}

#[derive(Clone, Copy)]
struct Arms {
    up: Option<Weight>,
    down: Option<Weight>,
    left: Option<Weight>,
    right: Option<Weight>,
}

fn arms(u: u8, d: u8, l: u8, r: u8) -> Option<Arms> {
    let w = |v| match v {
        1 => Some(Weight::Light),
        2 => Some(Weight::Heavy),
        _ => None,
    };
    Some(Arms { up: w(u), down: w(d), left: w(l), right: w(r) })
}

/// Arms of the U+2500–U+257F box glyphs that need no font glyph.
/// Weights: 0 = none, 1 = light, 2 = heavy (`─` vs `━`, `│` vs `┃`).
fn box_arms(c: char) -> Option<Arms> {
    match c {
        '─' | '┄' | '┈' => arms(0, 0, 1, 1),
        '━' | '┅' | '┉' => arms(0, 0, 2, 2),
        '│' | '┆' | '┊' => arms(1, 1, 0, 0),
        '┃' | '┇' | '┋' => arms(2, 2, 0, 0),
        '┌' => arms(0, 1, 0, 1),
        '┍' => arms(0, 1, 0, 2),
        '┎' => arms(0, 2, 0, 1),
        '┏' => arms(0, 2, 0, 2),
        '┐' => arms(0, 1, 1, 0),
        '┑' => arms(0, 1, 2, 0),
        '┒' => arms(0, 2, 1, 0),
        '┓' => arms(0, 2, 2, 0),
        '└' => arms(1, 0, 0, 1),
        '┕' => arms(1, 0, 0, 2),
        '┖' => arms(2, 0, 0, 1),
        '┗' => arms(2, 0, 0, 2),
        '┘' => arms(1, 0, 1, 0),
        '┙' => arms(1, 0, 2, 0),
        '┚' => arms(2, 0, 1, 0),
        '┛' => arms(2, 0, 2, 0),
        '├' => arms(1, 1, 0, 1),
        '┝' => arms(1, 1, 0, 2),
        '┞' => arms(2, 1, 0, 1),
        '┟' => arms(1, 2, 0, 1),
        '┠' => arms(2, 2, 0, 1),
        '┡' => arms(2, 1, 0, 2),
        '┢' => arms(1, 2, 0, 2),
        '┣' => arms(2, 2, 0, 2),
        '┤' => arms(1, 1, 1, 0),
        '┥' => arms(1, 1, 2, 0),
        '┦' => arms(2, 1, 1, 0),
        '┧' => arms(1, 2, 1, 0),
        '┨' => arms(2, 2, 1, 0),
        '┩' => arms(2, 1, 2, 0),
        '┪' => arms(1, 2, 2, 0),
        '┫' => arms(2, 2, 2, 0),
        '┬' => arms(0, 1, 1, 1),
        '┭' => arms(0, 1, 2, 1),
        '┮' => arms(0, 1, 1, 2),
        '┯' => arms(0, 1, 2, 2),
        '┰' => arms(0, 2, 1, 1),
        '┱' => arms(0, 2, 2, 1),
        '┲' => arms(0, 2, 1, 2),
        '┳' => arms(0, 2, 2, 2),
        '┴' => arms(1, 0, 1, 1),
        '┵' => arms(1, 0, 2, 1),
        '┶' => arms(1, 0, 1, 2),
        '┷' => arms(1, 0, 2, 2),
        '┸' => arms(2, 0, 1, 1),
        '┹' => arms(2, 0, 2, 1),
        '┺' => arms(2, 0, 1, 2),
        '┻' => arms(2, 0, 2, 2),
        '┼' => arms(1, 1, 1, 1),
        '┽' => arms(1, 1, 2, 1),
        '┾' => arms(1, 1, 1, 2),
        '┿' => arms(1, 1, 2, 2),
        '╀' => arms(2, 1, 1, 1),
        '╁' => arms(1, 2, 1, 1),
        '╂' => arms(2, 2, 1, 1),
        '╃' => arms(2, 1, 2, 1),
        '╄' => arms(2, 1, 1, 2),
        '╅' => arms(1, 2, 2, 1),
        '╆' => arms(1, 2, 1, 2),
        '╇' => arms(2, 1, 2, 2),
        '╈' => arms(1, 2, 2, 2),
        '╉' => arms(2, 2, 2, 1),
        '╊' => arms(2, 2, 1, 2),
        '╋' => arms(2, 2, 2, 2),
        // Half arms (╴╵╶╷ light, ╸╹╺╻ heavy, ╼–╿ mixed).
        '╴' => arms(0, 0, 1, 0),
        '╵' => arms(1, 0, 0, 0),
        '╶' => arms(0, 0, 0, 1),
        '╷' => arms(0, 1, 0, 0),
        '╸' => arms(0, 0, 2, 0),
        '╹' => arms(2, 0, 0, 0),
        '╺' => arms(0, 0, 0, 2),
        '╻' => arms(0, 2, 0, 0),
        '╼' => arms(0, 0, 1, 2),
        '╽' => arms(1, 2, 0, 0),
        '╾' => arms(0, 0, 2, 1),
        '╿' => arms(2, 1, 0, 0),
        _ => None,
    }
}

fn is_arc(c: char) -> bool {
    matches!(c, '╭'..='╰')
}

/// Glyphs painted as geometry; everything else in the box/block ranges keeps the font.
fn is_box_geometry(c: char) -> bool {
    box_arms(c).is_some()
        || is_arc(c)
        || matches!(c, '\u{2580}'..='\u{259F}')
}

/// Snap a span to whole physical pixels so neighbours share their edge exactly.
fn px_span(lo: f32, hi: f32, ppp: f32) -> (f32, f32) {
    let lo_px = (lo * ppp).round();
    let hi_px = (hi * ppp).round().max(lo_px + 1.0);
    (lo_px / ppp, hi_px / ppp)
}

/// Stroke band of a fixed whole-pixel thickness centred on `center`, so every
/// cell draws the same width regardless of its fractional position.
fn px_band(center: f32, thickness: f32, ppp: f32) -> (f32, f32) {
    let t = (thickness * ppp).round().max(1.0);
    let lo = (center * ppp - t * 0.5).round();
    (lo / ppp, (lo + t) / ppp)
}

/// Filled rect as a raw mesh: no anti-aliasing halo, so adjacent rects tile.
fn push_solid_rect(shapes: &mut Vec<Shape>, rect: Rect, color: Color32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    mesh.add_colored_rect(rect, color);
    shapes.push(Shape::mesh(mesh));
}

type Frac = (f32, f32, f32, f32);

#[allow(clippy::too_many_arguments)]
fn push_rect(
    shapes: &mut Vec<Shape>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    fg: Color32,
    fx: Frac,
    ppp: f32,
) {
    let (x0, x1) = px_span(x + w * fx.0, x + w * fx.2, ppp);
    let (y0, y1) = px_span(y + h * fx.1, y + h * fx.3, ppp);
    push_solid_rect(
        shapes,
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
        fg,
    );
}

/// U+2580–U+259F as cell fractions (eighths, halves, shades, quadrants).
#[allow(clippy::too_many_arguments)]
fn paint_block(
    shapes: &mut Vec<Shape>,
    c: char,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    fg: Color32,
    ppp: f32,
) -> bool {
    let parts: &[Frac] = match c {
        '▀' => &[(0.0, 0.0, 1.0, 0.5)],
        '▔' => &[(0.0, 0.0, 1.0, 0.125)],
        '▁' => &[(0.0, 0.875, 1.0, 1.0)],
        '▂' => &[(0.0, 0.75, 1.0, 1.0)],
        '▃' => &[(0.0, 0.625, 1.0, 1.0)],
        '▄' => &[(0.0, 0.5, 1.0, 1.0)],
        '▅' => &[(0.0, 0.375, 1.0, 1.0)],
        '▆' => &[(0.0, 0.25, 1.0, 1.0)],
        '▇' => &[(0.0, 0.125, 1.0, 1.0)],
        '█' => &[(0.0, 0.0, 1.0, 1.0)],
        '▉' => &[(0.0, 0.0, 0.875, 1.0)],
        '▊' => &[(0.0, 0.0, 0.75, 1.0)],
        '▋' => &[(0.0, 0.0, 0.625, 1.0)],
        '▌' => &[(0.0, 0.0, 0.5, 1.0)],
        '▍' => &[(0.0, 0.0, 0.375, 1.0)],
        '▎' => &[(0.0, 0.0, 0.25, 1.0)],
        '▏' => &[(0.0, 0.0, 0.125, 1.0)],
        '▐' => &[(0.5, 0.0, 1.0, 1.0)],
        '▕' => &[(0.875, 0.0, 1.0, 1.0)],
        '░' => &[(0.0, 0.0, 1.0, 1.0)],
        '▒' => &[(0.0, 0.0, 1.0, 1.0)],
        '▓' => &[(0.0, 0.0, 1.0, 1.0)],
        '▖' => &[(0.0, 0.5, 0.5, 1.0)],
        '▗' => &[(0.5, 0.5, 1.0, 1.0)],
        '▘' => &[(0.0, 0.0, 0.5, 0.5)],
        '▝' => &[(0.5, 0.0, 1.0, 0.5)],
        '▚' => &[(0.0, 0.0, 0.5, 0.5), (0.5, 0.5, 1.0, 1.0)],
        '▞' => &[(0.5, 0.0, 1.0, 0.5), (0.0, 0.5, 0.5, 1.0)],
        '▙' => &[(0.0, 0.0, 0.5, 0.5), (0.0, 0.5, 0.5, 1.0), (0.5, 0.5, 1.0, 1.0)],
        '▛' => &[(0.0, 0.0, 0.5, 0.5), (0.5, 0.0, 1.0, 0.5), (0.0, 0.5, 0.5, 1.0)],
        '▜' => &[(0.0, 0.0, 0.5, 0.5), (0.5, 0.0, 1.0, 0.5), (0.5, 0.5, 1.0, 1.0)],
        '▟' => &[(0.5, 0.0, 1.0, 0.5), (0.5, 0.5, 1.0, 1.0), (0.0, 0.5, 0.5, 1.0)],
        _ => return false,
    };
    let fg = match c {
        '░' => fg.gamma_multiply(0.25),
        '▒' => fg.gamma_multiply(0.5),
        '▓' => fg.gamma_multiply(0.75),
        _ => fg,
    };
    for &part in parts {
        push_rect(shapes, x, y, w, h, fg, part, ppp);
    }
    true
}

/// Quarter-circle elbow for ╭ ╮ ╯ ╰, with a straight arm reaching each cell edge.
fn paint_arc(
    shapes: &mut Vec<Shape>,
    c: char,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stroke: Stroke,
) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let r = (w.min(h) * 0.25).max(1.0);
    let (center, a0, a1) = match c {
        '╭' => (Pos2::new(cx + r, cy + r), std::f32::consts::PI, 1.5 * std::f32::consts::PI),
        '╮' => (Pos2::new(cx - r, cy + r), 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI),
        '╯' => (Pos2::new(cx - r, cy - r), 0.0, 0.5 * std::f32::consts::PI),
        _ => (Pos2::new(cx + r, cy - r), 0.5 * std::f32::consts::PI, std::f32::consts::PI), // ╰
    };
    let mut points = Vec::with_capacity(9);
    for i in 0..=8 {
        let a = a0 + (a1 - a0) * (i as f32 / 8.0);
        points.push(Pos2::new(center.x + r * a.cos(), center.y + r * a.sin()));
    }
    let first = points[0];
    let last = points[8];
    // Straight arms from the arc ends out to the edges.
    let (first_edge, last_edge) = match c {
        '╭' => (Pos2::new(cx, y + h), Pos2::new(x + w, cy)),
        '╮' => (Pos2::new(cx, y + h), Pos2::new(x, cy)),
        '╯' => (Pos2::new(x, cy), Pos2::new(cx, y)),
        _ => (Pos2::new(x + w, cy), Pos2::new(cx, y)),
    };
    shapes.push(Shape::line_segment([first_edge, first], stroke));
    shapes.push(Shape::line_segment([last, last_edge], stroke));
    shapes.push(Shape::line(points, stroke));
}

#[allow(clippy::too_many_arguments)]
fn paint_row_box_drawing(
    shapes: &mut Vec<Shape>,
    origin_x: f32,
    y: f32,
    cell_w: f32,
    cell_h: f32,
    row_h: f32,
    cells: &[&VisibleCell],
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    block_cursor: Option<Point>,
    ppp: f32,
) {
    for cell in cells {
        let c = cell.c;
        if !is_box_geometry(c) {
            continue;
        }
        let (fg, _bg, width_cells) = cell_colors(cell, selection, colors, block_cursor);
        let x = ui_theme::term_col_x(origin_x, cell.col, ppp);
        let x_end =
            ui_theme::term_col_x(origin_x, cell.col.saturating_add(width_cells as u16), ppp);
        let w = (x_end - x).max(cell_w * width_cells);
        let cx = x + w * 0.5;
        // Centre on whole physical pixels so the two arms share one clean edge.
        let cy = ui_theme::snap(y + row_h * 0.5, ppp);
        if paint_block(shapes, c, x, y, w, row_h, fg, ppp) {
            continue;
        }
        if is_arc(c) {
            paint_arc(
                shapes,
                c,
                x,
                y,
                w,
                row_h,
                Stroke::new(Weight::Light.px(cell_w, cell_h, ppp), fg),
            );
            continue;
        }
        let Some(arms) = box_arms(c) else {
            continue;
        };
        let vertical = |shapes: &mut Vec<Shape>, weight: Weight, y0: f32, y1: f32| {
            let (x0, x1) = px_band(cx, weight.px(cell_w, cell_h, ppp), ppp);
            push_solid_rect(
                shapes,
                Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                fg,
            );
        };
        let horizontal = |shapes: &mut Vec<Shape>, weight: Weight, x0: f32, x1: f32| {
            let (y0, y1) = px_band(cy, weight.px(cell_w, cell_h, ppp), ppp);
            push_solid_rect(
                shapes,
                Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                fg,
            );
        };
        // Full row spans: row N's bottom is exactly row N+1's top, so a vertical
        // bar stays solid instead of showing the line-cap anti-aliasing seams.
        if let Some(weight) = arms.up {
            vertical(shapes, weight, y, cy);
        }
        if let Some(weight) = arms.down {
            vertical(shapes, weight, cy, y + row_h);
        }
        if let Some(weight) = arms.left {
            horizontal(shapes, weight, x, cx);
        }
        if let Some(weight) = arms.right {
            horizontal(shapes, weight, cx, x + w);
        }
    }
}

fn cell_colors(
    cell: &VisibleCell,
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    block_cursor: Option<Point>,
) -> (Color32, Color32, f32) {
    let wide = cell.flags.contains(Flags::WIDE_CHAR);
    let inverse = cell.flags.contains(Flags::INVERSE);
    // DIM_BOLD is DIM|BOLD, so `intersects` would dim every bold cell: test DIM alone.
    let dim = cell.flags.contains(Flags::DIM);
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

    // Under a filled block cursor the glyph takes the cell's background colour.
    if block_cursor == Some(point) {
        fg = bg;
    }

    (fg, bg, width_cells)
}

#[allow(clippy::too_many_arguments)]
fn paint_row_backgrounds(
    shapes: &mut Vec<Shape>,
    origin_x: f32,
    y: f32,
    row_h: f32,
    cells: &[&VisibleCell],
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    ppp: f32,
) {
    let mut sorted: Vec<&VisibleCell> = cells.to_vec();
    sorted.sort_by_key(|c| c.col);

    let mut run_bg: Option<Color32> = None;
    let mut run_start_x = 0.0;
    let mut run_end_x = 0.0;

    // Whole-pixel mesh rects: edges come from the snapped column/row grid, so
    // neighbours share them exactly (no hairline, no bleed into the next cell).
    let flush = |shapes: &mut Vec<Shape>, bg: Color32, x0: f32, x1: f32| {
        if bg != colors.background && x1 > x0 {
            let (x0, x1) = px_span(x0, x1, ppp);
            let (y0, y1) = px_span(y, y + row_h, ppp);
            push_solid_rect(
                shapes,
                Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                bg,
            );
        }
    };

    for cell in &sorted {
        let (_fg, bg, width_cells) = cell_colors(cell, selection, colors, None);
        let x = ui_theme::term_col_x(origin_x, cell.col, ppp);
        let x_end = ui_theme::term_col_x(origin_x, cell.col.saturating_add(width_cells as u16), ppp);
        let w = x_end - x;

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
    cells: &[&VisibleCell],
    selection: Option<SelectionRange>,
    colors: &super::theme::TermColors,
    font_id: &FontId,
    block_cursor: Option<Point>,
    ppp: f32,
) {
    let mut sorted: Vec<&VisibleCell> = cells
        .iter()
        .copied()
        .filter(|c| {
            c.c != ' '
                && c.c != '\t'
                && !c.flags.contains(Flags::WIDE_CHAR_SPACER)
                && !is_box_geometry(c.c)
        })
        .collect();
    sorted.sort_by_key(|c| c.col);

    if sorted.is_empty() {
        return;
    }

    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let (fg0, _bg, _) = cell_colors(start, selection, colors, block_cursor);
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
            let (fg, _, width_cells) = cell_colors(cell, selection, colors, block_cursor);
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

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor, Rgb};
    use crate::terminal::backend::Snapshot;
    use crate::terminal::theme::TermColors;

    fn cell(c: char) -> VisibleCell {
        VisibleCell {
            line: 0,
            col: 0,
            c,
            fg: TermColor::Spec(Rgb {
                r: 118,
                g: 153,
                b: 254,
            }),
            bg: TermColor::Named(NamedColor::Background),
            flags: Flags::empty(),
        }
    }

    #[test]
    fn truecolor_is_rendered_as_sent_and_dim_only_on_request() {
        let colors = TermColors::default();
        let mut c = cell('┃');
        let (fg, bg, _) = cell_colors(&c, None, &colors, None);
        assert_eq!(fg, Color32::from_rgb(118, 153, 254), "SGR 38;2 untouched");
        assert_eq!(bg, colors.background);
        c.bg = TermColor::Spec(Rgb { r: 8, g: 8, b: 8 });
        assert_eq!(cell_colors(&c, None, &colors, None).1, Color32::from_rgb(8, 8, 8));
        c.flags = Flags::DIM;
        let dimmed = cell_colors(&c, None, &colors, None).0;
        assert_ne!(dimmed, Color32::from_rgb(118, 153, 254), "SGR 2 dims");
        c.flags = Flags::BOLD;
        assert_eq!(
            cell_colors(&c, None, &colors, None).0,
            Color32::from_rgb(118, 153, 254),
            "bold never brightens or remaps truecolor"
        );
    }

    fn snapshot_with_cursor(shape: CursorShape, color: Option<Rgb>) -> Snapshot {
        Snapshot {
            cursor: Point::new(Line(2), Column(5)),
            cursor_shape: shape,
            cursor_color: color,
            ..Snapshot::default()
        }
    }

    #[test]
    fn cursor_follows_decscusr_osc12_and_dectcem() {
        let colors = TermColors::default();
        let cyan = Rgb { r: 0x56, g: 0xd4, b: 0xdd };
        let c = host_cursor(&snapshot_with_cursor(CursorShape::Block, Some(cyan)), &colors, true)
            .unwrap();
        assert_eq!(c.shape, CursorShape::Block);
        assert_eq!(c.color, Color32::from_rgb(0x56, 0xd4, 0xdd));
        assert_eq!(c.point, Point::new(Line(2), Column(5)));

        let unset = host_cursor(&snapshot_with_cursor(CursorShape::Beam, None), &colors, true);
        assert_eq!(unset.unwrap().color, colors.cursor, "no OSC 12 → theme cursor");
        let blurred = host_cursor(&snapshot_with_cursor(CursorShape::Block, None), &colors, false);
        assert_eq!(blurred.unwrap().shape, CursorShape::HollowBlock);
        assert!(
            host_cursor(&snapshot_with_cursor(CursorShape::Hidden, None), &colors, true).is_none(),
            "DECTCEM off"
        );
    }

    #[test]
    fn block_cursor_fills_the_cell_and_inverts_its_glyph() {
        let ppp = 1.25;
        let cell_rect = Rect::from_min_max(Pos2::new(8.8, 17.6), Pos2::new(17.6, 35.2));
        let mut shapes = Vec::new();
        paint_cursor(&mut shapes, cell_rect, CursorShape::Block, Color32::RED, ppp);
        assert_eq!(shapes[0].visual_bounding_rect(), cell_rect);

        let mut shapes = Vec::new();
        paint_cursor(&mut shapes, cell_rect, CursorShape::Underline, Color32::RED, ppp);
        let underline = shapes[0].visual_bounding_rect();
        assert!((underline.max.y - cell_rect.max.y).abs() < 1e-4);
        assert!((underline.width() - cell_rect.width()).abs() < 1e-4);

        let glyph = cell('x');
        let at = Point::new(Line(0), Column(0));
        let (fg, bg, _) = cell_colors(&glyph, None, &TermColors::default(), Some(at));
        assert_eq!(fg, bg, "glyph under a block cursor uses the cell background");
    }

    #[test]
    fn heavy_vertical_fills_the_snapped_row_without_seams() {
        let cell = cell('┃');
        let ppp = 1.5;
        let mut shapes = Vec::new();
        paint_row_box_drawing(
            &mut shapes,
            0.0,
            0.0,
            CELL_W,
            CELL_H,
            18.0,
            &[&cell],
            None,
            &TermColors::default(),
            None,
            ppp,
        );
        assert_eq!(shapes.len(), 2, "up and down arms");
        let top = shapes[0].visual_bounding_rect();
        let bottom = shapes[1].visual_bounding_rect();
        assert!((top.min.y - 0.0).abs() < 1e-4);
        assert!((top.max.y - bottom.min.y).abs() < 1e-4, "shared row edge");
        assert!((bottom.max.y - 18.0).abs() < 1e-4);
        for v in [top.min.x, top.max.x, top.min.y, top.max.y] {
            assert!(((v * ppp) - (v * ppp).round()).abs() < 1e-4, "whole pixel");
        }
    }

    #[test]
    fn heavy_bar_keeps_one_device_width_in_every_column() {
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            let expected = Weight::Heavy.px(CELL_W, CELL_H, ppp) * ppp;
            assert_eq!(expected, expected.round(), "whole device pixels");
            for col in 0..40u16 {
                let mut cell = cell('┃');
                cell.col = col;
                let mut shapes = Vec::new();
                paint_row_box_drawing(
                    &mut shapes,
                    3.3,
                    0.0,
                    CELL_W,
                    CELL_H,
                    18.0,
                    &[&cell],
                    None,
                    &TermColors::default(),
                    None,
                    ppp,
                );
                let width = shapes[0].visual_bounding_rect().width() * ppp;
                assert!((width - expected).abs() < 1e-3, "col {col} ppp {ppp}: {width}");
            }
        }
    }

    #[test]
    fn backgrounds_tile_rows_and_columns_without_overlap() {
        let ppp = 1.25;
        let bg = TermColor::Spec(Rgb { r: 30, g: 30, b: 40 });
        let mut a = cell(' ');
        a.bg = bg;
        let mut b = a.clone();
        b.col = 1;
        b.bg = TermColor::Spec(Rgb { r: 50, g: 30, b: 40 });
        let rect_of = |row: i32, cells: &[&VisibleCell]| {
            let y = ui_theme::term_row_top(0.0, row, ppp);
            let h = ui_theme::term_row_top(0.0, row + 1, ppp) - y;
            let mut shapes = Vec::new();
            paint_row_backgrounds(
                &mut shapes,
                0.0,
                y,
                h,
                cells,
                None,
                &TermColors::default(),
                ppp,
            );
            shapes.iter().map(|s| s.visual_bounding_rect()).collect::<Vec<_>>()
        };
        let row0 = rect_of(0, &[&a, &b]);
        let row1 = rect_of(1, &[&a]);
        assert_eq!(row0.len(), 2);
        assert!((row0[0].max.x - row0[1].min.x).abs() < 1e-4, "columns share an edge");
        assert!((row0[0].max.y - row1[0].min.y).abs() < 1e-4, "rows share an edge");
    }

    #[test]
    fn heavy_is_thicker_and_blocks_keep_their_fraction() {
        let (w, h) = (8.0, 16.0);
        assert!(Weight::Heavy.px(w, h, 1.0) > Weight::Light.px(w, h, 1.0));
        assert_eq!(box_arms('│').unwrap().up, Some(Weight::Light));
        assert_eq!(box_arms('┃').unwrap().up, Some(Weight::Heavy));
        let mut shapes = Vec::new();
        assert!(paint_block(
            &mut shapes,
            '▌',
            0.0,
            0.0,
            w,
            h,
            Color32::WHITE,
            1.0
        ));
        let rect = shapes[0].visual_bounding_rect();
        assert!((rect.width() - w * 0.5).abs() < 0.01);
        assert!(is_box_geometry('▎') && is_box_geometry('╹'));
        assert!(!is_box_geometry('╱')); // diagonals keep the font glyph
    }

    #[test]
    fn block_rects_share_whole_pixel_edges() {
        let ppp = 1.5;
        let mut shapes = Vec::new();
        assert!(paint_block(
            &mut shapes,
            '█',
            0.0,
            0.0,
            CELL_W,
            CELL_H,
            Color32::WHITE,
            ppp
        ));
        assert!(paint_block(
            &mut shapes,
            '█',
            CELL_W,
            0.0,
            CELL_W,
            CELL_H,
            Color32::WHITE,
            ppp
        ));
        let left = shapes[0].visual_bounding_rect();
        let right = shapes[1].visual_bounding_rect();
        assert!((left.max.x - right.min.x).abs() < 1e-4);
        for v in [left.min.x, left.max.x, left.min.y, left.max.y] {
            assert!(((v * ppp) - (v * ppp).round()).abs() < 1e-4, "whole pixel");
        }

        let mut half = Vec::new();
        assert!(paint_block(
            &mut half,
            '▀',
            0.0,
            0.0,
            CELL_W,
            CELL_H,
            Color32::WHITE,
            ppp
        ));
        let top = half[0].visual_bounding_rect();
        assert!(((top.max.y * ppp) - (top.max.y * ppp).round()).abs() < 1e-4);
    }
}
