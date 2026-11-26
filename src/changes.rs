//! Uncommitted changes (Warp look): the shared git watcher behind the footer chips and the
//! tab bar's "± +N -N" button, and the right-hand pane with one collapsible diff card per file,
//! a file column, Commit / Push / Create PR, revert and discard. Every git call runs on a thread;
//! only Push and Create PR touch the network, and only when clicked.

use crate::gitinfo::{self, FileChange};
use crate::ui_theme::{self, Icon};
use egui::text::LayoutJob;
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Rect, Sense, Stroke, TextFormat, Ui, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const GIT_EVERY: Duration = Duration::from_secs(5);
const HEADER_H: f32 = 36.0;
const BAR_ROW_H: f32 = 34.0;
const CARD_HEAD_H: f32 = 30.0;
const LINE_H: f32 = 17.0;
const FILE_COL_W: f32 = 220.0;
/// Lines kept per file; the rest is summarised so a huge diff cannot blow the RAM budget.
const MAX_DIFF_LINES: usize = 3000;
const MAX_UNTRACKED_BYTES: u64 = 1 << 20;
const MESSAGE_FOR: Duration = Duration::from_secs(8);

// --- Git watcher -------------------------------------------------------------------------

/// What the last git refresh saw for the active tab's repo.
#[derive(Clone, Default)]
pub struct GitSnapshot {
    pub root: Option<PathBuf>,
    pub branch: Option<String>,
    pub changes: Option<Arc<Vec<FileChange>>>,
    /// Bumped whenever branch or changes differ from the previous refresh.
    pub generation: u64,
}

impl GitSnapshot {
    pub fn totals(&self) -> Option<(usize, u32, u32)> {
        self.changes.as_deref().map(|c| gitinfo::totals(c))
    }
}

#[derive(Default)]
struct WatchState {
    key: Option<String>,
    snap: GitSnapshot,
    checked: Option<Instant>,
    running: bool,
}

/// Polls git for the active tab's project every few seconds on an `ankar-git` thread.
#[derive(Default)]
pub struct GitWatch {
    state: Arc<Mutex<WatchState>>,
}

impl GitWatch {
    /// `project_key` is the active tab's git root (else cwd); `None` with no tab.
    pub fn refresh(&self, ctx: &egui::Context, project_key: Option<&str>) {
        let Ok(mut s) = self.state.lock() else {
            return;
        };
        if s.key.as_deref() != project_key {
            let root = project_key.map(PathBuf::from).filter(|r| r.join(".git").exists());
            let generation = s.snap.generation + 1;
            *s = WatchState { key: project_key.map(str::to_owned), ..WatchState::default() };
            s.snap = GitSnapshot { root, generation, ..GitSnapshot::default() };
        }
        if s.running || s.checked.is_some_and(|t| t.elapsed() < GIT_EVERY) {
            return;
        }
        s.checked = Some(Instant::now());
        let Some(root) = s.snap.root.clone() else {
            return;
        };
        s.running = true;
        let (state, ctx) = (self.state.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("ankar-git".into()).spawn(move || {
            let branch = gitinfo::branch(&root);
            let changes = gitinfo::changes(&root);
            if let Ok(mut s) = state.lock() {
                let snap = &mut s.snap;
                if snap.root.as_deref() == Some(root.as_path()) {
                    if snap.branch != branch || snap.changes.as_deref() != changes.as_ref() {
                        snap.generation += 1;
                    }
                    snap.branch = branch;
                    snap.changes = changes.map(Arc::new);
                }
                s.running = false;
            }
            ctx.request_repaint();
        });
        if spawned.is_err() {
            s.running = false;
        }
    }

    pub fn snapshot(&self) -> GitSnapshot {
        self.state.lock().map(|s| s.snap.clone()).unwrap_or_default()
    }

    /// Next `refresh` runs git now (after a commit, revert or discard).
    pub fn invalidate(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.checked = None;
        }
    }
}

// --- Diff parsing ------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Hunk,
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileDiff {
    pub lines: Vec<DiffLine>,
    pub binary: bool,
}

/// `@@ -12,3 +14,5 @@ fn x` -> (12, 14).
fn hunk_starts(line: &str) -> Option<(u32, u32)> {
    let mut parts = line.strip_prefix("@@ ")?.split(' ');
    let start = |s: Option<&str>, sign: char| s?.strip_prefix(sign)?.split(',').next()?.parse().ok();
    Some((start(parts.next(), '-')?, start(parts.next(), '+')?))
}

fn display(text: &str) -> String {
    text.replace('\t', "    ")
}

