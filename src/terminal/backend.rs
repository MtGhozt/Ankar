use super::osc::{AgentInfo, OscTap};
use super::resize::{grid_size, hold_frame, ResizeDebouncer};
use super::theme::{indexed_256, TermColors};
use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{self, viewport_to_point, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, CursorStyle, NamedColor, Rgb};
use egui::Context;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_SCROLLBACK: usize = 50_000;

/// What `TerminalSession::resize` did this frame.
pub enum ResizeState {
    Idle,
    /// A new size is waiting for the debounce: repaint after it elapses.
    Pending,
    /// Grid and PTY were just resized together: repaint so the TUI's redraw shows up.
    Applied,
}

impl ResizeState {
    fn from_pending(pending: bool) -> Self {
        if pending {
            Self::Pending
        } else {
            Self::Idle
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    /// Glyph cell size in pixels (float — used for cols/selection math).
    pub cell_width: f32,
    pub cell_height: f32,
    pub num_cols: u16,
    pub num_lines: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cell_width: 8.0,
            cell_height: 16.0,
            num_cols: 80,
            num_lines: 24,
        }
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        self.num_lines as usize
    }

    fn columns(&self) -> usize {
        self.num_cols as usize
    }

    fn last_column(&self) -> Column {
        Column(self.num_cols as usize - 1)
    }

    fn bottommost_line(&self) -> Line {
        Line(self.num_lines as i32 - 1)
    }
}

impl From<TerminalSize> for WindowSize {
    fn from(size: TerminalSize) -> Self {
        Self {
            num_lines: size.num_lines,
            num_cols: size.num_cols,
            cell_width: size.cell_width.round().max(1.0) as u16,
            cell_height: size.cell_height.round().max(1.0) as u16,
        }
    }
}

/// One painted cell from the visible viewport (not full scrollback).
#[derive(Clone, Debug)]
pub struct VisibleCell {
    pub line: i32,
    pub col: u16,
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}

/// Reusable viewport snapshot — capacity ≈ cols×lines, cleared each sync.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub cells: Vec<VisibleCell>,
    pub cursor: Point,
    /// DECSCUSR shape; `Hidden` when DECTCEM is off.
    pub cursor_shape: CursorShape,
    /// OSC 12 override, if the app set one.
    pub cursor_color: Option<Rgb>,
    pub display_offset: usize,
    pub selection: Option<SelectionRange>,
    pub mode: TermMode,
    pub size: TerminalSize,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            cells: Vec::new(),
            cursor: Point::new(Line(0), Column(0)),
            cursor_shape: CursorShape::Hidden,
            cursor_color: None,
            display_offset: 0,
            selection: None,
            mode: TermMode::empty(),
            size: TerminalSize::default(),
        }
    }
}

pub struct TerminalSession {
    term: Arc<FairMutex<Term<EventProxy>>>,
    size: TerminalSize,
    /// Debounces layout size changes; `size` above is the one applied to grid AND PTY.
    debounce: ResizeDebouncer,
    /// Next `resize` applies immediately (drag released, panel toggled).
    apply_now: bool,
    /// When `apply_size` last ran while a frame was on screen; `sync` keeps that frame until
    /// the app has redrawn (`resize::hold_frame`).
    applied_at: Option<Instant>,
    notifier: Notifier,
    snapshot: Snapshot,
    exited: Arc<AtomicBool>,
    last_output: Arc<Mutex<Instant>>,
    bell: Arc<AtomicBool>,
    visible: Arc<AtomicBool>,
    /// Latest absolute directory from OSC title (shell integration).
    cwd: Arc<Mutex<Option<PathBuf>>>,
    /// Latest non-cwd window title (agent activity).
    activity: Arc<Mutex<Option<String>>>,
    /// Latest OSC 7501 agent report (state / session id / title).
    agent: Arc<Mutex<AgentInfo>>,
    exit_code: Arc<Mutex<Option<i32>>>,
    /// Shared with the event thread so it can answer size queries (CSI 14/18 t).
    window_size: Arc<Mutex<WindowSize>>,
    /// OSC 52 copy waiting for the UI thread (the clipboard only works inside a frame).
    clipboard: Arc<Mutex<Option<String>>>,
    /// Last cell/button reported for motion, so pixels inside one cell stay quiet.
    last_motion: Option<(u16, u16, u8)>,
    /// Process id of the shell (ConPTY child), root of the tree `procs` walks.
    shell_pid: Option<u32>,
    pub colors: TermColors,
}

