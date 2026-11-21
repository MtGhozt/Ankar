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

        // Follow shell cwd from OSC title (focused pane of the active tab).
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

        // Sync file viewer root with active project (home fallback inside set_root).
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
                let font_size = self.config.font_size;
                let ratio_changed =
                    render_layout(ui, &mut self.workspace.tabs[active], font_size);
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

/// Returns true if a split ratio changed (caller should mark workspace dirty).
fn render_layout(ui: &mut egui::Ui, tab: &mut crate::layout::Tab, font_size: f32) -> bool {
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
    dirty: &mut bool,
) {
    const DIVIDER_PX: f32 = 5.0;
    match node {
        LayoutNode::Leaf(id) => {
            let is_focused = *id == focused;
            if let Some(session) = sessions.get_mut(id) {
                let stroke = if is_focused {
                    egui::Stroke::new(1.0, ui_theme::ACCENT)
                } else {
                    egui::Stroke::NONE
                };
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(*ui.layout()),
                );
                egui::Frame::NONE.stroke(stroke).show(&mut child, |ui| {
                    let response = TerminalView::show(ui, session, is_focused, font_size);
                    if response.clicked() {
                        *focused_pane = *id;
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
            let (first_rect, divider_rect, second_rect) = match *direction {
                SplitDir::Vertical => {
                    let split_x = rect.min.x + rect.width() * *ratio;
                    let first_rect = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(split_x - DIVIDER_PX * 0.5, rect.max.y),
                    );
                    let divider_rect = egui::Rect::from_min_max(
                        egui::pos2(split_x - DIVIDER_PX * 0.5, rect.min.y),
                        egui::pos2(split_x + DIVIDER_PX * 0.5, rect.max.y),
                    );
                    let second_rect = egui::Rect::from_min_max(
                        egui::pos2(split_x + DIVIDER_PX * 0.5, rect.min.y),
                        rect.max,
                    );
                    (first_rect, divider_rect, second_rect)
                }
                SplitDir::Horizontal => {
                    let split_y = rect.min.y + rect.height() * *ratio;
                    let first_rect = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(rect.max.x, split_y - DIVIDER_PX * 0.5),
                    );
                    let divider_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, split_y - DIVIDER_PX * 0.5),
                        egui::pos2(rect.max.x, split_y + DIVIDER_PX * 0.5),
                    );
                    let second_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, split_y + DIVIDER_PX * 0.5),
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
                ui_theme::ACCENT
            } else {
                egui::Color32::from_rgb(0x24, 0x24, 0x29)
            };
            if hovering {
                ui.painter().rect_filled(divider_rect, 0.0, div_color);
            } else {
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
            }

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
                dirty,
            );
        }
    }
}
