use crate::config::Config;
use crate::layout::{LayoutNode, SplitDir, Workspace};
use crate::persist;
use crate::sidebar::{self, SidebarState};
use crate::terminal::TerminalView;
use egui::{Color32, Context, Frame, Key, RichText, Theme, Visuals};

pub struct NacarApp {
    workspace: Workspace,
    config: Config,
    sidebar: SidebarState,
    show_profile_menu: bool,
}

impl NacarApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        apply_theme(&cc.egui_ctx);
        let config = Config::load_or_create().unwrap_or_default();
        let (mut workspace, archived) = Workspace::new(&cc.egui_ctx)?;
        for tab in &mut workspace.tabs {
            if config.pinned_sessions.contains(&tab.session_name) {
                tab.pinned = true;
            }
        }
        Ok(Self {
            workspace,
            config,
            sidebar: SidebarState {
                archived,
                show: true,
            },
            show_profile_menu: false,
        })
    }
}

impl eframe::App for NacarApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        handle_shortcuts(ui, &mut self.workspace, &mut self.show_profile_menu, &mut self.sidebar);

        egui::Panel::top("tab_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("nacar")
                        .strong()
                        .color(Color32::from_rgb(0xE8, 0xE8, 0xE8)),
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
                        Color32::from_rgb(0xE8, 0xE8, 0xE8)
                    } else {
                        Color32::from_rgb(0x88, 0x88, 0x88)
                    });
                    if ui.selectable_label(selected, label).clicked() {
                        switch_to = Some(i);
                    }
                    if ui
                        .small_button(RichText::new("×").color(Color32::from_rgb(0x88, 0x88, 0x88)))
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
            });

            if self.show_profile_menu {
                ui.horizontal_wrapped(|ui| {
                    let profiles = self.workspace.profiles.clone();
                    for profile in &profiles {
                        if ui.button(&profile.name).clicked() {
                            let cwd = self
                                .workspace
                                .active_tab()
                                .and_then(|t| t.cwd.clone());
                            let _ = self.workspace.open_tab(ui.ctx(), profile, cwd);
                            self.show_profile_menu = false;
                        }
                    }
                });
            }
        });

        if self.sidebar.show {
            egui::Panel::left("sidebar")
                .resizable(true)
                .default_size(240.0)
                .show(ui, |ui| {
                    sidebar::draw(ui, &mut self.workspace, &mut self.config, &mut self.sidebar);
                });
        }

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::from_rgb(0x12, 0x12, 0x14)))
            .show(ui, |ui| {
                if self.workspace.tabs.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label("Open a tab with Ctrl+T or the + button.");
                    });
                    return;
                }
                let active = self.workspace.active_tab;
                let focused = self.workspace.tabs[active].focused_pane;
                render_layout(ui, &mut self.workspace.tabs[active], focused);
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = persist::save_workspace(&self.workspace, &self.sidebar.archived);
        let _ = self.config.save();
    }
}

fn handle_shortcuts(
    ui: &egui::Ui,
    workspace: &mut Workspace,
    show_menu: &mut bool,
    sidebar: &mut SidebarState,
) {
    let ctx = ui.ctx().clone();
    let mut actions = Vec::new();
    ui.input(|i| {
        for event in &i.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                actions.push((*key, *modifiers));
            }
        }
    });

    for (key, modifiers) in actions {
        match (key, modifiers.ctrl, modifiers.shift, modifiers.alt) {
            (Key::T, true, false, false) => {
                let profile = crate::shell::default_profile();
                let cwd = workspace.active_tab().and_then(|t| t.cwd.clone());
                let _ = workspace.open_tab(&ctx, &profile, cwd);
            }
            (Key::W, true, false, false) => {
                if workspace.close_focused_pane() {
                    workspace.close_active_tab();
                }
            }
            (Key::D, true, true, false) => {
                let _ = workspace.split_focused(&ctx, SplitDir::Vertical);
            }
            (Key::E, true, true, false) => {
                let _ = workspace.split_focused(&ctx, SplitDir::Horizontal);
            }
            (Key::Tab, true, false, false) => {
                if !workspace.tabs.is_empty() {
                    workspace.active_tab = (workspace.active_tab + 1) % workspace.tabs.len();
                }
            }
            (Key::Tab, false, false, true) => {
                workspace.focus_next_pane();
            }
            (Key::N, true, false, false) => {
                *show_menu = !*show_menu;
            }
            (Key::B, true, false, false) => {
                sidebar.show = !sidebar.show;
            }
            (Key::P, true, false, false) => {
                workspace.toggle_pin_active();
            }
            (Key::S, true, true, false) => {
                let _ = persist::save_workspace(workspace, &sidebar.archived);
            }
            _ => {}
        }
    }

    ui.input(|i| {
        for n in 1..=9 {
            let key = match n {
                1 => Key::Num1,
                2 => Key::Num2,
                3 => Key::Num3,
                4 => Key::Num4,
                5 => Key::Num5,
                6 => Key::Num6,
                7 => Key::Num7,
                8 => Key::Num8,
                9 => Key::Num9,
                _ => continue,
            };
            if i.key_pressed(key) && i.modifiers.ctrl {
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
                    egui::Stroke::new(1.0, Color32::from_rgb(0x3A, 0x7C, 0xA5))
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

fn apply_theme(ctx: &Context) {
    let mut visuals = Visuals::dark();
    visuals.window_fill = Color32::from_rgb(0x12, 0x12, 0x14);
    visuals.panel_fill = Color32::from_rgb(0x12, 0x12, 0x14);
    visuals.override_text_color = Some(Color32::from_rgb(0xD0, 0xD0, 0xD0));
    visuals.selection.bg_fill = Color32::from_rgb(0x3A, 0x7C, 0xA5);
    ctx.set_visuals_of(Theme::Dark, visuals);
    ctx.style_mut_of(Theme::Dark, |style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    });
}