/// One file's `git diff` output: hunks with old/new line numbers; headers dropped.
pub fn parse_unified(text: &str) -> FileDiff {
    let mut diff = FileDiff::default();
    let (mut old, mut new) = (0u32, 0u32);
    let mut in_hunk = false;
    for line in text.lines() {
        if line.starts_with("diff --git ") {
            in_hunk = false;
            continue;
        }
        if let Some((o, n)) = hunk_starts(line) {
            (old, new, in_hunk) = (o, n, true);
            diff.lines.push(DiffLine { kind: LineKind::Hunk, old: None, new: None, text: line.to_owned() });
            continue;
        }
        if !in_hunk {
            diff.binary |= line.starts_with("Binary files ");
            continue;
        }
        let (kind, rest) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Added, &line[1..]),
            Some(b'-') => (LineKind::Removed, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            None => (LineKind::Context, ""),
            _ => continue,
        };
        let (o, n) = match kind {
            LineKind::Added => (None, Some(new)),
            LineKind::Removed => (Some(old), None),
            _ => (Some(old), Some(new)),
        };
        old += u32::from(o.is_some());
        new += u32::from(n.is_some());
        diff.lines.push(DiffLine { kind, old: o, new: n, text: display(rest) });
    }
    diff
}

/// An untracked file shown as one all-added hunk.
pub fn all_added(text: &str) -> FileDiff {
    let count = text.lines().count();
    let mut lines = vec![DiffLine { kind: LineKind::Hunk, old: None, new: None, text: format!("@@ -0,0 +1,{count} @@") }];
    lines.extend(text.lines().enumerate().map(|(i, l)| DiffLine {
        kind: LineKind::Added,
        old: None,
        new: Some(i as u32 + 1),
        text: display(l),
    }));
    FileDiff { lines, binary: false }
}

fn cap(mut diff: FileDiff) -> FileDiff {
    if diff.lines.len() > MAX_DIFF_LINES {
        let more = diff.lines.len() - MAX_DIFF_LINES;
        diff.lines.truncate(MAX_DIFF_LINES);
        diff.lines.push(DiffLine { kind: LineKind::Hunk, old: None, new: None, text: format!("… {more} more lines") });
    }
    diff
}

fn load_diff(root: &Path, change: &FileChange) -> FileDiff {
    if change.added.is_none() {
        return FileDiff { lines: Vec::new(), binary: true };
    }
    if change.code == "??" {
        let path = root.join(&change.path);
        let small = std::fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_UNTRACKED_BYTES);
        return match std::fs::read(&path) {
            Ok(bytes) if small && !bytes.contains(&0) => cap(all_added(&String::from_utf8_lossy(&bytes))),
            Ok(_) if small => FileDiff { lines: Vec::new(), binary: true },
            _ => FileDiff::default(),
        };
    }
    let args = ["diff", "HEAD", "--no-color", "--no-ext-diff", "--no-renames", "--", change.path.as_str()];
    cap(parse_unified(&gitinfo::run(root, &args).unwrap_or_default()))
}

/// GitHub "compare" page for `branch` from the `origin` URL (https, scp-style or ssh).
pub fn compare_url(remote: &str, branch: &str) -> Option<String> {
    let remote = remote.trim();
    let path = remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("http://github.com/"))
        .or_else(|| remote.strip_prefix("git@github.com:"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !repo.is_empty() && parts.next().is_none())
        .then(|| format!("https://github.com/{owner}/{repo}/compare/{branch}"))
}

// --- Actions -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Job {
    Commit(String),
    Push,
    OpenPr,
    Revert(FileChange),
    DiscardAll,
}

impl Job {
    fn confirm_text(&self, branch: Option<&str>) -> String {
        match self {
            Job::Push => format!("Push {} to its upstream?", branch.unwrap_or("this branch")),
            Job::Revert(c) if c.code == "??" => format!("Delete untracked {}? This cannot be undone.", c.path),
            Job::Revert(c) => format!("Revert {} to HEAD? This cannot be undone.", c.path),
            Job::DiscardAll => "Discard all changes to tracked files? Untracked files are kept. This cannot be undone.".into(),
            Job::Commit(_) | Job::OpenPr => String::new(),
        }
    }

