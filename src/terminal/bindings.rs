use alacritty_terminal::term::TermMode;
use egui::{Key, Modifiers};

#[derive(Clone, Debug)]
pub enum KeyAction {
    Bytes(Vec<u8>),
    Copy,
    Paste,
    None,
}

/// Map egui key events to PTY bytes (Alacritty-compatible sequences).
pub fn map_key(key: Key, modifiers: Modifiers, mode: TermMode, pressed: bool) -> KeyAction {
    if !pressed {
        return KeyAction::None;
    }

    // Copy / Paste (Windows: Ctrl+Shift+C/V)
    if modifiers.command && modifiers.shift && key == Key::C {
        return KeyAction::Copy;
    }
    if modifiers.command && modifiers.shift && key == Key::V {
        return KeyAction::Paste;
    }

    if modifiers.ctrl && !modifiers.alt && !modifiers.shift {
        if let Some(b) = ctrl_char(key) {
            return KeyAction::Bytes(vec![b]);
        }
    }

    let app = mode.contains(TermMode::APP_CURSOR);

    let esc = |s: &str| KeyAction::Bytes(s.as_bytes().to_vec());
    let ch = |c: char| KeyAction::Bytes(c.to_string().into_bytes());

    match (key, modifiers.shift, modifiers.alt, modifiers.ctrl) {
        (Key::Enter, _, _, _) => ch('\r'),
        (Key::Backspace, _, false, _) => ch('\x7f'),
        (Key::Backspace, _, true, _) => esc("\x1b\x7f"),
        (Key::Tab, false, _, _) => ch('\t'),
        (Key::Tab, true, _, _) => esc("\x1b[Z"),
        (Key::Escape, _, _, _) => ch('\x1b'),
        (Key::Insert, _, _, _) => esc("\x1b[2~"),
        (Key::Delete, _, _, false) => esc("\x1b[3~"),
        (Key::Delete, _, _, true) => esc("\x1b[3;5~"),
        (Key::PageUp, _, _, _) => esc("\x1b[5~"),
        (Key::PageDown, _, _, _) => esc("\x1b[6~"),
        (Key::Home, _, _, false) if app => esc("\x1bOH"),
        (Key::Home, _, _, false) => esc("\x1b[H"),
        (Key::End, _, _, false) if app => esc("\x1bOF"),
        (Key::End, _, _, false) => esc("\x1b[F"),
        (Key::ArrowUp, false, false, false) if app => esc("\x1bOA"),
        (Key::ArrowUp, false, false, false) => esc("\x1b[A"),
        (Key::ArrowDown, false, false, false) if app => esc("\x1bOB"),
        (Key::ArrowDown, false, false, false) => esc("\x1b[B"),
        (Key::ArrowRight, false, false, false) if app => esc("\x1bOC"),
        (Key::ArrowRight, false, false, false) => esc("\x1b[C"),
        (Key::ArrowLeft, false, false, false) if app => esc("\x1bOD"),
        (Key::ArrowLeft, false, false, false) => esc("\x1b[D"),
        (Key::ArrowUp, s, a, c) => arrow("A", s, a, c),
        (Key::ArrowDown, s, a, c) => arrow("B", s, a, c),
        (Key::ArrowRight, s, a, c) => arrow("C", s, a, c),
        (Key::ArrowLeft, s, a, c) => arrow("D", s, a, c),
        (Key::F1, _, _, _) => esc("\x1bOP"),
        (Key::F2, _, _, _) => esc("\x1bOQ"),
        (Key::F3, _, _, _) => esc("\x1bOR"),
        (Key::F4, _, _, _) => esc("\x1bOS"),
        (Key::F5, _, _, _) => esc("\x1b[15~"),
        (Key::F6, _, _, _) => esc("\x1b[17~"),
        (Key::F7, _, _, _) => esc("\x1b[18~"),
        (Key::F8, _, _, _) => esc("\x1b[19~"),
        (Key::F9, _, _, _) => esc("\x1b[20~"),
        (Key::F10, _, _, _) => esc("\x1b[21~"),
        (Key::F11, _, _, _) => esc("\x1b[23~"),
        (Key::F12, _, _, _) => esc("\x1b[24~"),
        _ => KeyAction::None,
    }
}

fn arrow(letter: &str, shift: bool, alt: bool, ctrl: bool) -> KeyAction {
    let mut mod_n = 1;
    if shift {
        mod_n += 1;
    }
    if alt {
        mod_n += 2;
    }
    if ctrl {
        mod_n += 4;
    }
    if mod_n == 1 {
        KeyAction::Bytes(format!("\x1b[{letter}").into_bytes())
    } else {
        KeyAction::Bytes(format!("\x1b[1;{mod_n}{letter}").into_bytes())
    }
}

fn ctrl_char(key: Key) -> Option<u8> {
    let c = match key {
        Key::A => 0x01,
        Key::B => 0x02,
        Key::C => 0x03,
        Key::D => 0x04,
        Key::E => 0x05,
        Key::F => 0x06,
        Key::G => 0x07,
        Key::H => 0x08,
        Key::I => 0x09,
        Key::J => 0x0a,
        Key::K => 0x0b,
        Key::L => 0x0c,
        Key::M => 0x0d,
        Key::N => 0x0e,
        Key::O => 0x0f,
        Key::P => 0x10,
        Key::Q => 0x11,
        Key::R => 0x12,
        Key::S => 0x13,
        Key::T => 0x14,
        Key::U => 0x15,
        Key::V => 0x16,
        Key::W => 0x17,
        Key::X => 0x18,
        Key::Y => 0x19,
        Key::Z => 0x1a,
        Key::OpenBracket => 0x1b,
        Key::Backslash => 0x1c,
        Key::CloseBracket => 0x1d,
        _ => return None,
    };
    Some(c)
}
