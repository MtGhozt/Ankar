//! OpenCode reports its state with `ESC ] 7501 ; state=working:id=ses_..:app=opencode:title=<b64> ST`.
//! alacritty_terminal only logs unknown OSCs, so a thin `Pty` wrapper scans the raw bytes first.

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{ChildEvent, EventedPty, EventedReadWrite};
use polling::{Event, PollMode, Poller};
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Longer OSC payloads (OSC 52 clipboard, images) are skipped, not buffered.
const MAX_OSC: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Working,
    Clear,
}

/// Latest 7501 report. `state` stays `None` for PTYs that never send one.
#[derive(Clone, Debug, Default)]
pub struct AgentInfo {
    pub state: Option<AgentState>,
    pub session_id: Option<String>,
    pub title: Option<String>,
    /// When the last report arrived; a `Working` that stays silent for too long is stale.
    pub updated: Option<Instant>,
}

#[derive(Clone, Copy)]
enum Mode {
    Ground,
    Esc,
    Csi,
    Osc,
    OscEsc,
}

/// Sends bytes back to the program on the PTY.
pub type Reply = Box<dyn Fn(String) + Send>;

/// `CSI > q` (XTVERSION). TUIs ask who the terminal is and fall back to a bare-bones mode
/// when nothing answers; alacritty_terminal does not handle it.
fn xtversion_reply() -> String {
    format!("\x1bP>|Ankar {}\x1b\\", env!("CARGO_PKG_VERSION"))
}

/// Byte-at-a-time OSC scanner; keeps its state between `feed` calls, so a sequence
/// split across two reads still parses.
pub struct OscScanner {
    mode: Mode,
    buf: Vec<u8>,
    overflow: bool,
    info: Arc<Mutex<AgentInfo>>,
    /// Same slot the OSC title parser writes: OSC 7 updates it mid-command.
    cwd: Arc<Mutex<Option<PathBuf>>>,
    /// Parameter and intermediate bytes of the CSI being read.
    csi: Vec<u8>,
    reply: Option<Reply>,
}

impl OscScanner {
    pub fn new(info: Arc<Mutex<AgentInfo>>, cwd: Arc<Mutex<Option<PathBuf>>>) -> Self {
        Self {
            mode: Mode::Ground,
            buf: Vec::new(),
            overflow: false,
            info,
            cwd,
            csi: Vec::new(),
            reply: None,
        }
    }

    pub fn with_reply(mut self, reply: Reply) -> Self {
        self.reply = Some(reply);
        self
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.mode = match (self.mode, b) {
                (Mode::Ground | Mode::Esc | Mode::Csi | Mode::OscEsc, 0x1b) => Mode::Esc,
                (Mode::Esc, b'[') => {
                    self.csi.clear();
                    Mode::Csi
                }
                (Mode::Csi, 0x20..=0x3f) => {
                    if self.csi.len() < 16 {
                        self.csi.push(b);
                    }
                    Mode::Csi
                }
                (Mode::Csi, 0x40..=0x7e) => {
                    if b == b'q' && (self.csi == b">" || self.csi == b">0") {
                        if let Some(reply) = &self.reply {
                            reply(xtversion_reply());
                        }
                    }
                    Mode::Ground
                }
                (Mode::Esc, b']') | (Mode::OscEsc, b']') => {
                    self.buf.clear();
                    self.overflow = false;
                    Mode::Osc
                }
                (Mode::OscEsc, b'\\') => self.finish(),
                (Mode::Osc, 0x07) => self.finish(),
                (Mode::Osc, 0x1b) => Mode::OscEsc,
                (Mode::Osc, 0x18 | 0x1a) => Mode::Ground,
                (Mode::Osc, _) => {
                    if self.buf.len() < MAX_OSC {
                        self.buf.push(b);
                    } else {
                        self.overflow = true;
                    }
                    Mode::Osc
                }
                _ => Mode::Ground,
            };
        }
    }

    fn finish(&mut self) -> Mode {
        if !self.overflow {
            if let Some(rest) = self.buf.strip_prefix(b"7501;") {
                if let Ok(report) = std::str::from_utf8(rest) {
                    if let Ok(mut info) = self.info.lock() {
                        apply_report(&mut info, report);
                    }
                }
            } else if let Some(rest) = self.buf.strip_prefix(b"12;") {
                // Only logged: alacritty_terminal applies it; this shows what the app really sends.
                log::info!("OSC 12 cursor colour: {:?}", String::from_utf8_lossy(rest));
            } else if self.buf == b"112" || self.buf.starts_with(b"112;") {
                log::info!("OSC 112 cursor colour reset");
            } else if let Some(rest) = self.buf.strip_prefix(b"7;") {
                // Keep everything after `7;`: a path may itself contain `;`.
                if let Ok(payload) = std::str::from_utf8(rest) {
                    if let Some(path) = parse_osc7(payload) {
                        if let Ok(mut guard) = self.cwd.lock() {
                            *guard = Some(path);
                        }
                    }
                }
            }
        }
        Mode::Ground
    }
}

