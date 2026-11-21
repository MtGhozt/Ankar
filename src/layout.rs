use crate::shell::ShellProfile;
use crate::terminal::TerminalSession;
use egui::Context;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDir {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug)]
pub enum LayoutNode {
    Leaf(u64),
    Split {
        direction: SplitDir,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Waiting,
    Finished,
}

pub struct Tab {
    pub id: u64,
    pub title: String,
    pub profile_id: String,
    pub cwd: Option<PathBuf>,
    pub layout: LayoutNode,
    pub sessions: HashMap<u64, TerminalSession>,
    pub focused_pane: u64,
    pub command: Option<String>,
    /// Executable used to spawn a launcher/command tab (for restore without re-wrapping).
    pub program: Option<String>,
    pub args: Vec<String>,
    pub session_name: String,
    pub pinned: bool,
    pub project_key: String,
    pub status: SessionStatus,
}

pub struct Workspace {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub next_id: u64,
    pub profiles: Vec<ShellProfile>,
    /// Scrollback lines for new sessions (from config; capped in TerminalSession).
    pub scrollback_lines: usize,
    /// Structural changes since last successful persist.
    pub dirty: bool,
    /// Sessions file was corrupt — do not overwrite until the user changes something.
    pub skip_save_until_dirty: bool,
    pub last_autosave: std::time::Instant,
}

impl Workspace {
    pub fn new(
        ctx: &Context,
        scrollback_lines: usize,
    ) -> anyhow::Result<(Self, Vec<crate::persist::ArchivedSession>)> {
        let profiles = crate::shell::list_profiles();
        let (tabs, active_tab, archived, next_id, corrupt) =
            crate::persist::load_workspace(ctx, &profiles, scrollback_lines).unwrap_or_else(|e| {
                log::warn!("session restore failed: {e}");
                (Vec::new(), 0, Vec::new(), 1, false)
            });
        let mut ws = Self {
            tabs,
            active_tab,
            next_id,
            profiles,
            scrollback_lines,
            dirty: false,
            skip_save_until_dirty: corrupt,
            last_autosave: std::time::Instant::now(),
        };
        if ws.tabs.is_empty() {
            let profile = crate::shell::default_profile();
            ws.open_tab(ctx, &profile, None)?;
            // Opening the first tab after a corrupt load should not force a save
            // of an empty replacement until the user makes a real change — but we
            // do need a working tab. Keep skip_save_until_dirty if corrupt.
            if !corrupt {
                ws.dirty = false;
            }
        }
        Ok((ws, archived))
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn should_persist(&self) -> bool {
        !self.skip_save_until_dirty || self.dirty
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn open_tab(
        &mut self,
        ctx: &Context,
        profile: &ShellProfile,
        cwd: Option<PathBuf>,
    ) -> anyhow::Result<u64> {
        let cwd = cwd.or_else(dirs::home_dir);
        let pane_id = self.alloc_id();
        let tab_id = self.alloc_id();
        let session = TerminalSession::spawn(
            pane_id,
            ctx.clone(),
            &profile.program,
            &profile.args,
            cwd.clone(),
            self.scrollback_lines,
        )?;
        let mut sessions = HashMap::new();
        sessions.insert(pane_id, session);
        let project_key = crate::persist::project_key_for(cwd.as_ref());
        let session_name = crate::persist::project_name(cwd.as_ref());
        self.tabs.push(Tab {
            id: tab_id,
            title: profile.name.clone(),
            profile_id: profile.id.clone(),
            cwd,
            layout: LayoutNode::Leaf(pane_id),
            sessions,
            focused_pane: pane_id,
            command: None,
            program: None,
            args: Vec::new(),
            session_name,
            pinned: false,
            project_key,
            status: SessionStatus::Active,
        });
        self.active_tab = self.tabs.len() - 1;
        self.mark_dirty();
        Ok(tab_id)
    }

    pub fn open_command_tab(
        &mut self,
        ctx: &Context,
        name: &str,
        program: &str,
        args: &[String],
        cwd: Option<PathBuf>,
    ) -> anyhow::Result<u64> {
        let pane_id = self.alloc_id();
        let tab_id = self.alloc_id();
        let session = TerminalSession::spawn(
            pane_id,
            ctx.clone(),
            program,
            args,
            cwd.clone(),
            self.scrollback_lines,
        )?;
        let mut sessions = HashMap::new();
        sessions.insert(pane_id, session);
        let project_key = crate::persist::project_key_for(cwd.as_ref());
        let cmd_display = std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        self.tabs.push(Tab {
            id: tab_id,
            title: name.to_string(),
            profile_id: "command".into(),
            cwd,
            layout: LayoutNode::Leaf(pane_id),
            sessions,
            focused_pane: pane_id,
            command: Some(cmd_display),
            program: Some(program.to_string()),
            args: args.to_vec(),
            session_name: name.to_string(),
            pinned: false,
            project_key,
            status: SessionStatus::Active,
        });
        self.active_tab = self.tabs.len() - 1;
        self.mark_dirty();
        Ok(tab_id)
    }

    pub fn toggle_pin_active(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            tab.pinned = !tab.pinned;
            self.mark_dirty();
        }
    }

    pub fn close_active_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.tabs.remove(self.active_tab);
        if self.tabs.is_empty() {
            self.active_tab = 0;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.mark_dirty();
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    pub fn profile_by_id(&self, id: &str) -> ShellProfile {
        self.profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .unwrap_or_else(crate::shell::default_profile)
    }

    pub fn split_focused(&mut self, ctx: &Context, direction: SplitDir) -> anyhow::Result<()> {
        let (profile_id, cwd, focused) = {
            let tab = self
                .active_tab_mut()
                .ok_or_else(|| anyhow::anyhow!("no tab"))?;
            (tab.profile_id.clone(), tab.cwd.clone(), tab.focused_pane)
        };
        let profile = self.profile_by_id(&profile_id);
        let new_id = self.alloc_id();
        let session = TerminalSession::spawn(
            new_id,
            ctx.clone(),
            &profile.program,
            &profile.args,
            cwd,
            self.scrollback_lines,
        )?;

        let tab = self
            .active_tab_mut()
            .ok_or_else(|| anyhow::anyhow!("no tab"))?;
        if !replace_leaf(
            &mut tab.layout,
            focused,
            LayoutNode::Split {
                direction,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(focused)),
                second: Box::new(LayoutNode::Leaf(new_id)),
            },
        ) {
            anyhow::bail!("focused pane not found");
        }
        tab.sessions.insert(new_id, session);
        tab.focused_pane = new_id;
        self.mark_dirty();
        Ok(())
    }

    pub fn focus_next_pane(&mut self) {
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        let ids = collect_leaves(&tab.layout);
        if ids.is_empty() {
            return;
        }
        let pos = ids
            .iter()
            .position(|id| *id == tab.focused_pane)
            .unwrap_or(0);
        tab.focused_pane = ids[(pos + 1) % ids.len()];
    }

    pub fn focus_prev_pane(&mut self) {
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        let ids = collect_leaves(&tab.layout);
        if ids.is_empty() {
            return;
        }
        let pos = ids
            .iter()
            .position(|id| *id == tab.focused_pane)
            .unwrap_or(0);
        tab.focused_pane = ids[(pos + ids.len() - 1) % ids.len()];
    }

    /// Returns true if the whole tab should be closed (only one pane left).
    pub fn close_focused_pane(&mut self) -> bool {
        {
            let Some(tab) = self.active_tab_mut() else {
                return false;
            };
            if matches!(tab.layout, LayoutNode::Leaf(_)) {
                return true;
            }
            let focused = tab.focused_pane;
            if let Some((new_layout, sibling)) = collapse_leaf(&tab.layout, focused) {
                tab.layout = new_layout;
                tab.sessions.remove(&focused);
                tab.focused_pane = sibling;
            } else {
                return false;
            }
        }
        self.mark_dirty();
        false
    }
}

pub fn collect_leaves(node: &LayoutNode) -> Vec<u64> {
    match node {
        LayoutNode::Leaf(id) => vec![*id],
        LayoutNode::Split { first, second, .. } => {
            let mut v = collect_leaves(first);
            v.extend(collect_leaves(second));
            v
        }
    }
}

fn replace_leaf(node: &mut LayoutNode, target: u64, replacement: LayoutNode) -> bool {
    match node {
        LayoutNode::Leaf(id) if *id == target => {
            *node = replacement;
            true
        }
        LayoutNode::Split { first, second, .. } => {
            replace_leaf(first, target, replacement.clone())
                || replace_leaf(second, target, replacement)
        }
        _ => false,
    }
}

/// If `target` is a direct leaf under a split, return the promoted sibling layout
/// and the surviving leaf id to focus.
fn collapse_leaf(node: &LayoutNode, target: u64) -> Option<(LayoutNode, u64)> {
    match node {
        LayoutNode::Split { first, second, .. } => {
            match (first.as_ref(), second.as_ref()) {
                (LayoutNode::Leaf(a), other) if *a == target => {
                    let focus = first_leaf_id(other)?;
                    Some((other.clone(), focus))
                }
                (other, LayoutNode::Leaf(b)) if *b == target => {
                    let focus = first_leaf_id(other)?;
                    Some((other.clone(), focus))
                }
                _ => {
                    if let Some((new_first, focus)) = collapse_leaf(first, target) {
                        Some((
                            LayoutNode::Split {
                                direction: split_dir(node),
                                ratio: split_ratio(node),
                                first: Box::new(new_first),
                                second: second.clone(),
                            },
                            focus,
                        ))
                    } else if let Some((new_second, focus)) = collapse_leaf(second, target) {
                        Some((
                            LayoutNode::Split {
                                direction: split_dir(node),
                                ratio: split_ratio(node),
                                first: first.clone(),
                                second: Box::new(new_second),
                            },
                            focus,
                        ))
                    } else {
                        None
                    }
                }
            }
        }
        _ => None,
    }
}

fn first_leaf_id(node: &LayoutNode) -> Option<u64> {
    collect_leaves(node).into_iter().next()
}

fn split_dir(node: &LayoutNode) -> SplitDir {
    match node {
        LayoutNode::Split { direction, .. } => *direction,
        _ => SplitDir::Horizontal,
    }
}

fn split_ratio(node: &LayoutNode) -> f32 {
    match node {
        LayoutNode::Split { ratio, .. } => *ratio,
        _ => 0.5,
    }
}
