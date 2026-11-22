use super::theme::TermColors;
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
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use egui::Context;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_SCROLLBACK: usize = 50_000;

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
            display_offset: 0,
            selection: None,
            mode: TermMode::empty(),
            size: TerminalSize::default(),
        }
    }
}

pub struct TerminalSession {
    #[allow(dead_code)]
    pub id: u64,
    #[allow(dead_code)]
    pub title: String,
    term: Arc<FairMutex<Term<EventProxy>>>,
    size: TerminalSize,
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
    exit_code: Arc<Mutex<Option<i32>>>,
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
        let pty_config = tty::Options {
            shell: Some(tty::Shell::new(program.to_string(), args.to_vec())),
            working_directory: cwd.clone(),
            ..tty::Options::default()
        };

        let history = scrollback_lines.min(MAX_SCROLLBACK);
        let config = term::Config {
            scrolling_history: history,
            ..term::Config::default()
        };
        let terminal_size = TerminalSize::default();
        let pty = tty::new(&pty_config, terminal_size.into(), id)
            .map_err(|e| anyhow::anyhow!("failed to open PTY for {program}: {e}"))?;

        let (event_sender, event_receiver) = mpsc::channel();
        let event_proxy = EventProxy(event_sender);
        let term = Term::new(config, &terminal_size, event_proxy.clone());

        let term = Arc::new(FairMutex::new(term));
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
        let cwd_state = Arc::new(Mutex::new(cwd));
        let cwd_flag = cwd_state.clone();
        let activity_state = Arc::new(Mutex::new(None::<String>));
        let activity_flag = activity_state.clone();
        let exit_code_state = Arc::new(Mutex::new(None::<i32>));
        let exit_code_flag = exit_code_state.clone();
        let title = program
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(program)
            .to_string();

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
                        _ => {
                            if visible_flag.load(Ordering::Relaxed) {
                                ctx.request_repaint_after(Duration::from_millis(8));
                            }
                        }
                    }
                }
            })?;

        Ok(Self {
            id,
            title,
            term,
            size: terminal_size,
            notifier,
            snapshot: Snapshot::default(),
            exited,
            last_output,
            bell,
            visible,
            cwd: cwd_state,
            activity: activity_state,
            exit_code: exit_code_state,
            colors: TermColors::default(),
        })
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

    pub fn resize(&mut self, pixel_w: f32, pixel_h: f32, metrics: CellMetrics) {
        let cell_w = metrics.width.max(1.0);
        let cell_h = metrics.height.max(1.0);
        let cols = (pixel_w / cell_w).floor() as u16;
        let lines = (pixel_h / cell_h).floor() as u16;
        if cols == 0 || lines == 0 {
            return;
        }
        if cols == self.size.num_cols
            && lines == self.size.num_lines
            && (self.size.cell_width - cell_w).abs() < 0.01
            && (self.size.cell_height - cell_h).abs() < 0.01
        {
            return;
        }

        self.size = TerminalSize {
            cell_width: cell_w,
            cell_height: cell_h,
            num_cols: cols,
            num_lines: lines,
        };

        self.notifier.on_resize(self.size.into());
        let mut term = self.term.lock();
        term.resize(TermSize::new(
            self.size.num_cols as usize,
            self.size.num_lines as usize,
        ));
    }

    pub fn sync(&mut self) -> &Snapshot {
        if !self.visible.load(Ordering::Relaxed) {
            self.release_snapshot();
            return &self.snapshot;
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
    ) {
        let mode = self.snapshot.mode;
        // 1-based coordinates for the protocol.
        let x = col.saturating_add(1);
        let y = row.saturating_add(1);
        let mut b = button;
        if motion {
            b |= 32;
        }
        let bytes = if mode.contains(TermMode::SGR_MOUSE) {
            let ch = if pressed || motion { b'M' } else { b'm' };
            format!("\x1b[<{b};{x};{y}{ch}").into_bytes()
        } else {
            // X10 classic — coords must fit in a byte after +32 (≤ 223).
            if x > 223 || y > 223 {
                return;
            }
            let release_or = if pressed || motion { b } else { 3 };
            vec![
                0x1b,
                b'[',
                b'M',
                release_or.saturating_add(32),
                (x as u8).saturating_add(32),
                (y as u8).saturating_add(32),
            ]
        };
        self.notifier.notify(bytes);
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