/// `file://<host>/<percent-encoded-path>` → local path. Remote hosts are ignored.
fn parse_osc7(payload: &str) -> Option<PathBuf> {
    let rest = payload.strip_prefix("file://")?;
    let slash = rest.find('/')?;
    let (host, path) = (&rest[..slash], &rest[slash..]);
    if !is_local_host(host) || path.is_empty() || path == "/" {
        return None;
    }
    let decoded = percent_decode(path);
    // `file:///C:/foo` → `C:/foo` on Windows.
    let path = if decoded.len() >= 3 && decoded.as_bytes()[0] == b'/' && decoded.as_bytes()[2] == b':'
    {
        decoded[1..].to_string()
    } else {
        decoded
    };
    Some(PathBuf::from(path))
}

fn is_local_host(host: &str) -> bool {
    let matches = |var: &str| {
        std::env::var(var)
            .map(|name| name.eq_ignore_ascii_case(host))
            .unwrap_or(false)
    };
    host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || matches("COMPUTERNAME")
        || matches("HOSTNAME")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `key=value` pairs; OpenCode separates them with `:` (`;` is accepted too).
/// Reports without a known `state` (the `7501;?` query) are ignored.
fn apply_report(info: &mut AgentInfo, report: &str) {
    let mut state = None;
    let mut id = None;
    let mut title = None;
    for pair in report.split([':', ';']) {
        match pair.split_once('=') {
            Some(("state", "idle")) => state = Some(AgentState::Idle),
            Some(("state", "working")) => state = Some(AgentState::Working),
            Some(("state", "clear")) => state = Some(AgentState::Clear),
            Some(("id", v)) => id = Some(v.to_owned()),
            Some(("title", v)) => title = decode_base64(v),
            _ => {}
        }
    }
    match state {
        Some(AgentState::Clear) => {
            *info = AgentInfo { state, updated: Some(Instant::now()), ..AgentInfo::default() }
        }
        Some(_) => {
            info.updated = Some(Instant::now());
            info.state = state;
            info.session_id = id.or(info.session_id.take());
            info.title = title.or(info.title.take());
        }
        None => {}
    }
}

/// Standard or URL-safe base64, padding optional; `None` if it is not UTF-8 text.
fn decode_base64(s: &str) -> Option<String> {
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0u32);
    for c in s.bytes().take_while(|&c| c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    String::from_utf8(out).ok().filter(|t| !t.trim().is_empty())
}

/// Wraps a `Pty` so every chunk the event loop reads also goes through the scanner.
pub struct OscTap<P: EventedPty + OnResize> {
    // Raw pointer so the reader wrapper and the pty can both be reached from `&mut self`.
    // It comes from `Box::into_raw` and is freed in `Drop`; it is only used from the
    // event-loop thread, through `&mut OscTap`, so accesses never overlap.
    pty: *mut P,
    reader: TapReader<P>,
}

// SAFETY: `OscTap` owns the pointee exclusively, so it is as `Send` as `P` itself.
unsafe impl<P: EventedPty + OnResize + Send> Send for OscTap<P> {}

impl<P: EventedPty + OnResize> OscTap<P> {
    pub fn new(
        pty: P,
        info: Arc<Mutex<AgentInfo>>,
        cwd: Arc<Mutex<Option<PathBuf>>>,
        reply: Reply,
    ) -> Self {
        let pty = Box::into_raw(Box::new(pty));
        let scanner = OscScanner::new(info, cwd).with_reply(reply);
        Self { pty, reader: TapReader { pty, scanner } }
    }

    fn pty(&mut self) -> &mut P {
        // SAFETY: see the field comment.
        unsafe { &mut *self.pty }
    }
}

impl<P: EventedPty + OnResize> Drop for OscTap<P> {
    fn drop(&mut self) {
        // SAFETY: allocated in `new`, never freed elsewhere.
        drop(unsafe { Box::from_raw(self.pty) });
    }
}

pub struct TapReader<P: EventedReadWrite> {
    pty: *mut P,
    scanner: OscScanner,
}

impl<P: EventedReadWrite> Read for TapReader<P> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // SAFETY: only called through `OscTap::reader(&mut self)`; see `OscTap::pty`.
        let n = unsafe { &mut *self.pty }.reader().read(buf)?;
        self.scanner.feed(&buf[..n]);
        Ok(n)
    }
}

impl<P: EventedPty + OnResize> EventedReadWrite for OscTap<P> {
    type Reader = TapReader<P>;
    type Writer = P::Writer;

    unsafe fn register(&mut self, poll: &Arc<Poller>, event: Event, mode: PollMode) -> io::Result<()> {
        self.pty().register(poll, event, mode)
    }

