use super::backend::{CellMetrics, TerminalSession};
use super::bindings::{map_key, KeyAction};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use egui::epaint::RectShape;
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2,
};

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
    let mode = session.last_content().terminal_mode;
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
                // Bracketed paste if supported.
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
                KeyAction::Paste => {
                    // egui will emit Paste event when clipboard paste is triggered;
                    // also try reading via input events — fallback no-op here.
                }
                KeyAction::None => {}
            },
            egui::Event::MouseWheel { unit, delta, .. } => {
                let lines = match unit {
                    egui::MouseWheelUnit::Line => delta.y.round() as i32,
                    egui::MouseWheelUnit::Point => {
                        (delta.y / FONT_SIZE).round() as i32
                    }
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

    let display_offset = content.grid.display_offset() as i32;
    let cursor_point = content.grid.cursor.point;
    let app_cursor = content.terminal_mode.contains(TermMode::APP_CURSOR);

    for indexed in content.grid.display_iter() {
        let flags = indexed.cell.flags;
        if flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }

        let wide = flags.contains(Flags::WIDE_CHAR);
        let inverse = flags.contains(Flags::INVERSE);
        let dim = flags.intersects(Flags::DIM | Flags::DIM_BOLD);
        let selected = content
            .selectable_range
            .is_some_and(|r| r.contains(indexed.point));

        let x = origin.x + cell_w * indexed.point.column.0 as f32;
        let line_num = indexed.point.line.0 + display_offset;
        let y = origin.y + cell_h * line_num as f32;

        let mut fg = colors.resolve(indexed.fg);
        let mut bg = colors.resolve(indexed.bg);
        let width = if wide { cell_w * 2.0 } else { cell_w };

        if dim {
            fg = fg.linear_multiply(0.7);
        }
        if inverse || selected {
            std::mem::swap(&mut fg, &mut bg);
        }

        if bg != colors.background {
            shapes.push(Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(width + 1.0, cell_h + 1.0)),
                CornerRadius::ZERO,
                bg,
            )));
        }

        // Cursor.
        if cursor_point == indexed.point {
            shapes.push(Shape::Rect(RectShape::filled(
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h)),
                CornerRadius::ZERO,
                colors.cursor,
            )));
            if app_cursor {
                std::mem::swap(&mut fg, &mut bg);
            } else {
                fg = colors.background;
            }
        }

        if indexed.c != ' ' && indexed.c != '\t' {
            let text_shape = ui.fonts_mut(|fonts| {
                Shape::text(
                    fonts,
                    Pos2::new(x + width / 2.0, y),
                    Align2::CENTER_TOP,
                    indexed.c,
                    font_id.clone(),
                    fg,
                )
            });
            shapes.push(text_shape);
        }

        if flags.contains(Flags::UNDERLINE) {
            let y2 = y + cell_h - 1.0;
            shapes.push(Shape::LineSegment {
                points: [Pos2::new(x, y2), Pos2::new(x + width, y2)],
                stroke: Stroke::new(1.0, fg).into(),
            });
        }
    }

    painter.extend(shapes);

    // Subtle border when focused.
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
