use crate::config::Config;
use crate::files::FileViewer;
use crate::launchers;
use crate::layout::{LayoutNode, SessionStatus, SplitDir, Workspace};
use crate::logos::LogoAtlas;
use crate::palette::{CommandPalette, PaletteAction};
use crate::persist;
use crate::sidebar::{self, SidebarState};
use crate::status::StatusTracker;
use crate::terminal::TerminalView;
use crate::ui_theme::{self, Icon};
use egui::{
    Color32, Context, CornerRadius, Frame, Key, Modifiers, Sense, Stroke, Vec2,
};
use std::time::Duration;

pub struct AnkarApp {
    workspace: Workspace,
    config: Config,
    /// When true, `on_exit` must not overwrite the user's broken config.toml.
    config_load_failed: bool,
    sidebar: SidebarState,
    show_profile_menu: bool,
    status: StatusTracker,
    palette: CommandPalette,
    files: FileViewer,
    logos: LogoAtlas,
}

impl AnkarApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        ui_theme::apply(&cc.egui_ctx);
        let (config, config_load_failed) = Config::load_or_create().unwrap_or_else(|e| {
            log::warn!("config load error: {e}");
            (Config::default(), true)
        });
        let (mut workspace, archived) = Workspace::new(&cc.egui_ctx, config.scrollback_lines)?;
        for tab in &mut workspace.tabs {
            if config.pinned_sessions.contains(&tab.session_name) {
                tab.pinned = true;
            }
        }
        Ok(Self {
            workspace,
            config,
            config_load_failed,
            sidebar: {
                let mut s = SidebarState::default();
                s.archived = archived;
                s.show = true;
                s
            },
            show_profile_menu: false,
            status: StatusTracker::default(),
            palette: CommandPalette::default(),
            files: FileViewer::default(),
            logos: LogoAtlas::default(),
        })
    }

    fn apply_palette_action(&mut self, ctx: &Context, action: PaletteAction) {
        match action {
            PaletteAction::FocusTab(i) => {
                if i < self.workspace.tabs.len() {
                    self.workspace.active_tab = i;
                }
            }
            PaletteAction::OpenProfile(id) => {
                let profile = self.workspace.profile_by_id(&id);
                let cwd = self.workspace.active_tab().and_then(|t| t.cwd.clone());
                let _ = self.workspace.open_tab(ctx, &profile, cwd);
            }
            PaletteAction::RunLauncher(launcher) => {
                let cwd = self.workspace.active_tab().and_then(|t| t.cwd.clone());
                let _ = launchers::run_launcher(ctx, &mut self.workspace, &launcher, cwd);
            }
            PaletteAction::ToggleSidebar => {
                self.sidebar.show = !self.sidebar.show;
            }
            PaletteAction::ToggleFiles => {
                self.files.show = !self.files.show;
            }
            PaletteAction::NewTab => {
                let profile = crate::shell::default_profile();
                let cwd = self.workspace.active_tab().and_then(|t| t.cwd.clone());
                let _ = self.workspace.open_tab(ctx, &profile, cwd);
            }
            PaletteAction::PinActive => {
                self.workspace.toggle_pin_active();
            }
            PaletteAction::RestoreArchived(i) => {
                if i < self.sidebar.archived.len() {
                    let arch = self.sidebar.archived.remove(i);
                    if let (Some(prog), args) = (arch.program.clone(), arch.args.clone()) {
                        let _ = self.workspace.open_command_tab(
                            ctx,
                            &arch.name,
                            &prog,
                            &args,
                            arch.cwd,
                        );
                    } else {
                        let profile = self.workspace.profile_by_id(&arch.profile_id);
                        let _ = self.workspace.open_tab(ctx, &profile, arch.cwd);
                        if let Some(tab) = self.workspace.active_tab_mut() {
                            tab.session_name = arch.name;
                            tab.command = arch.command;
                            tab.program = arch.program;
                            tab.args = arch.args;
                        }
                    }
                    self.workspace.mark_dirty();
                }
            }
        }
    }
}

