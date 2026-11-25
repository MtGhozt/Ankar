use crate::config::Config;
use crate::files::FileViewer;
use crate::launchers;
use crate::layout::{LayoutNode, SessionStatus, SplitDir, Workspace};
use crate::logos::LogoAtlas;
use crate::palette::{CommandPalette, PaletteAction};
use crate::persist;
use crate::sidebar::{self, SidebarState, SidebarView};
use crate::stats::{AnkarStatsStore, StatsPanel};
use crate::status::StatusTracker;
use crate::terminal::TerminalView;
use crate::ui_theme::{self, Icon};
use egui::{
    Color32, Context, CornerRadius, Frame, Key, Modifiers, Sense, Stroke, Vec2,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct AnkarApp {
    workspace: Workspace,
    config: Config,
    /// When true, `on_exit` must not overwrite the user's broken config.toml.
    config_load_failed: bool,
    sidebar: SidebarState,
    show_profile_menu: bool,
    /// Sidebar visibility last frame, to spot the discrete layout jump.
    last_sidebar_show: bool,
    status: StatusTracker,
    /// Last foreground-process probe (at most one per second).
    last_probe: std::time::Instant,
    palette: CommandPalette,
    files: FileViewer,
    logos: LogoAtlas,
    stats: StatsPanel,
    ankar_stats: Arc<Mutex<AnkarStatsStore>>,
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
        let ankar_stats = Arc::new(Mutex::new(AnkarStatsStore::default()));
        let launcher_commands = config.launchers.iter().map(|l| l.command.clone()).collect();
        let logos = LogoAtlas::new(&cc.egui_ctx, launcher_commands);
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
            last_sidebar_show: true,
            status: StatusTracker::default(),
            last_probe: std::time::Instant::now(),
            palette: CommandPalette::default(),
            files: FileViewer::default(),
            logos,
            stats: StatsPanel::new(ankar_stats.clone()),
            ankar_stats,
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
                self.sidebar.view = SidebarView::Files;
                self.sidebar.show = true;
            }
            PaletteAction::NewTab => {
                let profile = crate::shell::default_profile();
                let cwd = self.workspace.active_tab().and_then(|t| t.cwd.clone());
                let _ = self.workspace.open_tab(ctx, &profile, cwd);
            }
            PaletteAction::PinActive => {
                self.workspace.toggle_pin_active();
            }
            PaletteAction::ToggleStats => {
                self.stats.toggle();
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
    /// eframe's default is translucent #0C0C0C, which shows through unpainted gaps.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let focused = ui.input(|i| i.focused);
        self.status
            .tick(&mut self.workspace, &self.config, focused);
        if let Ok(mut store) = self.ankar_stats.lock() {
            store.tick(&self.workspace);
        }
        if self.last_probe.elapsed() >= Duration::from_secs(1) {
            self.last_probe = std::time::Instant::now();
            crate::procs::detect_active(&mut self.workspace);
        }
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
                            // The sidebar group follows the shell (OSC 7) into another project.
                            tab.project_key = persist::project_key_for(Some(&new_cwd));
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
        self.files.sync_shell_cwd(cwd);

        if let Some(size) = ui.input_mut(|i| zoom_request(i, self.config.font_size)) {
            self.config.font_size = size;
            resize_now(&mut self.workspace);
            if !self.config_load_failed {
                let _ = self.config.save();
            }
        }
        ui_theme::set_term_font_size(self.config.font_size);

        handle_shortcuts(
            ui,
            &mut self.workspace,
            &mut self.show_profile_menu,
            &mut self.sidebar,
            &mut self.palette,
            &mut self.stats,
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
                    sidebar::draw(
                        ui,
                        &mut self.workspace,
                        &mut self.config,
                        &mut self.sidebar,
                        &mut self.files,
                        &mut self.logos,
                        !self.config_load_failed,
                    );
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
            if resize.drag_stopped() {
                resize_now(&mut self.workspace);
                if !self.config_load_failed {
                    let _ = self.config.save();
                }
            }
        }

        // Our own title bar, right of the sidebar column: the OS frame is gone (chromeless window).
        egui::Panel::top("tab_bar")
            .exact_size(TAB_BAR_H)
            .frame(Frame::NONE.fill(ui_theme::BG))
            .show_separator_line(false)
            .show(ui, |ui| {
                let open_stats = draw_tab_bar(
                    ui,
                    &mut self.workspace,
                    &mut self.sidebar,
                    &mut self.show_profile_menu,
                    &self.config,
                    &mut self.logos,
                );
                if open_stats {
                    self.stats.toggle();
                }
            });

        // The tree is dropped (caches, highlighter) while the sidebar is not showing it.
        if !(self.sidebar.show && self.sidebar.view == SidebarView::Files) {
            self.files.release();
        }

        let recent_dirs = if self.show_profile_menu {
            recent_dirs(&self.workspace, self.files.root.as_ref())
        } else {
            Vec::new()
        };
        if self.show_profile_menu {
            egui::Area::new(egui::Id::unique("profile_menu"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8.0, TAB_BAR_H + 2.0))
                .show(ui.ctx(), |ui| {
                    Frame::NONE
                        .fill(ui_theme::SURFACE_1)
                        .stroke(Stroke::new(1.0, ui_theme::BORDER))
                        .corner_radius(CornerRadius::same(ui_theme::RADIUS_POPUP))
                        .shadow(ui_theme::popup_shadow())
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.set_min_width(180.0);
                            ui.label(ui_theme::text_sub("Profiles"));
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
                            ui.separator();
                            ui.label(ui_theme::text_sub("New tab in"));
                            for dir in &recent_dirs {
                                let name = dir
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| dir.display().to_string());
                                let resp = ui
                                    .add(
                                        egui::Button::new(ui_theme::text_title(name))
                                            .frame(false),
                                    )
                                    .on_hover_text(dir.display().to_string());
                                if resp.clicked() {
                                    let profile = crate::shell::default_profile();
                                    let _ = self.workspace.open_tab(
                                        ui.ctx(),
                                        &profile,
                                        Some(dir.clone()),
                                    );
                                    self.show_profile_menu = false;
                                }
                            }
                        });
                });
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.show_profile_menu = false;
            }
        }

        // Showing/hiding a panel is a discrete jump: no point debouncing it.
        if self.sidebar.show != self.last_sidebar_show {
            self.last_sidebar_show = self.sidebar.show;
            resize_now(&mut self.workspace);
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(ui_theme::BG))
            .show(ui, |ui| {
                if self.workspace.tabs.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(ui_theme::text_sub(
                            "Open a tab with Ctrl+Shift+T or the + button.",
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

        self.stats.draw(ui);
        window_edge_resize(ui.ctx());
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Ok(mut store) = self.ankar_stats.lock() {
            store.flush();
        }
        if self.workspace.should_persist() {
            let _ = persist::save_workspace(&self.workspace, &self.sidebar.archived);
        }
        if !self.config_load_failed {
            let _ = self.config.save();
        }
    }
}

/// Directories for the "+" menu: the Files root, open tab cwds, then home.
fn recent_dirs(workspace: &Workspace, files_root: Option<&PathBuf>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |dir: PathBuf| {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    if let Some(root) = files_root {
        push(root.clone());
    }
    for tab in &workspace.tabs {
        if let Some(cwd) = &tab.cwd {
            push(cwd.clone());
        }
    }
    if let Some(home) = dirs::home_dir() {
        push(home);
    }
    dirs.truncate(6);
    dirs
}

/// Drag released / panel toggled: the next layout size is applied at once, not debounced.
fn resize_now(workspace: &mut Workspace) {
    for session in workspace.tabs.iter_mut().flat_map(|t| t.sessions.values_mut()) {
        session.apply_next_resize_now();
    }
}

const TAB_WIDTH: f32 = 224.0;
/// Tabs shrink down to this when the bar is crowded; beyond that they are clipped.
const MIN_TAB_WIDTH: f32 = 96.0;
/// Windows-style caption buttons.
const WIN_BTN_W: f32 = 46.0;
/// Fixed slot at the right edge: stats icon (22 px), gap, 3 caption buttons.
const RIGHT_ICONS_W: f32 = 22.0 + 6.0 + 3.0 * WIN_BTN_W;
const TAB_BAR_H: f32 = ui_theme::BAR_H;
/// Caption-button hover for close, as in Windows.
const CLOSE_RED: Color32 = Color32::from_rgb(0xE8, 0x11, 0x23);
/// Invisible grab zones on the window edges (the OS frame is gone).
const EDGE_GRAB: f32 = 4.0;
const CORNER_GRAB: f32 = 10.0;

fn draw_tab_bar(
    ui: &mut egui::Ui,
    workspace: &mut Workspace,
    sidebar: &mut SidebarState,
    show_menu: &mut bool,
    config: &Config,
    logos: &mut LogoAtlas,
) -> bool {
    let bar_rect = ui.max_rect();
    // Registered first, so every control added later wins the pointer; what is left of the
    // bar is the window's drag handle (double click toggles maximize).
    window_drag_handle(ui, bar_rect, ui.scope_id().with("bar_drag"));
    let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
    let painter = ui.painter();
    // One continuous divider under the whole bar: nothing erases it under the active tab.
    painter.hline(
        bar_rect.x_range(),
        bar_rect.bottom() - 0.5,
        Stroke::new(1.0, ui_theme::CHROME_DIVIDER),
    );

    ui.set_min_height(TAB_BAR_H);
    ui.spacing_mut().item_spacing.x = 0.0;

    let mut switch_to = None;
    let mut close_idx = None;
    let mut open_stats = false;
    let active = workspace.active_tab;

    // The right icons get their own slot; tabs shrink and are clipped to what is left, so
    // they can never slide under the icons and steal their hover.
    let right_rect = egui::Rect::from_min_max(
        egui::pos2(bar_rect.right() - RIGHT_ICONS_W, bar_rect.top()),
        bar_rect.max,
    );
    let left_rect = egui::Rect::from_min_max(
        bar_rect.min,
        egui::pos2(right_rect.left(), bar_rect.bottom()),
    );
    // The "+" slot, plus the toggle while the sidebar is hidden.
    let fixed_left = 13.0 + 28.0 + if sidebar.show { 8.0 } else { 36.0 };
    let tab_w = ((left_rect.width() - fixed_left) / workspace.tabs.len().max(1) as f32)
        .clamp(MIN_TAB_WIDTH, TAB_WIDTH);

    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(left_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
        ui.set_clip_rect(left_rect.intersect(ui.clip_rect()));
        ui.set_min_height(TAB_BAR_H);
        ui.spacing_mut().item_spacing.x = 0.0;

        // Sidebar shown: its own header holds the toggle. Hidden: it moves to this bar's edge.
        ui.add_space(8.0);
        if !sidebar.show {
            let (slot, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
            if crate::sidebar::sidebar_toggle(ui, slot, false).clicked() {
                sidebar.show = true;
            }
        }

        for (i, tab) in workspace.tabs.iter().enumerate() {
            let selected = i == active;
            let (icon, color, logo_key) = crate::logos::agent_visual(tab, config);
            let logo_tex = logo_key.and_then(|k| logos.get(ui.ctx(), k, 16.0));
            let fill = if selected {
                ui_theme::ROW_SELECTED
            } else {
                ui_theme::BG
            };

            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(tab_w, TAB_BAR_H), Sense::click());
            // Fills stop above the divider, which runs unbroken under every tab.
            let body = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.bottom() - 1.0));
            ui.painter().rect_filled(body, CornerRadius::ZERO, fill);

            // Separator only between two inactive tabs.
            if i > 0 {
                let prev_active = i - 1 == active;
                if !selected && !prev_active {
                    let sep_x = rect.left();
                    ui.painter().vline(
                        sep_x,
                        egui::Rangef::new(rect.center().y - 7.33, rect.center().y + 7.33),
                        Stroke::new(1.0, ui_theme::TAB_SEP),
                    );
                }
            }

            // Not `resp.hovered()`: that turns false while the pointer is on the close button.
            let tab_hovered = ui.rect_contains_pointer(rect);
            if tab_hovered && !selected {
                ui.painter().rect_filled(body, CornerRadius::ZERO, ui_theme::HOVER);
            }

            // The X is painted on hover only (midterm), but its slot is always reserved:
            // titles stop before `close_rect`, so the X never lands on top of text.
            let show_close = tab_hovered;
            let close_rect = egui::Rect::from_center_size(
                egui::pos2(rect.right() - 14.0, rect.center().y),
                Vec2::splat(18.0),
            );
            let title_right = close_rect.left() - 4.0;

            let title = if tab.shown_name().is_empty() {
                tab.title.clone()
            } else {
                tab.shown_name().to_owned()
            };

            if selected {
                // Left-aligned: icon at x+5.3, title at x+26.7, fade last 20%.
                let icon_x = rect.left() + 5.3;
                let icon_y = rect.center().y - 8.0;
                paint_tab_icon(
                    ui,
                    logo_tex.as_ref(),
                    icon,
                    color,
                    egui::pos2(icon_x + 8.0, icon_y + 8.0),
                    16.0,
                    tab.status,
                );
                let title_x = rect.left() + 26.7;
                let title_baseline = rect.center().y + 6.33;
                paint_tab_title_fade(
                    ui,
                    &title,
                    egui::pos2(title_x, title_baseline),
                    title_right - title_x,
                    ui_theme::TEXT,
                    ui_theme::ROW_SELECTED,
                );
            } else {
                // Centered: icon + 1px gap + title.
                let font = egui::FontId::proportional(ui_theme::SIZE_UI);
                let galley = ui.fonts_mut(|f| {
                    f.layout_no_wrap(title.clone(), font.clone(), ui_theme::TEXT_MUTED)
                });
                let content_w = 16.0 + 1.0 + galley.size().x;
                // A title that does not fit between the left edge and the X slot is
                // left-aligned and faded out instead of centered under the X.
                let fits = content_w <= title_right - (rect.left() + 8.0);
                let start_x = if fits {
                    rect.center().x - content_w * 0.5
                } else {
                    rect.left() + 8.0
                };
                paint_tab_icon(
                    ui,
                    logo_tex.as_ref(),
                    icon,
                    color,
                    egui::pos2(start_x + 8.0, rect.center().y),
                    16.0,
                    tab.status,
                );
                if fits {
                    ui.painter().galley(
                        egui::pos2(start_x + 17.0, title_baseline_center(rect, &galley)),
                        galley,
                        ui_theme::TEXT_MUTED,
                    );
                } else {
                    let fade_to = if tab_hovered { ui_theme::HOVER } else { ui_theme::BG };
                    paint_tab_title_fade(
                        ui,
                        &title,
                        egui::pos2(start_x + 17.0, rect.center().y + 6.33),
                        title_right - (start_x + 17.0),
                        ui_theme::TEXT_MUTED,
                        fade_to,
                    );
                }
            }

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

        // "+" right after the last tab (gap ≈ 13).
        ui.add_space(13.0);
        let plus = icon_square(ui, Icon::Plus, "New tab (Ctrl+Shift+T)");
        if plus.clicked() {
            let profile = crate::shell::default_profile();
            let cwd = workspace.active_tab().and_then(|t| t.cwd.clone());
            let _ = workspace.open_tab(ui.ctx(), &profile, cwd);
        }
        if plus.secondary_clicked() {
            *show_menu = !*show_menu;
        }
        },
    );

    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(right_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // RTL: the first thing added sits at the window edge.
            let ctx = ui.ctx().clone();
            let cmd = |c| ctx.send_viewport_cmd(c);
            if window_button(ui, WinButton::Close, "Close").clicked() {
                cmd(egui::ViewportCommand::Close);
            }
            let max_kind = if maximized { WinButton::Restore } else { WinButton::Maximize };
            let max_tip = if maximized { "Restore" } else { "Maximize" };
            if window_button(ui, max_kind, max_tip).clicked() {
                cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            if window_button(ui, WinButton::Minimize, "Minimize").clicked() {
                cmd(egui::ViewportCommand::Minimized(true));
            }
            ui.add_space(6.0);
            // Midterm right icon: 18px activity.
            if tab_bar_right_icon(ui, Icon::ChartColumn, "Agent usage (Ctrl+Shift+I)").clicked()
            {
                open_stats = true;
            }
        },
    );

    if let Some(i) = switch_to {
        workspace.active_tab = i;
    }
    if let Some(i) = close_idx {
        if i < workspace.tabs.len() {
            workspace.tabs.remove(i);
            if workspace.active_tab >= workspace.tabs.len() && !workspace.tabs.is_empty() {
                workspace.active_tab = workspace.tabs.len() - 1;
            } else if i < workspace.active_tab {
                workspace.active_tab -= 1;
            }
            workspace.mark_dirty();
        }
    }
    open_stats
}

fn title_baseline_center(rect: egui::Rect, galley: &egui::Galley) -> f32 {
    rect.center().y - galley.size().y * 0.5
}

fn paint_tab_icon(
    ui: &egui::Ui,
    logo: Option<&egui::TextureHandle>,
    icon: Icon,
    color: Color32,
    center: egui::Pos2,
    size: f32,
    status: SessionStatus,
) {
    if let Some(tex) = logo {
        let img = egui::Rect::from_center_size(center, Vec2::splat(size));
        ui.painter().image(
            tex.id(),
            img,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            icon.codepoint().to_string(),
            egui::FontId::new(size, egui::FontFamily::Name(ui_theme::FONT_ICONS.into())),
            color,
        );
    }
    // Small status badge on the icon (Done green / Waiting amber / Working spinner).
    let badge = egui::pos2(center.x + size * 0.35, center.y - size * 0.35);
    match status {
        SessionStatus::Finished => {
            ui.painter()
                .circle_filled(badge, 3.0, ui_theme::STATUS_DOT_OK);
        }
        SessionStatus::Waiting => {
            ui.painter().circle_filled(badge, 3.0, ui_theme::WAITING);
        }
        SessionStatus::Active => {
            let time = ui.input(|i| i.time) as f32;
            ui.painter()
                .circle_filled(badge, 4.5, ui_theme::BG);
            const N: u32 = 8;
            let rot = time * 1.8;
            for i in 0..N {
                let a = rot + i as f32 * (std::f32::consts::TAU / N as f32);
                let t = 1.0 - (i as f32 / N as f32);
                let alpha = (0.18 + 0.82 * t).clamp(0.15, 1.0);
                let c = Color32::from_rgba_unmultiplied(0x90, 0x90, 0x90, (alpha * 255.0) as u8);
                let p = badge + Vec2::new(a.cos(), a.sin()) * 3.0;
                ui.painter().circle_filled(p, 1.0, c);
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

/// Paint title with a transparent→bg fade over the last 20% (no ellipsis).
fn paint_tab_title_fade(
    ui: &egui::Ui,
    title: &str,
    baseline_left: egui::Pos2,
    max_w: f32,
    color: Color32,
    fade_to: Color32,
) {
    if max_w <= 1.0 {
        return;
    }
    let font = egui::FontId::proportional(ui_theme::SIZE_UI);
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(title.to_owned(), font, color));
    let pos = egui::pos2(baseline_left.x, baseline_left.y - galley.size().y * 0.8);
    let clip = egui::Rect::from_min_size(pos, Vec2::new(max_w, galley.size().y + 4.0));
    let painter = ui.painter().with_clip_rect(clip);
    painter.galley(pos, galley, color);
    let fade_w = (max_w * 0.20).max(8.0);
    let fade_rect = egui::Rect::from_min_max(
        egui::pos2(clip.right() - fade_w, clip.top()),
        clip.max,
    );
    let [r, g, b, _] = fade_to.to_array();
    painter.rect_filled(
        fade_rect,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(r, g, b, 220),
    );
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
        Stroke::new(1.0, ui_theme::CHROME_DIVIDER)
    };
    ui.painter().vline(edge_x, y_range, stroke);
    resp
}

/// Empty bar space is the window's handle: drag moves it, double click toggles maximize.
/// Register before the controls on top of it so they win the pointer.
pub(crate) fn window_drag_handle(ui: &egui::Ui, rect: egui::Rect, id: egui::Id) {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
    if resp.double_clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if resp.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
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

/// Which window edge or corner a point at `pos` grabs (None inside, or when maximized).
fn edge_at(pos: egui::Pos2, win: egui::Rect) -> Option<egui::viewport::ResizeDirection> {
    use egui::viewport::ResizeDirection::*;
    let near = |v: f32, edge: f32, grab: f32| (v - edge).abs() <= grab;
    let (l, r) = (near(pos.x, win.left(), CORNER_GRAB), near(pos.x, win.right(), CORNER_GRAB));
    let (t, b) = (near(pos.y, win.top(), CORNER_GRAB), near(pos.y, win.bottom(), CORNER_GRAB));
    match (l, r, t, b) {
        (true, _, true, _) => return Some(NorthWest),
        (_, true, true, _) => return Some(NorthEast),
        (true, _, _, true) => return Some(SouthWest),
        (_, true, _, true) => return Some(SouthEast),
        _ => {}
    }
    if near(pos.x, win.left(), EDGE_GRAB) {
        Some(West)
    } else if near(pos.x, win.right(), EDGE_GRAB) {
        Some(East)
    } else if near(pos.y, win.top(), EDGE_GRAB) {
        Some(North)
    } else if near(pos.y, win.bottom(), EDGE_GRAB) {
        Some(South)
    } else {
        None
    }
}

/// Without the OS frame nothing resizes the window: grab zones on the edges do.
fn window_edge_resize(ctx: &Context) {
    use egui::viewport::ResizeDirection::*;
    let (pos, pressed, win, maximized) = ctx.input(|i| {
        (
            i.pointer.hover_pos(),
            i.pointer.button_pressed(egui::PointerButton::Primary),
            i.content_rect(),
            i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false),
        )
    });
    let Some(dir) = pos.filter(|_| !maximized).and_then(|p| edge_at(p, win)) else {
        return;
    };
    ctx.set_cursor_icon(match dir {
        North | South => egui::CursorIcon::ResizeVertical,
        East | West => egui::CursorIcon::ResizeHorizontal,
        NorthWest | SouthEast => egui::CursorIcon::ResizeNwSe,
        NorthEast | SouthWest => egui::CursorIcon::ResizeNeSw,
    });
    if pressed {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum WinButton {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// A 46 px caption button with a hairline glyph; close turns red on hover.
fn window_button(ui: &mut egui::Ui, kind: WinButton, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(WIN_BTN_W, TAB_BAR_H), Sense::click());
    let close = kind == WinButton::Close;
    let hot = resp.hovered();
    if hot {
        let fill = if close { CLOSE_RED } else { ui_theme::SURFACE_SEL };
        ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
    }
    let color = if hot && close {
        Color32::WHITE
    } else if hot {
        ui_theme::TEXT
    } else {
        ui_theme::TEXT_MUTED
    };
    let stroke = Stroke::new(1.0, color);
    let ppp = ui.ctx().pixels_per_point();
    // Half-pixel centre keeps 1 px strokes crisp.
    let c = egui::pos2(
        (rect.center().x * ppp).floor() / ppp + 0.5 / ppp,
        (rect.center().y * ppp).floor() / ppp + 0.5 / ppp,
    );
    let p = ui.painter();
    match kind {
        WinButton::Minimize => {
            p.hline(c.x - 5.0..=c.x + 5.0, c.y, stroke);
        }
        WinButton::Maximize => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, Vec2::splat(10.0)),
                CornerRadius::ZERO,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        WinButton::Restore => {
            let front = egui::Rect::from_center_size(c + Vec2::new(-1.0, 1.0), Vec2::splat(8.0));
            p.rect_stroke(front, CornerRadius::ZERO, stroke, egui::StrokeKind::Inside);
            // Back window: only the part that peeks out above and to the right.
            let (l, t, r) = (front.left() + 2.0, front.top() - 2.0, front.right() + 2.0);
            p.line_segment([egui::pos2(l, t), egui::pos2(r, t)], stroke);
            p.line_segment([egui::pos2(r, t), egui::pos2(r, t + 8.0)], stroke);
            p.line_segment([egui::pos2(l, t), egui::pos2(l, front.top())], stroke);
            p.line_segment([egui::pos2(front.right(), t + 8.0), egui::pos2(r, t + 8.0)], stroke);
        }
        WinButton::Close => {
            p.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], stroke);
            p.line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], stroke);
        }
    }
    resp.on_hover_text(tip)
}

