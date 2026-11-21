use crate::config::{Config, Launcher};
use crate::layout::Workspace;
use crate::persist::ArchivedSession;
use crate::ui_theme;
use egui::{Key, RichText, Ui};

#[derive(Clone, Debug)]
pub enum PaletteAction {
    FocusTab(usize),
    OpenProfile(String),
    RunLauncher(Launcher),
    ToggleSidebar,
    ToggleFiles,
    NewTab,
    PinActive,
    RestoreArchived(usize),
}

#[derive(Default)]
pub struct CommandPalette {
    pub open: bool,
    pub query: String,
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
        let width = (screen.width() * 0.5).clamp(420.0, 720.0);
        let height = 360.0;
        let rect = egui::Rect::from_center_size(
            screen.center(),
            egui::vec2(width, height),
        );

        egui::Area::new(egui::Id::unique("command_palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(ui_theme::BG_ELEVATED)
                    .stroke(egui::Stroke::new(1.0, ui_theme::BORDER))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(width - 24.0, height - 24.0));
                        ui.label(
                            RichText::new("Command palette")
                                .strong()
                                .color(ui_theme::TEXT),
                        );
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .desired_width(f32::INFINITY)
                                .hint_text("Search sessions, projects, launchers…")
                                .text_color(ui_theme::TEXT),
                        );
                        resp.request_focus();

                        if ui.input(|i| i.key_pressed(Key::Escape)) {
                            self.open = false;
                            self.query.clear();
                        }

                        let q = self.query.to_lowercase();
                        let mut items: Vec<(String, PaletteAction)> = vec![
                            ("New tab".into(), PaletteAction::NewTab),
                            ("Toggle sidebar".into(), PaletteAction::ToggleSidebar),
                            ("Toggle file viewer".into(), PaletteAction::ToggleFiles),
                            ("Pin active session".into(), PaletteAction::PinActive),
                        ];

                        for (i, tab) in workspace.tabs.iter().enumerate() {
                            let label = format!(
                                "Session: {} ({})",
                                tab.session_name,
                                tab.command.as_deref().unwrap_or(&tab.profile_id)
                            );
                            items.push((label, PaletteAction::FocusTab(i)));
                        }

                        for p in &workspace.profiles {
                            items.push((
                                format!("Profile: {}", p.name),
                                PaletteAction::OpenProfile(p.id.clone()),
                            ));
                        }

                        for l in &config.launchers {
                            items.push((
                                format!("Launcher: {}", l.name),
                                PaletteAction::RunLauncher(l.clone()),
                            ));
                        }

                        for (i, a) in archived.iter().enumerate() {
                            let label = format!("Archived: {}", a.name);
                            items.push((label, PaletteAction::RestoreArchived(i)));
                        }

                        items.retain(|(label, _)| q.is_empty() || label.to_lowercase().contains(&q));

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (label, act) in items {
                                if ui
                                    .selectable_label(false, RichText::new(label).color(ui_theme::TEXT))
                                    .clicked()
                                {
                                    action = Some(act);
                                    self.open = false;
                                    self.query.clear();
                                }
                            }
                        });
                    });
            });

        action
    }
}
