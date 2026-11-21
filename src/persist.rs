use crate::layout::{LayoutNode, SplitDir, Tab, Workspace};
use crate::shell::{self, ShellProfile};
use crate::terminal::TerminalSession;
use egui::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SavedWorkspace {
    pub active_tab: usize,
    pub tabs: Vec<SavedTab>,
    pub archived: Vec<ArchivedSession>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedTab {
    pub title: String,
    pub profile_id: String,
    pub cwd: Option<PathBuf>,
    pub layout: SavedLayout,
    pub focused_pane: u64,
    pub command: Option<String>,
    pub session_name: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SavedLayout {
    Leaf { id: u64, profile_id: String, cwd: Option<PathBuf>, command: Option<String> },
    Split {
        direction: SplitDirSerde,
        ratio: f32,
        first: Box<SavedLayout>,
        second: Box<SavedLayout>,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SplitDirSerde {
    Horizontal,
    Vertical,
}

impl From<SplitDir> for SplitDirSerde {
    fn from(d: SplitDir) -> Self {
        match d {
            SplitDir::Horizontal => Self::Horizontal,
            SplitDir::Vertical => Self::Vertical,
        }
    }
}

impl From<SplitDirSerde> for SplitDir {
    fn from(d: SplitDirSerde) -> Self {
        match d {
            SplitDirSerde::Horizontal => Self::Horizontal,
            SplitDirSerde::Vertical => Self::Vertical,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchivedSession {
    pub name: String,
    pub profile_id: String,
    pub cwd: Option<PathBuf>,
    pub command: Option<String>,
}

pub fn save_workspace(ws: &Workspace, archived: &[ArchivedSession]) -> anyhow::Result<()> {
    let path = crate::config::Config::sessions_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let saved = SavedWorkspace {
        active_tab: ws.active_tab,
        tabs: ws.tabs.iter().map(tab_to_saved).collect(),
        archived: archived.to_vec(),
    };
    let text = serde_json::to_string_pretty(&saved)?;
    fs::write(path, text)?;
    Ok(())
}

pub fn load_workspace(
    ctx: &Context,
    profiles: &[ShellProfile],
    scrollback_lines: usize,
) -> anyhow::Result<(Vec<Tab>, usize, Vec<ArchivedSession>, u64)> {
    let path = crate::config::Config::sessions_path()?;
    if !path.exists() {
        return Ok((Vec::new(), 0, Vec::new(), 1));
    }
    let text = fs::read_to_string(path)?;
    let saved: SavedWorkspace = serde_json::from_str(&text)?;
    let mut next_id = 1u64;
    let mut tabs = Vec::new();
    for st in &saved.tabs {
        match restore_tab(ctx, st, profiles, &mut next_id, scrollback_lines) {
            Ok(tab) => tabs.push(tab),
            Err(e) => log::warn!("skip restore tab {}: {e}", st.title),
        }
    }
    let active = saved.active_tab.min(tabs.len().saturating_sub(1));
    Ok((tabs, active, saved.archived, next_id))
}

fn tab_to_saved(tab: &Tab) -> SavedTab {
    SavedTab {
        title: tab.title.clone(),
        profile_id: tab.profile_id.clone(),
        cwd: tab.cwd.clone(),
        layout: layout_to_saved(&tab.layout, tab),
        focused_pane: tab.focused_pane,
        command: tab.command.clone(),
        session_name: tab.session_name.clone(),
        pinned: tab.pinned,
    }
}

fn layout_to_saved(node: &LayoutNode, tab: &Tab) -> SavedLayout {
    match node {
        LayoutNode::Leaf(id) => SavedLayout::Leaf {
            id: *id,
            profile_id: tab.profile_id.clone(),
            cwd: tab.cwd.clone(),
            command: tab.command.clone(),
        },
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
        } => SavedLayout::Split {
            direction: (*direction).into(),
            ratio: *ratio,
            first: Box::new(layout_to_saved(first, tab)),
            second: Box::new(layout_to_saved(second, tab)),
        },
    }
}

fn restore_tab(
    ctx: &Context,
    saved: &SavedTab,
    profiles: &[ShellProfile],
    next_id: &mut u64,
    scrollback_lines: usize,
) -> anyhow::Result<Tab> {
    let mut sessions = HashMap::new();
    let layout = restore_layout(
        ctx,
        &saved.layout,
        profiles,
        &mut sessions,
        next_id,
        scrollback_lines,
    )?;
    let focused = if sessions.contains_key(&saved.focused_pane) {
        saved.focused_pane
    } else {
        crate::layout::collect_leaves(&layout)
            .into_iter()
            .next()
            .unwrap_or(0)
    };
    let tab_id = *next_id;
    *next_id += 1;
    Ok(Tab {
        id: tab_id,
        title: saved.title.clone(),
        profile_id: saved.profile_id.clone(),
        cwd: saved.cwd.clone(),
        layout,
        sessions,
        focused_pane: focused,
        command: saved.command.clone(),
        session_name: saved.session_name.clone(),
        pinned: saved.pinned,
        project_key: project_key_for(saved.cwd.as_ref()),
        status: crate::layout::SessionStatus::Active,
    })
}

fn restore_layout(
    ctx: &Context,
    node: &SavedLayout,
    profiles: &[ShellProfile],
    sessions: &mut HashMap<u64, TerminalSession>,
    next_id: &mut u64,
    scrollback_lines: usize,
) -> anyhow::Result<LayoutNode> {
    match node {
        SavedLayout::Leaf {
            profile_id,
            cwd,
            command,
            ..
        } => {
            let id = *next_id;
            *next_id += 1;
            let profile = profiles
                .iter()
                .find(|p| p.id == *profile_id)
                .cloned()
                .unwrap_or_else(shell::default_profile);
            let (program, args) = if let Some(cmd) = command {
                // Launch via powershell -Command for custom commands
                (
                    shell::default_profile().program,
                    vec![
                        "-NoLogo".into(),
                        "-Command".into(),
                        cmd.clone(),
                    ],
                )
            } else {
                (profile.program.clone(), profile.args.clone())
            };
            let session = TerminalSession::spawn(
                id,
                ctx.clone(),
                &program,
                &args,
                cwd.clone(),
                scrollback_lines,
            )?;
            sessions.insert(id, session);
            Ok(LayoutNode::Leaf(id))
        }
        SavedLayout::Split {
            direction,
            ratio,
            first,
            second,
        } => Ok(LayoutNode::Split {
            direction: (*direction).into(),
            ratio: *ratio,
            first: Box::new(restore_layout(
                ctx,
                first,
                profiles,
                sessions,
                next_id,
                scrollback_lines,
            )?),
            second: Box::new(restore_layout(
                ctx,
                second,
                profiles,
                sessions,
                next_id,
                scrollback_lines,
            )?),
        }),
    }
}

pub fn project_key_for(cwd: Option<&PathBuf>) -> String {
    cwd.map(|p| p.display().to_string())
        .unwrap_or_else(|| "(no project)".into())
}

pub fn project_name(cwd: Option<&PathBuf>) -> String {
    cwd.and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "No project".into())
}