impl eframe::App for AnkarApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let focused = ui.input(|i| i.focused);
        self.status
            .tick(&mut self.workspace, &self.config, focused);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));

        if self.workspace.dirty
            && self.workspace.last_autosave.elapsed() >= Duration::from_secs(30)
        {
            if let Err(e) = persist::save_workspace(&self.workspace, &self.sidebar.archived) {
                log::warn!("autosave failed: {e}");
            } else {
                self.workspace.dirty = false;
                self.workspace.skip_save_until_dirty = false;
                self.workspace.last_autosave = std::time::Instant::now();
            }
        }

        let active = self.workspace.active_tab;
        for (i, tab) in self.workspace.tabs.iter_mut().enumerate() {
            let tab_visible = i == active;
            for session in tab.sessions.values_mut() {
                session.set_visible(tab_visible);
                if !tab_visible {
                    session.release_snapshot();
                }
            }
        }

        {
            let mut cwd_dirty = false;
            if let Some(tab) = self.workspace.active_tab_mut() {
                let focused = tab.focused_pane;
                if let Some(session) = tab.sessions.get(&focused) {
                    if let Some(new_cwd) = session.cwd() {
                        if tab.cwd.as_ref() != Some(&new_cwd) {
                            tab.cwd = Some(new_cwd);
                            cwd_dirty = true;
                        }
                    }
                }
            }
            if cwd_dirty {
                self.workspace.mark_dirty();
            }
        }

        let cwd = self.workspace.active_tab().and_then(|t| t.cwd.clone());
        self.files.set_root(cwd);

        handle_shortcuts(
            ui,
            &mut self.workspace,
            &mut self.show_profile_menu,
            &mut self.sidebar,
            &mut self.palette,
            &mut self.files,
        );

        // Manual panel widths — egui's built-in SidePanel resize fought PanelState
        // and bounced on release. We own the width via exact_size + drag handles.
        let window_w = ui
            .ctx()
            .input(|i| i.viewport().inner_rect.map(|r| r.width()))
            .unwrap_or(1100.0);
        if self.sidebar.show {
            let max_w = Config::max_sidebar_width(window_w);
            let width = Config::clamp_sidebar_width(self.config.sidebar_width, window_w);
            let panel = egui::Panel::left("sidebar_v6")
                .exact_size(width)
                .resizable(false)
                .frame(Frame::NONE.fill(ui_theme::SIDEBAR_BG))
                .show_separator_line(false)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_clip_rect(ui.max_rect());
                    let sidebar_rect = ui.max_rect();
                    let painter = ui.painter();

                    // Title strip (y 0–38): drag area x 0–70; panel-left toggle at (81.33, 10).
                    let toggle_rect = egui::Rect::from_min_size(
                        egui::pos2(sidebar_rect.left() + 81.33, sidebar_rect.top() + 10.0),
                        Vec2::splat(18.7),
                    );
                    let toggle_id = ui.scope_id().with("sidebar_toggle");
                    let toggle_resp = ui.interact(toggle_rect, toggle_id, Sense::click());
                    if toggle_resp.hovered() {
                        painter.rect_filled(
                            toggle_rect.expand(4.0),
                            CornerRadius::same(ui_theme::RADIUS_KBD),
                            ui_theme::HOVER,
                        );
                    }
                    painter.text(
                        toggle_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        Icon::PanelLeft.codepoint().to_string(),
                        egui::FontId::new(
                            18.7,
                            egui::FontFamily::Name(ui_theme::FONT_ICONS.into()),
                        ),
                        ui_theme::TEXT_MUTED,
                    );
                    let toggle_resp =
                        toggle_resp.on_hover_text("Hide sidebar (Ctrl+Shift+B)");
                    if toggle_resp.clicked() {
                        self.sidebar.show = false;
                    }

                    let content_top = sidebar_rect.top() + 38.0;
                    let content_rect = egui::Rect::from_min_max(
                        egui::pos2(sidebar_rect.left(), content_top),
                        sidebar_rect.max,
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                        ui.set_width(ui.available_width());
                        sidebar::draw(
                            ui,
                            &mut self.workspace,
                            &mut self.config,
                            &mut self.sidebar,
                            &mut self.logos,
                            !self.config_load_failed,
                            sidebar_rect,
                        );
                    });
                });

            let resize = drag_resize_edge(
                ui,
                ui.scope_id().with("sidebar_resize_v6"),
                panel.response.rect.right(),
                panel.response.rect.y_range(),
            );
            if resize.dragged() {
                let dx = ui.input(|i| i.pointer.delta().x);
                self.config.sidebar_width =
                    Config::clamp_sidebar_width(width + dx, window_w).min(max_w);
            }
            if resize.drag_stopped() && !self.config_load_failed {
                let _ = self.config.save();
            }
        }

        if self.files.show {
            let max_w = Config::max_files_width(window_w);
            let width = Config::clamp_files_width(self.config.files_width, window_w);
            let panel = egui::Panel::right("files_v4")
                .exact_size(width)
                .resizable(false)
                .frame(
                    Frame::NONE
                        .fill(ui_theme::SURFACE_1)
                        .inner_margin(egui::Margin::same(8)),
                )
                .show_separator_line(false)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let before = self.files.open_file.clone();
                    self.files.draw(ui);
                    if self.files.open_file != before {
                        if let Some(path) = self.files.open_file.clone() {
                            if path.is_file() {
                                self.files.open_path(path);
                            }
                        }
                    }
                });

            let resize = drag_resize_edge(
                ui,
                ui.scope_id().with("files_resize_v4"),
                panel.response.rect.left(),
                panel.response.rect.y_range(),
            );
            if resize.dragged() {
                // Dragging the left edge rightward shrinks the right panel.
                let dx = ui.input(|i| i.pointer.delta().x);
                self.config.files_width =
                    Config::clamp_files_width(width - dx, window_w).min(max_w);
            }
            if resize.drag_stopped() && !self.config_load_failed {
                let _ = self.config.save();
            }
        }

        // Tab bar over content only
        egui::Panel::top("tab_bar")
            .exact_size(36.0)
            .frame(Frame::NONE.fill(ui_theme::SURFACE_1))
            .show_separator_line(false)
            .show(ui, |ui| {
                draw_tab_bar(
                    ui,
                    &mut self.workspace,
                    &mut self.sidebar,
                    &mut self.files,
                    &mut self.show_profile_menu,
                    &self.config,
                    &mut self.logos,
                );
            });

        if self.show_profile_menu {
            egui::Area::new(egui::Id::unique("profile_menu"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, 40.0))
                .show(ui.ctx(), |ui| {
                    Frame::NONE
                        .fill(ui_theme::SURFACE_1)
                        .stroke(Stroke::new(1.0, ui_theme::BORDER))
                        .corner_radius(CornerRadius::same(ui_theme::RADIUS_POPUP))
                        .shadow(ui_theme::popup_shadow())
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.set_min_width(180.0);
                            ui.label(ui_theme::text_sub("Perfiles"));
                            let profiles = self.workspace.profiles.clone();
                            for profile in &profiles {
                                if ui
                                    .add(
                                        egui::Button::new(ui_theme::text_title(
                                            profile.name.clone(),
                                        ))
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    let cwd =
                                        self.workspace.active_tab().and_then(|t| t.cwd.clone());
                                    let _ = self.workspace.open_tab(ui.ctx(), profile, cwd);
                                    self.show_profile_menu = false;
                                }
                            }
                            ui.separator();
                            ui.label(ui_theme::text_sub("Launchers"));
                            let launchers = self.config.launchers.clone();
                            let cwd = self.workspace.active_tab().and_then(|t| t.cwd.clone());
                            for launcher in &launchers {
                                if ui
                                    .add(
                                        egui::Button::new(ui_theme::text_title(
                                            launcher.name.clone(),
                                        ))
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    let _ = launchers::run_launcher(
                                        ui.ctx(),
                                        &mut self.workspace,
                                        launcher,
                                        cwd.clone(),
                                    );
                                    self.show_profile_menu = false;
                                }
                            }
                        });
                });
            if ui.input(|i| i.pointer.any_click()) {
                // keep open if clicking inside — simplified: Esc closes
            }
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.show_profile_menu = false;
            }
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(ui_theme::BG))
            .show(ui, |ui| {
                if self.workspace.tabs.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(ui_theme::text_sub(
                            "Abrí una pestaña con Ctrl+Shift+T o el botón +.",
                        ));
                    });
                    return;
                }
                let active = self.workspace.active_tab;
                let font_size = self.config.font_size;
                let dim = self.config.dim_inactive_panes;
                let ratio_changed =
                    render_layout(ui, &mut self.workspace.tabs[active], font_size, dim);
                if ratio_changed {
                    self.workspace.mark_dirty();
                }
            });

        let ctx = ui.ctx().clone();
        if let Some(action) = self.palette.draw(
            ui,
            &self.workspace,
            &self.config,
            &self.sidebar.archived,
        ) {
            self.apply_palette_action(&ctx, action);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.workspace.should_persist() {
            let _ = persist::save_workspace(&self.workspace, &self.sidebar.archived);
        }
        if !self.config_load_failed {
            let _ = self.config.save();
        }
    }
}

