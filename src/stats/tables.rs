//! Stats page tables: MODELS and AGENTS (page-wide agent filter). Painter rects and short texts.

use super::format::{format_hours_short, format_tokens, format_with_commas};
use super::panel::{MUTED, TEXT};
use super::{AgentFilter, AgentRow, ModelRow, Source, StatsSnapshot, Tokens};
use crate::config::Config;
use crate::logos::{self, LogoAtlas};
use crate::ui_theme;
use egui::{pos2, Align2, Color32, CornerRadius, FontFamily, FontId, Rect, RichText, Sense, Ui, Vec2};

const PROVIDER_ANTHROPIC: Color32 = Color32::from_rgb(0xBB, 0x5E, 0x4C);
const PROVIDER_GOOGLE: Color32 = Color32::from_rgb(0x41, 0x78, 0xD3);
const PROVIDER_OPENAI: Color32 = Color32::from_rgb(0x30, 0x98, 0x7F);
const PROVIDER_XAI: Color32 = Color32::from_rgb(0x9A, 0x9A, 0x9A);
const PROVIDER_OTHER: Color32 = Color32::from_rgb(0x5A, 0x5A, 0x5A);
const PROVIDER_DEEPSEEK: Color32 = Color32::from_rgb(0x3D, 0x6B, 0x8E);
const PROVIDER_MOONSHOT: Color32 = Color32::from_rgb(0x6B, 0x5B, 0x8E);
const PROVIDER_QWEN: Color32 = Color32::from_rgb(0x8E, 0x6B, 0x3D);
const PROVIDER_MINIMAX: Color32 = Color32::from_rgb(0x5E, 0x7A, 0x6B);
const PROVIDER_MISTRAL: Color32 = Color32::from_rgb(0x7A, 0x5E, 0x5E);
const PROVIDER_META: Color32 = Color32::from_rgb(0x4A, 0x6B, 0x9A);
const PROVIDER_ZAI: Color32 = Color32::from_rgb(0x6E, 0x6E, 0x55);
const PROVIDER_ZEN_FREE: Color32 = Color32::from_rgb(0x55, 0x6E, 0x6E);
const SEP: Color32 = Color32::from_rgb(0x19, 0x19, 0x19);
const HOVER: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
const SELECTED: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1A);
const AGENT_BAR: Color32 = Color32::from_rgb(0x89, 0x89, 0x89);

const ROW_H: f32 = 20.5;
const COL_W: f32 = 54.0;
const TOP_MODELS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Anthropic,
    Google,
    OpenAi,
    Xai,
    DeepSeek,
    Moonshot,
    Qwen,
    MiniMax,
    Mistral,
    Meta,
    Zai,
    OpenCodeZenFree,
    Other,
}

impl Family {
    fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic",
            Self::Google => "Google",
            Self::OpenAi => "OpenAI",
            Self::Xai => "xAI",
            Self::DeepSeek => "DeepSeek",
            Self::Moonshot => "Moonshot",
            Self::Qwen => "Qwen",
            Self::MiniMax => "MiniMax",
            Self::Mistral => "Mistral",
            Self::Meta => "Meta",
            Self::Zai => "Z.ai",
            Self::OpenCodeZenFree => "OpenCode Zen free",
            Self::Other => "Other",
        }
    }

    fn color(self) -> Color32 {
        match self {
            Self::Anthropic => PROVIDER_ANTHROPIC,
            Self::Google => PROVIDER_GOOGLE,
            Self::OpenAi => PROVIDER_OPENAI,
            Self::Xai => PROVIDER_XAI,
            Self::DeepSeek => PROVIDER_DEEPSEEK,
            Self::Moonshot => PROVIDER_MOONSHOT,
            Self::Qwen => PROVIDER_QWEN,
            Self::MiniMax => PROVIDER_MINIMAX,
            Self::Mistral => PROVIDER_MISTRAL,
            Self::Meta => PROVIDER_META,
            Self::Zai => PROVIDER_ZAI,
            Self::OpenCodeZenFree => PROVIDER_ZEN_FREE,
            Self::Other => PROVIDER_OTHER,
        }
    }
}

