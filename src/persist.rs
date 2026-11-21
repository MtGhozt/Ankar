use crate::layout::{LayoutNode, SplitDir, Tab, Workspace};
use crate::shell::{self, ShellProfile};
use crate::terminal::TerminalSession;
use egui::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub session_name: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SavedLayout {
    Leaf {
        id: u64,
        profile_id: String,
        cwd: Option<PathBuf>,
        command: Option<String>,
        #[serde(default)]
        program: Option<String>,
        #[serde(default)]
        args: Vec<String>,
    },
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
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
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
    atomic_write(&path, &text)?;
    Ok(())
}

fn atomic_write(path: &Path, text: &str) -> anyhow::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text)?;
    if path.exists() {
        let bak = path.with_extension("json.bak");
        let _ = fs::remove_file(&bak);
        fs::rename(path, &bak)?;
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::rename(&bak, path);
            return Err(e.into());
        }
        let _ = fs::remove_file(&bak);
    } else {
        fs::rename(&tmp, path)?;
    }
    Ok(())
}

pub type LoadedWorkspace = (Vec<Tab>, usize, Vec<ArchivedSession>, u64, bool);

/// Returns `(tabs, active, archived, next_id, corrupt)`.
/// `corrupt` is true when the JSON did not parse — caller must not overwrite
/// until the user makes a structural change.
pub fn load_workspace(
    ctx: &Context,
    profiles: &[ShellProfile],
    scrollback_lines: usize,
) -> anyhow::Result<LoadedWorkspace> {
    let path = crate::config::Config::sessions_path()?;
    if !path.exists() {
        return Ok((Vec::new(), 0, Vec::new(), 1, false));
    }
    let text = fs::read_to_string(&path)?;
    let saved: SavedWorkspace = match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("sessions.json parse failed: {e}");
            let bad = path.with_extension("json.bad");
            if let Err(copy_err) = fs::copy(&path, &bad) {
                log::warn!("could not back up broken sessions.json: {copy_err}");
            } else {
                log::warn!("broken sessions copied to {}", bad.display());
            }
            return Ok((Vec::new(), 0, Vec::new(), 1, true));
        }
    };
    let mut next_id = 1u64;
    let mut tabs = Vec::new();
    for st in &saved.tabs {
        match restore_tab(ctx, st, profiles, &mut next_id, scrollback_lines) {
            Ok(tab) => tabs.push(tab),
            Err(e) => log::warn!("skip restore tab {}: {e}", st.title),
        }
    }
    let active = saved.active_tab.min(tabs.len().saturating_sub(1));
    Ok((tabs, active, saved.archived, next_id, false))
}

fn tab_to_saved(tab: &Tab) -> SavedTab {
    SavedTab {
        title: tab.title.clone(),
        profile_id: tab.profile_id.clone(),
        cwd: tab.cwd.clone(),
        layout: layout_to_saved(&tab.layout, tab),
        focused_pane: tab.focused_pane,
        command: tab.command.clone(),
        program: tab.program.clone(),
        args: tab.args.clone(),
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
            program: tab.program.clone(),
            args: tab.args.clone(),
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
    let cwd = resolve_cwd(saved.cwd.clone());
    Ok(Tab {
        id: tab_id,
        title: saved.title.clone(),
        profile_id: saved.profile_id.clone(),
        cwd: cwd.clone(),
        layout,
        sessions,
        focused_pane: focused,
        command: saved.command.clone(),
        program: saved.program.clone(),
        args: saved.args.clone(),
        session_name: saved.session_name.clone(),
        pinned: saved.pinned,
        project_key: project_key_for(cwd.as_ref()),
        status: crate::layout::SessionStatus::Active,
    })
}

fn resolve_cwd(cwd: Option<PathBuf>) -> Option<PathBuf> {
    match cwd {
        Some(p) if p.is_dir() => Some(p),
        Some(p) => {
            log::warn!(
                "saved cwd {} no longer exists; falling back to home",
                p.display()
            );
            dirs::home_dir()
        }
        None => None,
    }
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
            program,
            args,
            ..
        } => {
            let id = *next_id;
            *next_id += 1;
            let profile = profiles
                .iter()
                .find(|p| p.id == *profile_id)
                .cloned()
                .unwrap_or_else(shell::default_profile);
            let cwd = resolve_cwd(cwd.clone());
            let (spawn_program, spawn_args) = if let Some(prog) = program.as_ref().filter(|p| !p.is_empty()) {
                (prog.clone(), args.clone())
            } else if command.is_some() {
                // Legacy sessions.json: command-only tabs would be re-wrapped in
                // pwsh -Command and break on paths with spaces. Restore a plain
                // default-profile shell instead.
                log::warn!(
                    "legacy command tab without program/args ({:?}); opening default shell",
                    command
                );
                let def = shell::default_profile();
                (def.program, def.args)
            } else {
                (profile.program.clone(), profile.args.clone())
            };
            let session = TerminalSession::spawn(
                id,
                ctx.clone(),
                &spawn_program,
                &spawn_args,
                cwd,
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
