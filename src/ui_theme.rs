//! Midterm UI tokens.

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText,
    Shadow, Stroke, Style, TextStyle, Theme, ThemePreference, Visuals,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

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
/// The one gray line: window outline, under the tab bar and at the sidebar's right edge.
pub const CHROME_DIVIDER: Color32 = Color32::from_gray(0x2E);
/// Box behind the sidebar toggle while the sidebar is open.
pub const TOGGLE_OPEN_BG: Color32 = Color32::from_rgb(0x1C, 0x1C, 0x1C);
/// Height of the full-width caption bar (tabs), divider included.
pub const BAR_H: f32 = 37.0;
pub const TAB_SEP: Color32 = Color32::from_rgb(0x15, 0x15, 0x15);
pub const INPUT_BORDER: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
pub const DIVIDER: Color32 = Color32::from_rgb(0x19, 0x19, 0x19);
pub const SIDEBAR_DIVIDER: Color32 = DIVIDER;
pub const DIVIDER_ACTIVE: Color32 = Color32::from_gray(0x3A);
pub const BUTTON_HIGHLIGHT: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
pub const KBD_BG: Color32 = INPUT_BORDER;
pub const KBD_BORDER: Color32 = BUTTON_HIGHLIGHT;
/// Footer chips and popups over them (Warp look).
pub const CHIP_BG: Color32 = Color32::from_gray(0x1A);
pub const CHIP_BORDER: Color32 = Color32::from_gray(0x33);

// --- Text ---
pub const TEXT: Color32 = Color32::from_rgb(0xE7, 0xE7, 0xE7);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x83, 0x83, 0x83);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x5A, 0x5A, 0x5A);

// --- Status + accent ---
pub const OK: Color32 = Color32::from_rgb(0x6F, 0xA7, 0x7D);
pub const STATUS_DOT_OK: Color32 = Color32::from_rgb(0x53, 0xBE, 0x6B);
pub const WAITING: Color32 = Color32::from_rgb(0xE9, 0xB6, 0x32);
pub const FILTER_WAITING_DOT: Color32 = Color32::from_rgb(0x45, 0x45, 0x45);
pub const WORKING_RING: Color32 = Color32::from_rgb(0x85, 0x85, 0x85);
pub const WORKING_RING_DIM: Color32 = Color32::from_rgb(0x90, 0x90, 0x90);
pub const LAUNCHER_BADGE: Color32 = Color32::from_rgb(0x65, 0xB1, 0x77);
pub const ERROR: Color32 = Color32::from_rgb(0xE5, 0x69, 0x5B);
pub const ACCENT: Color32 = Color32::from_rgb(0x0F, 0x8C, 0xEC);
/// Lines added / removed in diff counts.
pub const DIFF_ADD: Color32 = STATUS_DOT_OK;
pub const DIFF_DEL: Color32 = ERROR;
/// Cursor when the app set no OSC 12 colour: the Ankar accent blue.
pub const CURSOR: Color32 = Color32::from_rgb(0x76, 0x99, 0xFE);
/// Default terminal foreground (`#D4D4D4`).
pub const TERM_FG: Color32 = Color32::from_rgb(0xD4, 0xD4, 0xD4);
/// Unfocused pane dim overlay (black @ 0.4).
pub const PANE_DIM: Color32 = Color32::from_black_alpha(102);

/// UI type scale (logical px). Only these two sizes for chrome text.
pub const SIZE_UI: f32 = 12.0;
pub const SIZE_UI_SMALL: f32 = 11.0;
/// Aliases kept for call sites; same values as midterm `ui` / `ui_small`.
pub const SIZE_TITLE: f32 = SIZE_UI;
pub const SIZE_SUB: f32 = SIZE_UI_SMALL;
pub const SIZE_KBD: f32 = SIZE_UI_SMALL;