/// Midterm tab-bar right icons: 18px glyph in a compact hit target.
fn tab_bar_right_icon(ui: &mut egui::Ui, kind: Icon, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(ui_theme::RADIUS_KBD), ui_theme::HOVER);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        kind.codepoint().to_string(),
        egui::FontId::new(18.0, egui::FontFamily::Name(ui_theme::FONT_ICONS.into())),
        ui_theme::TEXT_MUTED,
    );
    resp.on_hover_text(tip)
}

/// consume_key, but a chord also matches by physical key: a layout that reports another logical
/// key for Ctrl+Shift+<letter> must not silently kill the shortcut.
fn consume_chord(i: &mut egui::InputState, mods: Modifiers, key: Key) -> bool {
    if i.consume_key(mods, key) {
        return true;
    }
    let mut hit = false;
    i.events.retain(|e| {
        let is_match = matches!(e, egui::Event::Key { physical_key: Some(k), pressed: true, modifiers, .. }
            if *k == key && modifiers.matches_logically(mods));
        hit |= is_match;
        !is_match
    });
    hit
}

/// Ctrl+= / Ctrl+- / Ctrl+0: the new terminal font size, or `None` when no zoom key was hit.
/// "+" is Shift+= on most layouts, so both are accepted.
fn zoom_request(i: &mut egui::InputState, current: f32) -> Option<f32> {
    let cmd = Modifiers::COMMAND;
    let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
    let mut hit = |keys: &[Key]| {
        keys.iter()
            .fold(false, |acc, k| consume_chord(i, cmd, *k) | consume_chord(i, cmd_shift, *k) | acc)
    };
    if hit(&[Key::Plus, Key::Equals]) {
        Some(zoomed_font(current, 1.0))
    } else if hit(&[Key::Minus]) {
        Some(zoomed_font(current, -1.0))
    } else if hit(&[Key::Num0]) {
        Some(ui_theme::DEFAULT_TERM_FONT)
    } else {
        None
    }
}

