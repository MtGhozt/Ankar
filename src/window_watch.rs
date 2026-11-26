//! Window-state log and guard for the "pops" (flashes on focus, restore or DPI change). Every
//! change of focus, occlusion, minimize, maximize, DPI or size goes to the log with a
//! millisecond timestamp. A frame that cannot be a real layout (minimized, or smaller than the
//! window's minimum size) must not resize the terminals: the last good frame stays.

use std::time::Instant;

/// `with_min_inner_size` in `main.rs` is 640 x 400; anything well below it is a transient.
const MIN_REAL_W: f32 = 600.0;
const MIN_REAL_H: f32 = 360.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowFacts {
    pub focused: bool,
    pub occluded: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub ppp: f32,
    /// Screen rect in points.
    pub size: (f32, f32),
}

impl WindowFacts {
    pub fn read(ctx: &egui::Context) -> Self {
        ctx.input(|i| {
            let v = i.viewport();
            let r = i.content_rect();
            WindowFacts {
                focused: i.focused,
                occluded: v.occluded.unwrap_or(false),
                minimized: v.minimized.unwrap_or(false),
                maximized: v.maximized.unwrap_or(false),
                ppp: i.pixels_per_point,
                size: (r.width(), r.height()),
            }
        })
    }

    /// This frame's layout is not one the user sees: do not resize terminals to it.
    pub fn layout_untrusted(&self) -> bool {
        self.minimized || self.size.0 < MIN_REAL_W || self.size.1 < MIN_REAL_H
    }
}

/// Log lines for what changed between two frames.
pub fn changes(prev: &WindowFacts, now: &WindowFacts) -> Vec<String> {
    let mut out = Vec::new();
    let mut flag = |name: &str, a: bool, b: bool| {
        if a != b {
            out.push(format!("{name} {a} -> {b}"));
        }
    };
    flag("focused", prev.focused, now.focused);
    flag("occluded", prev.occluded, now.occluded);
    flag("minimized", prev.minimized, now.minimized);
    flag("maximized", prev.maximized, now.maximized);
    if prev.ppp != now.ppp {
        out.push(format!("pixels_per_point {} -> {}", prev.ppp, now.ppp));
    }
    let round = |s: (f32, f32)| (s.0.round(), s.1.round());
    if round(prev.size) != round(now.size) {
        let ((pw, ph), (nw, nh)) = (round(prev.size), round(now.size));
        out.push(format!("size {pw}x{ph} -> {nw}x{nh}"));
    }
    out
}

pub struct WindowWatch {
    prev: Option<WindowFacts>,
    started: Instant,
}

impl Default for WindowWatch {
    fn default() -> Self {
        Self { prev: None, started: Instant::now() }
    }
}

impl WindowWatch {
    /// Logs what changed since last frame; returns this frame's facts and whether the DPI moved.
    pub fn frame(&mut self, ctx: &egui::Context) -> (WindowFacts, bool) {
        let now = WindowFacts::read(ctx);
        let prev = self.prev.replace(now).unwrap_or(now);
        let lines = changes(&prev, &now);
        if !lines.is_empty() {
            let held = if now.layout_untrusted() { " (layout held)" } else { "" };
            log::info!("window t+{}ms: {}{held}", self.started.elapsed().as_millis(), lines.join(", "));
        }
        (now, prev.ppp != now.ppp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal() -> WindowFacts {
        WindowFacts { focused: true, ppp: 1.5, size: (1280.0, 760.0), ..WindowFacts::default() }
    }

    #[test]
    fn only_real_changes_are_logged() {
        let a = normal();
        assert!(changes(&a, &a).is_empty());
        assert!(changes(&a, &WindowFacts { size: (1280.2, 759.9), ..a }).is_empty(), "sub-point jitter");
        let b = WindowFacts { focused: false, occluded: true, ppp: 1.25, size: (1536.0, 912.0), ..a };
        assert_eq!(
            changes(&a, &b),
            vec!["focused true -> false", "occluded false -> true", "pixels_per_point 1.5 -> 1.25", "size 1280x760 -> 1536x912"]
        );
    }

    #[test]
    fn minimized_or_tiny_frames_hold_the_layout() {
        assert!(!normal().layout_untrusted());
        assert!(!WindowFacts { size: (640.0, 400.0), ..normal() }.layout_untrusted(), "the minimum window is real");
        assert!(WindowFacts { minimized: true, ..normal() }.layout_untrusted());
        assert!(WindowFacts { size: (160.0, 28.0), ..normal() }.layout_untrusted(), "iconic size during restore");
        assert!(WindowFacts { size: (0.0, 0.0), ..normal() }.layout_untrusted());
        assert!(!WindowFacts { occluded: true, ..normal() }.layout_untrusted(), "covered is still a real size");
    }
}
