use super::theme::TermColors;
use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{self, viewport_to_point, Term, TermMode};
use alacritty_terminal::{tty, Grid};
use egui::Context;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    pub cell_width: u16,
    pub cell_height: u16,
    pub num_cols: u16,
    pub num_lines: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cell_width: 8,
            cell_height: 16,
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
            cell_width: size.cell_width,
            cell_height: size.cell_height,
        }
    }
}

pub struct RenderableContent {
    pub grid: Grid<Cell>,
    pub selectable_range: Option<SelectionRange>,
    pub cursor: Cell,
    pub terminal_mode: TermMode,
    pub terminal_size: TerminalSize,
}

pub struct TerminalSession {
    #[allow(dead_code)]
    pub id: u64,
    #[allow(dead_code)]
    pub title: String,
    term: Arc<FairMutex<Term<EventProxy>>>,
    size: TerminalSize,
    notifier: Notifier,
    last_content: RenderableContent,
    exited: Arc<AtomicBool>,
    last_output: Arc<std::sync::Mutex<Instant>>,
    bell: Arc<AtomicBool>,
    pub colors: TermColors,
}

impl TerminalSession {
    pub fn spawn(
        id: u64,
        ctx: Context,
        program: &str,
        args: &[String],
        cwd: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let pty_config = tty::Options {
            shell: Some(tty::Shell::new(program.to_string(), args.to_vec())),
            working_directory: cwd,
            ..tty::Options::default()
        };

        let config = term::Config::default();
        let terminal_size = TerminalSize::default();
        let pty = tty::new(&pty_config, terminal_size.into(), id)
            .map_err(|e| anyhow::anyhow!("failed to open PTY for {program}: {e}"))?;

        let (event_sender, event_receiver) = mpsc::channel();
        let event_proxy = EventProxy(event_sender);
        let mut term = Term::new(config, &terminal_size, event_proxy.clone());

        let initial = RenderableContent {
            grid: term.grid().clone(),
            selectable_range: None,
            terminal_mode: *term.mode(),
            terminal_size,
            cursor: term.grid_mut().cursor_cell().clone(),
        };

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
        let last_output = Arc::new(std::sync::Mutex::new(Instant::now()));
        let last_output_flag = last_output.clone();
        let title = program
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(program)
            .to_string();

        std::thread::Builder::new()
            .name(format!("nacar-pty-{id}"))
            .spawn(move || {
                while let Ok(event) = event_receiver.recv() {
                    match &event {
                        Event::Exit | Event::ChildExit(_) => {
                            exited_flag.store(true, Ordering::SeqCst);
                            ctx.request_repaint();
                            break;
                        }
                        Event::PtyWrite(data) => {
                            pty_notifier.notify(data.as_bytes().to_vec());
                        }
                        Event::Bell => {
                            bell_flag.store(true, Ordering::SeqCst);
                            ctx.request_repaint();
                        }
                        Event::Wakeup => {
                            if let Ok(mut t) = last_output_flag.lock() {
                                *t = Instant::now();
                            }
                            ctx.request_repaint();
                        }
                        _ => {
                            ctx.request_repaint();
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
            last_content: initial,
            exited,
            last_output,
            bell,
            colors: TermColors::default(),
        })
    }

    pub fn is_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Returns true once if a BEL was received since the last call.
    pub fn saw_bell(&self) -> bool {
        self.bell.swap(false, Ordering::SeqCst)
    }

    pub fn write(&mut self, bytes: impl Into<Vec<u8>>) {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return;
        }
        self.notifier.notify(bytes);
        let mut term = self.term.lock();
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
        let cols = (pixel_w / metrics.width.floor()).floor() as u16;
        let lines = (pixel_h / metrics.height.floor()).floor() as u16;
        if cols == 0 || lines == 0 {
            return;
        }
        if cols == self.size.num_cols
            && lines == self.size.num_lines
            && metrics.width as u16 == self.size.cell_width
            && metrics.height as u16 == self.size.cell_height
        {
            return;
        }

        self.size = TerminalSize {
            cell_width: metrics.width.max(1.0) as u16,
            cell_height: metrics.height.max(1.0) as u16,
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

    pub fn sync(&mut self) -> &RenderableContent {
        let mut term = self.term.lock();
        let selectable_range = term.selection.as_ref().and_then(|s| s.to_range(&term));
        let cursor = term.grid_mut().cursor_cell().clone();
        self.last_content.grid = term.grid().clone();
        self.last_content.selectable_range = selectable_range;
        self.last_content.cursor = cursor;
        self.last_content.terminal_mode = *term.mode();
        self.last_content.terminal_size = self.size;
        &self.last_content
    }

    pub fn last_content(&self) -> &RenderableContent {
        &self.last_content
    }

    pub fn start_selection(&mut self, x: f32, y: f32) {
        let mut term = self.term.lock();
        let point = selection_point(x, y, &self.size, term.grid().display_offset());
        term.selection = Some(Selection::new(SelectionType::Simple, point, side_for(x, &self.size)));
    }

    pub fn update_selection(&mut self, x: f32, y: f32) {
        let mut term = self.term.lock();
        let display_offset = term.grid().display_offset();
        if let Some(ref mut selection) = term.selection {
            let point = selection_point(x, y, &self.size, display_offset);
            selection.update(point, side_for(x, &self.size));
        }
    }

    #[allow(dead_code)]
    pub fn clear_selection(&mut self) {
        let mut term = self.term.lock();
        term.selection = None;
    }

    pub fn selected_text(&self) -> String {
        let content = &self.last_content;
        let Some(range) = content.selectable_range else {
            return String::new();
        };
        let mut out = String::new();
        let mut last_line = None;
        for indexed in content.grid.display_iter() {
            if range.contains(indexed.point) {
                if let Some(prev) = last_line {
                    if indexed.point.line != prev {
                        out.push('\n');
                    }
                }
                out.push(indexed.c);
                last_line = Some(indexed.point.line);
            }
        }
        out
    }

    #[allow(dead_code)]
    pub fn mark_output(&mut self) {
        if let Ok(mut t) = self.last_output.lock() {
            *t = Instant::now();
        }
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

fn selection_point(x: f32, y: f32, size: &TerminalSize, display_offset: usize) -> Point {
    let col = ((x / size.cell_width as f32) as usize).min(size.num_cols as usize - 1);
    let line = ((y / size.cell_height as f32) as usize).min(size.num_lines as usize - 1);
    viewport_to_point(display_offset, Point::new(line, Column(col)))
}

fn side_for(x: f32, size: &TerminalSize) -> Side {
    let cell_x = x as usize % size.cell_width as usize;
    if cell_x > (size.cell_width as usize / 2) {
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