impl TerminalSession {
    pub fn spawn(
        id: u64,
        ctx: Context,
        program: &str,
        args: &[String],
        cwd: Option<PathBuf>,
        scrollback_lines: usize,
    ) -> anyhow::Result<Self> {
        // Merged into the inherited environment by alacritty_terminal. TUIs (OpenCode) only
        // enable truecolor and OSC color queries when TERM is a real xterm-style value.
        let env = HashMap::from([
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
            // Dark-background hint (foreground 15, background 0) for apps that cannot query it.
            ("COLORFGBG".to_owned(), "15;0".to_owned()),
            ("TERM_PROGRAM".to_owned(), "Ankar".to_owned()),
            ("TERM_PROGRAM_VERSION".to_owned(), env!("CARGO_PKG_VERSION").to_owned()),
        ]);
        let pty_config = tty::Options {
            shell: Some(tty::Shell::new(program.to_string(), args.to_vec())),
            working_directory: cwd.clone(),
            env,
            ..tty::Options::default()
        };

        let history = scrollback_lines.min(MAX_SCROLLBACK);
        let config = term::Config {
            scrolling_history: history,
            // Midterm bar until an app picks a shape with DECSCUSR (DECSCUSR 0 returns here).
            default_cursor_style: CursorStyle {
                shape: CursorShape::Beam,
                blinking: false,
            },
            ..term::Config::default()
        };
        let terminal_size = TerminalSize::default();
        let pty = tty::new(&pty_config, terminal_size.into(), id)
            .map_err(|e| anyhow::anyhow!("failed to open PTY for {program}: {e}"))?;

        let agent = Arc::new(Mutex::new(AgentInfo::default()));
        let cwd_state = Arc::new(Mutex::new(cwd.clone()));
        let cwd_flag = cwd_state.clone();
        #[cfg(windows)]
        let shell_pid = pty.child_watcher().pid().map(|p| p.get());
        #[cfg(not(windows))]
        let shell_pid = None;
        let (event_sender, event_receiver) = mpsc::channel();
        // Queries alacritty_terminal ignores are answered from the raw-byte scanner, through
        // the same PtyWrite path as its own replies.
        let reply_sender = event_sender.clone();
        let pty = OscTap::new(
            pty,
            agent.clone(),
            cwd_state.clone(),
            Box::new(move |text| {
                let _ = reply_sender.send(Event::PtyWrite(text));
            }),
        );
        let event_proxy = EventProxy(event_sender);
        let term = Term::new(config, &terminal_size, event_proxy.clone());

        let term = Arc::new(FairMutex::new(term));
        let term_events = term.clone();
        let event_loop = EventLoop::new(term.clone(), event_proxy, pty, false, false)
            .map_err(|e| anyhow::anyhow!("failed to create PTY event loop: {e}"))?;
        let notifier = Notifier(event_loop.channel());
        let pty_notifier = Notifier(event_loop.channel());
        let _loop_thread = event_loop.spawn();

        let exited = Arc::new(AtomicBool::new(false));
        let exited_flag = exited.clone();
        let bell = Arc::new(AtomicBool::new(false));
        let bell_flag = bell.clone();
        let last_output = Arc::new(Mutex::new(Instant::now()));
        let last_output_flag = last_output.clone();
        let visible = Arc::new(AtomicBool::new(true));
        let visible_flag = visible.clone();
        let activity_state = Arc::new(Mutex::new(None::<String>));
        let activity_flag = activity_state.clone();
        let exit_code_state = Arc::new(Mutex::new(None::<i32>));
        let exit_code_flag = exit_code_state.clone();
        let window_size = Arc::new(Mutex::new(WindowSize::from(terminal_size)));
        let window_size_flag = window_size.clone();
        let clipboard = Arc::new(Mutex::new(None::<String>));
        let clipboard_flag = clipboard.clone();
        let colors = TermColors::default();
        let query_colors = colors.clone();
        std::thread::Builder::new()
            .name(format!("ankar-pty-{id}"))
            .spawn(move || {
                while let Ok(event) = event_receiver.recv() {
                    match &event {
                        Event::Exit => {
                            exited_flag.store(true, Ordering::SeqCst);
                            ctx.request_repaint();
                            break;
                        }
                        Event::ChildExit(status) => {
                            if let Some(code) = status.code() {
                                if let Ok(mut g) = exit_code_flag.lock() {
                                    *g = Some(code);
                                }
                            }
                            exited_flag.store(true, Ordering::SeqCst);
                            ctx.request_repaint();
                            break;
                        }
                        Event::PtyWrite(data) => {
                            log::debug!("pty reply: {data:?}");
                            pty_notifier.notify(data.as_bytes().to_vec());
                        }
                        Event::Bell => {
                            bell_flag.store(true, Ordering::SeqCst);
                            if visible_flag.load(Ordering::Relaxed) {
                                ctx.request_repaint();
                            }
                        }
                        Event::Wakeup => {
                            if let Ok(mut t) = last_output_flag.lock() {
                                *t = Instant::now();
                            }
                            if visible_flag.load(Ordering::Relaxed) {
                                ctx.request_repaint_after(Duration::from_millis(8));
                            }
                        }
                        Event::Title(t) => {
                            if let Some(path) = parse_cwd_title(t) {
                                if let Ok(mut guard) = cwd_flag.lock() {
                                    *guard = Some(path);
                                }
                            } else {
                                let trimmed = t.trim();
                                if !trimmed.is_empty() {
                                    if let Ok(mut guard) = activity_flag.lock() {
                                        *guard = Some(trimmed.to_string());
                                    }
                                }
                            }
                            ctx.request_repaint();
                        }
                        Event::ColorRequest(index, format) => {
                            // An OSC 12 query must answer with the colour the app set, not the default.
                            let set_cursor = (*index == NamedColor::Cursor as usize)
                                .then(|| term_events.lock().colors()[NamedColor::Cursor])
                                .flatten();
                            let reply =
                                query_color(&query_colors, *index, set_cursor).map(|rgb| format(rgb));
                            log::debug!("pty reply: color index={index} {reply:?}");
                            if let Some(reply) = reply {
                                pty_notifier.notify(reply.into_bytes());
                            }
                        }
                        Event::TextAreaSizeRequest(format) => {
                            let size = window_size_flag.lock().ok().map(|g| *g);
                            if let Some(size) = size {
                                let reply = format(size);
                                log::debug!("pty reply: text area {reply:?}");
                                pty_notifier.notify(reply.into_bytes());
                            }
                        }
                        Event::ClipboardStore(_, text) => {
                            if let Ok(mut g) = clipboard_flag.lock() {
                                *g = Some(text.clone());
                            }
                            ctx.request_repaint();
                        }
                        // OSC 52 paste is ignored on purpose: no program may read the clipboard.
                        // Event::ClipboardLoad(..) => {}
                        _ => {
                            if visible_flag.load(Ordering::Relaxed) {
                                ctx.request_repaint_after(Duration::from_millis(8));
                            }
                        }
                    }
                }
            })?;

        Ok(Self {
            term,
            size: terminal_size,
            debounce: ResizeDebouncer::default(),
            apply_now: false,
            applied_at: None,
            notifier,
            snapshot: Snapshot::default(),
            exited,
            last_output,
            bell,
            visible,
            cwd: cwd_state,
            activity: activity_state,
            agent,
            exit_code: exit_code_state,
            window_size,
            clipboard,
            last_motion: None,
            shell_pid,
            colors,
        })
    }