    fn destructive(&self) -> bool {
        matches!(self, Job::Revert(_) | Job::DiscardAll)
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = gitinfo::command(root, args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let out_text = String::from_utf8_lossy(&out.stdout);
    Err(err.lines().chain(out_text.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("git failed").to_owned())
}

fn run_job(root: &Path, branch: Option<&str>, job: Job) -> Result<String, String> {
    match job {
        Job::Commit(msg) => git(root, &["commit", "-a", "-m", &msg]).map(|_| "Committed".into()),
        Job::Push => git(root, &["push"]).map(|_| "Pushed".into()),
        Job::OpenPr => {
            let remote = git(root, &["remote", "get-url", "origin"])?;
            let branch = branch.ok_or("No branch checked out")?;
            let url = compare_url(&remote, branch).ok_or("origin is not a GitHub remote")?;
            std::process::Command::new("explorer.exe").arg(&url).spawn().map_err(|e| e.to_string())?;
            Ok(format!("Opened {url}"))
        }
        Job::Revert(c) if c.code == "??" => {
            std::fs::remove_file(root.join(&c.path)).map_err(|e| e.to_string())?;
            Ok(format!("Deleted {}", c.path))
        }
        Job::Revert(c) if c.code.starts_with('A') => {
            git(root, &["rm", "-f", "--", &c.path]).map(|_| format!("Reverted {}", c.path))
        }
        Job::Revert(c) => git(root, &["restore", "--source=HEAD", "--staged", "--worktree", "--", &c.path])
            .map(|_| format!("Reverted {}", c.path)),
        Job::DiscardAll => git(root, &["reset", "--hard", "HEAD"]).map(|_| "Discarded all changes".into()),
    }
}

// --- Pane --------------------------------------------------------------------------------

pub enum ChangesAction {
    Close,
    OpenFile(PathBuf),
}

#[derive(Default)]
pub struct ChangesPane {
    pub open: bool,
    pub maximized: bool,
    /// Set when opened; the app pins the pane to the active tab.
    pub just_opened: bool,
    hide_files: bool,
    collapsed: HashSet<String>,
    diffs: Option<(u64, HashMap<String, FileDiff>)>,
    loading: Option<(u64, Receiver<HashMap<String, FileDiff>>)>,
    scroll_to: Option<String>,
    commit_msg: Option<String>,
    focus_commit: bool,
    confirm: Option<Job>,
    job: Option<Receiver<Result<String, String>>>,
    message: Option<(String, bool, Instant)>,
}

fn fmt(color: Color32) -> TextFormat {
    TextFormat::simple(FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional), color)
}

/// "+N -N" in green and red.
fn counts_job(added: u32, removed: u32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(&format!("+{added}"), 0.0, fmt(ui_theme::DIFF_ADD));
    job.append(&format!(" -{removed}"), 0.0, fmt(ui_theme::DIFF_DEL));
    job
}

/// The tab bar's "± +N -N" button; `galley` comes from [`bar_label`].
pub fn bar_button(ui: &mut Ui, galley: Arc<egui::Galley>, open: bool) -> egui::Response {
    let size = Vec2::new(galley.size().x + 32.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    if resp.hovered() || open {
        p.rect_filled(rect, CornerRadius::same(ui_theme::RADIUS_KBD), ui_theme::HOVER);
    }
    paint_plus_minus(p, egui::pos2(rect.left() + 13.0, rect.center().y), ui_theme::TEXT_MUTED);
    p.galley(egui::pos2(rect.left() + 24.0, rect.center().y - galley.size().y / 2.0), galley, ui_theme::TEXT_MUTED);
    resp.on_hover_text("Uncommitted changes")
}

pub fn bar_label(ui: &Ui, added: u32, removed: u32) -> Arc<egui::Galley> {
    ui.painter().layout_job(counts_job(added, removed))
}

/// "±" drawn as strokes: the UI fonts are subsets.
fn paint_plus_minus(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(1.2, color);
    p.line_segment([c + Vec2::new(-4.0, -2.5), c + Vec2::new(4.0, -2.5)], s);
    p.line_segment([c + Vec2::new(0.0, -6.5), c + Vec2::new(0.0, 1.5)], s);
    p.line_segment([c + Vec2::new(-4.0, 5.0), c + Vec2::new(4.0, 5.0)], s);
}

fn icon_button(ui: &mut Ui, rect: Rect, glyph: char, family: FontFamily, tip: &str, on: bool) -> egui::Response {
    let resp = ui.interact(rect, ui.scope_id().with(("changes_btn", tip, rect.min.x as i32, rect.min.y as i32)), Sense::click());
    let p = ui.painter();
    if resp.hovered() || on {
        p.rect_filled(rect, CornerRadius::same(4), ui_theme::HOVER);
    }
    let color = if on { ui_theme::TEXT } else { ui_theme::TEXT_MUTED };
    p.text(rect.center(), Align2::CENTER_CENTER, glyph.to_string(), FontId::new(13.0, family), color);
    resp.on_hover_text(tip)
}

fn icons() -> FontFamily {
    FontFamily::Name(ui_theme::FONT_ICONS.into())
}

impl ChangesPane {
    /// Opens fresh; the app pins it to the active tab.
    pub fn open_here(&mut self) {
        self.close();
        self.open = true;
        self.just_opened = true;
    }

    /// Closes the pane and frees the diffs.
    pub fn close(&mut self) {
        *self = ChangesPane { hide_files: self.hide_files, ..ChangesPane::default() };
    }

    fn start(&mut self, ctx: &egui::Context, snap: &GitSnapshot, job: Job) {
        let Some(root) = snap.root.clone() else {
            return;
        };
        let branch = snap.branch.clone();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("ankar-git-job".into()).spawn(move || {
            let _ = tx.send(run_job(&root, branch.as_deref(), job));
            ctx.request_repaint();
        });
        if spawned.is_ok() {
            self.job = Some(rx);
        }
    }

    fn poll(&mut self, git: &GitWatch) {
        if let Some(Ok(result)) = self.job.as_ref().map(|rx| rx.try_recv()) {
            self.job = None;
            let (text, err) = match result {
                Ok(t) => (t, false),
                Err(e) => (e, true),
            };
            self.message = Some((text, err, Instant::now()));
            git.invalidate();
        }
        if self.message.as_ref().is_some_and(|m| m.2.elapsed() > MESSAGE_FOR) {
            self.message = None;
        }
        if let Some((generation, Ok(diffs))) = self.loading.as_ref().map(|(g, rx)| (*g, rx.try_recv())) {
            self.loading = None;
            self.diffs = Some((generation, diffs));
        }
    }

    fn ensure_diffs(&mut self, ctx: &egui::Context, snap: &GitSnapshot) {
        let want = snap.generation;
        let have = self.loading.as_ref().map(|l| l.0).or(self.diffs.as_ref().map(|d| d.0));
        let (Some(root), Some(changes)) = (snap.root.clone(), snap.changes.clone()) else {
            return;
        };
        if have == Some(want) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("ankar-git-diff".into()).spawn(move || {
            let diffs = changes.iter().map(|c| (c.path.clone(), load_diff(&root, c))).collect();
            let _ = tx.send(diffs);
            ctx.request_repaint();
        });
        if spawned.is_ok() {
            self.loading = Some((want, rx));
        }
    }

    pub fn draw(&mut self, ui: &mut Ui, rect: Rect, snap: &GitSnapshot, git: &GitWatch) -> Option<ChangesAction> {
        self.poll(git);
        self.ensure_diffs(ui.ctx(), snap);
        if self.job.is_some() || self.loading.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        ui.painter().rect_filled(rect, 0.0, ui_theme::BG);
        let mut action = None;

        let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), HEADER_H));
        ui.painter().hline(header.x_range(), header.bottom() - 0.5, Stroke::new(1.0, ui_theme::CHROME_DIVIDER));
        let btn = |i: f32| Rect::from_center_size(egui::pos2(header.right() - 18.0 - 28.0 * i, header.center().y), Vec2::splat(24.0));
        if icon_button(ui, btn(0.0), Icon::X.codepoint(), icons(), "Close", false).clicked() {
            action = Some(ChangesAction::Close);
        }
        let max_tip = if self.maximized { "Restore pane" } else { "Maximize pane" };
        if icon_button(ui, btn(1.0), Icon::Square.codepoint(), icons(), max_tip, self.maximized).clicked() {
            self.maximized = !self.maximized;
        }
        let files_tip = if self.hide_files { "Show file navigation" } else { "Hide file navigation" };
        if icon_button(ui, btn(2.0), Icon::PanelLeft.codepoint(), icons(), files_tip, !self.hide_files).clicked() {
            self.hide_files = !self.hide_files;
        }
        let menu = icon_button(ui, btn(3.0), '\u{22EE}', FontFamily::Proportional, "More", false);
        egui::Popup::menu(&menu).show(|ui| {
            let discard = egui::RichText::new("Discard all").color(ui_theme::ERROR);
            if ui.button(discard).clicked() {
                self.confirm = Some(Job::DiscardAll);
            }
        });
        let commit_rect = Rect::from_min_max(
            egui::pos2(btn(3.0).left() - 8.0 - 84.0, header.center().y - 12.0),
            egui::pos2(btn(3.0).left() - 8.0, header.center().y + 12.0),
        );
        let commit = ui.interact(commit_rect, ui.scope_id().with("changes_commit"), Sense::click());
        let fill = if commit.hovered() { ui_theme::SURFACE_SEL } else { ui_theme::CHIP_BG };
        ui.painter().rect(commit_rect, CornerRadius::same(4), fill, Stroke::new(1.0, ui_theme::CHIP_BORDER), egui::StrokeKind::Inside);
        let font = FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional);
        ui.painter().text(egui::pos2(commit_rect.left() + 12.0, commit_rect.center().y), Align2::LEFT_CENTER, "Commit", font.clone(), ui_theme::TEXT);
        ui.painter().text(
            egui::pos2(commit_rect.right() - 14.0, commit_rect.center().y),
            Align2::CENTER_CENTER,
            Icon::ChevronDown.codepoint().to_string(),
            FontId::new(12.0, icons()),
            ui_theme::TEXT_MUTED,
        );
        egui::Popup::menu(&commit).show(|ui| {
            if ui.button("Commit").clicked() {
                self.commit_msg = Some(String::new());
                self.focus_commit = true;
            }
            if ui.button("Push").clicked() {
                self.confirm = Some(Job::Push);
            }
            if ui.button("Create PR").clicked() {
                self.start(ui.ctx(), snap, Job::OpenPr);
            }
        });

