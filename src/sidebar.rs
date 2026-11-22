use crate::config::Config;
use crate::launchers;
use crate::layout::{SessionStatus, Workspace};
use crate::logos::LogoAtlas;
use crate::persist::{self, ArchivedSession};
use crate::ui_theme::{self, Icon};
use egui::{
    Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Margin, Rect, RichText,
    Sense, Stroke, TextureHandle, Ui, Vec2,
};
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
    sidebar_rect: Rect,
) {
    let _ = sidebar.now();
    let painter = ui.painter().clone();
    let divider_launcher_y = sidebar_rect.bottom() - 92.0;
    let divider_archived_y = sidebar_rect.bottom() - 46.0;
    let filter_bottom_y = sidebar_rect.top() + 151.33;
    let archived_list_h = if sidebar.show_archived {
        (sidebar.archived.len() as f32 * 28.0).min(160.0) + 8.0
    } else {
        0.0
    };
    let scroll_h = (divider_launcher_y - filter_bottom_y - archived_list_h).max(60.0);
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

        let new_inner = Frame::NONE
            .fill(ui_theme::BUTTON_BG)
            .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
            .inner_margin(Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(main_w, 30.0));
                ui.set_max_height(30.0);
                ui.horizontal_centered(|ui| {
                    ui.label(ui_theme::icon(Icon::SquarePen, 16.0, ui_theme::TEXT));
                    ui.add_space(6.0);
                    ui.label(ui_theme::text_title("New session"));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let shortcut = if ui.available_width() > 52.0 {
                            "Ctrl+Shift+T"
                        } else {
                            "⇧^T"
                        };
                        ui.label(
                            RichText::new(shortcut)
                                .size(ui_theme::SIZE_UI)
                                .color(ui_theme::TEXT_MUTED),
                        );
                    });
                });
            })
            .response;
        let new_resp = ui.interact(new_inner.rect, search_field_id().with("new_session"), Sense::click());
        paint_button_top_highlight(&painter, new_resp.rect, ui_theme::RADIUS_ROW);
        if new_resp.hovered() {
            painter.rect_filled(
                new_resp.rect,
                CornerRadius::same(ui_theme::RADIUS_ROW),
                ui_theme::ROW_HOVER,
            );
            paint_button_top_highlight(&painter, new_resp.rect, ui_theme::RADIUS_ROW);
        }
        if new_resp.clicked() {
            new_session = true;
        }

        let folder_inner = Frame::NONE
            .fill(ui_theme::BUTTON_BG)
            .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
            .show(ui, |ui| {
                ui.set_min_size(Vec2::splat(30.0));
                ui.centered_and_justified(|ui| {
                    ui.label(ui_theme::icon(Icon::FolderPlus, 17.0, ui_theme::TEXT_MUTED));
                });
            })
            .response;
        let folder_id = search_field_id().with("new_in_folder");
        let folder_resp = ui
            .interact(folder_inner.rect, folder_id, Sense::click())
            .on_hover_text("New session in active folder");
        paint_button_top_highlight(&painter, folder_resp.rect, ui_theme::RADIUS_ROW);
        if folder_resp.hovered() {
            painter.rect_filled(
                folder_resp.rect,
                CornerRadius::same(ui_theme::RADIUS_ROW),
                ui_theme::ROW_HOVER,
            );
            paint_button_top_highlight(&painter, folder_resp.rect, ui_theme::RADIUS_ROW);
        }
        if folder_resp.clicked() {
            new_in_folder = true;
        }
    });

    ui.add_space(8.0);

    // --- Search ---
    let search_id = search_field_id();
    let search_frame = Frame::NONE
        .fill(ui_theme::INPUT_BG)
        .stroke(Stroke::new(1.0, ui_theme::INPUT_BORDER))
        .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
        .inner_margin(Margin::symmetric(8, 0))
        .show(ui, |ui| {
            ui.set_min_height(29.33);
            ui.set_max_height(29.33);
            ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                ui.set_min_height(29.33);
                ui.label(ui_theme::icon(Icon::Search, 12.5, ui_theme::TEXT_MUTED));
                ui.add_space(4.0);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    kbd_chip(ui, "/");
                    let edit = egui::TextEdit::singleline(&mut sidebar.search)
                        .id(search_id)
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .align(Align2::LEFT_CENTER)
                        .desired_width(ui.available_width())
                        .hint_text(RichText::new("Search agents").color(ui_theme::TEXT_MUTED))
                        .font(FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional))
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
                    if resp.clicked()
                        || (ui.rect_contains_pointer(ui.max_rect())
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

    draw_filter_bar(ui, &mut sidebar.filter, n_all, n_waiting, n_working);

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
            let key = project_group_key(&tab.project_key);
            groups.entry(key).or_default().push(i);
        }
    }

    egui::ScrollArea::vertical()
        .id_salt("sidebar_sessions")
        .max_height(scroll_h)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !pinned.is_empty() {
                draw_group_header(ui, "pinned", false, false, || {});
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
                let display = group_display_name(&key);
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

    // --- Footer (absolute y from sidebar_rect) ---
    painter.hline(
        sidebar_rect.x_range(),
        divider_launcher_y,
        Stroke::new(1.0, ui_theme::SIDEBAR_DIVIDER),
    );
    painter.hline(
        sidebar_rect.x_range(),
        divider_archived_y,
        Stroke::new(1.0, ui_theme::SIDEBAR_DIVIDER),
    );

    let launcher_rect = Rect::from_min_max(
        egui::pos2(sidebar_rect.left(), divider_launcher_y + 1.0),
        egui::pos2(sidebar_rect.right(), divider_archived_y - 1.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(launcher_rect), |ui| {
        ui.set_width(ui.available_width());
        let launchers = config.launchers.clone();
        let cwd = workspace.active_tab().and_then(|t| t.cwd.clone());
        let mut x = sidebar_rect.left() + 16.67;
        let y_center = launcher_rect.center().y;
        for launcher in &launchers {
            let pos = egui::pos2(x, y_center - 11.0);
            let btn_rect = Rect::from_min_size(pos, Vec2::splat(22.0));
            let tip = logos.launcher_tooltip(&launcher.name, &launcher.command, &launcher.args);
            let installed = logos.is_installed(&launcher.command);
            let tex = logos.get(ui.ctx(), launcher.logo_key());
            paint_launcher_button(ui, btn_rect, tex.as_ref(), launcher, installed);
            let resp = ui
                .interact(btn_rect, ui.scope_id().with(&launcher.name), Sense::click())
                .on_hover_text(tip);
            if resp.hovered() {
                ui.painter().rect_filled(
                    btn_rect,
                    CornerRadius::same(ui_theme::RADIUS_KBD),
                    ui_theme::ROW_HOVER,
                );
                paint_launcher_button(ui, btn_rect, tex.as_ref(), launcher, installed);
            }
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
                        "Write-Host '{name} is not installed.' -ForegroundColor Yellow; Write-Host \"Command: {cmd}\" -ForegroundColor Cyan",
                        name = launcher.name.replace('\'', "''"),
                        cmd = cmd_line.replace('"', "'"),
                    );
                    let args = vec!["-NoLogo".into(), "-NoExit".into(), "-Command".into(), script];
                    let _ = workspace.open_command_tab(
                        ui.ctx(),
                        &format!("{} (missing)", launcher.name),
                        &ps.program,
                        &args,
                        cwd.clone(),
                    );
                }
            }
            x += 35.0;
        }
    });

    let archived_rect = Rect::from_min_max(
        egui::pos2(sidebar_rect.left(), divider_archived_y + 1.0),
        egui::pos2(sidebar_rect.right(), sidebar_rect.bottom()),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(archived_rect), |ui| {
        ui.set_width(ui.available_width());
        let arch_count = sidebar.archived.len();
        let row_h = 28.0;
        ui.add_space(4.0);
        let arch_resp = ui
            .horizontal(|ui| {
                ui.add_space(5.33);
                ui.label(ui_theme::icon(Icon::Archive, 18.0, ui_theme::TEXT_MUTED));
                ui.label(
                    RichText::new("Archived")
                        .size(ui_theme::SIZE_UI)
                        .color(ui_theme::TEXT_MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(6.0);
                    if arch_count > 0 {
                        ui.label(
                            RichText::new(format!("{arch_count}"))
                                .size(ui_theme::SIZE_UI_SMALL)
                                .color(ui_theme::TEXT_MUTED),
                        );
                    }
                });
            })
            .response
            .interact(Sense::click());
        if arch_resp.clicked() {
            sidebar.show_archived = !sidebar.show_archived;
        }

        if sidebar.show_archived {
            let list_max = (archived_rect.height() - row_h - 8.0).max(40.0);
            egui::ScrollArea::vertical()
                .id_salt("sidebar_archived")
                .max_height(list_max)
                .show(ui, |ui| {
                    for i in 0..sidebar.archived.len() {
                        let name = sidebar.archived[i].name.clone();
                        ui.horizontal(|ui| {
                            ui.label(ui_theme::text_sub(name));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if sidebar.confirm_delete == Some(i) {
                                    if ui
                                        .add(egui::Button::new(ui_theme::text_kbd("Yes")).frame(false))
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
                                    ui.label(ui_theme::text_kbd("Delete?"));
                                } else {
                                    if icon_btn(ui, Icon::Trash2, ui_theme::TEXT_MUTED, 20.0)
                                        .on_hover_text("Delete")
                                        .clicked()
                                    {
                                        sidebar.confirm_delete = Some(i);
                                    }
                                    if icon_btn(ui, Icon::ArchiveRestore, ui_theme::TEXT_MUTED, 20.0)
                                        .on_hover_text("Restore")
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
    });

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

fn project_group_key(project_key: &str) -> String {
    if project_key.is_empty() || project_key == "(no project)" {
        return "other".into();
    }
    PathBuf::from(project_key)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "other".into())
}

fn group_display_name(key: &str) -> String {
    if key == "other" {
        "other".into()
    } else {
        key.to_string()
    }
}

fn draw_filter_bar(
    ui: &mut Ui,
    filter: &mut SessionFilter,
    n_all: usize,
    n_waiting: usize,
    n_working: usize,
) {
    // Spinner glyph always visible — keep it turning.
    ui.ctx()
        .request_repaint_after(Duration::from_millis(100));
    let time = ui.input(|i| i.time) as f32;

    // midterm: container x=6, w=297.33, h=28, padding 2, 3×96.67 segments.
    let row = ui.available_rect_before_wrap();
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(row.width(), 28.0), Sense::hover());
    let side_l = 6.0;
    let side_r = 5.0;
    let bar_w = (row_rect.width() - side_l - side_r).max(180.0);
    let bar = Rect::from_min_size(
        egui::pos2(row_rect.left() + side_l, row_rect.top()),
        Vec2::new(bar_w, 28.0),
    );
    // Scale mockup x coords (designed for bar at x=6, w=297.33) onto current bar.
    let s = bar.width() / 297.33;
    let map_x = |mx: f32| bar.left() + (mx - 6.0) * s;
    let y_mid = bar.center().y;

    let painter = ui.painter();
    painter.rect(
        bar,
        CornerRadius::same(ui_theme::RADIUS_ROW),
        ui_theme::INPUT_BG,
        Stroke::new(1.0, ui_theme::INPUT_BORDER),
        egui::StrokeKind::Inside,
    );

    let inner = bar.shrink(2.0);
    let seg_w = inner.width() / 3.0;
    let font = FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional);
    let segments = [
        (SessionFilter::All, "All", n_all),
        (SessionFilter::Waiting, "Waiting", n_waiting),
        (SessionFilter::Working, "Working", n_working),
    ];

    for (idx, (seg_filter, label, count)) in segments.iter().enumerate() {
        let seg_rect = Rect::from_min_size(
            inner.min + Vec2::new(idx as f32 * seg_w, 0.0),
            Vec2::new(seg_w, inner.height()),
        );
        let selected = *filter == *seg_filter;
        if selected {
            painter.rect_filled(
                seg_rect,
                CornerRadius::same(5),
                ui_theme::SEGMENT_ACTIVE,
            );
        }
        let id = ui.scope_id().with(format!("filter_{idx}"));
        let resp = ui.interact(seg_rect, id, Sense::click());
        if resp.clicked() {
            *filter = *seg_filter;
        }
        let text_color = if selected {
            ui_theme::TEXT
        } else {
            ui_theme::TEXT_MUTED
        };

        match seg_filter {
            SessionFilter::All => {
                // "All" @ x 42.67, count @ x 62.67 — both 13px.
                let label_rect = painter.text(
                    egui::pos2(map_x(42.67), y_mid),
                    Align2::LEFT_CENTER,
                    *label,
                    font.clone(),
                    text_color,
                );
                painter.text(
                    egui::pos2(label_rect.right() + 3.0, y_mid),
                    Align2::LEFT_CENTER,
                    format!("{count}"),
                    font.clone(),
                    ui_theme::TEXT_MUTED,
                );
            }
            SessionFilter::Waiting => {
                // Dot Ø7 #454545 centered at (128.7, mid); label @ x 139.33.
                painter.circle_filled(
                    egui::pos2(map_x(128.7), y_mid),
                    3.5 * s.clamp(0.85, 1.25),
                    ui_theme::FILTER_WAITING_DOT,
                );
                let label_rect = painter.text(
                    egui::pos2(map_x(139.33), y_mid),
                    Align2::LEFT_CENTER,
                    *label,
                    font.clone(),
                    text_color,
                );
                // Count hidden when 0 (spec).
                if *count > 0 {
                    painter.text(
                        egui::pos2(label_rect.right() + 3.0, y_mid),
                        Align2::LEFT_CENTER,
                        format!("{count}"),
                        font.clone(),
                        ui_theme::TEXT_MUTED,
                    );
                }
            }
            SessionFilter::Working => {
                // Spinner Ø13 at mockup icon box (212, …); label @ 229.33, count @ 284.67.
                let ring_r = 6.5 * s.clamp(0.85, 1.25);
                let ring_c = egui::pos2(map_x(212.0) + ring_r, y_mid);
                paint_working_spinner(painter, ring_c, ring_r, ui_theme::WORKING_RING, time);
                let label_rect = painter.text(
                    egui::pos2(map_x(229.33), y_mid),
                    Align2::LEFT_CENTER,
                    *label,
                    font.clone(),
                    text_color,
                );
                if *count > 0 {
                    let count_x = map_x(284.67).max(label_rect.right() + 3.0);
                    painter.text(
                        egui::pos2(count_x, y_mid),
                        Align2::LEFT_CENTER,
                        format!("{count}"),
                        font.clone(),
                        ui_theme::TEXT_MUTED,
                    );
                }
            }
        }
    }
}

fn draw_group_header(
    ui: &mut Ui,
    name: &str,
    collapsed: bool,
    interactive: bool,
    mut on_toggle: impl FnMut(),
) {
    ui.add_space(8.67);
    let resp = ui
        .horizontal(|ui| {
            ui.add_space(7.33);
            ui.set_min_height(18.0);
            if interactive {
                let chev = if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                };
                ui.label(ui_theme::icon(chev, 12.0, ui_theme::TEXT_FAINT));
            }
            ui.label(
                RichText::new(name)
                    .size(ui_theme::SIZE_UI_SMALL)
                    .color(ui_theme::TEXT_MUTED),
            );
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

    ui.add_space(4.0);
    let fill = if selected {
        ui_theme::ROW_SELECTED
    } else {
        Color32::TRANSPARENT
    };

    let frame = Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(ui_theme::RADIUS_ROW))
        .inner_margin(Margin::symmetric(6, 4));

    let inner = frame.show(ui, |ui| {
        ui.set_min_height(37.0);
        ui.horizontal(|ui| {
            let icon_box = Vec2::splat(26.7);
            let (icon_rect, icon_resp) = ui.allocate_exact_size(icon_box, Sense::hover());
            let glyph_rect = egui::Rect::from_center_size(icon_rect.center(), Vec2::splat(18.0));
            if let Some(tex) = logo_tex.as_ref() {
                ui.painter().image(
                    tex.id(),
                    glyph_rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                ui.painter().text(
                    glyph_rect.center(),
                    Align2::CENTER_CENTER,
                    agent_icon.codepoint().to_string(),
                    FontId::new(18.0, FontFamily::Name(ui_theme::FONT_ICONS.into())),
                    agent_color,
                );
            }
            let badge_center =
                egui::pos2(icon_rect.right() - 3.0, icon_rect.top() + 4.0);
            paint_status_badge(ui, badge_center, tab.status);

            ui.add_space(4.0);
            let time_w = 36.0;
            let text_w = (ui.available_width() - time_w).max(40.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let title_font = FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional);
                if sidebar.rename_idx == Some(idx) {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut sidebar.rename_buf)
                            .desired_width(text_w)
                            .font(egui::TextStyle::Body),
                    );
                    edit.request_focus();
                } else {
                    let title = ellipsize(ui, &tab.session_name, title_font.clone(), text_w);
                    ui.label(
                        RichText::new(title)
                            .size(ui_theme::SIZE_UI)
                            .color(ui_theme::TEXT),
                    );
                }
                let sub_font = FontId::new(ui_theme::SIZE_UI_SMALL, FontFamily::Proportional);
                let sub_text = ellipsize(ui, &sub, sub_font.clone(), text_w);
                ui.label(
                    RichText::new(sub_text)
                        .size(ui_theme::SIZE_UI_SMALL)
                        .color(sub_color),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let hovered = icon_resp.hovered()
                    || ui.rect_contains_pointer(ui.max_rect().expand(2.0));
                if hovered {
                    if icon_btn(ui, Icon::Archive, ui_theme::TEXT_MUTED, 20.0)
                        .on_hover_text("Archive")
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
                        .on_hover_text(if tab.pinned { "Unpin" } else { "Pin" })
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
            ui_theme::ROW_HOVER,
        );
    }
    if resp.clicked() {
        *activate = Some(idx);
    }
    if resp.double_clicked() {
        *start_rename = Some(idx);
    }
    resp.context_menu(|ui| {
        if ui.button("Rename").clicked() {
            *start_rename = Some(idx);
            ui.close();
        }
        let pin_label = if tab.pinned { "Unpin" } else { "Pin" };
        if ui.button(pin_label).clicked() {
            *toggle_pin = Some(idx);
            ui.close();
        }
        if ui.button("Archive").clicked() {
            *archive_idx = Some(idx);
            ui.close();
        }
        if ui.button("Close").clicked() {
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

fn paint_launcher_button(
    ui: &Ui,
    rect: Rect,
    tex: Option<&TextureHandle>,
    launcher: &crate::config::Launcher,
    installed: bool,
) {
    let painter = ui.painter();
    let img_rect = rect.shrink(2.0);
    if let Some(tex) = tex {
        let tint = if installed {
            Color32::WHITE
        } else {
            Color32::from_white_alpha(110)
        };
        painter.image(
            tex.id(),
            img_rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            tint,
        );
    } else {
        let ic = Icon::from_name(&launcher.icon).unwrap_or(Icon::Bot);
        let mut color =
            ui_theme::parse_hex_color(&launcher.color).unwrap_or(ui_theme::TEXT_MUTED);
        if !installed {
            color = color.gamma_multiply(0.45);
        }
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            ic.codepoint().to_string(),
            FontId::new(18.0, FontFamily::Name(ui_theme::FONT_ICONS.into())),
            color,
        );
    }
    if installed {
        let badge = egui::pos2(rect.right() - 2.0, rect.top() + 2.0);
        painter.circle_filled(badge, 4.5, ui_theme::SIDEBAR_BG);
        painter.circle_stroke(
            badge,
            4.5,
            Stroke::new(2.0, ui_theme::LAUNCHER_BADGE),
        );
    }
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
                ("Working".into(), ui_theme::TEXT_MUTED)
            }
        }
        SessionStatus::Waiting => ("Waiting for input".into(), ui_theme::TEXT_MUTED),
        SessionStatus::Finished => {
            if let Some(code) = tab.exit_code {
                if code == 0 {
                    ("Done".into(), ui_theme::OK)
                } else {
                    (format!("Exited (code {code})"), ui_theme::ERROR)
                }
            } else {
                ("Done".into(), ui_theme::OK)
            }
        }
    }
}

fn paint_status_badge(ui: &Ui, center: egui::Pos2, status: SessionStatus) {
    let ring_bg = ui_theme::SIDEBAR_BG;
    let time = ui.input(|i| i.time) as f32;
    let painter = ui.painter();
    match status {
        SessionStatus::Active => {
            painter.circle_filled(center, 6.0, ring_bg);
            paint_dashed_ring(
                painter,
                center,
                4.5,
                ui_theme::WORKING_RING_DIM,
                time,
                1.0,
                8,
            );
        }
        SessionStatus::Waiting => {
            painter.circle_filled(center, 6.0, ring_bg);
            painter.circle_filled(center, 4.0, ui_theme::WAITING);
        }
        SessionStatus::Finished => {
            painter.circle_filled(center, 5.5, ring_bg);
            painter.circle_filled(center, 4.0, ui_theme::STATUS_DOT_OK);
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
        .fill(ui_theme::KBD_BG)
        .stroke(Stroke::new(1.0, ui_theme::KBD_BORDER))
        .corner_radius(CornerRadius::same(ui_theme::RADIUS_KBD))
        .inner_margin(Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(ui_theme::SIZE_UI)
                    .color(ui_theme::TEXT_MUTED),
            );
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

fn ellipsize(ui: &Ui, text: &str, font_id: FontId, max_width: f32) -> String {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font_id.clone(), ui_theme::TEXT);
    if galley.size().x <= max_width {
        return text.to_string();
    }
    const ELLIPSIS: &str = "…";
    let mut end = text.chars().count();
    while end > 0 {
        let prefix: String = text.chars().take(end).collect();
        let trial = format!("{prefix}{ELLIPSIS}");
        let g = ui
            .painter()
            .layout_no_wrap(trial.clone(), font_id.clone(), ui_theme::TEXT);
        if g.size().x <= max_width {
            return trial;
        }
        end -= 1;
    }
    ELLIPSIS.to_string()
}

fn paint_button_top_highlight(painter: &egui::Painter, rect: Rect, radius: u8) {
    let inset = radius as f32 + 1.0;
    if rect.width() > inset * 2.0 {
        painter.hline(
            rect.left() + inset..=rect.right() - inset,
            rect.top() + 1.0,
            Stroke::new(1.0, ui_theme::BUTTON_HIGHLIGHT),
        );
    }
}

/// Filter-chip Working glyph: 8 dots on a ring with alpha fade (matches `i_working.png`).
fn paint_working_spinner(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    color: Color32,
    time: f32,
) {
    const N: u32 = 8;
    let rot = time * 1.8;
    let dot_r = (radius * 0.28).clamp(1.2, 2.4);
    for i in 0..N {
        let a = rot + i as f32 * (std::f32::consts::TAU / N as f32);
        // Brightest at the head; fade clockwise like the midterm asset.
        let t = 1.0 - (i as f32 / N as f32);
        let alpha = (0.18 + 0.82 * t).clamp(0.15, 1.0);
        let c = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (alpha * 255.0) as u8);
        let p = center + Vec2::new(a.cos(), a.sin()) * radius;
        painter.circle_filled(p, dot_r, c);
    }
}

fn paint_dashed_ring(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    color: Color32,
    time: f32,
    stroke_width: f32,
    n_dashes: u32,
) {
    let rot = time * 1.8;
    let arc_span = std::f32::consts::TAU / n_dashes as f32;
    let dash = arc_span * 0.55;
    for i in 0..n_dashes {
        let a0 = rot + i as f32 * arc_span;
        let a1 = a0 + dash;
        let steps = 3usize;
        for s in 0..steps {
            let t0 = a0 + (a1 - a0) * s as f32 / steps as f32;
            let t1 = a0 + (a1 - a0) * (s + 1) as f32 / steps as f32;
            let p0 = center + Vec2::new(t0.cos(), t0.sin()) * radius;
            let p1 = center + Vec2::new(t1.cos(), t1.sin()) * radius;
            painter.line_segment([p0, p1], Stroke::new(stroke_width, color));
        }
    }
}
