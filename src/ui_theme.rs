//! Midterm UI tokens. Some aliases are wired in later steps.
#![allow(dead_code)]

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText,
    Shadow, Stroke, Style, TextStyle, Theme, ThemePreference, Visuals,
};
use std::collections::BTreeMap;
use std::sync::Arc;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicBool, Ordering};

// --- Surfaces (midterm global tokens) ---
pub const BG: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
/// Sidebar, inactive tabs, tab bar (`sidebar_bg` / `tabbar_bg`).
pub const SURFACE_1: Color32 = Color32::from_rgb(0x0C, 0x0C, 0x0C);
pub const SIDEBAR_BG: Color32 = SURFACE_1;
pub const SURFACE_SUNKEN: Color32 = Color32::from_rgb(0x08, 0x08, 0x08);
pub const INPUT_BG: Color32 = SURFACE_SUNKEN;
pub const SURFACE_2: Color32 = Color32::from_rgb(0x18, 0x18, 0x18);
pub const BUTTON_BG: Color32 = SURFACE_2;
pub const SURFACE_SEL: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);
pub const SEGMENT_ACTIVE: Color32 = SURFACE_SEL;
pub const ROW_HOVER: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
pub const ROW_SELECTED: Color32 = BUTTON_BG;
pub const HOVER: Color32 = ROW_HOVER;
pub const PRESSED: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1A);

// --- Lines ---
pub const BORDER: Color32 = Color32::from_rgb(0x1C, 0x1C, 0x1C);
pub const SIDEBAR_BORDER: Color32 = BORDER;
pub const INPUT_BORDER: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
pub const DIVIDER: Color32 = Color32::from_rgb(0x19, 0x19, 0x19);
pub const SIDEBAR_DIVIDER: Color32 = DIVIDER;
pub const DIVIDER_ACTIVE: Color32 = Color32::from_gray(0x3A);
pub const BUTTON_HIGHLIGHT: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
pub const KBD_BG: Color32 = INPUT_BORDER;
pub const KBD_BORDER: Color32 = BUTTON_HIGHLIGHT;

// --- Text ---
pub const TEXT: Color32 = Color32::from_rgb(0xE7, 0xE7, 0xE7);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x83, 0x83, 0x83);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x5A, 0x5A, 0x5A);
pub const TAB_INACTIVE_TEXT: Color32 = TEXT_MUTED;

// --- Status + accent ---
pub const OK: Color32 = Color32::from_rgb(0x6F, 0xA7, 0x7D);
pub const STATUS_DOT_OK: Color32 = Color32::from_rgb(0x53, 0xBE, 0x6B);
pub const WAITING: Color32 = Color32::from_rgb(0xE9, 0xB6, 0x32);
pub const WARN: Color32 = WAITING;
pub const FILTER_WAITING_DOT: Color32 = Color32::from_rgb(0x45, 0x45, 0x45);
pub const WORKING_RING: Color32 = Color32::from_rgb(0x85, 0x85, 0x85);
pub const WORKING_RING_DIM: Color32 = Color32::from_rgb(0x90, 0x90, 0x90);
pub const LAUNCHER_BADGE: Color32 = Color32::from_rgb(0x65, 0xB1, 0x77);
pub const ERROR: Color32 = Color32::from_rgb(0xE5, 0x69, 0x5B);
pub const ACCENT: Color32 = Color32::from_rgb(0x0F, 0x8C, 0xEC);
pub const CURSOR: Color32 = ACCENT;

/// UI type scale (logical px). Only these two sizes for chrome text.
pub const SIZE_UI: f32 = 13.0;
pub const SIZE_UI_SMALL: f32 = 12.0;
/// Aliases kept for call sites; same values as midterm `ui` / `ui_small`.
pub const SIZE_TITLE: f32 = SIZE_UI;
pub const SIZE_SUB: f32 = SIZE_UI_SMALL;
pub const SIZE_KBD: f32 = SIZE_UI_SMALL;

/// Terminal (Hack) size and fixed midterm cell metrics (logical px).
pub const TERM_FONT_SIZE: f32 = 14.33;
pub const CELL_W: f32 = 8.627;
pub const CELL_H: f32 = 17.733;
/// Padding inside each pane before col 0 / row 0.
pub const PANE_PAD_X: f32 = 14.0;
pub const PANE_PAD_Y: f32 = 15.33;

/// Round a logical coordinate onto a physical pixel boundary.
#[inline]
pub fn snap(v: f32, ppp: f32) -> f32 {
    if ppp <= 0.0 {
        return v;
    }
    (v * ppp).round() / ppp
}

