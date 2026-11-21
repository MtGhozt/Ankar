use egui::{Color32, Context, CornerRadius, Shadow, Stroke, Theme, ThemePreference, Visuals};

/// Warp-like black/gray palette. Single accent.
pub const BG_APP: Color32 = Color32::from_rgb(0x0E, 0x0E, 0x10);
pub const BG_TERM: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1E);
pub const BG_WIDGET: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x22);
pub const BG_HOVER: Color32 = Color32::from_rgb(0x27, 0x27, 0x2C);
pub const BG_ACTIVE: Color32 = Color32::from_rgb(0x31, 0x31, 0x37);
pub const BORDER: Color32 = Color32::from_rgb(0x24, 0x24, 0x29);
pub const TEXT: Color32 = Color32::from_rgb(0xD4, 0xD4, 0xD8);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8A, 0x8A, 0x93);
pub const TEXT_STRONG: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF4);
pub const ACCENT: Color32 = Color32::from_rgb(0x3A, 0x7C, 0xA5);

/// Force dark theme always — never follow the system.
pub fn apply(ctx: &Context) {
    ctx.set_theme(ThemePreference::Dark);
    ctx.options_mut(|o| {
        o.fallback_theme = Theme::Dark;
        o.sync_window_theme = true;
    });

    let v = visuals();
    ctx.set_visuals_of(Theme::Dark, v.clone());
    ctx.set_visuals_of(Theme::Light, v);
    for theme in [Theme::Dark, Theme::Light] {
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        });
    }
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = None;
    v.panel_fill = BG_APP;
    v.window_fill = BG_ELEVATED;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(6);
    v.faint_bg_color = Color32::from_rgb(0x14, 0x14, 0x17);
    v.extreme_bg_color = BG_TERM;
    v.text_edit_bg_color = Some(BG_ELEVATED);
    v.code_bg_color = BG_ELEVATED;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.6);
    v.selection.stroke = Stroke::new(1.0, TEXT_STRONG);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG_APP;
    w.noninteractive.weak_bg_fill = BG_APP;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.bg_fill = BG_WIDGET;
    w.inactive.weak_bg_fill = BG_WIDGET;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.bg_fill = BG_HOVER;
    w.hovered.weak_bg_fill = BG_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, BORDER);
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    w.active.bg_fill = BG_ACTIVE;
    w.active.weak_bg_fill = BG_ACTIVE;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    w.open.bg_fill = BG_ELEVATED;
    w.open.weak_bg_fill = BG_ELEVATED;
    w.open.bg_stroke = Stroke::new(1.0, BORDER);
    w.open.fg_stroke = Stroke::new(1.0, TEXT_STRONG);
    for wv in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        wv.corner_radius = CornerRadius::same(4);
        wv.expansion = 0.0;
    }
    v
}