/// Size the terminal starts at, before config is applied. Hack's em.
pub const TERM_FONT_SIZE: f32 = 14.33;
/// Box-drawing tests pass these as a cell; the live grid uses [`cell_size_px`].
#[cfg(test)]
pub const CELL_W: f32 = 8.627;
#[cfg(test)]
pub const CELL_H: f32 = 17.196;
/// Default `font_size` in config.toml and the zoom limits.
pub const DEFAULT_TERM_FONT: f32 = 12.0;
pub const MIN_TERM_FONT: f32 = 8.0;
pub const MAX_TERM_FONT: f32 = 24.0;
/// Padding inside each pane before col 0 / row 0.
pub const PANE_PAD_X: f32 = 12.0;
pub const PANE_PAD_Y: f32 = 12.0;

/// Hack's hhea metrics (egui's line box; USE_TYPO_METRICS is off). The cell uses the
/// shorter OS/2 typo box via [`pick_line_box`]. Design units.
const HACK_UPEM: f32 = 2048.0;
const HACK_ADVANCE: f32 = 1233.0;
const HACK_ASCENT: f32 = 1901.0;
const HACK_DESCENT: f32 = 483.0;

/// Terminal size in use, as f32 bits. Set once per frame from the config; the row/column
/// helpers below read it so every painter and hit test agrees on the grid.
static TERM_FONT_BITS: AtomicU32 = AtomicU32::new(TERM_FONT_SIZE.to_bits());
/// Glyph advance, ascent, descent, line gap, in logical px at the current font size.
static ADVANCE_BITS: AtomicU32 = AtomicU32::new((HACK_ADVANCE / HACK_UPEM * TERM_FONT_SIZE).to_bits());
static ASCENT_BITS: AtomicU32 = AtomicU32::new((HACK_ASCENT / HACK_UPEM * TERM_FONT_SIZE).to_bits());
static DESCENT_BITS: AtomicU32 = AtomicU32::new((HACK_DESCENT / HACK_UPEM * TERM_FONT_SIZE).to_bits());
static GAP_BITS: AtomicU32 = AtomicU32::new(0.0f32.to_bits());
static PPP_BITS: AtomicU32 = AtomicU32::new(1.0f32.to_bits());
static DESIGN: OnceLock<DesignMetrics> = OnceLock::new();

pub fn set_term_font_size(size: f32) {
    TERM_FONT_BITS.store(size.clamp(MIN_TERM_FONT, MAX_TERM_FONT).to_bits(), Ordering::Relaxed);
}

pub fn term_font_size() -> f32 {
    f32::from_bits(TERM_FONT_BITS.load(Ordering::Relaxed))
}

/// Vertical metrics of one face, in font design units. `descent` is positive.
#[derive(Clone, Copy)]
struct DesignMetrics {
    units_per_em: f32,
    advance: f32,
    ascent: f32,
    descent: f32,
    line_gap: f32,
    /// egui `FontTweak::scale` (1 for Hack).
    tweak: f32,
}

/// Font line box at the size the terminal is drawing. All fields are logical px
/// (egui points). `descent` is the distance below the baseline, so it is positive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontLineMetrics {
    pub advance: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

/// Cell width and height in logical px.
///
/// `metrics` are already in points at the font size (egui does not put DPI into
/// them). `dpi` (`pixels_per_point`) only snaps the line box onto a physical
/// pixel; it is not a second scale. Width is the glyph advance, not ceiled.
pub fn cell_size_px(metrics: FontLineMetrics, dpi: f32) -> (f32, f32) {
    let dpi = if dpi.is_finite() && dpi > 0.0 { dpi } else { 1.0 };
    let width = metrics.advance;
    // Line gap is egui/font leading, not part of the cell. Do not invent 1.2 em.
    let height = ((metrics.ascent + metrics.descent) * dpi).round().max(1.0) / dpi;
    (width, height)
}