fn draw_tab_bar(
    ui: &mut egui::Ui,
    workspace: &mut Workspace,
    sidebar: &mut SidebarState,
    files: &mut FileViewer,
    show_menu: &mut bool,
    config: &Config,
    logos: &mut LogoAtlas,
) {
    let bar_rect = ui.max_rect();
    // Bottom border line (gap under active tab painted later)
    ui.painter().hline(
        bar_rect.x_range(),
        bar_rect.bottom() - 0.5,
        Stroke::new(1.0, ui_theme::BORDER),
    );

    ui.horizontal(|ui| {
        ui.set_min_height(36.0);
        ui.spacing_mut().item_spacing.x = 0.0;

        if !sidebar.show
            && icon_square(ui, Icon::PanelLeft, "Show sidebar (Ctrl+Shift+B)").clicked()
        {
            sidebar.show = true;
        }

        let n = workspace.tabs.len().max(1);
        let actions_w = 28.0 * 3.0 + 8.0;
        let avail = (ui.available_width() - actions_w).max(110.0);
        let tab_w = (avail / n as f32).clamp(110.0, 260.0);

        let mut switch_to = None;
        let mut close_idx = None;
        let active = workspace.active_tab;

        for (i, tab) in workspace.tabs.iter().enumerate() {
            let selected = i == active;
            let (icon, color, logo_key) = tab_agent_visual(tab, config);
            let logo_tex = logo_key.and_then(|k| logos.get(ui.ctx(), k));
            let fill = if selected {
                ui_theme::BG
            } else {
                ui_theme::SURFACE_1
            };
            let text_color = if selected {
                ui_theme::TEXT
            } else {
                ui_theme::TAB_INACTIVE_TEXT
            };

            // Full-height tab rect flush with panel (avoids the "cut" bar look).
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(tab_w, ui.available_height().max(36.0)), Sense::click());
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, fill);

            if i > 0 {
                ui.painter().vline(
                    rect.left(),
                    rect.y_range(),
                    Stroke::new(1.0, ui_theme::BORDER),
                );
            }

            if resp.hovered() && !selected {
                ui.painter()
                    .rect_filled(rect, CornerRadius::ZERO, ui_theme::HOVER);
            }

            // Active tab blends into terminal: erase bottom hairline.
            if selected {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - 2.0),
                        egui::pos2(rect.right(), rect.bottom() + 1.0),
                    ),
                    CornerRadius::ZERO,
                    ui_theme::BG,
                );
            }

            let mut x = rect.left() + 12.0;
            let cy = rect.center().y;

            match tab.status {
                SessionStatus::Waiting => {
                    ui.painter()
                        .circle_filled(egui::pos2(x + 3.0, cy), 3.0, ui_theme::WARN);
                    x += 10.0;
                }
                SessionStatus::Finished => {
                    ui.painter()
                        .circle_filled(egui::pos2(x + 3.0, cy), 3.0, ui_theme::OK);
                    x += 10.0;
                }
                SessionStatus::Active => {}
            }

            if let Some(tex) = logo_tex.as_ref() {
                let img = egui::Rect::from_center_size(egui::pos2(x + 7.0, cy), Vec2::splat(14.0));
                ui.painter().image(
                    tex.id(),
                    img,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                ui.painter().text(
                    egui::pos2(x, cy),
                    egui::Align2::LEFT_CENTER,
                    icon.codepoint().to_string(),
                    egui::FontId::new(14.0, egui::FontFamily::Name(ui_theme::FONT_ICONS.into())),
                    color,
                );
            }
            x += 18.0;

            let title = if tab.session_name.is_empty() {
                tab.title.clone()
            } else {
                tab.session_name.clone()
            };
            ui.painter().text(
                egui::pos2(x, cy),
                egui::Align2::LEFT_CENTER,
                title,
                egui::FontId::proportional(ui_theme::SIZE_TITLE),
                if resp.hovered() {
                    ui_theme::TEXT
                } else {
                    text_color
                },
            );

            // Close: stable id, consume click so the tab doesn't also activate (flicker).
            let show_close = resp.hovered() || selected;
            let close_rect = egui::Rect::from_center_size(
                egui::pos2(rect.right() - 14.0, cy),
                Vec2::splat(20.0),
            );
            let close_id = ui.scope_id().with("tab_close").with(tab.id);
            let close_resp = ui.interact(close_rect, close_id, Sense::click());
            if show_close {
                if close_resp.hovered() {
                    ui.painter().rect_filled(
                        close_rect,
                        CornerRadius::same(ui_theme::RADIUS_KBD),
                        ui_theme::PRESSED,
                    );
                }
                ui.painter().text(
                    close_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    Icon::X.codepoint().to_string(),
                    egui::FontId::new(
                        12.0,
                        egui::FontFamily::Name(ui_theme::FONT_ICONS.into()),
                    ),
                    ui_theme::TEXT_MUTED,
                );
            }
            if close_resp.clicked() || resp.middle_clicked() {
                close_idx = Some(i);
            } else if resp.clicked() && !close_resp.hovered() {
                switch_to = Some(i);
            }
        }

        if let Some(i) = switch_to {
            workspace.active_tab = i;
        }
        if let Some(i) = close_idx {
            if i < workspace.tabs.len() {
                workspace.tabs.remove(i);
                if workspace.active_tab >= workspace.tabs.len() && !workspace.tabs.is_empty() {
                    workspace.active_tab = workspace.tabs.len() - 1;
                } else if i < workspace.active_tab {
                    workspace.active_tab = workspace.active_tab.saturating_sub(1);
                }
                workspace.mark_dirty();
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Stats panel lives in prompt 05 — show the affordance so it's discoverable.
            let _ = icon_square(
                ui,
                Icon::ChartColumn,
                "Estadísticas (próximamente · Ctrl+Shift+I)",
            );
            if icon_square(ui, Icon::Folder, "Archivos (Ctrl+Shift+F)").clicked() {
                files.show = !files.show;
            }
            let plus = icon_square(ui, Icon::Plus, "Nueva terminal (Ctrl+Shift+T)");
            if plus.clicked() {
                let profile = crate::shell::default_profile();
                let cwd = workspace.active_tab().and_then(|t| t.cwd.clone());
                let _ = workspace.open_tab(ui.ctx(), &profile, cwd);
            }
            if plus.secondary_clicked() {
                *show_menu = !*show_menu;
            }
        });
    });
}

