use crate::config::{Config, Launcher};
use crate::layout::Workspace;
use crate::persist::ArchivedSession;
use crate::ui_theme::{self, Icon};
use egui::{CornerRadius, Key, RichText, Sense, Stroke, Ui, Vec2};

#[derive(Clone, Debug)]
pub enum PaletteAction {
    FocusTab(usize),
    OpenProfile(String),
    RunLauncher(Launcher),
    ToggleSidebar,
    ToggleFiles,
    ToggleStats,
    NewTab,
    PinActive,
    OpenLogFolder,
    RestoreArchived(usize),
}

struct PaletteItem {
    label: String,
    icon: Option<Icon>,
    shortcut: String,
    action: PaletteAction,
}

struct PaletteGroup {
    heading: &'static str,
    items: Vec<PaletteItem>,
}

#[derive(Default)]
pub struct CommandPalette {
    pub open: bool,
    pub query: String,
    pub selected: usize,
}

impl CommandPalette {
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        workspace: &Workspace,
        config: &Config,
        archived: &[ArchivedSession],
    ) -> Option<PaletteAction> {
        if !self.open {
            return None;
        }

        let mut action = None;
        let screen = ui.ctx().viewport_rect();
        let width = (screen.width() * 0.6).clamp(360.0, 620.0);
        let top = screen.top() + 64.0;
        let left = screen.center().x - width * 0.5;

        egui::Area::new(egui::Id::unique("command_palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(left, top))
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(ui_theme::SURFACE_1)
                    .stroke(Stroke::new(1.0, ui_theme::BORDER))
                    .corner_radius(CornerRadius::same(ui_theme::RADIUS_POPUP))
                    .shadow(ui_theme::popup_shadow())
                    .inner_margin(0.0)
                    .show(ui, |ui| {
                        ui.set_min_width(width);
                        ui.set_max_width(width);

                        ui.horizontal(|ui| {
                            ui.set_min_height(40.0);
                            ui.add_space(12.0);
                            ui.label(ui_theme::icon(Icon::Search, 14.0, ui_theme::TEXT_MUTED));
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .frame(egui::Frame::NONE)
                                    .desired_width(width - 48.0)
                                    .hint_text("Search…")
                                    .text_color(ui_theme::TEXT),
                            );
                            resp.request_focus();
                        });
                        let (sep, _) =
                            ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
                        ui.painter().rect_filled(sep, 0.0, ui_theme::BORDER);

                        if ui.input(|i| i.key_pressed(Key::Escape)) {
                            self.open = false;
                            self.query.clear();
                            self.selected = 0;
                        }

                        let q = self.query.to_lowercase();
                        let groups = build_groups(workspace, config, archived, &q);

                        let flat: Vec<&PaletteItem> =
                            groups.iter().flat_map(|g| g.items.iter()).collect();
                        let total = flat.len();
                        if ui.input(|i| i.key_pressed(Key::ArrowDown)) && total > 0 {
                            self.selected = (self.selected + 1) % total;
                        }
                        if ui.input(|i| i.key_pressed(Key::ArrowUp)) && total > 0 {
                            self.selected = (self.selected + total - 1) % total;
                        }
                        if ui.input(|i| i.key_pressed(Key::Enter)) {
                            if let Some(item) = flat.get(self.selected) {
                                action = Some(item.action.clone());
                                self.open = false;
                                self.query.clear();
                                self.selected = 0;
                            }
                        }

                        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                            let mut flat_i = 0usize;
                            for group in &groups {
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(12.0);
                                    ui.label(ui_theme::text_sub(group.heading.to_string()));
                                });
                                for item in &group.items {
                                    let selected = flat_i == self.selected;
                                    let fill = if selected {
                                        ui_theme::SURFACE_SEL
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    };
                                    let row = egui::Frame::NONE
                                        .fill(fill)
                                        .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
                                        .inner_margin(egui::Margin::symmetric(12, 6))
                                        .show(ui, |ui| {
                                            ui.set_min_height(20.0);
                                            ui.horizontal(|ui| {
                                                if let Some(ic) = item.icon {
                                                    ui.label(ui_theme::icon(
                                                        ic,
                                                        16.0,
                                                        ui_theme::TEXT_MUTED,
                                                    ));
                                                }
                                                ui.label(
                                                    RichText::new(item.label.clone())
                                                        .size(ui_theme::SIZE_TITLE)
                                                        .color(ui_theme::TEXT),
                                                );
                                                if !item.shortcut.is_empty() {
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            egui::Frame::NONE
                                                                .stroke(Stroke::new(
                                                                    1.0,
                                                                    ui_theme::BORDER,
                                                                ))
                                                                .corner_radius(CornerRadius::same(
                                                                    ui_theme::RADIUS_KBD,
                                                                ))
                                                                .inner_margin(
                                                                    egui::Margin::symmetric(4, 1),
                                                                )
                                                                .show(ui, |ui| {
                                                                    ui.label(ui_theme::text_kbd(
                                                                        item.shortcut.clone(),
                                                                    ));
                                                                });
                                                        },
                                                    );
                                                }
                                            });
                                        });
                                    let resp = row.response.interact(Sense::click());
                                    if resp.hovered() {
                                        self.selected = flat_i;
                                    }
                                    if resp.clicked() {
                                        action = Some(item.action.clone());
                                        self.open = false;
                                        self.query.clear();
                                        self.selected = 0;
                                    }
                                    flat_i += 1;
                                }
                            }
                        });
                    });
            });

        action
    }
}