/// Prefix or separated token (`-`, `_`, `/`) match — not bare substring for short ids.
fn id_has_token(id: &str, token: &str) -> bool {
    if id == token {
        return true;
    }
    if id.starts_with(&format!("{token}-")) || id.starts_with(&format!("{token}_")) {
        return true;
    }
    id.split(&['/', '-', '_'][..]).any(|seg| seg == token)
}

fn provider_has_token(provider: &str, token: &str) -> bool {
    if token.len() <= 3 {
        id_has_token(provider, token)
    } else {
        provider.contains(token) || id_has_token(provider, token)
    }
}

fn model_has_token(model: &str, token: &str) -> bool {
    id_has_token(model, token)
}

fn openai_o_series(model: &str) -> bool {
    ["o1", "o3", "o4"]
        .iter()
        .any(|p| model == *p || model.starts_with(&format!("{p}-")))
}

const FAMILY_TOKENS: &[(Family, &[&str])] = &[
    (Family::Anthropic, &["anthropic", "claude"]),
    (Family::OpenAi, &["openai", "gpt", "codex"]),
    (Family::Google, &["google", "gemini"]),
    (Family::Xai, &["xai", "grok"]),
    (Family::DeepSeek, &["deepseek"]),
    (Family::Moonshot, &["moonshot", "kimi"]),
    (Family::Qwen, &["qwen"]),
    (Family::MiniMax, &["minimax"]),
    (Family::Mistral, &["mistral"]),
    (Family::Meta, &["meta", "llama"]),
    (Family::Zai, &["zai", "glm"]),
];

fn family_from_tokens<F>(hay: &str, has_token: F) -> Option<Family>
where
    F: Fn(&str, &str) -> bool,
{
    for (family, tokens) in FAMILY_TOKENS {
        if tokens.iter().any(|t| has_token(hay, t)) {
            return Some(*family);
        }
    }
    None
}

/// Provider family from the provider id, falling back to hints in the model id
/// (a router such as `openrouter` serves other companies' models).
fn family_of(provider: &str, model: &str) -> Family {
    let p = provider.to_lowercase();
    let m = model.to_lowercase();

    if m.ends_with("-free") {
        return Family::OpenCodeZenFree;
    }

    if let Some(f) = family_from_tokens(&p, provider_has_token) {
        return f;
    }

    if openai_o_series(&m) {
        return Family::OpenAi;
    }

    family_from_tokens(&m, model_has_token).unwrap_or(Family::Other)
}

/// Models by tokens: the first `n` rows plus one summed `N more` row for the rest.
fn top_with_rest(models: &[ModelRow], n: usize) -> (Vec<ModelRow>, Option<ModelRow>) {
    let mut rows = models.to_vec();
    rows.sort_by_key(|m| std::cmp::Reverse(m.tokens.total()));
    if rows.len() <= n {
        return (rows, None);
    }
    let rest = rows.split_off(n);
    let mut more = ModelRow {
        provider: "other".into(),
        model: format!("{} more", rest.len()),
        steps: 0,
        tokens: Tokens::default(),
        cost: 0.0,
    };
    for r in &rest {
        more.steps = more.steps.saturating_add(r.steps);
        more.tokens.add_assign(&r.tokens);
        more.cost += r.cost;
    }
    (rows, Some(more))
}

/// Tokens per provider family, biggest first (families without tokens are dropped).
fn family_shares(models: &[ModelRow]) -> Vec<(Family, u64)> {
    let mut out: Vec<(Family, u64)> = Vec::new();
    for m in models {
        let (fam, n) = (family_of(&m.provider, &m.model), m.tokens.total());
        match out.iter_mut().find(|(f, _)| *f == fam) {
            Some(e) => e.1 = e.1.saturating_add(n),
            None => out.push((fam, n)),
        }
    }
    out.retain(|e| e.1 > 0);
    out.sort_by_key(|e| std::cmp::Reverse(e.1));
    out
}

/// Bar length basis of an agent row: tokens when it has them, else busy seconds.
fn agent_weight(a: &AgentRow) -> (bool, u64) {
    match a.tokens.as_ref().map(Tokens::total) {
        Some(t) if t > 0 => (true, t),
        _ => (false, a.busy_secs),
    }
}