/// Hit-test strip on a panel edge. Returns the interact response (check `dragged`).
fn drag_resize_edge(
    ui: &mut egui::Ui,
    id: egui::Id,
    edge_x: f32,
    y_range: egui::Rangef,
) -> egui::Response {
    let grab = 6.0;
    let handle = egui::Rect::from_x_y_ranges((edge_x - grab)..=(edge_x + grab), y_range);
    // Allocate on the foreground layer so terminal/content doesn't eat the drag.
    let resp = ui.interact(handle, id, Sense::click_and_drag());
    let stroke = if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        ui.style().visuals.widgets.active.fg_stroke
    } else if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        ui.style().visuals.widgets.hovered.fg_stroke
    } else {
        Stroke::new(1.0, ui_theme::SIDEBAR_BORDER)
    };
    ui.painter().vline(edge_x, y_range, stroke);
    resp
}

fn tab_agent_visual(
    tab: &crate::layout::Tab,
    config: &Config,
) -> (Icon, Color32, Option<&'static str>) {
    let name = tab.session_name.to_lowercase();
    let cmd = tab.command.as_deref().unwrap_or("").to_lowercase();
    let hay = format!("{name} {cmd}");
    for launcher in &config.launchers {
        let ln = launcher.name.to_lowercase();
        if hay.contains(&ln) || name == ln {
            let ic = Icon::from_name(&launcher.icon).unwrap_or(Icon::Bot);
            let color =
                ui_theme::parse_hex_color(&launcher.color).unwrap_or(ui_theme::TEXT_MUTED);
            let key = match launcher.logo_key() {
                "opencode" => Some("opencode"),
                "openai" | "codex" => Some("openai"),
                "claude" | "anthropic" => Some("claude"),
                "grok" => Some("grok"),
                "antigravity" => Some("antigravity"),
                "cursor" | "cursor-agent" => Some("cursor"),
                "copilot" | "github-copilot" => Some("copilot"),
                _ => None,
            };
            return (ic, color, key);
        }
    }
    if hay.contains("claude") {
        return (Icon::Sparkles, ui_theme::rgb(0xD9, 0x77, 0x57), Some("claude"));
    }
    if hay.contains("opencode") {
        return (Icon::Sparkles, ui_theme::rgb(0xD9, 0x77, 0x57), Some("opencode"));
    }
    if hay.contains("codex") || hay.contains("openai") {
        return (Icon::Bot, ui_theme::rgb(0x10, 0xA3, 0x7F), Some("openai"));
    }
    if hay.contains("cursor") {
        return (Icon::Bot, ui_theme::rgb(0x5A, 0xA0, 0xF2), Some("cursor"));
    }
    if hay.contains("grok") {
        return (Icon::Bot, ui_theme::TEXT, Some("grok"));
    }
    if hay.contains("antigravity") {
        return (Icon::Bot, ui_theme::rgb(0x5A, 0xA0, 0xF2), Some("antigravity"));
    }
    if hay.contains("copilot") {
        return (Icon::Bot, ui_theme::TEXT, Some("copilot"));
    }
    if tab.command.is_some() {
        return (Icon::Bot, ui_theme::TEXT_MUTED, None);
    }
    (Icon::Terminal, ui_theme::TEXT_MUTED, None)
}

