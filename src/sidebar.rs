use crate::config::Config;
use crate::layout::{SessionStatus, Workspace};
use crate::persist::{self, ArchivedSession};
use crate::ui_theme;
use egui::{RichText, Ui};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct SidebarState {
    pub archived: Vec<ArchivedSession>,
    pub show: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            archived: Vec::new(),
            show: true,
        }
    }
}

pub fn draw(
    ui: &mut Ui,
    workspace: &mut Workspace,
    config: &mut Config,
    sidebar: &mut SidebarState,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Sessions")
                .strong()
                .color(ui_theme::TEXT_STRONG),
        );
        if ui.small_button("Hide").clicked() {
            sidebar.show = false;
        }
    });
    ui.separator();

    // Group live tabs by project
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, tab) in workspace.tabs.iter().enumerate() {
        groups.entry(tab.project_key.clone()).or_default().push(i);
    }

    // Sort: pinned projects first
    let mut keys: Vec<String> = groups.keys().cloned().collect();
    keys.sort_by(|a, b| {
        let ap = config.pinned_projects.iter().any(|p| p == a);
        let bp = config.pinned_projects.iter().any(|p| p == b);
        match (ap, bp) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });

    let mut activate = None;
    let mut archive_idx = None;
    let mut toggle_pin_session = None;
    let mut toggle_pin_project: Option<String> = None;

    for key in keys {
        let indices = groups.get(&key).cloned().unwrap_or_default();
        let project_pinned = config.pinned_projects.iter().any(|p| p == &key);
        let display = if key == "(no project)" {
            "No project".into()
        } else {
            PathBuf::from(&key)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| key.clone())
        };

        ui.horizontal(|ui| {
            let pin_label = if project_pinned { "Unpin" } else { "Pin" };
            if ui.small_button(pin_label).clicked() {
                toggle_pin_project = Some(key.clone());
            }
            ui.label(RichText::new(display).strong().color(ui_theme::TEXT));
        });

        // Pinned sessions first within group
        let mut ordered = indices;
        ordered.sort_by_key(|i| !workspace.tabs[*i].pinned);

        for i in ordered {
            let tab = &workspace.tabs[i];
            let status = status_label(tab.status);
            let cmd = tab
                .command
                .clone()
                .unwrap_or_else(|| tab.profile_id.clone());
            let selected = i == workspace.active_tab;
            let row = format!(
                "{} {} — {} [{}]",
                if tab.pinned { "★" } else { "·" },
                tab.session_name,
                cmd,
                status
            );
            ui.horizontal(|ui| {
                if ui.selectable_label(selected, RichText::new(row).small()).clicked() {
                    activate = Some(i);
                }
                if ui.small_button(if tab.pinned { "unpin" } else { "pin" }).clicked() {
                    toggle_pin_session = Some(i);
                }
                if ui.small_button("arch").clicked() {
                    archive_idx = Some(i);
                }
            });
        }
        ui.add_space(6.0);
    }

    if !sidebar.archived.is_empty() {
        ui.separator();
        ui.label(
            RichText::new("Archived")
                .strong()
                .color(ui_theme::TEXT_MUTED),
        );
        let mut restore = None;
        let mut forget = None;
        for (i, arch) in sidebar.archived.iter().enumerate() {
            let label = format!(
                "{} ({})",
                arch.name,
                arch.command.as_deref().unwrap_or(arch.profile_id.as_str())
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).small().color(ui_theme::TEXT_MUTED));
                if ui.small_button("open").clicked() {
                    restore = Some(i);
                }
                if ui.small_button("del").clicked() {
                    forget = Some(i);
                }
            });
        }
        if let Some(i) = restore {
            let arch = sidebar.archived.remove(i);
            let profile = workspace.profile_by_id(&arch.profile_id);
            let _ = workspace.open_tab(ui.ctx(), &profile, arch.cwd);
            if let Some(tab) = workspace.active_tab_mut() {
                tab.session_name = arch.name;
                tab.command = arch.command;
            }
        }
        if let Some(i) = forget {
            sidebar.archived.remove(i);
        }
    }

    if let Some(i) = activate {
        workspace.active_tab = i;
    }
    if let Some(i) = toggle_pin_session {
        if let Some(tab) = workspace.tabs.get_mut(i) {
            tab.pinned = !tab.pinned;
            let name = tab.session_name.clone();
            if tab.pinned {
                if !config.pinned_sessions.contains(&name) {
                    config.pinned_sessions.push(name);
                }
            } else {
                config.pinned_sessions.retain(|s| s != &name);
            }
            let _ = config.save();
        }
    }
    if let Some(key) = toggle_pin_project {
        if let Some(pos) = config.pinned_projects.iter().position(|p| p == &key) {
            config.pinned_projects.remove(pos);
        } else {
            config.pinned_projects.push(key);
        }
        let _ = config.save();
    }
    if let Some(i) = archive_idx {
        if i < workspace.tabs.len() {
            let tab = workspace.tabs.remove(i);
            sidebar.archived.push(ArchivedSession {
                name: tab.session_name,
                profile_id: tab.profile_id,
                cwd: tab.cwd,
                command: tab.command,
            });
            if workspace.active_tab >= workspace.tabs.len() && !workspace.tabs.is_empty() {
                workspace.active_tab = workspace.tabs.len() - 1;
            } else if i < workspace.active_tab {
                workspace.active_tab = workspace.active_tab.saturating_sub(1);
            }
            let _ = persist::save_workspace(workspace, &sidebar.archived);
        }
    }
}

fn status_label(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Active => "active",
        SessionStatus::Waiting => "waiting",
        SessionStatus::Finished => "done",
    }
}
