use crate::config::Config;
use crate::launchers;
use crate::layout::{SessionStatus, Workspace};
use crate::logos::LogoAtlas;
use crate::persist::{self, ArchivedSession};
use crate::ui_theme::{self, Icon};
use egui::{Color32, CornerRadius, Frame, RichText, Sense, Stroke, TextureHandle, Ui, Vec2};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Stable focus id for the sidebar search field (`/` shortcut in `app.rs`).
#[inline]
pub fn search_field_id() -> egui::Id {
    egui::Id::unique("ankar_sidebar_search")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SessionFilter {
    All,
    Waiting,
    Working,
}

pub struct SidebarState {
    pub archived: Vec<ArchivedSession>,
    pub show: bool,
    pub search: String,
    pub filter: SessionFilter,
    pub show_archived: bool,
    pub search_focus: bool,
    pub rename_idx: Option<usize>,
    pub rename_buf: String,
    pub confirm_delete: Option<usize>,
    last_relative_tick: Instant,
    cached_now: Instant,
}

impl Default for SidebarState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            archived: Vec::new(),
            show: true,
            search: String::new(),
            filter: SessionFilter::All,
            show_archived: false,
            search_focus: false,
            rename_idx: None,
            rename_buf: String::new(),
            confirm_delete: None,
            last_relative_tick: now,
            cached_now: now,
        }
    }
}

impl SidebarState {
    fn now(&mut self) -> Instant {
        if self.last_relative_tick.elapsed() >= Duration::from_secs(1) {
            self.cached_now = Instant::now();
            self.last_relative_tick = self.cached_now;
        }
        self.cached_now
    }
}