        let mut title = LayoutJob::default();
        let root = snap.root.as_ref().map(|r| r.display().to_string()).unwrap_or_else(|| "Not a git repository".into());
        title.append(&root, 0.0, fmt(ui_theme::TEXT));
        if let Some(branch) = &snap.branch {
            title.append(&format!(": {branch}"), 0.0, fmt(ui_theme::TEXT_MUTED));
        }
        if let Some((files, added, removed)) = snap.totals() {
            title.append(&format!("   {files}  "), 0.0, fmt(ui_theme::TEXT_MUTED));
            title.append(&format!("+{added}"), 0.0, fmt(ui_theme::DIFF_ADD));
            title.append(&format!(" -{removed}"), 0.0, fmt(ui_theme::DIFF_DEL));
        }
        let title_clip = Rect::from_min_max(header.min, egui::pos2(commit_rect.left() - 8.0, header.bottom()));
        let galley = ui.painter().layout_job(title);
        ui.painter().with_clip_rect(title_clip).galley(
            egui::pos2(header.left() + 12.0, header.center().y - galley.size().y / 2.0),
            galley,
            ui_theme::TEXT,
        );

        let mut top = header.bottom();
        if self.commit_msg.is_some() || self.confirm.is_some() || self.message.is_some() || self.job.is_some() {
            let bar = Rect::from_min_size(egui::pos2(rect.left(), top), Vec2::new(rect.width(), BAR_ROW_H));
            self.draw_bar(ui, bar, snap);
            top = bar.bottom();
        }