fn zoomed_font(current: f32, step: f32) -> f32 {
    (current + step).clamp(ui_theme::MIN_TERM_FONT, ui_theme::MAX_TERM_FONT)
}

/// Debug aid for "my shortcut does nothing": every key event that arrives with Ctrl+Shift down,
/// before any shortcut consumes it, plus who owns the keyboard focus.
fn log_chord_events(events: &[egui::Event], focused: Option<egui::Id>) {
    for e in events {
        if let egui::Event::Key { key, physical_key, pressed, repeat, modifiers } = e {
            if modifiers.command && modifiers.shift {
                log::debug!(
                    "ctrl+shift key: {key:?} physical={physical_key:?} pressed={pressed} repeat={repeat} focus={focused:?}"
                );
            }
        }
    }
}

fn handle_shortcuts(
    ui: &egui::Ui,
    workspace: &mut Workspace,
    show_menu: &mut bool,
    sidebar: &mut SidebarState,
    palette: &mut CommandPalette,
    stats: &mut StatsPanel,
) {
    let ctx = ui.ctx().clone();
    let cs = Modifiers::COMMAND | Modifiers::SHIFT;
    let cmd = Modifiers::COMMAND;
    let alt = Modifiers::ALT;

    if log::log_enabled!(log::Level::Debug) {
        let focused = ctx.memory(|m| m.focused());
        ui.input(|i| log_chord_events(&i.events, focused));
    }

    ui.input_mut(|i| {
        if consume_chord(i, cs, Key::I) {
            stats.toggle();
        }
        if consume_chord(i, cs, Key::T) {
            log::debug!("Ctrl+Shift+T: new session");
            workspace.new_session(&ctx);
        }
        if consume_chord(i, cs, Key::W) && workspace.close_focused_pane() {
            workspace.close_active_tab();
        }
        if consume_chord(i, cs, Key::K) {
            palette.open = !palette.open;
            if !palette.open {
                palette.query.clear();
            }
        }
        if consume_chord(i, cs, Key::B) {
            sidebar.show = !sidebar.show;
        }
        if consume_chord(i, cs, Key::F) {
            sidebar.view = SidebarView::Files;
            sidebar.show = true;
        }
        if consume_chord(i, cs, Key::L) {
            sidebar.show = true;
            sidebar.view = SidebarView::Sessions;
            sidebar.search_focus = true;
        }
        if consume_chord(i, cs, Key::P) {
            workspace.toggle_pin_active();
        }
        if consume_chord(i, cs, Key::N) {
            *show_menu = !*show_menu;
        }
        if consume_chord(i, cs, Key::S) {
            if let Err(e) = persist::save_workspace(workspace, &sidebar.archived) {
                log::warn!("manual save failed: {e}");
            } else {
                workspace.dirty = false;
                workspace.skip_save_until_dirty = false;
                workspace.last_autosave = std::time::Instant::now();
            }
        }
        if consume_chord(i, cs, Key::D) {
            let _ = workspace.split_focused(&ctx, SplitDir::Vertical);
        }
        if consume_chord(i, cs, Key::E) {
            let _ = workspace.split_focused(&ctx, SplitDir::Horizontal);
        }
        if consume_chord(i, cs, Key::Tab) && !workspace.tabs.is_empty() {
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
                    // Midterm: black @ alpha 0.4 (measured brightness ×0.6).
                    ui.painter().rect_filled(
                        rect,
                        CornerRadius::ZERO,
                        ui_theme::PANE_DIM,
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
                ui_theme::BORDER // pane divider #1C1C1C
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::viewport::ResizeDirection::*;

    #[test]
    fn chord_matches_by_logical_or_physical_key() {
        let cs = Modifiers::COMMAND | Modifiers::SHIFT;
        let key_event = |key, physical_key| egui::Event::Key {
            key,
            physical_key,
            pressed: true,
            repeat: false,
            modifiers: cs,
        };
        let ctx = Context::default();
        let hit = |ev| {
            let raw = egui::RawInput { events: vec![ev], ..Default::default() };
            let mut hit = false;
            ctx.begin_pass(raw);
            ctx.input_mut(|i| hit = consume_chord(i, cs, Key::T));
            let _ = ctx.end_pass();
            hit
        };
        assert!(hit(key_event(Key::T, Some(Key::T))));
        assert!(hit(key_event(Key::Y, Some(Key::T))), "other logical key, T by position");
        assert!(!hit(key_event(Key::Y, Some(Key::Y))));
    }

    /// The whole path a real Ctrl+Shift+T takes inside egui: the event as Windows reports it
    /// (ctrl and command both set), `handle_shortcuts` run first in the frame, and nothing left
    /// over for the terminal view to forward to the PTY.
    #[test]
    fn ctrl_shift_t_opens_a_session_and_is_consumed_before_the_terminal_sees_it() {
        let modifiers = Modifiers { ctrl: true, shift: true, command: true, ..Default::default() };
        let key = |k| egui::Event::Key {
            key: k,
            physical_key: Some(Key::T),
            pressed: true,
            repeat: false,
            modifiers,
        };
        for event in [key(Key::T), key(Key::Y)] {
            let ctx = Context::default();
            let mut ws = Workspace {
                tabs: Vec::new(),
                active_tab: 0,
                next_id: 1,
                profiles: crate::shell::list_profiles(),
                scrollback_lines: 100,
                dirty: false,
                skip_save_until_dirty: true,
                last_autosave: std::time::Instant::now(),
            };
            let mut menu = false;
            let mut sidebar = SidebarState::default();
            let mut palette = CommandPalette::default();
            let mut stats = StatsPanel::new(Arc::new(Mutex::new(AnkarStatsStore::default())));
            let mut left_over = usize::MAX;
            let raw = egui::RawInput { events: vec![event], ..Default::default() };
            let _ = ctx.run_ui(raw, |ui| {
                handle_shortcuts(ui, &mut ws, &mut menu, &mut sidebar, &mut palette, &mut stats);
                left_over = ui.input(|i| i.events.len());
            });
            assert_eq!(ws.tabs.len(), 1, "a session was opened");
            assert_eq!(left_over, 0, "the key event never reaches the terminal");
        }
    }

    #[test]
    fn ctrl_equals_minus_zero_zoom_the_terminal_font_within_limits() {
        let press = |key, modifiers| {
            let ev = egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers };
            let ctx = Context::default();
            ctx.begin_pass(egui::RawInput { events: vec![ev], ..Default::default() });
            let size = ctx.input_mut(|i| zoom_request(i, 12.0));
            let _ = ctx.end_pass();
            size
        };
        let ctrl = Modifiers::COMMAND;
        assert_eq!(press(Key::Equals, ctrl), Some(13.0));
        assert_eq!(press(Key::Plus, ctrl | Modifiers::SHIFT), Some(13.0));
        assert_eq!(press(Key::Minus, ctrl), Some(11.0));
        assert_eq!(press(Key::Num0, ctrl), Some(ui_theme::DEFAULT_TERM_FONT));
        assert_eq!(press(Key::Equals, Modifiers::NONE), None);
        assert_eq!(zoomed_font(ui_theme::MAX_TERM_FONT, 1.0), ui_theme::MAX_TERM_FONT);
        assert_eq!(zoomed_font(ui_theme::MIN_TERM_FONT, -1.0), ui_theme::MIN_TERM_FONT);
    }

    #[test]
    fn window_edges_and_corners_map_to_resize_directions() {
        let win = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(800.0, 600.0));
        let at = |x, y| edge_at(egui::pos2(x, y), win);
        assert_eq!(at(0.0, 300.0), Some(West));
        assert_eq!(at(799.0, 300.0), Some(East));
        assert_eq!(at(400.0, 1.0), Some(North));
        assert_eq!(at(400.0, 599.0), Some(South));
        assert_eq!(at(3.0, 3.0), Some(NorthWest));
        assert_eq!(at(797.0, 597.0), Some(SouthEast));
        assert_eq!(at(400.0, 300.0), None);
        assert_eq!(at(400.0, 16.0), None, "the bar below the top edge only drags");
    }
}