fn icon_square(ui: &mut egui::Ui, kind: Icon, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(ui_theme::RADIUS_KBD), ui_theme::HOVER);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        kind.codepoint().to_string(),
        egui::FontId::new(14.0, egui::FontFamily::Name(ui_theme::FONT_ICONS.into())),
        ui_theme::TEXT_MUTED,
    );
    resp.on_hover_text(tip)
}

fn handle_shortcuts(
    ui: &egui::Ui,
    workspace: &mut Workspace,
    show_menu: &mut bool,
    sidebar: &mut SidebarState,
    palette: &mut CommandPalette,
    files: &mut FileViewer,
) {
    let ctx = ui.ctx().clone();
    let cs = Modifiers::COMMAND | Modifiers::SHIFT;
    let cmd = Modifiers::COMMAND;
    let alt = Modifiers::ALT;

    ui.input_mut(|i| {
        if i.consume_key(cs, Key::T) {
            let profile = crate::shell::default_profile();
            let cwd = workspace.active_tab().and_then(|t| t.cwd.clone());
            let _ = workspace.open_tab(&ctx, &profile, cwd);
        }
        if i.consume_key(cs, Key::W) && workspace.close_focused_pane() {
            workspace.close_active_tab();
        }
        if i.consume_key(cs, Key::K) {
            palette.open = !palette.open;
            if !palette.open {
                palette.query.clear();
            }
        }
        if i.consume_key(cs, Key::B) {
            sidebar.show = !sidebar.show;
        }
        if i.consume_key(cs, Key::F) {
            files.show = !files.show;
        }
        if i.consume_key(cs, Key::L) {
            sidebar.show = true;
            sidebar.search_focus = true;
        }
        if i.key_pressed(Key::Slash) && !i.modifiers.any() && !palette.open {
            let focus = ui.memory(|m| m.focused());
            if focus.is_none() || focus == Some(sidebar::search_field_id()) {
                sidebar.show = true;
                sidebar.search_focus = true;
            }
        }
        if i.consume_key(cs, Key::P) {
            workspace.toggle_pin_active();
        }
        if i.consume_key(cs, Key::N) {
            *show_menu = !*show_menu;
        }
        if i.consume_key(cs, Key::S) {
            if let Err(e) = persist::save_workspace(workspace, &sidebar.archived) {
                log::warn!("manual save failed: {e}");
            } else {
                workspace.dirty = false;
                workspace.skip_save_until_dirty = false;
                workspace.last_autosave = std::time::Instant::now();
            }
        }
        if i.consume_key(cs, Key::D) {
            let _ = workspace.split_focused(&ctx, SplitDir::Vertical);
        }
        if i.consume_key(cs, Key::E) {
            let _ = workspace.split_focused(&ctx, SplitDir::Horizontal);
        }
        if i.consume_key(cs, Key::Tab) && !workspace.tabs.is_empty() {
            let n = workspace.tabs.len();
            workspace.active_tab = (workspace.active_tab + n - 1) % n;
        }
        if i.consume_key(cmd, Key::Tab) && !workspace.tabs.is_empty() {
            workspace.active_tab = (workspace.active_tab + 1) % workspace.tabs.len();
        }
        if i.consume_key(alt, Key::ArrowRight) || i.consume_key(alt, Key::ArrowDown) {
            workspace.focus_next_pane();
        }
        if i.consume_key(alt, Key::ArrowLeft) || i.consume_key(alt, Key::ArrowUp) {
            workspace.focus_prev_pane();
        }
        for (n, key) in [
            (1, Key::Num1),
            (2, Key::Num2),
            (3, Key::Num3),
            (4, Key::Num4),
            (5, Key::Num5),
            (6, Key::Num6),
            (7, Key::Num7),
            (8, Key::Num8),
            (9, Key::Num9),
        ] {
            if i.consume_key(cmd, key) {
                let idx = (n - 1) as usize;
                if idx < workspace.tabs.len() {
                    workspace.active_tab = idx;
                }
            }
        }
    });
}

