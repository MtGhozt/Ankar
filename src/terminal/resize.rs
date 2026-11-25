//! Debounce for terminal size changes. The grid and the PTY must always have the same size,
//! so a size is only ever *applied* (to both) once it has been stable for `DEBOUNCE`;
//! until then the old grid keeps being painted untouched.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const DEBOUNCE: Duration = Duration::from_millis(80);

/// Set by the app for frames whose layout is not real (minimized, a transient tiny window):
/// terminals keep their size and their last frame instead of shrinking to it.
static LAYOUT_HELD: AtomicBool = AtomicBool::new(false);

pub fn hold_layout(held: bool) {
    LAYOUT_HELD.store(held, Ordering::Relaxed);
}

pub fn layout_held() -> bool {
    LAYOUT_HELD.load(Ordering::Relaxed)
}

/// Grid size for a content rect: floor from the exact content area, `None` for a collapsed
/// pane (0 rows/cols). Minimum 2 columns so a fullwidth character still fits.
pub fn grid_size(pixel_w: f32, pixel_h: f32, cell_w: f32, cell_h: f32) -> Option<(u16, u16)> {
    let cols = (pixel_w / cell_w.max(1.0)).floor();
    let lines = (pixel_h / cell_h.max(1.0)).floor();
    if !cols.is_finite() || !lines.is_finite() || cols < 1.0 || lines < 1.0 {
        return None;
    }
    let cols = (cols as u32).clamp(2, u16::MAX as u32) as u16;
    let lines = (lines as u32).min(u16::MAX as u32) as u16;
    Some((cols, lines))
}

/// Longest a TUI's redraw is waited for after a resize.
const HOLD_MAX: Duration = Duration::from_millis(200);
/// Output silence that means the redraw is done.
const SETTLE: Duration = Duration::from_millis(30);

/// After `apply_size` the grid is blank or half-redrawn until the app repaints it; keep painting
/// the last frame until its output goes quiet, never longer than `HOLD_MAX`.
pub fn hold_frame(now: Instant, applied_at: Instant, last_output: Instant) -> bool {
    if now.saturating_duration_since(applied_at) >= HOLD_MAX {
        return false;
    }
    last_output <= applied_at || now.saturating_duration_since(last_output) < SETTLE
}

#[derive(Default)]
pub struct ResizeDebouncer {
    applied: Option<(u16, u16)>,
    pending: Option<((u16, u16), Instant)>,
}

impl ResizeDebouncer {
    /// Feed the (cols, lines) the layout wants right now. Returns the size to apply to the
    /// grid and the PTY together, or `None` to keep the current one. The first size and
    /// `immediate` requests (drag released, panel toggled) are applied at once.
    pub fn update(&mut self, wanted: (u16, u16), now: Instant, immediate: bool) -> Option<(u16, u16)> {
        if self.applied == Some(wanted) {
            self.pending = None;
            return None;
        }
        if immediate || self.applied.is_none() {
            return self.apply(wanted);
        }
        if self.pending.is_none_or(|(p, _)| p != wanted) {
            self.pending = Some((wanted, now));
        }
        match self.pending {
            Some((p, since)) if now.duration_since(since) >= DEBOUNCE => self.apply(p),
            _ => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn apply(&mut self, size: (u16, u16)) -> Option<(u16, u16)> {
        self.applied = Some(size);
        self.pending = None;
        Some(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-in for `TerminalSession::apply_size`: grid and PTY are always set together.
    #[derive(Default)]
    struct Sizes {
        grid: Option<(u16, u16)>,
        pty: Option<(u16, u16)>,
        applied: Vec<(u16, u16)>,
    }

    impl Sizes {
        fn feed(&mut self, d: &mut ResizeDebouncer, wanted: (u16, u16), now: Instant, immediate: bool) {
            if let Some(s) = d.update(wanted, now, immediate) {
                self.grid = Some(s);
                self.pty = Some(s);
                self.applied.push(s);
            }
            assert_eq!(self.grid, self.pty);
        }
    }

    #[test]
    fn the_old_frame_is_held_until_the_redraw_settles_or_200_ms() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let before = t0 - Duration::from_millis(500);
        assert!(hold_frame(ms(5), t0, before), "the app has not redrawn yet");
        assert!(hold_frame(ms(40), t0, ms(20)), "output 20 ms ago: still drawing");
        assert!(!hold_frame(ms(60), t0, ms(20)), "quiet for 40 ms: show the new frame");
        assert!(!hold_frame(ms(200), t0, ms(199)), "never longer than 200 ms");
        assert!(!hold_frame(ms(250), t0, before), "an app that never redraws is not waited on");
    }

    #[test]
    fn zero_size_is_ignored_and_one_column_clamps() {
        assert_eq!(grid_size(0.0, 200.0, 8.0, 16.0), None);
        assert_eq!(grid_size(200.0, 0.0, 8.0, 16.0), None);
        assert_eq!(grid_size(8.0, 16.0, 8.0, 16.0), Some((2, 1)));
        // Floor, never round up into a partial cell.
        assert_eq!(grid_size(23.9, 33.9, 8.0, 16.0), Some((2, 2)));
    }

    #[test]
    fn burst_back_to_start_applies_once() {
        let (mut d, mut s, t0) = (ResizeDebouncer::default(), Sizes::default(), Instant::now());
        for (i, cols) in [131, 193, 150, 131].into_iter().enumerate() {
            s.feed(&mut d, (cols, 56), t0 + Duration::from_millis(10 * i as u64), false);
        }
        s.feed(&mut d, (131, 56), t0 + Duration::from_millis(500), false);
        assert_eq!(s.applied, vec![(131, 56)]);
        assert!(!d.is_pending());
    }

    #[test]
    fn settles_once_and_flushes_on_demand() {
        let (mut d, mut s, t0) = (ResizeDebouncer::default(), Sizes::default(), Instant::now());
        let ms = |n| t0 + Duration::from_millis(n);
        s.feed(&mut d, (131, 56), ms(0), false);
        for (i, cols) in [193, 150, 160].into_iter().enumerate() {
            s.feed(&mut d, (cols, 56), ms(10 + 10 * i as u64), false);
        }
        assert!(d.is_pending());
        s.feed(&mut d, (160, 56), ms(60), false); // 40 ms since the last change
        assert_eq!(s.applied.len(), 1);
        s.feed(&mut d, (160, 56), ms(110), false); // stable for 80 ms
        assert_eq!(s.applied, vec![(131, 56), (160, 56)]);
        s.feed(&mut d, (100, 56), ms(115), true); // sidebar toggled / drag released
        assert_eq!(s.applied.last(), Some(&(100, 56)));
    }
}
