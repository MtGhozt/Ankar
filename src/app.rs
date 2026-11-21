use crate::config::Config;
use crate::files::FileViewer;
use crate::launchers;
use crate::layout::{LayoutNode, SplitDir, Workspace};
use crate::palette::{CommandPalette, PaletteAction};
use crate::persist;
use crate::sidebar::{self, SidebarState};
use crate::status::StatusTracker;
use crate::terminal::TerminalView;
use crate::ui_theme;
use egui::{Context, Frame, Key, Modifiers, RichText};
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
            sidebar: SidebarState {
                archived,
                show: true,
            },
            show_profile_menu: false,
            status: StatusTracker::default(),
            palette: CommandPalette::default(),
            files: FileViewer::default(),
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

        // Autosave structural changes every 30s.
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

        // Mark which sessions are on the active tab; drop snapshots for the rest.
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

        // Sync file viewer root with active project.
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

        egui::Panel::top("tab_bar")
            .frame(
                egui::Frame::new()
                    .fill(ui_theme::BG_APP)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Ankar")
                            .strong()
                            .color(ui_theme::TEXT_STRONG),
                    );
                    ui.separator();

                    let mut switch_to = None;
                    let mut close_idx = None;
                    for (i, tab) in self.workspace.tabs.iter().enumerate() {
                        let selected = i == self.workspace.active_tab;
                        let title = if tab.pinned {
                            format!("★ {}", tab.title)
                        } else {
                            tab.title.clone()
                        };
                        let label = RichText::new(title).color(if selected {
                            ui_theme::TEXT_STRONG
                        } else {
                            ui_theme::TEXT_MUTED
                        });
                        let tab_resp = ui.selectable_label(selected, label);
                        if selected {
                            let rect = tab_resp.rect;
                            ui.painter().hline(
                                rect.left()..=rect.right(),
                                rect.bottom() - 1.0,
                                egui::Stroke::new(1.5, ui_theme::ACCENT),
                            );
                        }
                        if tab_resp.clicked() {
                            switch_to = Some(i);
                        }
                        if ui
                            .small_button(RichText::new("×").color(ui_theme::TEXT_MUTED))
                            .clicked()
                        {
                            close_idx = Some(i);
                        }
                    }
                    if let Some(i) = switch_to {
                        self.workspace.active_tab = i;
                    }
                    if let Some(i) = close_idx {
                        if i < self.workspace.tabs.len() {
                            self.workspace.tabs.remove(i);
                            if self.workspace.active_tab >= self.workspace.tabs.len()
                                && !self.workspace.tabs.is_empty()
                            {
                                self.workspace.active_tab = self.workspace.tabs.len() - 1;
                            } else if i < self.workspace.active_tab {
                                self.workspace.active_tab =
                                    self.workspace.active_tab.saturating_sub(1);
                            }
                        }
                    }

                    if ui.button("+").clicked() {
                        self.show_profile_menu = !self.show_profile_menu;
                    }
                    if !self.sidebar.show && ui.button("Sessions").clicked() {
                        self.sidebar.show = true;
                    }
                    if ui.button("Files").clicked() {
                        self.files.show = !self.files.show;
                    }
                });

                if self.show_profile_menu {
                    ui.horizontal_wrapped(|ui| {
                        let profiles = self.workspace.profiles.clone();
                        for profile in &profiles {
                            if ui.button(&profile.name).clicked() {
                                let cwd = self.workspace.active_tab().and_then(|t| t.cwd.clone());
                                let _ = self.workspace.open_tab(ui.ctx(), profile, cwd);
                                self.show_profile_menu = false;
                            }
                        }
                    });
                }
            });

        egui::Panel::bottom("launchers")
            .frame(
                egui::Frame::new()
                    .fill(ui_theme::BG_APP)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show_separator_line(true)
            .show(ui, |ui| {
                launchers::draw_bar(ui, &self.config, &mut self.workspace);
            });

        if self.sidebar.show {
            egui::Panel::left("sidebar")
                .resizable(true)
                .default_size(240.0)
                .frame(
                    egui::Frame::new()
                        .fill(ui_theme::BG_APP)
                        .inner_margin(egui::Margin::same(8)),
                )
                .show_separator_line(true)
                .show(ui, |ui| {
                    sidebar::draw(
                        ui,
                        &mut self.workspace,
                        &mut self.config,
                        &mut self.sidebar,
                        !self.config_load_failed,
                    );
                });
        }

        if self.files.show {
            egui::Panel::right("files")
                .resizable(true)
                .default_size(320.0)
                .frame(
                    egui::Frame::new()
                        .fill(ui_theme::BG_APP)
                        .inner_margin(egui::Margin::same(8)),
                )
                .show_separator_line(true)
                .show(ui, |ui| {
                    // Detect selection changes and load file.
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
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(ui_theme::BG_TERM))
            .show(ui, |ui| {
                if self.workspace.tabs.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label("Open a tab with Ctrl+Shift+T or the + button.");
                    });
                    return;
                }
                let active = self.workspace.active_tab;
                let focused_pane = self.workspace.tabs[active].focused_pane;
                render_layout(ui, &mut self.workspace.tabs[active], focused_pane);
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

    // Most-specific (Ctrl+Shift) first so consume_key matching stays correct.
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

fn render_layout(ui: &mut egui::Ui, tab: &mut crate::layout::Tab, focused: u64) {
    render_node(ui, &tab.layout.clone(), tab, focused);
}

fn render_node(
    ui: &mut egui::Ui,
    node: &LayoutNode,
    tab: &mut crate::layout::Tab,
    focused: u64,
) {
    match node {
        LayoutNode::Leaf(id) => {
            let is_focused = *id == focused;
            if let Some(session) = tab.sessions.get_mut(id) {
                let stroke = if is_focused {
                    egui::Stroke::new(1.0, ui_theme::ACCENT)
                } else {
                    egui::Stroke::NONE
                };
                egui::Frame::NONE.stroke(stroke).show(ui, |ui| {
                    let response = TerminalView::show(ui, session, is_focused);
                    if response.clicked() {
                        tab.focused_pane = *id;
                    }
                });
            }
        }
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let available = ui.available_size();
            match direction {
                SplitDir::Vertical => {
                    let left_w = available.x * *ratio;
                    ui.horizontal(|ui| {
                        ui.allocate_ui(egui::vec2(left_w, available.y), |ui| {
                            render_node(ui, first, tab, focused);
                        });
                        ui.separator();
                        ui.allocate_ui(egui::vec2(available.x - left_w - 6.0, available.y), |ui| {
                            render_node(ui, second, tab, focused);
                        });
                    });
                }
                SplitDir::Horizontal => {
                    let top_h = available.y * *ratio;
                    ui.vertical(|ui| {
                        ui.allocate_ui(egui::vec2(available.x, top_h), |ui| {
                            render_node(ui, first, tab, focused);
                        });
                        ui.separator();
                        ui.allocate_ui(egui::vec2(available.x, available.y - top_h - 6.0), |ui| {
                            render_node(ui, second, tab, focused);
                        });
                    });
                }
            }
        }
    }
}