/// Galley origin relative to the row top.
///
/// The face's ascent sits on the row top. `baseline_from_galley_top` is where
/// egui put that baseline (it adds leading and bottom-aligns inside a taller
/// galley row). The difference is snapped to a physical pixel and subtracted,
/// so a taller row does not push the glyph down.
pub fn glyph_top_offset(
    _row_h: f32,
    ascent: f32,
    _descent: f32,
    baseline_from_galley_top: f32,
    ppp: f32,
) -> f32 {
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    let raw = ascent - baseline_from_galley_top;
    if raw.is_finite() { (raw * ppp).round() / ppp } else { 0.0 }
}

/// What `glyph_top_offset` was after aef1df6: baseline centred in the row.
fn centered_glyph_top_offset(
    row_h: f32,
    ascent: f32,
    descent: f32,
    baseline_from_galley_top: f32,
    ppp: f32,
) -> f32 {
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    let baseline = (row_h - (ascent + descent)) * 0.5 + ascent;
    let baseline = (baseline * ppp).round() / ppp;
    let extra = (baseline_from_galley_top - ascent).max(0.0);
    baseline - ascent - extra
}

fn load_bits(bits: &AtomicU32) -> f32 {
    f32::from_bits(bits.load(Ordering::Relaxed))
}

fn store_bits(bits: &AtomicU32, value: f32) {
    bits.store(value.to_bits(), Ordering::Relaxed);
}

pub fn font_line_metrics() -> FontLineMetrics {
    FontLineMetrics {
        advance: load_bits(&ADVANCE_BITS),
        ascent: load_bits(&ASCENT_BITS),
        descent: load_bits(&DESCENT_BITS),
        line_gap: load_bits(&GAP_BITS),
    }
}

fn pixels_per_point() -> f32 {
    load_bits(&PPP_BITS)
}

pub fn cell_w() -> f32 {
    cell_size_px(font_line_metrics(), pixels_per_point()).0
}

pub fn cell_h() -> f32 {
    cell_size_px(font_line_metrics(), pixels_per_point()).1
}

/// Read the monospace face once and store its line box at the current font size.
/// Advance comes from egui's glyph width, which is already in points.
pub fn sync_cell_metrics(ctx: &Context) {
    let ppp = ctx.pixels_per_point();
    store_bits(&PPP_BITS, if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 });
    let design = *DESIGN.get_or_init(|| read_primary_mono(ctx));
    let font_px = term_font_size();
    // One scale: font size (and the face's tweak) over units-per-em. Not pixels_per_point.
    let em = font_px * design.tweak / design.units_per_em;
    let mut metrics = FontLineMetrics {
        advance: design.advance * em,
        ascent: design.ascent * em,
        descent: design.descent * em,
        line_gap: design.line_gap * em,
    };
    // `glyph_width` is in points (the face is asked at pixels_per_point 1). Do not multiply by dpi.
    let font_id = FontId::new(font_px, FontFamily::Monospace);
    let measured = ctx.fonts_mut(|fonts| fonts.glyph_width(&font_id, '0'));
    if measured.is_finite()
        && measured > 0.0
        && (metrics.advance <= 0.0 || measured <= metrics.advance + 1.0)
    {
        metrics.advance = measured;
    }
    store_bits(&ADVANCE_BITS, metrics.advance);
    store_bits(&ASCENT_BITS, metrics.ascent);
    store_bits(&DESCENT_BITS, metrics.descent);
    store_bits(&GAP_BITS, metrics.line_gap);
}

/// One line: font, DPI, the four font metrics, the cell, and the grid.
/// `grid` is `None` at startup, before a pane has been laid out.
pub fn log_cell_metrics(grid: Option<(u16, u16)>) {
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    let m = font_line_metrics();
    let (w, h) = cell_size_px(m, pixels_per_point());
    let grid = match grid {
        Some((cols, rows)) => format!("{cols}x{rows}"),
        None => "pending".to_owned(),
    };
    log::info!(
        "cell metrics: font {:.2} px, dpi {:.3}, advance {:.3}, ascent {:.3}, descent {:.3}, line gap {:.3}, cell {:.3}x{:.3} px, grid {grid}",
        term_font_size(),
        pixels_per_point(),
        m.advance,
        m.ascent,
        m.descent,
        m.line_gap,
        w,
        h
    );
}