        let body = Rect::from_min_max(egui::pos2(rect.left(), top), rect.max);
        let show_files = !self.hide_files && body.width() > FILE_COL_W * 2.2;
        let main = if show_files {
            Rect::from_min_max(body.min, egui::pos2(body.right() - FILE_COL_W, body.bottom()))
        } else {
            body
        };
        let changes = snap.changes.clone().unwrap_or_default();
        if show_files {
            let col = Rect::from_min_max(egui::pos2(main.right(), body.top()), body.max);
            self.draw_file_column(ui, col, &changes);
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(main.shrink2(Vec2::new(12.0, 8.0))));
        egui::ScrollArea::vertical().id_salt("changes_scroll").auto_shrink([false, false]).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            ui.label(egui::RichText::new("Uncommitted changes").size(13.0).color(ui_theme::TEXT));
            ui.add_space(8.0);
            if snap.root.is_none() {
                ui.label(ui_theme::text_sub("This tab is not inside a git repository."));
            } else if changes.is_empty() {
                ui.label(ui_theme::text_sub("No changes."));
            }
            for change in changes.iter() {
                if let Some(a) = self.draw_card(ui, change, snap) {
                    action = Some(a);
                }
                ui.add_space(10.0);
            }
        });
        action
    }

    /// Commit message box, a confirm prompt, or the last result, under the header.
    fn draw_bar(&mut self, ui: &mut Ui, bar: Rect, snap: &GitSnapshot) {
        ui.painter().hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, ui_theme::CHROME_DIVIDER));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar.shrink2(Vec2::new(12.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut child;
        ui.spacing_mut().item_spacing.x = 8.0;
        if self.job.is_some() {
            ui.label(ui_theme::text_sub("Running git…"));
            return;
        }
        if let Some(job) = self.confirm.clone() {
            let color = if job.destructive() { ui_theme::ERROR } else { ui_theme::TEXT };
            ui.label(egui::RichText::new(job.confirm_text(snap.branch.as_deref())).color(color));
            let label = match job {
                Job::Push => "Push",
                Job::DiscardAll => "Discard all",
                _ => "Revert",
            };
            let go = egui::Button::new(egui::RichText::new(label).color(if job.destructive() { Color32::WHITE } else { ui_theme::TEXT }))
                .fill(if job.destructive() { ui_theme::ERROR.gamma_multiply(0.7) } else { ui_theme::BUTTON_BG });
            if ui.add(go).clicked() {
                self.confirm = None;
                self.start(ui.ctx(), snap, job);
            } else if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.confirm = None;
            }
            return;
        }
        if let Some(msg) = self.commit_msg.as_mut() {
            let edit = egui::TextEdit::singleline(msg).hint_text("Commit message").desired_width(ui.available_width() - 140.0);
            let resp = ui.add(edit);
            if std::mem::take(&mut self.focus_commit) {
                resp.request_focus();
            }
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let ready = !msg.trim().is_empty();
            let commit = ui.add_enabled(ready, egui::Button::new("Commit")).clicked();
            if ready && (commit || enter) {
                let msg = msg.trim().to_owned();
                self.commit_msg = None;
                self.start(ui.ctx(), snap, Job::Commit(msg));
            } else if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.commit_msg = None;
            }
            return;
        }
        if let Some((text, err, _)) = &self.message {
            let color = if *err { ui_theme::ERROR } else { ui_theme::TEXT_MUTED };
            ui.label(egui::RichText::new(text.as_str()).color(color));
        }
    }

    fn draw_file_column(&mut self, ui: &mut Ui, col: Rect, changes: &[FileChange]) {
        ui.painter().vline(col.left() + 0.5, col.y_range(), Stroke::new(1.0, ui_theme::CHROME_DIVIDER));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(col.shrink2(Vec2::new(8.0, 8.0))));
        egui::ScrollArea::vertical().id_salt("changes_files").auto_shrink([false, false]).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for change in changes {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(4), ui_theme::HOVER);
                }
                let name = change.path.rsplit('/').next().unwrap_or(&change.path);
                let mut job = LayoutJob::default();
                job.append(name, 0.0, fmt(ui_theme::TEXT_MUTED));
                if let Some(added) = change.added {
                    job.append(&format!("  +{added}"), 0.0, fmt(ui_theme::DIFF_ADD));
                }
                let galley = ui.painter().layout_job(job);
                ui.painter().with_clip_rect(rect).galley(
                    egui::pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0),
                    galley,
                    ui_theme::TEXT_MUTED,
                );
                if resp.on_hover_text(&change.path).clicked() {
                    self.collapsed.remove(&change.path);
                    self.scroll_to = Some(change.path.clone());
                }
            }
        });
    }

    fn draw_card(&mut self, ui: &mut Ui, change: &FileChange, snap: &GitSnapshot) -> Option<ChangesAction> {
        let mut action = None;
        let width = ui.available_width();
        let (head, head_resp) = ui.allocate_exact_size(Vec2::new(width, CARD_HEAD_H), Sense::click());
        if self.scroll_to.as_deref() == Some(change.path.as_str()) {
            ui.scroll_to_rect(head, Some(egui::Align::TOP));
            self.scroll_to = None;
        }
        let collapsed = self.collapsed.contains(&change.path);
        let radius = if collapsed { CornerRadius::same(6) } else { CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 } };
        ui.painter().rect(head, radius, ui_theme::CHIP_BG, Stroke::new(1.0, ui_theme::CHIP_BORDER), egui::StrokeKind::Inside);
        let chevron = if collapsed { Icon::ChevronRight } else { Icon::ChevronDown };
        let p = ui.painter().with_clip_rect(head);
        p.text(egui::pos2(head.left() + 14.0, head.center().y), Align2::CENTER_CENTER, chevron.codepoint().to_string(), FontId::new(12.0, icons()), ui_theme::TEXT_MUTED);

        let b = |i: f32| Rect::from_center_size(egui::pos2(head.right() - 16.0 - 26.0 * i, head.center().y), Vec2::splat(22.0));
        let open = icon_button(ui, b(0.0), Icon::File.codepoint(), icons(), "Open in viewer", false);
        let revert = icon_button(ui, b(1.0), '\u{21BB}', FontFamily::Proportional, "Revert file…", false);
        let mut counts = LayoutJob::default();
        match (change.added, change.removed) {
            (Some(a), Some(r)) => counts = counts_job(a, r),
            _ => counts.append("binary", 0.0, fmt(ui_theme::TEXT_FAINT)),
        }
        let counts = ui.painter().layout_job(counts);
        let counts_left = b(1.0).left() - 8.0 - counts.size().x;
        ui.painter().galley(egui::pos2(counts_left, head.center().y - counts.size().y / 2.0), counts, ui_theme::TEXT);

        let path_font = FontId::new(ui_theme::SIZE_UI, FontFamily::Proportional);
        let path_galley = ui.painter().layout_no_wrap(change.path.clone(), path_font, ui_theme::TEXT);
        let path_right = (head.left() + 28.0 + path_galley.size().x).min(counts_left - 34.0);
        ui.painter()
            .with_clip_rect(Rect::from_min_max(head.min, egui::pos2(path_right, head.bottom())))
            .galley(egui::pos2(head.left() + 28.0, head.center().y - path_galley.size().y / 2.0), path_galley, ui_theme::TEXT);
        let copy_rect = Rect::from_center_size(egui::pos2(path_right + 14.0, head.center().y), Vec2::splat(22.0));
        let copy = icon_button(ui, copy_rect, Icon::Copy.codepoint(), icons(), "Copy path", false);

        if copy.clicked() {
            ui.ctx().copy_text(change.path.clone());
        } else if open.clicked() {
            if let Some(root) = &snap.root {
                action = Some(ChangesAction::OpenFile(root.join(&change.path)));
            }
        } else if revert.clicked() {
            self.confirm = Some(Job::Revert(change.clone()));
        } else if head_resp.clicked() {
            if collapsed {
                self.collapsed.remove(&change.path);
            } else {
                self.collapsed.insert(change.path.clone());
            }
        }
        if collapsed {
            return action;
        }

        let diff = self.diffs.as_ref().and_then(|(_, d)| d.get(&change.path));
        let body_top = head.bottom();
        match diff {
            None => {
                ui.add_space(6.0);
                ui.label(ui_theme::text_sub("  Loading…"));
            }
            Some(d) if d.binary => {
                ui.add_space(6.0);
                ui.label(ui_theme::text_sub("  Binary file - no diff available"));
            }
            Some(d) => paint_diff(ui, width, d),
        }
        ui.add_space(2.0);
        let body = Rect::from_min_max(egui::pos2(head.left(), body_top - 1.0), egui::pos2(head.right(), ui.cursor().top()));
        ui.painter().rect_stroke(
            body,
            CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 },
            Stroke::new(1.0, ui_theme::CHIP_BORDER),
            egui::StrokeKind::Inside,
        );
        action
    }
}