pub fn draw(
    ui: &mut Ui,
    workspace: &mut Workspace,
    config: &mut Config,
    sidebar: &mut SidebarState,
    logos: &mut LogoAtlas,
    allow_config_save: bool,
    _sidebar_rect: egui::Rect,
) {
    let _ = sidebar.now();
    let mut activate = None;
    let mut archive_idx = None;
    let mut close_idx = None;
    let mut toggle_pin_session = None;
    let mut new_session = false;
    let mut new_in_folder = false;
    let mut restore = None;
    let mut forget = None;
    let mut toggle_group: Option<String> = None;
    let mut start_rename: Option<usize> = None;

    // --- Top actions ---
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        let avail = ui.available_width();
        let square = 30.0;
        let gap = 6.0;
        let main_w = (avail - square - gap).max(80.0);

        let new_resp = Frame::NONE
            .fill(ui_theme::SURFACE_2)
            .corner_radius(CornerRadius::same(ui_theme::RADIUS_CTRL))
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(main_w, 30.0));
                ui.set_max_height(30.0);
                ui.horizontal_centered(|ui| {
                    ui.label(ui_theme::icon(Icon::SquarePen, 14.0, ui_theme::TEXT));
                    ui.add_space(6.0);
                    ui.label(ui_theme::text_title("Nueva terminal"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(ui_theme::text_kbd("⌃⇧T"));
                    });
                });
            })
            .response
            .interact(Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if new_resp.hovered() {
            ui.painter().rect_filled(
                new_resp.rect,
                CornerRadius::same(ui_theme::RADIUS_CTRL),
                ui_theme::HOVER,
            );
        }
        if new_resp.clicked() {
            new_session = true;
        }

        let folder_resp = Frame::NONE
            .fill(ui_theme::SURFACE_2)
            .corner_radius(CornerRadius::same(ui_theme::RADIUS_CTRL))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::splat(30.0));
                ui.centered_and_justified(|ui| {
                    ui.label(ui_theme::icon(Icon::FolderPlus, 14.0, ui_theme::TEXT));
                });
            })
            .response
            .interact(Sense::click())
            .on_hover_text("Nueva sesión en la carpeta activa");
        if folder_resp.clicked() {
            new_in_folder = true;
        }
    });

    ui.add_space(8.0);

    // --- Search ---
    let search_id = search_field_id();
    let search_frame = Frame::NONE
        .fill(ui_theme::SURFACE_SUNKEN)
        .stroke(Stroke::new(1.0, ui_theme::BORDER))
        .corner_radius(CornerRadius::same(ui_theme::RADIUS_CTRL))
        .inner_margin(egui::Margin::symmetric(8, 0))
        .show(ui, |ui| {
            ui.set_min_height(30.0);
            ui.set_max_height(30.0);
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                ui.label(ui_theme::icon(Icon::Search, 14.0, ui_theme::TEXT_MUTED));
                ui.add_space(4.0);
                // Chip first (RTL) so TextEdit gets a stable remaining width.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    kbd_chip(ui, "⌃⇧L");
                    let edit = egui::TextEdit::singleline(&mut sidebar.search)
                        .id(search_id)
                        .frame(Frame::NONE)
                        .desired_width(ui.available_width())
                        .hint_text("Buscar sesiones")
                        .text_color(ui_theme::TEXT);
                    let resp = ui.add(edit);
                    if sidebar.search_focus {
                        resp.request_focus();
                        sidebar.search_focus = false;
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        sidebar.search.clear();
                        resp.surrender_focus();
                    }
                    // Clicking empty search chrome focuses the field.
                    if resp.clicked() || (ui.rect_contains_pointer(ui.max_rect())
                        && ui.input(|i| i.pointer.primary_clicked())
                        && !resp.has_focus())
                    {
                        resp.request_focus();
                    }
                });
            });
        });
    if search_frame.response.clicked() {
        ui.memory_mut(|m| m.request_focus(search_id));
    }

    ui.add_space(8.0);

    // Counts for filters
    let n_all = workspace.tabs.len();
    let n_waiting = workspace
        .tabs
        .iter()
        .filter(|t| t.status == SessionStatus::Waiting)
        .count();
    let n_working = workspace
        .tabs
        .iter()
        .filter(|t| t.status == SessionStatus::Active)
        .count();

    // --- Segmented filter (short labels so they fit a narrow sidebar) ---
    Frame::NONE
        .fill(ui_theme::SURFACE_SUNKEN)
        .stroke(Stroke::new(1.0, ui_theme::BORDER))
        .corner_radius(CornerRadius::same(ui_theme::RADIUS_CTRL))
        .inner_margin(2.0)
        .show(ui, |ui| {
            ui.set_min_height(28.0);
            let w = (ui.available_width() / 3.0).max(1.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                segment(
                    ui,
                    w,
                    sidebar.filter == SessionFilter::All,
                    "Todas",
                    n_all,
                    None,
                    None,
                    || sidebar.filter = SessionFilter::All,
                );
                segment(
                    ui,
                    w,
                    sidebar.filter == SessionFilter::Waiting,
                    "Esp.",
                    n_waiting,
                    Some((Icon::Circle, ui_theme::WARN)),
                    Some("Esperando"),
                    || sidebar.filter = SessionFilter::Waiting,
                );
                segment(
                    ui,
                    w,
                    sidebar.filter == SessionFilter::Working,
                    "Trab.",
                    n_working,
                    Some((Icon::LoaderCircle, ui_theme::TEXT_MUTED)),
                    Some("Trabajando"),
                    || sidebar.filter = SessionFilter::Working,
                );
            });
        });

    ui.add_space(8.0);

    // Collect filtered indices
    let q = sidebar.search.to_lowercase();
    let filter = sidebar.filter;
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut pinned: Vec<usize> = Vec::new();

    for (i, tab) in workspace.tabs.iter().enumerate() {
        if !matches_filter(tab.status, filter) {
            continue;
        }
        if !q.is_empty() {
            let hay = format!(
                "{} {} {} {}",
                tab.session_name,
                tab.title,
                tab.command.as_deref().unwrap_or(""),
                tab.project_key
            )
            .to_lowercase();
            if !hay.contains(&q) {
                continue;
            }
        }
        if tab.pinned {
            pinned.push(i);
        } else {
            let key = if tab.project_key.is_empty() || tab.project_key == "(no project)" {
                "otras".into()
            } else {
                tab.project_key.clone()
            };
            groups.entry(key).or_default().push(i);
        }
    }

    // Pin launchers + archived to the bottom (midterm).
    let footer_h = if sidebar.show_archived {
        28.0 + 8.0 + (sidebar.archived.len() as f32 * 28.0).min(160.0) + 40.0
    } else {
        28.0 + 8.0 + 40.0
    };
    let list_h = (ui.available_height() - footer_h - 8.0).max(60.0);

    egui::ScrollArea::vertical()
        .id_salt("sidebar_sessions")
        .max_height(list_h)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !pinned.is_empty() {
                draw_group_header(ui, "fijadas", false, false, || {});
                for i in &pinned {
                    draw_session_row(
                        ui,
                        workspace,
                        config,
                        sidebar,
                        logos,
                        *i,
                        &mut activate,
                        &mut archive_idx,
                        &mut close_idx,
                        &mut toggle_pin_session,
                        &mut start_rename,
                    );
                }
            }

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

            for key in keys {
                let indices = groups.get(&key).cloned().unwrap_or_default();
                let display = if key == "otras" {
                    "otras".into()
                } else {
                    PathBuf::from(&key)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| key.clone())
                };
                let collapsed = config.collapsed_groups.iter().any(|g| g == &key);
                let key_clone = key.clone();
                draw_group_header(ui, &display, collapsed, true, || {
                    toggle_group = Some(key_clone.clone());
                });
                if collapsed {
                    continue;
                }
                let mut ordered = indices;
                ordered.sort_by_key(|i| !workspace.tabs[*i].pinned);
                for i in ordered {
                    draw_session_row(
                        ui,
                        workspace,
                        config,
                        sidebar,
                        logos,
                        i,
                        &mut activate,
                        &mut archive_idx,
                        &mut close_idx,
                        &mut toggle_pin_session,
                        &mut start_rename,
                    );
                }
            }
        });

    // Fill remaining space so the footer sits at the bottom.
    let leftover = (ui.available_height() - footer_h).max(0.0);
    if leftover > 0.0 {
        ui.add_space(leftover);
    }

    // --- Footer ---
    let (sep_rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 1.0),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(sep_rect, 0.0, ui_theme::BORDER);
    ui.add_space(8.0);

    // Launcher logos — hover shows name + install status + command
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
        let launchers = config.launchers.clone();
        let cwd = workspace.active_tab().and_then(|t| t.cwd.clone());
        for launcher in &launchers {
            let tip = logos.launcher_tooltip(&launcher.name, &launcher.command, &launcher.args);
            let installed = logos.is_installed(&launcher.command);
            let tex = logos.get(ui.ctx(), launcher.logo_key());
            let resp = logo_or_icon_btn(ui, tex.as_ref(), launcher, installed).on_hover_text(tip);
            if resp.clicked() {
                if installed {
                    let _ = launchers::run_launcher(ui.ctx(), workspace, launcher, cwd.clone());
                } else {
                    let ps = crate::shell::ShellProfile::powershell();
                    let cmd_line = if launcher.args.is_empty() {
                        launcher.command.clone()
                    } else {
                        format!("{} {}", launcher.command, launcher.args.join(" "))
                    };
                    let script = format!(
                        "Write-Host '{name} no está instalado.' -ForegroundColor Yellow; Write-Host \"Comando: {cmd}\" -ForegroundColor Cyan",
                        name = launcher.name.replace('\'', "''"),
                        cmd = cmd_line.replace('"', "'"),
                    );
                    let args = vec!["-NoLogo".into(), "-NoExit".into(), "-Command".into(), script];
                    let _ = workspace.open_command_tab(
                        ui.ctx(),
                        &format!("{} (faltante)", launcher.name),
                        &ps.program,
                        &args,
                        cwd.clone(),
                    );
                }
            }
        }
    });

    ui.add_space(6.0);

    // Archived row
    let arch_count = sidebar.archived.len();
    let arch_resp = ui
        .horizontal(|ui| {
            ui.label(ui_theme::icon(Icon::Archive, 14.0, ui_theme::TEXT_MUTED));
            ui.label(ui_theme::text_title(format!("Archivadas {arch_count}")).color(ui_theme::TEXT_MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(ui_theme::icon(Icon::ChevronRight, 12.0, ui_theme::TEXT_FAINT));
            });
        })
        .response
        .interact(Sense::click());
    if arch_resp.clicked() {
        sidebar.show_archived = !sidebar.show_archived;
    }

    if sidebar.show_archived {
        egui::ScrollArea::vertical()
            .id_salt("sidebar_archived")
            .max_height(160.0)
            .show(ui, |ui| {
                for i in 0..sidebar.archived.len() {
                    let name = sidebar.archived[i].name.clone();
                    ui.horizontal(|ui| {
                        ui.label(ui_theme::text_sub(name));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if sidebar.confirm_delete == Some(i) {
                                if ui
                                    .add(egui::Button::new(ui_theme::text_kbd("Sí")).frame(false))
                                    .clicked()
                                {
                                    forget = Some(i);
                                    sidebar.confirm_delete = None;
                                }
                                if ui
                                    .add(egui::Button::new(ui_theme::text_kbd("No")).frame(false))
                                    .clicked()
                                {
                                    sidebar.confirm_delete = None;
                                }
                                ui.label(ui_theme::text_kbd("¿Borrar?"));
                            } else {
                                if icon_btn(ui, Icon::Trash2, ui_theme::TEXT_MUTED, 20.0)
                                    .on_hover_text("Borrar")
                                    .clicked()
                                {
                                    sidebar.confirm_delete = Some(i);
                                }
                                if icon_btn(ui, Icon::ArchiveRestore, ui_theme::TEXT_MUTED, 20.0)
                                    .on_hover_text("Restaurar")
                                    .clicked()
                                {
                                    restore = Some(i);
                                }
                            }
                        });
                    });
                }
            });
    }

    // --- Apply deferred actions ---
    if let Some(key) = toggle_group {
        if let Some(pos) = config.collapsed_groups.iter().position(|g| g == &key) {
            config.collapsed_groups.remove(pos);
        } else {
            config.collapsed_groups.push(key);
        }
        if allow_config_save {
            let _ = config.save();
        }
    }
    if new_session {
        let profile = crate::shell::default_profile();
        let _ = workspace.open_tab(ui.ctx(), &profile, None);
    }
    if new_in_folder {
        let profile = crate::shell::default_profile();
        let cwd = workspace.active_tab().and_then(|t| t.cwd.clone());
        let _ = workspace.open_tab(ui.ctx(), &profile, cwd);
    }
    if let Some(i) = start_rename {
        if let Some(tab) = workspace.tabs.get(i) {
            sidebar.rename_idx = Some(i);
            sidebar.rename_buf = tab.session_name.clone();
        }
    }
    if let Some(i) = activate {
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
            workspace.mark_dirty();
            if allow_config_save {
                let _ = config.save();
            }
        }
    }
    if let Some(i) = archive_idx {
        if i < workspace.tabs.len() {
            let tab = workspace.tabs.remove(i);
            sidebar.archived.push(ArchivedSession {
                name: tab.session_name,
                profile_id: tab.profile_id,
                cwd: tab.cwd,
                command: tab.command,
                program: tab.program,
                args: tab.args,
            });
            if workspace.active_tab >= workspace.tabs.len() && !workspace.tabs.is_empty() {
                workspace.active_tab = workspace.tabs.len() - 1;
            } else if i < workspace.active_tab {
                workspace.active_tab = workspace.active_tab.saturating_sub(1);
            }
            workspace.mark_dirty();
            let _ = persist::save_workspace(workspace, &sidebar.archived);
            workspace.dirty = false;
            workspace.skip_save_until_dirty = false;
        }
    }
    if let Some(i) = restore {
        let arch = sidebar.archived.remove(i);
        if let (Some(prog), args) = (arch.program.clone(), arch.args.clone()) {
            let _ = workspace.open_command_tab(ui.ctx(), &arch.name, &prog, &args, arch.cwd);
        } else {
            let profile = workspace.profile_by_id(&arch.profile_id);
            let _ = workspace.open_tab(ui.ctx(), &profile, arch.cwd);
            if let Some(tab) = workspace.active_tab_mut() {
                tab.session_name = arch.name;
                tab.command = arch.command;
                tab.program = arch.program;
                tab.args = arch.args;
            }
        }
        workspace.mark_dirty();
    }
    if let Some(i) = forget {
        if i < sidebar.archived.len() {
            sidebar.archived.remove(i);
            workspace.mark_dirty();
        }
    }

    // Commit rename on Enter / click away handled inside row; apply here if buffer set
    if let Some(i) = sidebar.rename_idx {
        if ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
            if let Some(tab) = workspace.tabs.get_mut(i) {
                if !sidebar.rename_buf.trim().is_empty() {
                    tab.session_name = sidebar.rename_buf.trim().to_string();
                    tab.title = tab.session_name.clone();
                    workspace.mark_dirty();
                }
            }
            sidebar.rename_idx = None;
        }
        if ui.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            sidebar.rename_idx = None;
        }
    }
}