/// Row top for terminal row `n` (0-based), given the pane's content top (after pad).
#[inline]
pub fn term_row_top(content_top: f32, row: i32, ppp: f32) -> f32 {
    snap(content_top + row as f32 * CELL_H, ppp)
}

/// Glyph left edge for column `col` (0-based).
#[inline]
pub fn term_col_x(content_left: f32, col: u16, ppp: f32) -> f32 {
    snap(content_left + col as f32 * CELL_W, ppp)
}

pub const RADIUS_CTRL: u8 = 8;
pub const RADIUS_ROW: u8 = 6;
pub const RADIUS_POPUP: u8 = 10;
pub const RADIUS_KBD: u8 = 4;

pub const FONT_MEDIUM: &str = "medium";
pub const FONT_ICONS: &str = "icons";

/// Lucide codepoints (from lucide-static `codepoints.json`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Activity,
    Archive,
    ArchiveRestore,
    Bot,
    ChartColumn,
    ChevronDown,
    ChevronRight,
    Circle,
    Copy,
    File,
    Folder,
    FolderPlus,
    LoaderCircle,
    Minus,
    PanelLeft,
    Pin,
    PinOff,
    Plus,
    Search,
    Sparkles,
    Square,
    SquarePen,
    Terminal,
    Trash2,
    X,
}

impl Icon {
    pub const fn codepoint(self) -> char {
        match self {
            Self::Activity => '\u{E038}',
            Self::Archive => '\u{E041}',
            Self::ArchiveRestore => '\u{E2CD}',
            Self::Bot => '\u{E1BB}',
            Self::ChartColumn => '\u{E2A3}',
            Self::ChevronDown => '\u{E06D}',
            Self::ChevronRight => '\u{E06F}',
            Self::Circle => '\u{E076}',
            Self::Copy => '\u{E09E}',
            Self::File => '\u{E0C0}',
            Self::Folder => '\u{E0D7}',
            Self::FolderPlus => '\u{E0D9}',
            Self::LoaderCircle => '\u{E10A}',
            Self::Minus => '\u{E11C}',
            Self::PanelLeft => '\u{E12A}',
            Self::Pin => '\u{E259}',
            Self::PinOff => '\u{E2B6}',
            Self::Plus => '\u{E13D}',
            Self::Search => '\u{E151}',
            Self::Sparkles => '\u{E412}',
            Self::Square => '\u{E167}',
            Self::SquarePen => '\u{E172}',
            Self::Terminal => '\u{E181}',
            Self::Trash2 => '\u{E18E}',
            Self::X => '\u{E1B2}',
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "activity" => Self::Activity,
            "archive" => Self::Archive,
            "archive-restore" => Self::ArchiveRestore,
            "bot" => Self::Bot,
            "chart-column" => Self::ChartColumn,
            "chevron-down" => Self::ChevronDown,
            "chevron-right" => Self::ChevronRight,
            "circle" => Self::Circle,
            "copy" => Self::Copy,
            "file" => Self::File,
            "folder" => Self::Folder,
            "folder-plus" => Self::FolderPlus,
            "loader-circle" => Self::LoaderCircle,
            "minus" => Self::Minus,
            "panel-left" => Self::PanelLeft,
            "pin" => Self::Pin,
            "pin-off" => Self::PinOff,
            "plus" => Self::Plus,
            "search" => Self::Search,
            "sparkles" => Self::Sparkles,
            "square" => Self::Square,
            "square-pen" => Self::SquarePen,
            "terminal" => Self::Terminal,
            "trash-2" => Self::Trash2,
            "x" => Self::X,
            _ => return None,
        })
    }
}

pub fn icon(kind: Icon, size: f32, color: Color32) -> RichText {
    RichText::new(kind.codepoint().to_string())
        .font(FontId::new(size, FontFamily::Name(FONT_ICONS.into())))
        .color(color)
}

pub fn text_title(s: impl Into<String>) -> RichText {
    RichText::new(s).size(SIZE_TITLE).color(TEXT)
}

pub fn text_sub(s: impl Into<String>) -> RichText {
    RichText::new(s).size(SIZE_SUB).color(TEXT_MUTED)
}

pub fn text_kbd(s: impl Into<String>) -> RichText {
    RichText::new(s).size(SIZE_KBD).color(TEXT_FAINT)
}

#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn parse_hex_color(hex: &str) -> Option<Color32> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

pub fn popup_shadow() -> Shadow {
    Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(160),
    }
}