fn is_selected(filter: Option<&AgentFilter>, a: &AgentRow) -> bool {
    AgentFilter::allows(filter, a.source, &a.name) && filter.is_some()
}

/// Section title, `right` muted at the far end, then a #191919 line.
fn table_title(ui: &mut Ui, title: &str, right: &str, width: f32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(11.0).color(MUTED).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(right).size(11.0).color(MUTED));
        });
    });
    ui.add_space(4.0);
    let (line, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
    ui.painter().rect_filled(line, CornerRadius::ZERO, SEP);
    ui.add_space(6.0);
}

/// Column geometry of a row: name column, bar span, right edges of the three number columns.
struct Cols {
    name_x: f32,
    bar_l: f32,
    bar_r: f32,
    x: [f32; 3],
}

fn cols(rect: Rect) -> Cols {
    let last = rect.right() - 4.0;
    let x = [last - 2.0 * COL_W, last - COL_W, last];
    let name_w = (rect.width() - 18.0 - 3.0 * COL_W - 100.0).clamp(60.0, 132.0);
    let bar_l = rect.left() + 18.0 + name_w + 8.0;
    Cols {
        name_x: rect.left() + 18.0,
        bar_l,
        bar_r: (x[0] - 44.0).max(bar_l + 8.0),
        x,
    }
}

fn header_row(ui: &mut Ui, labels: [&str; 3], width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
    let c = cols(rect);
    for (x, label) in c.x.iter().zip(labels) {
        ui.painter().text(
            pos2(*x, rect.center().y),
            Align2::RIGHT_CENTER,
            label,
            FontId::proportional(10.0),
            MUTED,
        );
    }
}

fn paint_bar(ui: &Ui, c: &Cols, y: f32, frac: f32, color: Color32) {
    let w = (frac.clamp(0.0, 1.0) * (c.bar_r - c.bar_l)).max(4.0);
    let r = Rect::from_min_size(pos2(c.bar_l, y - 3.0), Vec2::new(w, 6.0));
    ui.painter().rect_filled(r, CornerRadius::same(3), color);
}

fn paint_numbers(ui: &Ui, c: &Cols, y: f32, vals: [String; 3], color: Color32) {
    for (x, v) in c.x.iter().zip(vals) {
        ui.painter()
            .text(pos2(*x, y), Align2::RIGHT_CENTER, v, FontId::proportional(11.0), color);
    }
}

pub(super) fn draw_models(ui: &mut Ui, snap: &StatsSnapshot, width: f32) {
    table_title(ui, "MODELS", &format!("{} models", snap.models.len()), width);

    // Stacked provider bar (4 px, radius 2) and its legend.
    let shares = family_shares(&snap.models);
    let total: u64 = shares.iter().map(|s| s.1).sum();
    let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 4.0), Sense::hover());
    ui.painter().rect_filled(bar, CornerRadius::same(2), SEP);
    let mut x = bar.left();
    for (i, (fam, n)) in shares.iter().enumerate() {
        let last = i + 1 == shares.len();
        let w = if last { bar.right() - x } else { (*n as f32 / total as f32 * bar.width()).max(1.0) };
        let round = CornerRadius {
            nw: if i == 0 { 2 } else { 0 },
            sw: if i == 0 { 2 } else { 0 },
            ne: if last { 2 } else { 0 },
            se: if last { 2 } else { 0 },
        };
        let r = Rect::from_min_size(pos2(x, bar.top()), Vec2::new(w, bar.height()));
        ui.painter().rect_filled(r, round, fam.color());
        x += w;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (fam, n) in shares.iter().take(4) {
            let pct = *n as f64 / total.max(1) as f64 * 100.0;
            ui.label(RichText::new("●").size(10.0).color(fam.color()));
            ui.label(RichText::new(format!("{} {pct:.0}%", fam.label())).size(11.0).color(MUTED));
            ui.add_space(8.0);
        }
    });
    ui.add_space(6.0);

    header_row(ui, ["input", "output", "tasks"], width);
    let (rows, more) = top_with_rest(&snap.models, TOP_MODELS);
    let max = rows.iter().map(|m| m.tokens.total()).max().unwrap_or(0).max(1);
    for (m, is_more) in rows.iter().map(|m| (m, false)).chain(more.iter().map(|m| (m, true))) {
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::hover());
        let c = cols(rect);
        let y = rect.center().y;
        let color = if is_more { MUTED } else { TEXT };
        let fam = family_of(&m.provider, &m.model);
        if !is_more {
            ui.painter().circle_filled(pos2(rect.left() + 6.0, y), 3.5, fam.color());
        }
        let name = truncate_to(&m.model, c.bar_l - c.name_x - 6.0);
        let name_x = if is_more { rect.left() + 6.0 } else { c.name_x };
        ui.painter().text(pos2(name_x, y), Align2::LEFT_CENTER, name, FontId::proportional(12.0), color);
        if !is_more {
            paint_bar(ui, &c, y, m.tokens.total() as f32 / max as f32, fam.color());
            resp.on_hover_text(format!("{} / {}", m.provider, m.model));
        }
        paint_numbers(
            ui,
            &c,
            y,
            [format_tokens(m.tokens.input), format_tokens(m.tokens.output), format_with_commas(m.steps)],
            color,
        );
    }
}