    fn reregister(&mut self, poll: &Arc<Poller>, event: Event, mode: PollMode) -> io::Result<()> {
        self.pty().reregister(poll, event, mode)
    }

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.pty().deregister(poll)
    }

    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }

    fn writer(&mut self) -> &mut Self::Writer {
        self.pty().writer()
    }
}

impl<P: EventedPty + OnResize> EventedPty for OscTap<P> {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.pty().next_child_event()
    }
}

impl<P: EventedPty + OnResize> OnResize for OscTap<P> {
    fn on_resize(&mut self, window_size: WindowSize) {
        self.pty().on_resize(window_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(chunks: &[&[u8]]) -> AgentInfo {
        let info = Arc::new(Mutex::new(AgentInfo::default()));
        let mut scanner = OscScanner::new(info.clone(), Arc::new(Mutex::new(None)));
        for c in chunks {
            scanner.feed(c);
        }
        let out = info.lock().unwrap().clone();
        out
    }

    fn scan_cwd(chunks: &[&[u8]]) -> Option<PathBuf> {
        let cwd = Arc::new(Mutex::new(None));
        let mut scanner = OscScanner::new(Arc::new(Mutex::new(AgentInfo::default())), cwd.clone());
        for c in chunks {
            scanner.feed(c);
        }
        let out = cwd.lock().unwrap().clone();
        out
    }

    #[test]
    fn osc7_sets_the_same_cwd_slot_across_reads() {
        assert_eq!(
            scan_cwd(&[b"\x1b]7;file://localhost/C:/foo/bar\x07"]),
            Some(PathBuf::from("C:/foo/bar"))
        );
        assert_eq!(
            scan_cwd(&[b"\x1b]7;file://localhost/tmp/fo", b"o%20bar\x1b\\"]),
            Some(PathBuf::from("/tmp/foo bar"))
        );
        assert_eq!(scan_cwd(&[b"\x1b]7;file://remote/C:/foo\x07"]), None);
        // Empty host is local, and the leading slash before the drive is dropped.
        assert_eq!(
            scan_cwd(&[b"\x1b]7;file:///C:/trunc\x07"]),
            Some(PathBuf::from("C:/trunc"))
        );
        assert_eq!(scan_cwd(&[b"\x1b]2;C:\\not-osc7\x07"]), None);
    }

    #[test]
    fn parses_bel_and_st_split_across_reads() {
        let a = scan(&[b"junk\x1b]7501;state=working:id=ses_1:app=opencode:title=TmV3IHNlc3Npb24=\x07"]);
        assert_eq!(a.state, Some(AgentState::Working));
        assert_eq!(a.session_id.as_deref(), Some("ses_1"));
        assert_eq!(a.title.as_deref(), Some("New session"));

        let b = scan(&[b"\x1b]75", b"01;state=id", b"le:app=opencode\x1b", b"\\"]);
        assert_eq!(b.state, Some(AgentState::Idle));
    }

    #[test]
    fn clear_wipes_and_queries_are_ignored() {
        let a = scan(&[
            b"\x1b]7501;state=working:id=ses_1:title=SGk=\x07",
            b"\x1b]7501;?\x07",
            b"\x1b]99;i=x:p=?;\x07",
            b"\x1b]52;c;aGVsbG8=\x07",
        ]);
        assert_eq!((a.state, a.title.as_deref()), (Some(AgentState::Working), Some("Hi")));
        let b = scan(&[b"\x1b]7501;state=working:title=SGk=\x07\x1b]7501;state=clear:id=ses_1\x07"]);
        assert_eq!(b.state, Some(AgentState::Clear));
        assert!(b.title.is_none() && b.session_id.is_none());
    }

    fn replies(chunks: &[&[u8]]) -> Vec<String> {
        let out = Arc::new(Mutex::new(Vec::new()));
        let sink = out.clone();
        let mut scanner = OscScanner::new(Arc::new(Mutex::new(AgentInfo::default())), Arc::new(Mutex::new(None)))
            .with_reply(Box::new(move |s| sink.lock().unwrap().push(s)));
        for c in chunks {
            scanner.feed(c);
        }
        let out = out.lock().unwrap().clone();
        out
    }

    #[test]
    fn xtversion_is_answered_with_a_dcs_naming_ankar() {
        let want = format!("\x1bP>|Ankar {}\x1b\\", env!("CARGO_PKG_VERSION"));
        assert_eq!(replies(&[b"\x1b[>q"]), std::slice::from_ref(&want));
        assert_eq!(replies(&[b"abc\x1b[>0", b"q"]), [want], "split across reads, explicit 0");
        // DECSCUSR, DA2 and other CSI finals are not XTVERSION.
        assert!(replies(&[b"\x1b[2 q\x1b[>c\x1b[?25h\x1b[31;1m"]).is_empty());
    }

    #[test]
    fn plain_ptys_stay_none() {
        assert!(scan(&[b"hello\x1b[31mworld\x1b[0m\x1b]0;title\x07"]).state.is_none());
    }
}