/// Force dark theme and register Inter + icon fonts.
/// Does **not** call `set_pixels_per_point` — eframe keeps the monitor's native DPI.
pub fn apply(ctx: &Context) {
    install_fonts(ctx);
    ctx.set_theme(ThemePreference::Dark);
    ctx.options_mut(|o| {
        o.fallback_theme = Theme::Dark;
        o.sync_window_theme = true;
    });
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));

    let v = visuals();
    ctx.set_visuals_of(Theme::Dark, v.clone());
    ctx.set_visuals_of(Theme::Light, v);
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, |s| {
            apply_spacing(s);
            apply_text_styles(s);
        });
    }

    #[cfg(debug_assertions)]
    {
        static LOGGED_PPP: AtomicBool = AtomicBool::new(false);
        if !LOGGED_PPP.swap(true, Ordering::Relaxed) {
            log::debug!(
                "pixels_per_point = {:.3} (native DPI; not overridden)",
                ctx.pixels_per_point()
            );
        }
    }
}

fn apply_spacing(s: &mut Style) {
    s.spacing.item_spacing = egui::vec2(8.0, 4.0);
    s.spacing.button_padding = egui::vec2(10.0, 5.0);
    s.spacing.interact_size.y = 30.0;
    s.spacing.scroll = egui::style::ScrollStyle::floating();
}

fn apply_text_styles(s: &mut Style) {
    use FontFamily::{Monospace, Proportional};
    s.text_styles = BTreeMap::from([
        (TextStyle::Small, FontId::new(SIZE_UI_SMALL, Proportional)),
        (TextStyle::Body, FontId::new(SIZE_UI, Proportional)),
        (TextStyle::Button, FontId::new(SIZE_UI, Proportional)),
        (TextStyle::Heading, FontId::new(SIZE_UI, Proportional)),
        (TextStyle::Monospace, FontId::new(TERM_FONT_SIZE, Monospace)),
    ]);
}

fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "Inter".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Regular.subset.ttf"
        ))),
    );
    fonts.font_data.insert(
        "InterMedium".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Medium.subset.ttf"
        ))),
    );
    fonts.font_data.insert(
        "AnkarIcons".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/ankar-icons.ttf"
        ))),
    );

    // Start with Inter Medium for all UI text (egui strokes lighter than SF on macOS).
    // Keep Inter Regular + egui fallbacks after it; drop Ubuntu-Light from the chain.
    let proportional = fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default();
    proportional.retain(|name| name != "Ubuntu-Light");
    proportional.insert(0, "Inter".to_owned());
    proportional.insert(0, "InterMedium".to_owned());

    // Named Medium family for call sites that opt in explicitly (same face for now).
    fonts.families.insert(
        FontFamily::Name(FONT_MEDIUM.into()),
        vec!["InterMedium".into(), "Inter".into()],
    );
    fonts
        .families
        .insert(FontFamily::Name(FONT_ICONS.into()), vec!["AnkarIcons".into()]);

    // Monospace stays egui's embedded Hack (default_fonts feature).
    ctx.set_fonts(fonts);
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = None;
    v.panel_fill = SURFACE_1;
    v.window_fill = SURFACE_1;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = popup_shadow();
    v.window_corner_radius = CornerRadius::same(RADIUS_POPUP);
    v.menu_corner_radius = CornerRadius::same(RADIUS_POPUP);
    v.faint_bg_color = HOVER;
    v.extreme_bg_color = SURFACE_SUNKEN;
    v.text_edit_bg_color = Some(SURFACE_SUNKEN);
    v.code_bg_color = SURFACE_SUNKEN;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0, TEXT);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = SURFACE_1;
    w.noninteractive.weak_bg_fill = SURFACE_1;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);

    // Flat buttons: transparent at rest.
    w.inactive.bg_fill = Color32::TRANSPARENT;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);

    w.hovered.bg_fill = HOVER;
    w.hovered.weak_bg_fill = HOVER;
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT);

    w.active.bg_fill = PRESSED;
    w.active.weak_bg_fill = PRESSED;
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.0, TEXT);

    w.open.bg_fill = SURFACE_2;
    w.open.weak_bg_fill = SURFACE_2;
    w.open.bg_stroke = Stroke::NONE;
    w.open.fg_stroke = Stroke::new(1.0, TEXT);

    for wv in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        wv.corner_radius = CornerRadius::same(RADIUS_CTRL);
        wv.expansion = 0.0;
    }

    // Scrollbars
    v.widgets.noninteractive.bg_fill = SURFACE_1;
    v
}