/// Returns true if a split ratio changed (caller should mark workspace dirty).
fn render_layout(
    ui: &mut egui::Ui,
    tab: &mut crate::layout::Tab,
    font_size: f32,
    dim_inactive: bool,
) -> bool {
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, egui::Sense::hover());
    let focused = tab.focused_pane;
    let crate::layout::Tab {
        layout,
        sessions,
        focused_pane,
        ..
    } = tab;
    let mut dirty = false;
    render_node(
        ui,
        layout,
        sessions,
        focused_pane,
        focused,
        rect,
        &[],
        font_size,
        dim_inactive,
        &mut dirty,
    );
    dirty
}

#[allow(clippy::too_many_arguments)]
fn render_node(
    ui: &mut egui::Ui,
    node: &mut LayoutNode,
    sessions: &mut std::collections::HashMap<u64, crate::terminal::TerminalSession>,
    focused_pane: &mut u64,
    focused: u64,
    rect: egui::Rect,
    path: &[u8],
    font_size: f32,
    dim_inactive: bool,
    dirty: &mut bool,
) {
    const DIVIDER_HIT: f32 = 5.0;
    match node {
        LayoutNode::Leaf(id) => {
            let is_focused = *id == focused;
            if let Some(session) = sessions.get_mut(id) {
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(*ui.layout()),
                );
                // Edge-to-edge: padding created a visible black frame around TUIs.
                Frame::NONE.fill(ui_theme::BG).show(&mut child, |ui| {
                    let response = TerminalView::show(ui, session, is_focused, font_size);
                    if response.clicked() {
                        *focused_pane = *id;
                    }
                });
                if dim_inactive && !is_focused {
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::ZERO,
                        Color32::from_black_alpha(70),
                    );
                }
            }
        }
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let (first_rect, divider_rect, second_rect) = match *direction {
                SplitDir::Vertical => {
                    let split_x = rect.min.x + rect.width() * *ratio;
                    let first_rect = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(split_x - DIVIDER_HIT * 0.5, rect.max.y),
                    );
                    let divider_rect = egui::Rect::from_min_max(
                        egui::pos2(split_x - DIVIDER_HIT * 0.5, rect.min.y),
                        egui::pos2(split_x + DIVIDER_HIT * 0.5, rect.max.y),
                    );
                    let second_rect = egui::Rect::from_min_max(
                        egui::pos2(split_x + DIVIDER_HIT * 0.5, rect.min.y),
                        rect.max,
                    );
                    (first_rect, divider_rect, second_rect)
                }
                SplitDir::Horizontal => {
                    let split_y = rect.min.y + rect.height() * *ratio;
                    let first_rect = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(rect.max.x, split_y - DIVIDER_HIT * 0.5),
                    );
                    let divider_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, split_y - DIVIDER_HIT * 0.5),
                        egui::pos2(rect.max.x, split_y + DIVIDER_HIT * 0.5),
                    );
                    let second_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, split_y + DIVIDER_HIT * 0.5),
                        rect.max,
                    );
                    (first_rect, divider_rect, second_rect)
                }
            };

            let id = ui.scope_id().with(("split_div", path));
            let response = ui.interact(divider_rect, id, egui::Sense::click_and_drag());
            let hovering = response.hovered() || response.dragged();
            if hovering {
                let icon = match *direction {
                    SplitDir::Vertical => egui::CursorIcon::ResizeHorizontal,
                    SplitDir::Horizontal => egui::CursorIcon::ResizeVertical,
                };
                ui.ctx().set_cursor_icon(icon);
            }
            if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let new_ratio = match *direction {
                        SplitDir::Vertical => {
                            ((pos.x - rect.min.x) / rect.width()).clamp(0.1, 0.9)
                        }
                        SplitDir::Horizontal => {
                            ((pos.y - rect.min.y) / rect.height()).clamp(0.1, 0.9)
                        }
                    };
                    if (new_ratio - *ratio).abs() > f32::EPSILON {
                        *ratio = new_ratio;
                        *dirty = true;
                    }
                }
            }
            if response.double_clicked() {
                *ratio = 0.5;
                *dirty = true;
            }
            let div_color = if hovering {
                ui_theme::DIVIDER_ACTIVE
            } else {
                ui_theme::DIVIDER
            };
            let mid = match *direction {
                SplitDir::Vertical => {
                    let x = divider_rect.center().x;
                    egui::Rect::from_min_max(
                        egui::pos2(x, divider_rect.min.y),
                        egui::pos2(x + 1.0, divider_rect.max.y),
                    )
                }
                SplitDir::Horizontal => {
                    let y = divider_rect.center().y;
                    egui::Rect::from_min_max(
                        egui::pos2(divider_rect.min.x, y),
                        egui::pos2(divider_rect.max.x, y + 1.0),
                    )
                }
            };
            ui.painter().rect_filled(mid, 0.0, div_color);

            let mut path_first = path.to_vec();
            path_first.push(0);
            render_node(
                ui,
                first,
                sessions,
                focused_pane,
                focused,
                first_rect,
                &path_first,
                font_size,
                dim_inactive,
                dirty,
            );
            let mut path_second = path.to_vec();
            path_second.push(1);
            render_node(
                ui,
                second,
                sessions,
                focused_pane,
                focused,
                second_rect,
                &path_second,
                font_size,
                dim_inactive,
                dirty,
            );
        }
    }
}