pub(super) fn draw_agents(
    ui: &mut Ui,
    snap: &StatsSnapshot,
    filter: &mut Option<AgentFilter>,
    config: &Config,
    logos: &mut LogoAtlas,
    width: f32,
) {
    table_title(ui, "AGENTS", "click one to see only its usage", width);
    header_row(ui, ["input", "output", "time"], width);
    let max_tokens = snap.agents.iter().filter_map(|a| Some(agent_weight(a)).filter(|w| w.0)).map(|w| w.1).max().unwrap_or(1);
    let max_secs = snap.agents.iter().filter(|a| !agent_weight(a).0).map(|a| a.busy_secs).max().unwrap_or(1);
    for a in &snap.agents {
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::click());
        let selected = is_selected(filter.as_ref(), a);
        let dim = filter.is_some() && !selected;
        let fade = |c: Color32| if dim { c.gamma_multiply(0.4) } else { c };
        let c = cols(rect);
        let y = rect.center().y;
        if selected || resp.hovered() {
            let bg = if selected { SELECTED } else { HOVER };
            ui.painter().rect_filled(rect, CornerRadius::same(4), bg);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // Logo from the same lookup as the sidebar; the source tells which program wrote the row.
        let hay = match a.source {
            Source::OpenCode => "opencode",
            Source::CursorAgent => "cursor",
            Source::Ankar => a.name.as_str(),
        };
        let (icon, icon_color, key) = logos::agent_visual_named(hay, config);
        let logo_rect = Rect::from_center_size(pos2(rect.left() + 9.0, y), Vec2::splat(14.0));
        match key.and_then(|k| logos.get(ui.ctx(), k, 14.0)) {
            Some(tex) => {
                ui.painter().image(
                    tex.id(),
                    logo_rect,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    fade(Color32::WHITE),
                );
            }
            None => {
                ui.painter().text(
                    logo_rect.center(),
                    Align2::CENTER_CENTER,
                    icon.codepoint().to_string(),
                    FontId::new(14.0, FontFamily::Name(ui_theme::FONT_ICONS.into())),
                    fade(icon_color),
                );
            }
        }
        let name_x = rect.left() + 22.0;
        let name = truncate_to(&a.name, c.bar_l - name_x - 6.0);
        ui.painter()
            .text(pos2(name_x, y), Align2::LEFT_CENTER, name, FontId::proportional(12.0), fade(TEXT));

        let (has_tokens, w) = agent_weight(a);
        if w > 0 {
            let frac = w as f32 / if has_tokens { max_tokens } else { max_secs }.max(1) as f32;
            paint_bar(ui, &c, y, frac, fade(AGENT_BAR));
        }
        let tok = |f: fn(&Tokens) -> u64| {
            a.tokens.as_ref().map_or_else(|| "—".to_string(), |t| format_tokens(f(t)))
        };
        paint_numbers(
            ui,
            &c,
            y,
            [tok(|t| t.input), tok(|t| t.output), format_hours_short(a.busy_secs)],
            fade(TEXT),
        );

        if resp.clicked() {
            *filter = (!selected).then(|| AgentFilter { source: a.source, name: a.name.clone() });
        }
        let mut tip = format!(
            "{} tasks · busy {} · waiting {}",
            format_with_commas(a.tasks),
            format_hours_short(a.busy_secs),
            format_hours_short(a.waiting_secs)
        );
        if a.name == "Junie" && a.tokens.is_none() {
            tip.push_str("\nTime is only what Ankar measured in the terminal.");
        } else if a.tokens.is_none() {
            tip.push_str("\ncursor-agent and Ankar do not store tokens locally");
        }
        if !a.modes.is_empty() {
            tip.push_str("\nmodes: ");
            tip.push_str(&a.modes);
        }
        // Popup on the tooltip layer: `on_hover_text` is clipped to the row and covers the rows under it.
        if resp.hovered() {
            let pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or(resp.rect.min) + egui::vec2(14.0, 18.0);
            egui::Area::new(resp.id.with("agent_tip"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pos)
                .constrain(true)
                .interactable(false)
                .show(ui.ctx(), |tip_ui| {
                    egui::Frame::popup(tip_ui.style()).show(tip_ui, |tip_ui| {
                        tip_ui.set_max_width(280.0);
                        tip_ui.label(tip);
                    });
                });
        }
    }
}

/// `s` cut with `…` to fit `max_w` px at 12 px (about 6.2 px per character).
fn truncate_to(s: &str, max_w: f32) -> String {
    let max = (max_w / 6.2).floor().max(4.0) as usize;
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(provider: &str, name: &str, tokens: u64) -> ModelRow {
        ModelRow {
            provider: provider.into(),
            model: name.into(),
            steps: 1,
            tokens: Tokens { input: tokens, ..Tokens::default() },
            cost: 0.0,
        }
    }

    #[test]
    fn families_come_from_the_provider_then_the_model() {
        assert_eq!(family_of("anthropic", "opus"), Family::Anthropic);
        assert_eq!(family_of("openrouter", "google/gemini-3"), Family::Google);
        assert_eq!(family_of("openrouter", "gemini-2.5-flash"), Family::Google);
        assert_eq!(family_of("github-copilot", "gpt-5"), Family::OpenAi);
        assert_eq!(family_of("openai", "x"), Family::OpenAi);
        assert_eq!(family_of("opencode", "grok-code"), Family::Xai);
        assert_eq!(family_of("local", "llama"), Family::Meta);
        assert_eq!(family_of("opencode", "claude-sonnet-free"), Family::OpenCodeZenFree);
        assert_eq!(family_of("unknown-vendor", "mystery-model"), Family::Other);
    }

    #[test]
    fn top_models_keep_six_and_sum_the_rest() {
        let models: Vec<ModelRow> = (1..=9).map(|i| model("anthropic", &format!("m{i}"), i * 10)).collect();
        let (top, more) = top_with_rest(&models, 6);
        assert_eq!(top.len(), 6);
        assert_eq!(top[0].model, "m9");
        let more = more.unwrap();
        assert_eq!(more.model, "3 more");
        assert_eq!((more.tokens.input, more.steps), (60, 3));
        assert!(top_with_rest(&models[..6], 6).1.is_none());
    }

    #[test]
    fn shares_group_by_family_and_drop_empty_ones() {
        let models = vec![model("anthropic", "a", 30), model("x", "claude-b", 70), model("google", "g", 0)];
        assert_eq!(family_shares(&models), vec![(Family::Anthropic, 100)]);
    }

    #[test]
    fn a_row_is_selected_for_the_same_tool_from_either_source() {
        let row = AgentRow {
            source: Source::Ankar,
            name: "codex".into(),
            busy_secs: 0,
            waiting_secs: 0,
            tasks: 0,
            tokens: None,
            modes: String::new(),
        };
        let same = AgentFilter { source: Source::Ankar, name: "codex".into() };
        let other_source = AgentFilter { source: Source::OpenCode, name: "codex".into() };
        let other_tool = AgentFilter { source: Source::OpenCode, name: "Junie".into() };
        assert!(is_selected(Some(&same), &row));
        assert!(is_selected(Some(&other_source), &row));
        assert!(!is_selected(Some(&other_tool), &row));
        assert!(!is_selected(None, &row));
    }
}