    pub fn shell_pid(&self) -> Option<u32> {
        self.shell_pid
    }

    /// Text an app copied via OSC 52 since the last call.
    pub fn take_clipboard(&self) -> Option<String> {
        self.clipboard.lock().ok().and_then(|mut g| g.take())
    }

    pub fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::SeqCst);
    }

    /// Drop viewport cells for background tabs so clones do not linger.
    pub fn release_snapshot(&mut self) {
        self.snapshot.cells.clear();
        self.snapshot.cells.shrink_to(0);
    }

    pub fn is_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Returns true once if a BEL was received since the last call.
    pub fn saw_bell(&self) -> bool {
        self.bell.swap(false, Ordering::SeqCst)
    }

    pub fn cwd(&self) -> Option<PathBuf> {
        self.cwd.lock().ok().and_then(|g| g.clone())
    }

    pub fn activity(&self) -> Option<String> {
        self.activity.lock().ok().and_then(|g| g.clone())
    }

    pub fn agent(&self) -> AgentInfo {
        self.agent.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code.lock().ok().and_then(|g| *g)
    }

    pub fn write(&mut self, bytes: impl Into<Vec<u8>>) {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return;
        }
        self.notifier.notify(bytes);
        let mut term = self.term.lock();
        term.selection = None;
        term.scroll_display(Scroll::Bottom);
    }

    pub fn scroll(&mut self, lines: i32) {
        let mut term = self.term.lock();
        if lines == 0 {
            return;
        }
        term.scroll_display(Scroll::Delta(lines));
    }

    /// Follows the layout size. While it is still changing the old grid keeps being painted
    /// (clipped / background-padded by the view); `apply_size` runs once it is stable.
    /// alacritty_terminal only reflows the primary grid (`grid.resize(!ALT_SCREEN, ..)`), so on
    /// the alternate screen a resize just changes the dimensions — no rewrapped rows, no old
    /// frames pushed into scrollback.
    pub fn resize(&mut self, pixel_w: f32, pixel_h: f32, metrics: CellMetrics) -> ResizeState {
        let Some((cols, lines)) =
            grid_size(pixel_w, pixel_h, metrics.width, metrics.height)
        else {
            return ResizeState::from_pending(self.debounce.is_pending());
        };
        let cell_w = metrics.width.max(1.0);
        let cell_h = metrics.height.max(1.0);
        let immediate = std::mem::take(&mut self.apply_now);
        if let Some((cols, lines)) = self.debounce.update((cols, lines), Instant::now(), immediate) {
            self.apply_size(cols, lines, cell_w, cell_h);
            return ResizeState::Applied;
        }
        ResizeState::from_pending(self.debounce.is_pending())
    }

    /// Skip the debounce on the next `resize` (drag released, panel shown/hidden).
    pub fn apply_next_resize_now(&mut self) {
        self.apply_now = true;
    }

    /// The only place that changes the size: grid first, then the PTY, so they never differ.
    fn apply_size(&mut self, cols: u16, lines: u16, cell_w: f32, cell_h: f32) {
        self.size = TerminalSize {
            cell_width: cell_w,
            cell_height: cell_h,
            num_cols: cols,
            num_lines: lines,
        };
        if let Ok(mut g) = self.window_size.lock() {
            *g = self.size.into();
        }
        self.term.lock().resize(TermSize::new(cols as usize, lines as usize));
        self.notifier.on_resize(self.size.into());
        if !self.snapshot.cells.is_empty() {
            self.applied_at = Some(Instant::now());
        }
    }

    /// True while `sync` is still returning the pre-resize frame; repaint soon.
    pub fn holding_frame(&self) -> bool {
        self.applied_at.is_some()
    }

    pub fn sync(&mut self) -> &Snapshot {
        if !self.visible.load(Ordering::Relaxed) {
            self.applied_at = None;
            self.release_snapshot();
            return &self.snapshot;
        }
        if let Some(applied_at) = self.applied_at {
            let last_output = self.last_output.lock().map(|t| *t).unwrap_or(applied_at);
            if hold_frame(Instant::now(), applied_at, last_output) {
                return &self.snapshot;
            }
            self.applied_at = None;
        }

        let term = self.term.lock();
        let content = term.renderable_content();
        let capacity = (self.size.num_cols as usize).saturating_mul(self.size.num_lines as usize);
        self.snapshot.cells.clear();
        if self.snapshot.cells.capacity() < capacity {
            self.snapshot
                .cells
                .reserve(capacity - self.snapshot.cells.capacity());
        }

        let selection = content.selection;
        for indexed in content.display_iter {
            let flags = indexed.flags;
            if flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let selected = selection.is_some_and(|r| r.contains(indexed.point));
            if is_default_skip(indexed.c, indexed.fg, indexed.bg, flags, selected) {
                continue;
            }
            self.snapshot.cells.push(VisibleCell {
                line: indexed.point.line.0,
                col: indexed.point.column.0 as u16,
                c: indexed.c,
                fg: indexed.fg,
                bg: indexed.bg,
                flags,
            });
        }

        self.snapshot.cursor = content.cursor.point;
        self.snapshot.cursor_shape = content.cursor.shape;
        self.snapshot.cursor_color = content.colors[NamedColor::Cursor];
        self.snapshot.display_offset = content.display_offset;
        self.snapshot.selection = selection;
        self.snapshot.mode = content.mode;
        self.snapshot.size = self.size;
        &self.snapshot
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn start_selection(&mut self, x: f32, y: f32, ty: SelectionType) {
        let mut term = self.term.lock();
        let point = selection_point(x, y, &self.size, term.grid().display_offset());
        term.selection = Some(Selection::new(ty, point, side_for(x, &self.size)));
    }

    pub fn update_selection(&mut self, x: f32, y: f32) {
        let mut term = self.term.lock();
        let display_offset = term.grid().display_offset();
        if let Some(ref mut selection) = term.selection {
            let point = selection_point(x, y, &self.size, display_offset);
            selection.update(point, side_for(x, &self.size));
        }
    }

    pub fn clear_selection(&mut self) {
        let mut term = self.term.lock();
        term.selection = None;
    }

    pub fn has_selection(&self) -> bool {
        let term = self.term.lock();
        term.selection
            .as_ref()
            .and_then(|s| s.to_range(&term))
            .is_some()
    }

    pub fn selected_text(&self) -> String {
        let term = self.term.lock();
        term.selection_to_string().unwrap_or_default()
    }

    /// Cell under local pixel coords (0-based). Used for mouse reporting.
    pub fn cell_at(&self, x: f32, y: f32) -> (u16, u16) {
        let col = ((x / self.size.cell_width).floor() as i32)
            .clamp(0, self.size.num_cols as i32 - 1) as u16;
        let row = ((y / self.size.cell_height).floor() as i32)
            .clamp(0, self.size.num_lines as i32 - 1) as u16;
        (col, row)
    }

    pub fn report_mouse(
        &mut self,
        button: u8,
        col: u16,
        row: u16,
        pressed: bool,
        motion: bool,
        modifiers: u8,
    ) {
        let key = (col, row, button | modifiers | if motion { 32 } else { 0 });
        if motion {
            // Motion is only useful when the cell changes; pixel jitter inside one
            // cell would otherwise flood the PTY.
            if self.last_motion == Some(key) {
                return;
            }
            self.last_motion = Some(key);
        } else {
            self.last_motion = None;
        }
        if let Some(bytes) = mouse_sequence(
            self.snapshot.mode,
            button,
            col,
            row,
            pressed,
            motion,
            modifiers,
        ) {
            self.notifier.notify(bytes);
        }
    }

    /// Forget the last motion cell (pointer left the pane).
    pub fn clear_mouse_motion(&mut self) {
        self.last_motion = None;
    }

    pub fn idle_for(&self) -> Duration {
        self.last_output
            .lock()
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}

/// Encode a mouse report. `button` is 0/1/2, 3 for no button, 64/65 for wheel;
/// `modifiers` uses the protocol bits (4 shift, 8 alt, 16 ctrl).
fn mouse_sequence(
    mode: TermMode,
    button: u8,
    col: u16,
    row: u16,
    pressed: bool,
    motion: bool,
    modifiers: u8,
) -> Option<Vec<u8>> {
    // 1-based coordinates for the protocol.
    let x = col.saturating_add(1);
    let y = row.saturating_add(1);
    let mut b = button | modifiers;
    if motion {
        b |= 32;
    }
    if mode.contains(TermMode::SGR_MOUSE) {
        let ch = if pressed || motion { 'M' } else { 'm' };
        Some(format!("\x1b[<{b};{x};{y}{ch}").into_bytes())
    } else {
        // X10 classic — coords must fit in a byte after +32 (≤ 223).
        if x > 223 || y > 223 {
            return None;
        }
        let code = if pressed || motion {
            b
        } else {
            3 | modifiers
        };
        Some(vec![
            0x1b,
            b'[',
            b'M',
            code.saturating_add(32),
            (x as u8).saturating_add(32),
            (y as u8).saturating_add(32),
        ])
    }
}

fn parse_cwd_title(title: &str) -> Option<PathBuf> {
    let trimmed = title.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip common PowerShell / oh-my-posh suffixes.
    let trimmed = trimmed
        .split(['\n', '|', '—', '–'])
        .next()
        .unwrap_or(trimmed)
        .trim();
    let path = PathBuf::from(trimmed);
    // Absolute Windows / Unix paths count as cwd even if the dir check races.
    let looks_abs = path.is_absolute()
        || (trimmed.len() >= 3
            && trimmed.as_bytes()[1] == b':'
            && (trimmed.as_bytes()[2] == b'\\' || trimmed.as_bytes()[2] == b'/'))
        || trimmed.starts_with("\\\\");
    if !looks_abs {
        return None;
    }
    // Reject executables mistakenly set as window title / cwd (was saving powershell.exe).
    if path.is_file() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with(".exe")
        || lower.ends_with(".cmd")
        || lower.ends_with(".bat")
        || lower.ends_with(".com")
    {
        return None;
    }
    Some(path)
}

/// Palette entry reported for OSC 4 / 10 / 11 / 12 queries.
/// cursor is the colour the app set with OSC 12, if any.
fn query_color(colors: &TermColors, index: usize, cursor: Option<Rgb>) -> Option<Rgb> {
    if index == NamedColor::Cursor as usize && cursor.is_some() {
        return cursor;
    }
    let c = match index {
        0..=15 => colors.ansi[index],
        16..=255 => indexed_256(index as u8),
        i if i == NamedColor::Foreground as usize => colors.foreground,
        i if i == NamedColor::Background as usize => colors.background,
        i if i == NamedColor::Cursor as usize => colors.cursor,
        _ => return None,
    };
    Some(Rgb {
        r: c.r(),
        g: c.g(),
        b: c.b(),
    })
}

fn is_default_skip(c: char, _fg: Color, bg: Color, flags: Flags, selected: bool) -> bool {
    if selected {
        return false;
    }
    c == ' '
        && flags.is_empty()
        && bg == Color::Named(NamedColor::Background)
}

fn selection_point(x: f32, y: f32, size: &TerminalSize, display_offset: usize) -> Point {
    let cols = size.num_cols.max(1) as usize;
    let lines = size.num_lines.max(1) as usize;
    let col = ((x / size.cell_width).floor() as isize)
        .clamp(0, cols as isize - 1) as usize;
    let line = ((y / size.cell_height).floor() as isize)
        .clamp(0, lines as isize - 1) as usize;
    viewport_to_point(display_offset, Point::new(line, Column(col)))
}

fn side_for(x: f32, size: &TerminalSize) -> Side {
    let cell_x = x.rem_euclid(size.cell_width);
    if cell_x > size.cell_width * 0.5 {
        Side::Right
    } else {
        Side::Left
    }
}

#[derive(Clone)]
struct EventProxy(mpsc::Sender<Event>);

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_reports_motion_release_wheel_and_modifiers() {
        let mode = TermMode::SGR_MOUSE | TermMode::MOUSE_MOTION;
        assert_eq!(
            mouse_sequence(mode, 3, 4, 2, true, true, 0).unwrap(),
            b"\x1b[<35;5;3M",
            "no-button motion"
        );
        assert_eq!(
            mouse_sequence(mode, 0, 4, 2, false, false, 0).unwrap(),
            b"\x1b[<0;5;3m",
            "release uses lowercase m"
        );
        assert_eq!(
            mouse_sequence(mode, 64, 4, 2, true, false, 4).unwrap(),
            b"\x1b[<68;5;3M",
            "wheel up with shift"
        );
        assert_eq!(
            mouse_sequence(mode, 0, 4, 2, true, false, 16 | 8).unwrap(),
            b"\x1b[<24;5;3M",
            "click with ctrl+alt"
        );
    }

    #[test]
    fn color_queries_get_16_bit_replies_with_the_query_terminator() {
        use alacritty_terminal::vte::ansi::Processor;
        let (tx, rx) = mpsc::channel();
        let mut term = Term::new(
            term::Config::default(),
            &TermSize::new(80, 24),
            EventProxy(tx),
        );
        let mut parser: Processor = Processor::new();
        // OSC 10 / 11 / 12 / 4, with both BEL and ST terminators.
        parser.advance(&mut term, b"\x1b]10;?\x07\x1b]11;?\x1b\\\x1b]12;?\x07\x1b]4;4;?\x1b\\");
        let colors = TermColors::default();
        let set_cursor = term.colors()[NamedColor::Cursor];
        let replies: Vec<String> = rx
            .try_iter()
            .filter_map(|event| match event {
                Event::ColorRequest(index, format) => {
                    query_color(&colors, index, set_cursor).map(|rgb| format(rgb))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            replies,
            [
                "\x1b]10;rgb:e8e8/e8e8/e8e8\x07",
                "\x1b]11;rgb:0000/0000/0000\x1b\\",
                "\x1b]12;rgb:7676/9999/fefe\x07",
                "\x1b]4;4;rgb:5a5a/a0a0/f2f2\x1b\\",
            ],
            "bytes doubled, same terminator as the query"
        );
    }

    /// What Junie (and any TUI picking light/dark) sends: OSC 11 with BEL, expecting the
    /// background back with the same terminator; black means "dark theme".
    #[test]
    fn osc_11_query_with_bel_gets_the_black_background_back_with_bel() {
        use alacritty_terminal::vte::ansi::Processor;
        let (tx, rx) = mpsc::channel();
        let mut term = Term::new(term::Config::default(), &TermSize::new(80, 24), EventProxy(tx));
        Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new().advance(&mut term, b"\x1b]11;?\x07");
        let colors = TermColors::default();
        let replies: Vec<Vec<u8>> = rx
            .try_iter()
            .filter_map(|e| match e {
                Event::ColorRequest(i, format) => query_color(&colors, i, None).map(|c| format(c).into_bytes()),
                _ => None,
            })
            .collect();
        assert_eq!(replies, [b"\x1b]11;rgb:0000/0000/0000\x07".to_vec()]);
    }

    #[test]
    fn device_attribute_queries_are_answered() {
        use alacritty_terminal::vte::ansi::Processor;
        let (tx, rx) = mpsc::channel();
        let mut term = Term::new(term::Config::default(), &TermSize::new(80, 24), EventProxy(tx));
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, b"\x1b[c\x1b[>c");
        let writes: Vec<String> = rx
            .try_iter()
            .filter_map(|e| if let Event::PtyWrite(s) = e { Some(s) } else { None })
            .collect();
        assert_eq!(writes.len(), 2);
        assert!(writes[0].starts_with("\x1b[?") && writes[0].ends_with('c'), "DA1");
        assert!(writes[1].starts_with("\x1b[>") && writes[1].ends_with('c'), "DA2");
    }

    #[test]
    fn cursor_query_answers_with_the_colour_the_app_set_until_osc_112() {
        use alacritty_terminal::vte::ansi::Processor;
        let (tx, _rx) = mpsc::channel();
        let mut term = Term::new(term::Config::default(), &TermSize::new(80, 24), EventProxy(tx));
        let mut parser: Processor = Processor::new();
        let colors = TermColors::default();
        let blue = Rgb { r: 0x3b, g: 0x82, b: 0xf6 };
        let reply = |term: &Term<EventProxy>| {
            query_color(&colors, NamedColor::Cursor as usize, term.colors()[NamedColor::Cursor])
        };
        parser.advance(&mut term, b"\x1b]12;#3b82f6\x07");
        assert_eq!(term.colors()[NamedColor::Cursor], Some(blue));
        assert_eq!(reply(&term), Some(blue));
        parser.advance(&mut term, b"\x1b]112\x07");
        assert_eq!(term.colors()[NamedColor::Cursor], None);
        assert_eq!(
            reply(&term).map(|c| (c.r, c.g, c.b)),
            Some((0x76, 0x99, 0xFE)),
            "default is the Ankar accent blue"
        );
    }

    #[test]
    fn x10_drops_coordinates_that_do_not_fit_a_byte() {
        let mode = TermMode::MOUSE_REPORT_CLICK;
        assert!(mouse_sequence(mode, 0, 223, 0, true, false, 0).is_none());
        assert_eq!(
            mouse_sequence(mode, 0, 0, 0, false, false, 4).unwrap(),
            vec![0x1b, b'[', b'M', 32 + 7, 32 + 1, 32 + 1],
            "release is button 3 plus modifiers"
        );
    }
}