fn build_groups(
    workspace: &Workspace,
    config: &Config,
    archived: &[ArchivedSession],
    q: &str,
) -> Vec<PaletteGroup> {
    let mut groups = Vec::new();

    let mut actions = vec![
        PaletteItem {
            label: "New tab".into(),
            icon: Some(Icon::Plus),
            shortcut: "Ctrl+Shift+T".into(),
            action: PaletteAction::NewTab,
        },
        PaletteItem {
            label: "Toggle sidebar".into(),
            icon: Some(Icon::PanelLeft),
            shortcut: "Ctrl+Shift+B".into(),
            action: PaletteAction::ToggleSidebar,
        },
        PaletteItem {
            label: "Toggle files".into(),
            icon: Some(Icon::Folder),
            shortcut: "Ctrl+Shift+F".into(),
            action: PaletteAction::ToggleFiles,
        },
        PaletteItem {
            label: "Agent usage".into(),
            icon: Some(Icon::ChartColumn),
            shortcut: "Ctrl+Shift+I".into(),
            action: PaletteAction::ToggleStats,
        },
        PaletteItem {
            label: "Pin active session".into(),
            icon: Some(Icon::Pin),
            shortcut: "Ctrl+Shift+P".into(),
            action: PaletteAction::PinActive,
        },
        PaletteItem {
            label: "Open log folder".into(),
            icon: Some(Icon::Folder),
            shortcut: String::new(),
            action: PaletteAction::OpenLogFolder,
        },
    ];
    actions.retain(|i| q.is_empty() || i.label.to_lowercase().contains(q));
    if !actions.is_empty() {
        groups.push(PaletteGroup {
            heading: "Acciones",
            items: actions,
        });
    }

    let mut sessions = Vec::new();
    for (i, tab) in workspace.tabs.iter().enumerate() {
        let label = format!(
            "{} ({})",
            tab.session_name,
            tab.command.as_deref().unwrap_or(&tab.profile_id)
        );
        if q.is_empty() || label.to_lowercase().contains(q) {
            sessions.push(PaletteItem {
                label,
                icon: Some(Icon::Terminal),
                shortcut: String::new(),
                action: PaletteAction::FocusTab(i),
            });
        }
    }
    if !sessions.is_empty() {
        groups.push(PaletteGroup {
            heading: "Sesiones",
            items: sessions,
        });
    }

    let mut profiles = Vec::new();
    for p in &workspace.profiles {
        if q.is_empty() || p.name.to_lowercase().contains(q) {
            profiles.push(PaletteItem {
                label: p.name.clone(),
                icon: Some(Icon::Terminal),
                shortcut: String::new(),
                action: PaletteAction::OpenProfile(p.id.clone()),
            });
        }
    }
    if !profiles.is_empty() {
        groups.push(PaletteGroup {
            heading: "Profiles",
            items: profiles,
        });
    }

    let mut launchers = Vec::new();
    for l in &config.launchers {
        if q.is_empty() || l.name.to_lowercase().contains(q) {
            launchers.push(PaletteItem {
                label: l.name.clone(),
                icon: Some(Icon::from_name(&l.icon).unwrap_or(Icon::Bot)),
                shortcut: String::new(),
                action: PaletteAction::RunLauncher(l.clone()),
            });
        }
    }
    if !launchers.is_empty() {
        groups.push(PaletteGroup {
            heading: "Launchers",
            items: launchers,
        });
    }

    let mut arch = Vec::new();
    for (i, a) in archived.iter().enumerate() {
        if q.is_empty() || a.name.to_lowercase().contains(q) {
            arch.push(PaletteItem {
                label: a.name.clone(),
                icon: Some(Icon::Archive),
                shortcut: String::new(),
                action: PaletteAction::RestoreArchived(i),
            });
        }
    }
    if !arch.is_empty() {
        groups.push(PaletteGroup {
            heading: "Archived",
            items: arch,
        });
    }

    groups
}
