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
            row_h,
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Weight {
    Light,
    Heavy,
}

impl Weight {
    /// Light strokes are ~1/8 of the cell; heavy ones are clearly double.
    fn px(self, cell_w: f32, cell_h: f32) -> f32 {
        let light = (cell_w.min(cell_h) / 8.0).round().max(1.0);
        match self {
            Self::Light => light,
            Self::Heavy => (light * 2.0).max(light + 1.0),
        }
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
    ppp: f32,
) {
    for cell in cells {
        let c = cell.c;
        if !is_box_geometry(c) {
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
                Stroke::new(Weight::Light.px(cell_w, cell_h), fg),
            );
            continue;
        }
        let Some(arms) = box_arms(c) else {
            continue;
        };
        let vertical = |shapes: &mut Vec<Shape>, weight: Weight, y0: f32, y1: f32| {
            let half = weight.px(cell_w, cell_h) * 0.5;
            let (x0, x1) = px_span(cx - half, cx + half, ppp);
            push_solid_rect(
                shapes,
                Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                fg,
            );
        };
        let horizontal = |shapes: &mut Vec<Shape>, weight: Weight, x0: f32, x1: f32| {
            let half = weight.px(cell_w, cell_h) * 0.5;
            let (y0, y1) = px_span(cy - half, cy + half, ppp);
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

    // Whole-pixel mesh rects that overlap by 1 physical px: no anti-aliased
    // hairline between rows or between adjacent runs of the same colour.
    let overlap = 1.0 / ppp;
    let flush = |shapes: &mut Vec<Shape>, bg: Color32, x0: f32, x1: f32| {
        if bg != colors.background && x1 > x0 {
            let (x0, x1) = px_span(x0, x1 + overlap, ppp);
            let (y0, y1) = px_span(y, y + cell_h + overlap, ppp);
            push_solid_rect(
                shapes,
                Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)),
                bg,
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor, Rgb};
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
    fn heavy_is_thicker_and_blocks_keep_their_fraction() {
        let (w, h) = (8.0, 16.0);
        assert!(Weight::Heavy.px(w, h) > Weight::Light.px(w, h));
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