fn matches_filter(status: SessionStatus, filter: SessionFilter) -> bool {
    match filter {
        SessionFilter::All => true,
        SessionFilter::Waiting => status == SessionStatus::Waiting,
        SessionFilter::Working => status == SessionStatus::Active,
    }
}

#[allow(clippy::too_many_arguments)]
fn segment(
    ui: &mut Ui,
    width: f32,
    selected: bool,
    label: &str,
    count: usize,
    leading: Option<(Icon, Color32)>,
    tip: Option<&str>,
    mut on_click: impl FnMut(),
) {
    let fill = if selected {
        ui_theme::SURFACE_SEL
    } else {
        Color32::TRANSPARENT
    };
    let text_color = if selected {
        ui_theme::TEXT
    } else {
        ui_theme::TEXT_MUTED
    };
    let mut resp = Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.set_min_width((width - 4.0).max(1.0));
            ui.set_max_width((width - 2.0).max(1.0));
            ui.set_max_height(24.0);
            ui.horizontal_centered(|ui| {
                if let Some((ic, c)) = leading {
                    ui.label(ui_theme::icon(ic, 10.0, c));
                }
                ui.label(RichText::new(label).size(ui_theme::SIZE_SUB).color(text_color));
                if count > 0 {
                    ui.label(
                        RichText::new(format!(" {count}"))
                            .size(ui_theme::SIZE_SUB)
                            .color(ui_theme::TEXT_MUTED),
                    );
                }
            });
        })
        .response
        .interact(Sense::click());
    if let Some(t) = tip {
        resp = resp.on_hover_text(t);
    }
    if resp.clicked() {
        on_click();
    }
}

