use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use egui::Color32;

/// Warp-like dark palette with a single steel-blue accent.
#[derive(Clone, Debug)]
pub struct TermColors {
    pub background: Color32,
    pub foreground: Color32,
    pub cursor: Color32,
    pub accent: Color32,
    pub ansi: [Color32; 16],
}

impl Default for TermColors {
    fn default() -> Self {
        Self {
            background: Color32::from_rgb(0x12, 0x12, 0x14),
            foreground: Color32::from_rgb(0xE0, 0xE0, 0xE0),
            cursor: Color32::from_rgb(0x3A, 0x7C, 0xA5),
            accent: Color32::from_rgb(0x3A, 0x7C, 0xA5),
            ansi: [
                Color32::from_rgb(0x1C, 0x1C, 0x1E), // black
                Color32::from_rgb(0xC7, 0x4E, 0x4E), // red
                Color32::from_rgb(0x6A, 0xA8, 0x4F), // green
                Color32::from_rgb(0xC4, 0xA0, 0x35), // yellow
                Color32::from_rgb(0x3A, 0x7C, 0xA5), // blue (accent)
                Color32::from_rgb(0x9B, 0x6B, 0x9E), // magenta
                Color32::from_rgb(0x4E, 0xA8, 0xA4), // cyan
                Color32::from_rgb(0xD0, 0xD0, 0xD0), // white
                Color32::from_rgb(0x5A, 0x5A, 0x5E), // bright black
                Color32::from_rgb(0xE0, 0x6C, 0x6C), // bright red
                Color32::from_rgb(0x8B, 0xC4, 0x6F), // bright green
                Color32::from_rgb(0xE0, 0xC0, 0x55), // bright yellow
                Color32::from_rgb(0x5A, 0x9C, 0xC5), // bright blue
                Color32::from_rgb(0xB8, 0x88, 0xBB), // bright magenta
                Color32::from_rgb(0x6E, 0xC8, 0xC4), // bright cyan
                Color32::from_rgb(0xF0, 0xF0, 0xF0), // bright white
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
                    // 256-color cube / grayscale approximation
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

fn indexed_256(index: u8) -> Color32 {
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