/// One line, the first time a terminal row is painted.
/// `before` is the centred offset from aef1df6; `after` puts the glyph top on the row top.
pub fn log_row_text_once(
    cell_h: f32,
    galley_row_h: f32,
    row_h: f32,
    ascent: f32,
    descent: f32,
    baseline_from_galley_top: f32,
    ppp: f32,
) {
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    let before = centered_glyph_top_offset(row_h, ascent, descent, baseline_from_galley_top, ppp);
    let after = glyph_top_offset(row_h, ascent, descent, baseline_from_galley_top, ppp);
    log::info!(
        "row text: cell_h {cell_h:.3}, galley row {galley_row_h:.3}, text y offset before {before:.3} after {after:.3}"
    );
}

fn hack_design() -> DesignMetrics {
    DesignMetrics {
        units_per_em: HACK_UPEM,
        advance: HACK_ADVANCE,
        ascent: HACK_ASCENT,
        descent: HACK_DESCENT,
        line_gap: 0.0,
        tweak: 1.0,
    }
}

fn read_primary_mono(ctx: &Context) -> DesignMetrics {
    ctx.fonts(|fonts| {
        let defs = fonts.definitions();
        let Some(name) = defs
            .families
            .get(&FontFamily::Monospace)
            .and_then(|names| names.first())
        else {
            return hack_design();
        };
        let Some(data) = defs.font_data.get(name) else {
            return hack_design();
        };
        let mut design = read_design(data.bytes()).unwrap_or_else(hack_design);
        let tweak = data.tweak.scale;
        if tweak.is_finite() && tweak > 0.0 {
            design.tweak = tweak;
        }
        design
    })
}

fn read_design(data: &[u8]) -> Option<DesignMetrics> {
    let head = table(data, b"head")?;
    let hhea = table(data, b"hhea")?;
    let upem = u16_at(head, 18)? as f32;
    if upem <= 0.0 {
        return None;
    }
    let (h_a, h_d, h_g) = hhea_line(hhea)?;
    let mut ascent = h_a;
    let mut descent = h_d;
    let mut gap = h_g;
    if let Some(os2) = table(data, b"OS/2") {
        if let Some((t_a, t_d, _)) = typo_line(os2) {
            if let Some((a, d)) = pick_line_box(h_a, h_d, t_a, t_d) {
                if (a, d) != (h_a, h_d) {
                    gap = 0.0;
                }
                ascent = a;
                descent = d;
            }
        }
        if ascent == 0.0 && descent == 0.0 {
            if let Some(win) = win_line(os2) {
                (ascent, descent, gap) = win;
            }
        }
    }
    let advance = glyph_advance(data, hhea, 0x30).or_else(|| u16_at(hhea, 10).map(|n| n as f32))?;
    Some(DesignMetrics {
        units_per_em: upem,
        advance,
        ascent,
        descent,
        line_gap: gap,
        tweak: 1.0,
    })
}

/// Design-unit line box (`descent` positive). Typo ascent+descent when that box is
/// positive and shorter than hhea; otherwise hhea. The line gap is not part of this.
fn pick_line_box(hhea_ascent: f32, hhea_descent: f32, typo_ascent: f32, typo_descent: f32) -> Option<(f32, f32)> {
    let typo_ok = typo_ascent > 0.0 && typo_descent > 0.0;
    let hhea_sum = hhea_ascent + hhea_descent;
    if typo_ok && (hhea_sum <= 0.0 || typo_ascent + typo_descent < hhea_sum) {
        Some((typo_ascent, typo_descent))
    } else if hhea_ascent != 0.0 || hhea_descent != 0.0 {
        Some((hhea_ascent, hhea_descent))
    } else {
        None
    }
}

/// hhea ascender / descender / lineGap. Descender is stored signed; we return a positive depth.
fn hhea_line(hhea: &[u8]) -> Option<(f32, f32, f32)> {
    Some((
        i16_at(hhea, 4)? as f32,
        (i16_at(hhea, 6)? as f32).abs(),
        i16_at(hhea, 8)? as f32,
    ))
}