fn draw_group_header(
    ui: &mut Ui,
    name: &str,
    collapsed: bool,
    interactive: bool,
    mut on_toggle: impl FnMut(),
) {
    ui.add_space(12.0);
    let resp = ui
        .horizontal(|ui| {
            ui.set_min_height(24.0);
            if interactive {
                let chev = if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                };
                // Only show chevron on hover via memory — paint faint always for simplicity.
                ui.label(ui_theme::icon(chev, 12.0, ui_theme::TEXT_FAINT));
            }
            ui.label(ui_theme::text_sub(name.to_string()));
        })
        .response;
    if interactive && resp.interact(Sense::click()).clicked() {
        on_toggle();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_session_row(
    ui: &mut Ui,
    workspace: &Workspace,
    config: &Config,
    sidebar: &mut SidebarState,
    logos: &mut LogoAtlas,
    idx: usize,
    activate: &mut Option<usize>,
    archive_idx: &mut Option<usize>,
    close_idx: &mut Option<usize>,
    toggle_pin: &mut Option<usize>,
    start_rename: &mut Option<usize>,
) {
    let Some(tab) = workspace.tabs.get(idx) else {
        return;
    };
    let selected = idx == workspace.active_tab;
    let (agent_icon, agent_color, logo_key) = agent_visual(tab, config);
    let logo_tex = logo_key.and_then(|k| logos.get(ui.ctx(), k));
    let now = sidebar.cached_now;
    let rel = relative_time(now.duration_since(tab.status_changed_at));
    let (sub, sub_color) = subtitle_for(tab);

    ui.add_space(2.0);
    let fill = if selected {
        ui_theme::SURFACE_SEL
    } else {
        Color32::TRANSPARENT
    };

    let frame = Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
        .inner_margin(egui::Margin::symmetric(8, 6));

    let inner = frame.show(ui, |ui| {
        ui.set_min_height(44.0 - 12.0);
        ui.horizontal(|ui| {
            // Agent logo / icon + status badge
            let icon_resp = if let Some(tex) = logo_tex.as_ref() {
                let size = Vec2::splat(16.0);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                resp
            } else {
                ui.label(ui_theme::icon(agent_icon, 16.0, agent_color))
            };
            let badge_center = egui::pos2(icon_resp.rect.right() - 2.0, icon_resp.rect.top() + 2.0);
            paint_status_badge(ui, badge_center, tab.status, selected);

            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if sidebar.rename_idx == Some(idx) {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut sidebar.rename_buf)
                            .desired_width(ui.available_width() - 40.0)
                            .font(egui::TextStyle::Body),
                    );
                    edit.request_focus();
                } else {
                    ui.label(
                        RichText::new(truncate(&tab.session_name, 28))
                            .size(ui_theme::SIZE_TITLE)
                            .color(ui_theme::TEXT),
                    );
                }
                ui.label(
                    RichText::new(truncate(&sub, 36))
                        .size(ui_theme::SIZE_SUB)
                        .color(sub_color),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Hover actions replace relative time
                let hovered = ui.rect_contains_pointer(ui.max_rect().expand(4.0));
                if hovered {
                    if icon_btn(ui, Icon::Archive, ui_theme::TEXT_MUTED, 20.0)
                        .on_hover_text("Archivar")
                        .clicked()
                    {
                        *archive_idx = Some(idx);
                    }
                    let pin_ic = if tab.pinned {
                        Icon::PinOff
                    } else {
                        Icon::Pin
                    };
                    if icon_btn(ui, pin_ic, ui_theme::TEXT_MUTED, 20.0)
                        .on_hover_text(if tab.pinned { "Desfijar" } else { "Fijar" })
                        .clicked()
                    {
                        *toggle_pin = Some(idx);
                    }
                } else {
                    ui.label(ui_theme::text_sub(rel));
                }
            });
        });
    });

    let resp = inner.response.interact(Sense::click());
    if resp.hovered() && !selected {
        ui.painter().rect_filled(
            resp.rect,
            CornerRadius::same(ui_theme::RADIUS_ROW),
            ui_theme::HOVER,
        );
    }
    if resp.clicked() {
        *activate = Some(idx);
    }
    if resp.double_clicked() {
        *start_rename = Some(idx);
    }
    resp.context_menu(|ui| {
        if ui.button("Renombrar").clicked() {
            *start_rename = Some(idx);
            ui.close();
        }
        let pin_label = if tab.pinned { "Desfijar" } else { "Fijar" };
        if ui.button(pin_label).clicked() {
            *toggle_pin = Some(idx);
            ui.close();
        }
        if ui.button("Archivar").clicked() {
            *archive_idx = Some(idx);
            ui.close();
        }
        if ui.button("Cerrar").clicked() {
            *close_idx = Some(idx);
            ui.close();
        }
    });

    // Spin loader only when working row visible
    if tab.status == SessionStatus::Active && resp.rect.intersects(ui.clip_rect()) {
        // Active = working in midterm terms when not waiting; spin only if we want "working"
        // Prompt: working = loader-circle spinning. We treat Active as working.
        ui.ctx()
            .request_repaint_after(Duration::from_millis(100));
    }
}

