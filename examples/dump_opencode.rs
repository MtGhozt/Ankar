//! Temporary: spawn opencode in a ConPTY and dump the rendered grid cells.
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::EventLoop;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::Color;
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy)]
struct Size {
    w: usize,
    h: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.h
    }
    fn screen_lines(&self) -> usize {
        self.h
    }
    fn columns(&self) -> usize {
        self.w
    }
}

#[derive(Clone)]
struct Proxy(mpsc::Sender<Event>);
impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

fn main() {
    let size = Size { w: 120, h: 40 };
    let env = HashMap::from([
        ("TERM".to_owned(), "xterm-256color".to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
    ]);
    let pty_config = tty::Options {
        shell: Some(tty::Shell::new(
            "powershell.exe".to_owned(),
            vec![
                "-NoLogo".to_owned(),
                "-Command".to_owned(),
                "opencode".to_owned(),
            ],
        )),
        working_directory: Some(std::env::current_dir().unwrap()),
        env,
        ..tty::Options::default()
    };
    let ws = WindowSize {
        num_lines: size.h as u16,
        num_cols: size.w as u16,
        cell_width: 9,
        cell_height: 18,
    };
    let pty = tty::new(&pty_config, ws, 0).expect("pty");
    let (tx, _rx) = mpsc::channel();
    let proxy = Proxy(tx);
    let term = Arc::new(FairMutex::new(Term::new(
        alacritty_terminal::term::Config::default(),
        &size,
        proxy.clone(),
    )));
    let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false).expect("loop");
    let _thread = event_loop.spawn();

    std::thread::sleep(Duration::from_secs(12));

    let term = term.lock();
    let content = term.renderable_content();
    println!(
        "mode={:?} cursor={:?} display_offset={}",
        content.mode, content.cursor.point, content.display_offset
    );
    println!(
        "cursor_shape={:?} style={:?} osc12={:?} fg_osc={:?} bg_osc={:?}",
        content.cursor.shape,
        term.cursor_style(),
        content.colors[alacritty_terminal::vte::ansi::NamedColor::Cursor],
        content.colors[alacritty_terminal::vte::ansi::NamedColor::Foreground],
        content.colors[alacritty_terminal::vte::ansi::NamedColor::Background],
    );
    let mut rows: std::collections::BTreeMap<i32, (String, Vec<String>)> = Default::default();
    for indexed in content.display_iter {
        let ch = indexed.c;
        let is_box = ('\u{2500}'..='\u{259f}').contains(&ch);
        let special = is_box
            || ch != ' '
            || indexed.bg != Color::Spec(alacritty_terminal::vte::ansi::Rgb { r: 8, g: 8, b: 8 });
        let entry = rows.entry(indexed.point.line.0).or_default();
        entry.0.push(if ch == ' ' { '.' } else { ch });
        if special {
            let fg = match indexed.fg {
                Color::Spec(c) => Some(c),
                _ => None,
            };
            let bg = match indexed.bg {
                Color::Spec(c) => Some(c),
                _ => None,
            };
            let fg = fg.map(|c| (c.r, c.g, c.b));
            let bg = bg.map(|c| (c.r, c.g, c.b));
            entry.1.push(format!(
                "c{}={:?}U+{:04X} fg={:?} bg={:?} inv={}",
                indexed.point.column.0,
                ch,
                ch as u32,
                fg,
                bg,
                indexed
                    .flags
                    .contains(alacritty_terminal::term::cell::Flags::INVERSE)
            ));
        }
    }
    for (line, (strip, cells)) in rows {
        if cells.is_empty() {
            continue;
        }
        println!("LINE {line:02} |{}|", strip);
        println!("        {}", cells.join(" | "));
    }
    let _ = content.mode.contains(TermMode::MOUSE_MOTION);
    let _ = Column(0);
    let _ = Line(0);
}