fn typo_line(os2: &[u8]) -> Option<(f32, f32, f32)> {
    Some((
        i16_at(os2, 68)? as f32,
        (i16_at(os2, 70)? as f32).abs(),
        i16_at(os2, 72)? as f32,
    ))
}

fn win_line(os2: &[u8]) -> Option<(f32, f32, f32)> {
    Some((u16_at(os2, 74)? as f32, u16_at(os2, 76)? as f32, 0.0))
}

fn glyph_advance(data: &[u8], hhea: &[u8], cp: u16) -> Option<f32> {
    let cmap = table(data, b"cmap")?;
    let nenc = u16_at(cmap, 2)? as usize;
    let mut sub = None;
    for i in 0..nenc {
        let o = 4 + i * 8;
        if u16_at(cmap, o)? == 3 && u16_at(cmap, o + 2)? == 1 {
            sub = Some(u32_at(cmap, o + 4)? as usize);
        }
    }
    let off = sub?;
    if u16_at(cmap, off)? != 4 {
        return None;
    }
    let segc = u16_at(cmap, off + 6)? as usize / 2;
    let end = off + 14;
    let start = end + 2 * segc + 2;
    let delta = start + 2 * segc;
    let range_off = delta + 2 * segc;
    let mut gid = None;
    for s in 0..segc {
        let (e, st) = (u16_at(cmap, end + 2 * s)?, u16_at(cmap, start + 2 * s)?);
        if (st..=e).contains(&cp) {
            let ro = u16_at(cmap, range_off + 2 * s)? as usize;
            gid = Some(if ro == 0 {
                (cp as i32 + i16_at(cmap, delta + 2 * s)? as i32) as u16
            } else {
                u16_at(cmap, range_off + 2 * s + ro + 2 * (cp - st) as usize)?
            });
            break;
        }
    }
    let gid = gid? as usize;
    let hmtx = table(data, b"hmtx")?;
    let num = u16_at(hhea, 34)? as usize;
    let at = if gid < num {
        gid * 4
    } else if num > 0 {
        (num - 1) * 4
    } else {
        return None;
    };
    Some(u16_at(hmtx, at)? as f32)
}

fn table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let n = u16_at(data, 4)? as usize;
    if data.len() < 12 + n * 16 {
        return None;
    }
    for i in 0..n {
        let o = 12 + i * 16;
        if data.get(o..o + 4)? == tag {
            let start = u32_at(data, o + 8)? as usize;
            let len = u32_at(data, o + 12)? as usize;
            return data.get(start..start.saturating_add(len));
        }
    }
    None
}

fn u16_at(data: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(o..o + 2)?.try_into().ok()?))
}

fn i16_at(data: &[u8], o: usize) -> Option<i16> {
    Some(u16_at(data, o)? as i16)
}

fn u32_at(data: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(o..o + 4)?.try_into().ok()?))
}

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
    snap(content_top + row as f32 * cell_h(), ppp)
}

/// Glyph left edge for column `col` (0-based).
#[inline]
pub fn term_col_x(content_left: f32, col: u16, ppp: f32) -> f32 {
    snap(content_left + col as f32 * cell_w(), ppp)
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
        // Ctrl+= / Ctrl+- / Ctrl+0 zoom the terminal font (app.rs), not the whole UI.
        o.zoom_with_keyboard = false;
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
    s.spacing.item_spacing = egui::vec2(6.0, 3.0);
    s.spacing.button_padding = egui::vec2(8.0, 4.0);
    s.spacing.interact_size.y = 26.0;
    s.spacing.scroll = egui::style::ScrollStyle::floating();
}

fn apply_text_styles(s: &mut Style) {
    use FontFamily::{Monospace, Proportional};
    s.text_styles = BTreeMap::from([
        (TextStyle::Small, FontId::new(SIZE_UI_SMALL, Proportional)),
        (TextStyle::Body, FontId::new(SIZE_UI, Proportional)),
        (TextStyle::Button, FontId::new(SIZE_UI, Proportional)),
        (TextStyle::Heading, FontId::new(SIZE_UI, Proportional)),
        (TextStyle::Monospace, FontId::new(DEFAULT_TERM_FONT, Monospace)),
    ]);
}

