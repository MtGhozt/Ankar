//! Debounce for terminal size changes. The grid and the PTY must always have the same size,
//! so a size is only ever *applied* (to both) once it has been stable for `DEBOUNCE`;
//! until then the old grid keeps being painted untouched.

use std::time::{Duration, Instant};

pub const DEBOUNCE: Duration = Duration::from_millis(80);

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