fn paint_diff(ui: &mut Ui, width: f32, diff: &FileDiff) {
    let mono = FontId::monospace(12.0);
    let digits = diff.lines.iter().filter_map(|l| l.old.max(l.new)).max().unwrap_or(1).to_string().len().max(3) as f32;
    let num_w = digits * 7.3 + 10.0;
    for line in &diff.lines {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, LINE_H), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let p = ui.painter().with_clip_rect(rect.shrink2(Vec2::new(1.0, 0.0)));
        let (bg, sign, fg) = match line.kind {
            LineKind::Added => (Color32::from_rgba_unmultiplied(0x53, 0xBE, 0x6B, 34), "+", ui_theme::TEXT),
            LineKind::Removed => (Color32::from_rgba_unmultiplied(0xE5, 0x69, 0x5B, 34), "-", ui_theme::TEXT),
            LineKind::Hunk => (ui_theme::SURFACE_1, "", ui_theme::TEXT_FAINT),
            LineKind::Context => (Color32::TRANSPARENT, " ", ui_theme::TERM_FG),
        };
        p.rect_filled(rect, 0.0, bg);
        let y = rect.center().y;
        let mut x = rect.left() + num_w;
        for n in [line.old, line.new] {
            if let Some(n) = n {
                p.text(egui::pos2(x - 6.0, y), Align2::RIGHT_CENTER, n.to_string(), mono.clone(), ui_theme::TEXT_FAINT);
            }
            x += num_w;
        }
        let text_x = rect.left() + 2.0 * num_w + 4.0;
        if line.kind == LineKind::Hunk {
            p.text(egui::pos2(rect.left() + 10.0, y), Align2::LEFT_CENTER, &line.text, mono.clone(), fg);
        } else {
            p.text(egui::pos2(text_x, y), Align2::LEFT_CENTER, format!("{sign} {}", line.text), mono.clone(), fg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(d: &FileDiff) -> Vec<(LineKind, Option<u32>, Option<u32>, &str)> {
        d.lines.iter().map(|l| (l.kind, l.old, l.new, l.text.as_str())).collect()
    }

    #[test]
    fn unified_hunks_number_old_and_new_lines_and_drop_headers() {
        let text = "diff --git a/src/x.rs b/src/x.rs\nindex 1..2 100644\n--- a/src/x.rs\n+++ b/src/x.rs\n\
                    @@ -10,3 +10,4 @@ fn main() {\n ctx\n-old\n+new\n+more\n\\ No newline at end of file\n \tend\n";
        let d = parse_unified(text);
        assert!(!d.binary);
        assert_eq!(
            kinds(&d),
            vec![
                (LineKind::Hunk, None, None, "@@ -10,3 +10,4 @@ fn main() {"),
                (LineKind::Context, Some(10), Some(10), "ctx"),
                (LineKind::Removed, Some(11), None, "old"),
                (LineKind::Added, None, Some(11), "new"),
                (LineKind::Added, None, Some(12), "more"),
                (LineKind::Context, Some(12), Some(13), "    end"),
            ]
        );
    }

    #[test]
    fn second_hunk_restarts_numbering_and_removed_dash_lines_stay_removed() {
        let d = parse_unified("@@ -1 +1 @@\n-a\n+b\n@@ -40,2 +40,2 @@\n---- x\n+++ y\n");
        assert_eq!(
            kinds(&d)[3..],
            [
                (LineKind::Hunk, None, None, "@@ -40,2 +40,2 @@"),
                (LineKind::Removed, Some(40), None, "--- x"),
                (LineKind::Added, None, Some(40), "++ y"),
            ]
        );
    }

    #[test]
    fn binary_files_are_flagged_and_have_no_lines() {
        let d = parse_unified("diff --git a/a.png b/a.png\nindex 1..2\nBinary files a/a.png and b/a.png differ\n");
        assert!(d.binary);
        assert!(d.lines.is_empty());
    }

    #[test]
    fn an_untracked_file_is_one_added_hunk() {
        let d = all_added("one\ntwo\n");
        assert_eq!(
            kinds(&d),
            vec![
                (LineKind::Hunk, None, None, "@@ -0,0 +1,2 @@"),
                (LineKind::Added, None, Some(1), "one"),
                (LineKind::Added, None, Some(2), "two"),
            ]
        );
    }

    #[test]
    fn huge_diffs_are_capped_with_a_summary_line() {
        let text: String = (0..MAX_DIFF_LINES + 5).map(|i| format!("l{i}\n")).collect();
        let d = cap(all_added(&text));
        assert_eq!(d.lines.len(), MAX_DIFF_LINES + 1);
        assert_eq!(d.lines.last().unwrap().text, "… 6 more lines");
    }

    #[test]
    fn the_pane_draws_cards_bars_and_binary_files_without_tripping_egui() {
        let ctx = egui::Context::default();
        ui_theme::apply(&ctx);
        let change = |path: &str, added, removed| FileChange { path: path.into(), code: " M".into(), added, removed };
        let snap = GitSnapshot {
            root: Some(PathBuf::from(r"C:\repo")),
            branch: Some("main".into()),
            changes: Some(Arc::new(vec![change("src/a.rs", Some(2), Some(1)), change("logo.png", None, None)])),
            generation: 7,
        };
        let mut diffs = HashMap::new();
        diffs.insert("src/a.rs".to_owned(), parse_unified("@@ -1,2 +1,3 @@\n x\n-y\n+z\n+w\n"));
        diffs.insert("logo.png".to_owned(), FileDiff { lines: Vec::new(), binary: true });
        let mut pane = ChangesPane { open: true, diffs: Some((7, diffs)), ..ChangesPane::default() };
        let git = GitWatch::default();
        let rect = Rect::from_min_size(egui::pos2(400.0, 40.0), Vec2::new(800.0, 600.0));
        for bar in [None, Some(Job::DiscardAll), Some(Job::Push)] {
            pane.confirm = bar;
            pane.commit_msg = pane.confirm.is_none().then(String::new);
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1200.0, 700.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                assert!(pane.draw(ui, rect, &snap, &git).is_none());
            });
            out.textures_delta.clear();
        }
        assert!(pane.loading.is_none(), "diffs for this generation are already loaded");
    }

    /// Needs git and this checkout: `cargo test real_repo -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_repo_changes_and_diffs_load() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let changes = gitinfo::changes(root).expect("git status");
        println!("totals {:?}", gitinfo::totals(&changes));
        for c in &changes {
            let d = load_diff(root, c);
            println!("{} {} +{:?} -{:?}: {} lines, binary {}", c.code, c.path, c.added, c.removed, d.lines.len(), d.binary);
            let added = d.lines.iter().filter(|l| l.kind == LineKind::Added).count() as u32;
            if let (Some(a), false) = (c.added, d.binary) {
                assert!(added == a || d.lines.len() > MAX_DIFF_LINES, "{}: numstat +{a}, diff +{added}", c.path);
            }
        }
    }

    #[test]
    fn github_remotes_give_a_compare_url_and_others_none() {
        let want = Some("https://github.com/MtGhozt/ankar/compare/estabilizacion".to_owned());
        for remote in [
            "https://github.com/MtGhozt/ankar.git\n",
            "https://github.com/MtGhozt/ankar",
            "git@github.com:MtGhozt/ankar.git",
            "ssh://git@github.com/MtGhozt/ankar.git",
        ] {
            assert_eq!(compare_url(remote, "estabilizacion"), want, "{remote}");
        }
        assert_eq!(compare_url("https://gitlab.com/a/b.git", "main"), None);
        assert_eq!(compare_url("https://github.com/only-owner", "main"), None);
    }
}