fn install_fonts(ctx: &Context) {
    let mut fonts = font_definitions();
    add_system_symbols(&mut fonts);
    ctx.set_fonts(fonts);
}

/// Monochrome emoji (U+2705, U+1F525): the bundled symbols font has none, Windows ships Segoe UI
/// Symbol. Last in the proportional and monospace chains; skipped when the file is missing.
fn add_system_symbols(fonts: &mut FontDefinitions) {
    let Some(windir) = std::env::var_os("WINDIR") else {
        return;
    };
    let path = std::path::Path::new(&windir).join("Fonts").join("seguisym.ttf");
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    fonts.font_data.insert("SegoeSymbols".to_owned(), Arc::new(FontData::from_owned(bytes)));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("SegoeSymbols".to_owned());
    }
}

/// Inter + icons + bundled symbols on top of egui's defaults (Hack for monospace).
fn font_definitions() -> FontDefinitions {
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

    // Symbols Hack/Inter lack (braille spinner, arrows, geometric shapes); last in every chain.
    fonts.font_data.insert(
        "AnkarSymbols".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/ankar-symbols.ttf"
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
    proportional.push("AnkarSymbols".to_owned());

    // Named Medium family for call sites that opt in explicitly (same face for now).
    fonts.families.insert(
        FontFamily::Name(FONT_MEDIUM.into()),
        vec!["InterMedium".into(), "Inter".into(), "AnkarSymbols".into()],
    );
    fonts
        .families
        .insert(FontFamily::Name(FONT_ICONS.into()), vec!["AnkarIcons".into()]);

    // Monospace stays egui's embedded Hack (default_fonts feature); symbols only as fallback.
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("AnkarSymbols".to_owned());
    fonts
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal cmap lookup (formats 4 and 12) so the test needs no font crate.
    fn font_has(font: &[u8], c: u32) -> bool {
        let u16_at = |o: usize| u16::from_be_bytes([font[o], font[o + 1]]) as usize;
        let u32_at = |o: usize| u32::from_be_bytes([font[o], font[o + 1], font[o + 2], font[o + 3]]) as usize;
        let cmap = (0..u16_at(4))
            .map(|i| 12 + i * 16)
            .find(|&r| &font[r..r + 4] == b"cmap")
            .map(|r| u32_at(r + 8))
            .expect("cmap table");
        (0..u16_at(cmap + 2)).any(|i| {
            let sub = cmap + u32_at(cmap + 4 + i * 8 + 4);
            match u16_at(sub) {
                12 => (0..u32_at(sub + 12)).any(|g| {
                    let o = sub + 16 + g * 12;
                    (u32_at(o)..=u32_at(o + 4)).contains(&(c as usize))
                }),
                4 => {
                    let segs = u16_at(sub + 6) / 2;
                    let (end, start) = (sub + 14, sub + 16 + segs * 2);
                    (0..segs).any(|s| {
                        let (e, st) = (u16_at(end + s * 2), u16_at(start + s * 2));
                        let range_pos = start + segs * 4 + s * 2;
                        let (delta, range) = (u16_at(start + segs * 2 + s * 2), u16_at(range_pos));
                        if !(st..=e).contains(&(c as usize)) {
                            return false;
                        }
                        if range == 0 {
                            return (c as usize + delta) & 0xFFFF != 0;
                        }
                        u16_at(range_pos + range + 2 * (c as usize - st)) != 0
                    })
                }
                _ => false,
            }
        })
    }

    #[test]
    fn hack_at_12px_scale_1_uses_the_font_line_not_an_extra_1_2() {
        // Hack hhea (egui's line metrics): advance 1233, ascent 1901, descent 483, gap 0, upem 2048.
        let m = FontLineMetrics {
            advance: 1233.0 * 12.0 / 2048.0,
            ascent: 1901.0 * 12.0 / 2048.0,
            descent: 483.0 * 12.0 / 2048.0,
            line_gap: 0.0,
        };
        let (w, h) = cell_size_px(m, 1.0);
        assert!((w - 7.224_609_4).abs() < 1e-4, "real advance, not ceiled: {w}");
        assert!(w <= m.advance + 1.0);
        assert_eq!(h, 14.0, "round(ascent+descent) = 14; a zero gap adds nothing");
        let (w2, h2) = cell_size_px(m, 2.0);
        assert_eq!(w2, w, "dpi must not scale an advance already in points");
        assert_eq!(h2, 14.0);
        let mut gapped = m;
        gapped.line_gap = 4.0;
        assert_eq!(cell_size_px(gapped, 1.0).1, h, "a positive line gap is not extra leading");
        // Glyph top on the row top. A taller row must not centre it downward.
        let top = glyph_top_offset(h, m.ascent, m.descent, m.ascent, 1.0);
        assert_eq!(top, 0.0, "glyph top on the row top, not centred; got {top}");
        assert_eq!(
            glyph_top_offset(h + 8.0, m.ascent, m.descent, m.ascent, 1.0),
            top,
            "a taller row must not push the glyph down"
        );
        let lower = glyph_top_offset(h, m.ascent, m.descent, m.ascent + 3.0, 1.0);
        assert!(
            (lower - (-3.0)).abs() < 1e-4,
            "egui leading above the ascent is pulled up, got {lower}"
        );
    }

    #[test]
    fn hack_cell_uses_the_typo_box_and_shifts_the_galley_up_to_it() {
        // Hack OS/2 typo is the em (1556+492=2048). hhea (1901+483) leaves empty space above caps.
        let (a, d) = pick_line_box(1901.0, 483.0, 1556.0, 492.0).unwrap();
        assert_eq!((a, d), (1556.0, 492.0));
        let m = FontLineMetrics {
            advance: 1233.0 * 12.0 / 2048.0,
            ascent: a * 12.0 / 2048.0,
            descent: d * 12.0 / 2048.0,
            line_gap: 410.0 * 12.0 / 2048.0,
        };
        assert_eq!(cell_size_px(m, 1.0).1, 12.0, "round(ascent+descent)=12, not hhea 14");
        let (_, h15) = cell_size_px(m, 1.5);
        let sum = m.ascent + m.descent;
        assert_eq!(h15, (sum * 1.5).round() / 1.5, "pixel-snapped, not a second dpi scale");
        // egui still baselines on hhea ascent; pull the galley up so the typo top is the row top.
        let hhea_ascent = 1901.0 * 12.0 / 2048.0;
        let offset = glyph_top_offset(12.0, m.ascent, m.descent, hhea_ascent, 1.0);
        assert_eq!(offset, (m.ascent - hhea_ascent).round(), "got {offset}");
        assert!(offset < 0.0);
        assert!(pick_line_box(1901.0, 483.0, 0.0, 0.0).unwrap().0 == 1901.0);
    }

    #[test]
    fn symbols_font_has_the_missing_glyphs_and_is_last_in_every_text_chain() {
        let font = include_bytes!("../assets/fonts/ankar-symbols.ttf");
        for c in (0x280B..=0x283C).chain([0x2B1D, 0x21BB, 0x25CF]) {
            assert!(font_has(font, c), "U+{c:04X} missing from ankar-symbols.ttf");
        }
        assert!(!font_has(font, 0x41), "the lookup is not a constant true");
        let fonts = font_definitions();
        for family in [
            FontFamily::Proportional,
            FontFamily::Monospace,
            FontFamily::Name(FONT_MEDIUM.into()),
        ] {
            let chain = &fonts.families[&family];
            assert_eq!(chain.last().map(String::as_str), Some("AnkarSymbols"), "{family:?}");
            assert!(fonts.font_data.contains_key("AnkarSymbols"));
        }
    }
}