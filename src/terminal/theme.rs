use crate::ui_theme;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use egui::Color32;

/// Dark palette in the spirit of Warp's default (picked by eye): saturated hues that hold up on
/// black, a visible grey for ANSI black, blue accent cursor.
#[derive(Clone, Debug)]
pub struct TermColors {
    pub background: Color32,
    pub foreground: Color32,
    pub cursor: Color32,
    pub ansi: [Color32; 16],
}

impl Default for TermColors {
    fn default() -> Self {
        Self {
            background: ui_theme::BG,
            foreground: ui_theme::TERM_FG,
            cursor: ui_theme::CURSOR,
            ansi: [
                Color32::from_rgb(0x41, 0x41, 0x41), // black
                Color32::from_rgb(0xF3, 0x65, 0x5F), // red
                Color32::from_rgb(0x4F, 0xCF, 0x7A), // green
                Color32::from_rgb(0xF2, 0xC1, 0x4E), // yellow
                Color32::from_rgb(0x4E, 0x9B, 0xFF), // blue
                Color32::from_rgb(0xC6, 0x78, 0xDD), // magenta
                Color32::from_rgb(0x3F, 0xC8, 0xD6), // cyan
                Color32::from_rgb(0xD4, 0xD4, 0xD4), // white
                Color32::from_rgb(0x6B, 0x6B, 0x6B), // bright black
                Color32::from_rgb(0xFF, 0x7B, 0x72), // bright red
                Color32::from_rgb(0x6F, 0xE3, 0x97), // bright green
                Color32::from_rgb(0xFF, 0xD9, 0x6A), // bright yellow
                Color32::from_rgb(0x79, 0xB4, 0xFF), // bright blue
                Color32::from_rgb(0xDA, 0x9B, 0xF0), // bright magenta
                Color32::from_rgb(0x66, 0xDC, 0xE8), // bright cyan
                Color32::from_rgb(0xF5, 0xF5, 0xF5), // bright white
            ],
        }
    }
}

impl TermColors {
    pub fn resolve(&self, color: Color) -> Color32 {
        match color {
            Color::Named(named) => self.named(named),
            Color::Spec(rgb) => rgb_to_color32(rgb),
            Color::Indexed(index) => {
                if (index as usize) < self.ansi.len() {
                    self.ansi[index as usize]
                } else {
                    indexed_256(index)
                }
            }
        }
    }

    fn named(&self, named: NamedColor) -> Color32 {
        match named {
            NamedColor::Foreground => self.foreground,
            NamedColor::Background => self.background,
            NamedColor::Cursor => self.cursor,
            NamedColor::Black => self.ansi[0],
            NamedColor::Red => self.ansi[1],
            NamedColor::Green => self.ansi[2],
            NamedColor::Yellow => self.ansi[3],
            NamedColor::Blue => self.ansi[4],
            NamedColor::Magenta => self.ansi[5],
            NamedColor::Cyan => self.ansi[6],
            NamedColor::White => self.ansi[7],
            NamedColor::BrightBlack => self.ansi[8],
            NamedColor::BrightRed => self.ansi[9],
            NamedColor::BrightGreen => self.ansi[10],
            NamedColor::BrightYellow => self.ansi[11],
            NamedColor::BrightBlue => self.ansi[12],
            NamedColor::BrightMagenta => self.ansi[13],
            NamedColor::BrightCyan => self.ansi[14],
            NamedColor::BrightWhite => self.ansi[15],
            NamedColor::DimForeground => self.foreground.linear_multiply(0.7),
            NamedColor::DimBlack => self.ansi[0].linear_multiply(0.7),
            NamedColor::DimRed => self.ansi[1].linear_multiply(0.7),
            NamedColor::DimGreen => self.ansi[2].linear_multiply(0.7),
            NamedColor::DimYellow => self.ansi[3].linear_multiply(0.7),
            NamedColor::DimBlue => self.ansi[4].linear_multiply(0.7),
            NamedColor::DimMagenta => self.ansi[5].linear_multiply(0.7),
            NamedColor::DimCyan => self.ansi[6].linear_multiply(0.7),
            NamedColor::DimWhite => self.ansi[7].linear_multiply(0.7),
            _ => self.foreground,
        }
    }
}

fn rgb_to_color32(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.r, rgb.g, rgb.b)
}

pub(crate) fn indexed_256(index: u8) -> Color32 {
    match index {
        0..=15 => TermColors::default().ansi[index as usize],
        16..=231 => {
            let i = index - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(level(r), level(g), level(b))
        }
        _ => {
            let gray = 8 + 10 * (index as u16 - 232) as u8;
            Color32::from_rgb(gray, gray, gray)
        }
    }
}