fn agent_visual(
    tab: &crate::layout::Tab,
    config: &Config,
) -> (Icon, Color32, Option<&'static str>) {
    let name = tab.session_name.to_lowercase();
    let cmd = tab.command.as_deref().unwrap_or("").to_lowercase();
    let hay = format!("{name} {cmd} {}", tab.profile_id);

    for launcher in &config.launchers {
        let ln = launcher.name.to_lowercase();
        if hay.contains(&ln) || name == ln {
            let ic = Icon::from_name(&launcher.icon).unwrap_or(Icon::Bot);
            let color =
                ui_theme::parse_hex_color(&launcher.color).unwrap_or(ui_theme::TEXT_MUTED);
            // Leak-free: map known keys to static strs for logo lookup.
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

fn logo_or_icon_btn(
    ui: &mut Ui,
    tex: Option<&TextureHandle>,
    launcher: &crate::config::Launcher,
    installed: bool,
) -> egui::Response {
    let size = 26.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(ui_theme::RADIUS_KBD),
            ui_theme::PRESSED,
        );
    }
    let pad = 3.0;
    let img_rect = rect.shrink(pad);
    if let Some(tex) = tex {
        let tint = if installed {
            Color32::WHITE
        } else {
            Color32::from_white_alpha(110)
        };
        ui.painter().image(
            tex.id(),
            img_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            tint,
        );
    } else {
        let ic = Icon::from_name(&launcher.icon).unwrap_or(Icon::Bot);
        let mut color =
            ui_theme::parse_hex_color(&launcher.color).unwrap_or(ui_theme::TEXT_MUTED);
        if !installed {
            color = color.gamma_multiply(0.45);
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            ic.codepoint().to_string(),
            egui::FontId::new(
                size * 0.55,
                egui::FontFamily::Name(ui_theme::FONT_ICONS.into()),
            ),
            color,
        );
    }
    // Green ring when installed (midterm-style)
    if installed {
        let badge = egui::pos2(rect.right() - 3.0, rect.top() + 3.0);
        ui.painter()
            .circle_filled(badge, 3.0, ui_theme::SURFACE_1);
        ui.painter().circle_filled(badge, 2.2, ui_theme::OK);
    }
    resp
}

fn subtitle_for(tab: &crate::layout::Tab) -> (String, Color32) {
    let activity_ok = tab.activity.as_ref().filter(|a| {
        let t = a.trim();
        // Don't show absolute paths as the activity line.
        !(t.chars().nth(1) == Some(':') || t.starts_with('/') || t.starts_with('\\'))
    });
    match tab.status {
        SessionStatus::Active => {
            if let Some(a) = activity_ok {
                (a.clone(), ui_theme::TEXT_MUTED)
            } else {
                ("Trabajando".into(), ui_theme::TEXT_MUTED)
            }
        }
        SessionStatus::Waiting => ("Esperando input".into(), ui_theme::WARN),
        SessionStatus::Finished => {
            if let Some(code) = tab.exit_code {
                if code == 0 {
                    ("Listo".into(), ui_theme::OK)
                } else {
                    (format!("Salió (código {code})"), ui_theme::ERROR)
                }
            } else {
                ("Listo".into(), ui_theme::OK)
            }
        }
    }
}

fn paint_status_badge(ui: &Ui, center: egui::Pos2, status: SessionStatus, selected: bool) {
    let ring = if selected {
        ui_theme::SURFACE_SEL
    } else {
        ui_theme::SURFACE_1
    };
    let r = 3.5;
    match status {
        SessionStatus::Active => {
            // loader-circle: draw rotating arc via icon would need font; use pulsing ring
            let t = ui.input(|i| i.time) as f32;
            let color = ui_theme::TEXT_MUTED;
            ui.painter().circle_filled(center, r + 1.5, ring);
            ui.painter().circle_stroke(
                center,
                r,
                Stroke::new(1.5, color.gamma_multiply(0.5 + 0.5 * t.sin().abs())),
            );
        }
        SessionStatus::Waiting => {
            ui.painter().circle_filled(center, r + 1.5, ring);
            ui.painter().circle_filled(center, r, ui_theme::WARN);
        }
        SessionStatus::Finished => {
            let color = ui_theme::OK;
            ui.painter().circle_filled(center, r + 1.5, ring);
            ui.painter().circle_filled(center, r, color);
        }
    }
}

fn icon_btn(ui: &mut Ui, kind: Icon, color: Color32, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(ui_theme::RADIUS_KBD),
            ui_theme::PRESSED,
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        kind.codepoint().to_string(),
        egui::FontId::new(size * 0.55, egui::FontFamily::Name(ui_theme::FONT_ICONS.into())),
        color,
    );
    resp
}

fn kbd_chip(ui: &mut Ui, label: &str) {
    Frame::NONE
        .stroke(Stroke::new(1.0, ui_theme::BORDER))
        .corner_radius(CornerRadius::same(ui_theme::RADIUS_KBD))
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.label(ui_theme::text_kbd(label));
        });
}

fn relative_time(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}
