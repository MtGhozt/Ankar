//! Background loader + 60s cache keyed by range and OpenCode mtimes.

use super::ankar::AnkarStatsStore;
use super::format::pct_change;
use super::{cursor_agent, opencode, StatsSnapshot};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatsRange {
    Days7,
    Days30,
    Days90,
    All,
}

impl StatsRange {
    pub fn label(self) -> &'static str {
        match self {
            Self::Days7 => "7 days",
            Self::Days30 => "30 days",
            Self::Days90 => "90 days",
            Self::All => "All",
        }
    }

    pub fn days(self) -> Option<i64> {
        match self {
            Self::Days7 => Some(7),
            Self::Days30 => Some(30),
            Self::Days90 => Some(90),
            Self::All => None,
        }
    }

    pub fn all() -> [StatsRange; 4] {
        [Self::Days7, Self::Days30, Self::Days90, Self::All]
    }
}

struct CacheEntry {
    range: StatsRange,
    snap: StatsSnapshot,
    loaded_at: Instant,
    db_mtime: (Option<u64>, Option<u64>),
}

/// Request scheduling: one worker at a time, newest requested range wins.
#[derive(Default)]
struct Sched {
    generation: u64,
    inflight: bool,
    pending: Option<StatsRange>,
}

impl Sched {
    /// Some(generation) = launch now; None = queued behind the running load.
    fn submit(&mut self, range: StatsRange) -> Option<u64> {
        if self.inflight {
            self.pending = Some(range);
            return None;
        }
        self.inflight = true;
        self.generation += 1;
        Some(self.generation)
    }

    /// A result arrived: (keep it?, next load to launch).
    fn finish(&mut self, generation: u64) -> (bool, Option<(u64, StatsRange)>) {
        if generation != self.generation {
            return (false, None);
        }
        match self.pending.take() {
            Some(r) => {
                self.generation += 1;
                (false, Some((self.generation, r)))
            }
            None => {
                self.inflight = false;
                (true, None)
            }
        }
    }

    fn abort(&mut self) {
        self.inflight = false;
        self.pending = None;
    }
}

pub struct StatsLoader {
    tx: Option<mpsc::Sender<(u64, StatsSnapshot)>>,
    rx: Receiver<(u64, StatsSnapshot)>,
    sched: Sched,
    /// Warning snapshot when the worker thread could not start.
    failed: Option<StatsSnapshot>,
    cache: Option<CacheEntry>,
    /// Shared read of Ankar store for the worker (cloned snapshot of hours).
    ankar: Arc<Mutex<AnkarStatsStore>>,
}

impl StatsLoader {
    pub fn new(ankar: Arc<Mutex<AnkarStatsStore>>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx: Some(tx),
            rx,
            sched: Sched::default(),
            failed: None,
            cache: None,
            ankar,
        }
    }

    pub fn request(&mut self, range: StatsRange, force: bool) {
        if !self.sched.inflight && !force {
            if let Some(c) = &self.cache {
                if c.range == range && c.loaded_at.elapsed() < Duration::from_secs(60) {
                    let mtime = opencode::db_path()
                        .map(|p| opencode::mtime_pair(&p))
                        .unwrap_or((None, None));
                    if mtime == c.db_mtime {
                        return;
                    }
                }
            }
        }
        if let Some(generation) = self.sched.submit(range) {
            self.spawn(generation, range);
        }
    }

    fn spawn(&mut self, generation: u64, range: StatsRange) {
        let Some(tx) = self.tx.clone() else {
            self.sched.abort();
            return;
        };
        let ankar = self.ankar.clone();
        let spawned = thread::Builder::new()
            .name("ankar-stats".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    compute(range, &ankar)
                }));
                let snap = match result {
                    Ok(s) => s,
                    Err(_) => {
                        warning_snapshot(range, "Stats loader panicked; showing empty snapshot")
                    }
                };
                let _ = tx.send((generation, snap));
            });
        if spawned.is_err() {
            self.sched.abort();
            self.failed = Some(warning_snapshot(
                range,
                "Stats loader could not start its thread",
            ));
        }
    }

    pub fn poll(&mut self) -> Option<StatsSnapshot> {
        if let Some(w) = self.failed.take() {
            return Some(w);
        }
        match self.rx.try_recv() {
            Ok((generation, snap)) => {
                let (keep, next) = self.sched.finish(generation);
                if let Some((g, r)) = next {
                    self.spawn(g, r);
                }
                if !keep {
                    return self.failed.take();
                }
                let mtime = opencode::db_path()
                    .map(|p| opencode::mtime_pair(&p))
                    .unwrap_or((None, None));
                self.cache = Some(CacheEntry {
                    range: snap.range,
                    snap: snap.clone(),
                    loaded_at: Instant::now(),
                    db_mtime: mtime,
                });
                Some(snap)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.sched.abort();
                // Recreate channel.
                let (tx, rx) = mpsc::channel();
                self.tx = Some(tx);
                self.rx = rx;
                None
            }
        }
    }

    pub fn is_loading(&self) -> bool {
        self.sched.inflight
    }

    pub fn clear_cache(&mut self) {
        self.cache = None;
    }

    pub fn cached(&self) -> Option<&StatsSnapshot> {
        self.cache.as_ref().map(|c| &c.snap)
    }
}

fn warning_snapshot(range: StatsRange, msg: &str) -> StatsSnapshot {
    StatsSnapshot {
        range,
        warnings: vec![msg.into()],
        computed_at: SystemTime::now(),
        ..StatsSnapshot::default()
    }
}

fn compute(range: StatsRange, ankar: &Arc<Mutex<AnkarStatsStore>>) -> StatsSnapshot {
    let mut snap = StatsSnapshot {
        range,
        computed_at: SystemTime::now(),
        ..StatsSnapshot::default()
    };

    let now = jiff::ZonedDateTime::now();
    let until_ms = now.timestamp().as_millisecond();
    let (since_ms, prev_since, prev_until) = match range.days() {
        Some(d) => {
            let since = now
                .checked_sub(jiff::Span::new().days(d))
                .map(|z| z.timestamp().as_millisecond())
                .unwrap_or(0);
            let prev_until = since;
            let prev_since = now
                .checked_sub(jiff::Span::new().days(d * 2))
                .map(|z| z.timestamp().as_millisecond())
                .unwrap_or(0);
            (since, prev_since, prev_until)
        }
        None => (0, 0, 0),
    };

    snap.range_label = range_label(range, since_ms, until_ms);

    let tz = jiff::tz::TimeZone::system();
    opencode::load_into(&mut snap, &tz, since_ms, until_ms);
    cursor_agent::load_into(&mut snap, &tz, since_ms, until_ms);

    // Hold the Ankar lock only to copy the hour buckets; the UI waits on it every frame.
    let hours = ankar.lock().ok().map(|store| store.hours_clone());
    if let Some(hours) = &hours {
        let (busy, waiting, starts, agents, days, activity) =
            AnkarStatsStore::snapshot_hours(hours, &tz, since_ms, until_ms);
        snap.ankar_busy_secs = busy;
        snap.ankar_waiting_secs = waiting;
        snap.ankar_starts = starts;
        for a in agents {
            snap.agents.push(a);
        }
        for d in days {
            if let Some(ex) = snap.days.iter_mut().find(|x| x.day == d.day) {
                ex.busy_secs = ex.busy_secs.saturating_add(d.busy_secs);
                ex.tasks = ex.tasks.saturating_add(d.tasks);
            } else {
                snap.days.push(d);
            }
        }
        for wd in 0..7 {
            for h in 0..24 {
                snap.activity[wd][h] = snap.activity[wd][h].saturating_add(activity[wd][h]);
            }
        }
    }

    // Tasks = OpenCode sessions + cursor chats + Ankar starts (sessions already includes both).
    // OpenCode sessions counted in snap.sessions; Ankar starts added for card.
    let tasks_total = snap.sessions.saturating_add(snap.ankar_starts);

    // Primary hours: Ankar busy if any, else OpenCode, else cursor.
    snap.agent_hours = if snap.ankar_busy_secs > 0 {
        snap.ankar_busy_secs as f64 / 3600.0
    } else if snap.opencode_hours > 0.0 {
        snap.opencode_hours
    } else {
        snap.cursor_hours
    };

    // Sort days & agents.
    snap.days.sort_by(|a, b| a.day.cmp(&b.day));
    snap.agents.sort_by(|a, b| {
        let at = a.tokens.as_ref().map(|t| t.total()).unwrap_or(0);
        let bt = b.tokens.as_ref().map(|t| t.total()).unwrap_or(0);
        bt.cmp(&at)
            .then(b.busy_secs.cmp(&a.busy_secs))
            .then(b.tasks.cmp(&a.tasks))
    });

    // Previous window deltas (skip for All).
    if range.days().is_some() && prev_until > prev_since {
        let mut prev = StatsSnapshot::default();
        opencode::load_into(&mut prev, &tz, prev_since, prev_until);
        cursor_agent::load_into(&mut prev, &tz, prev_since, prev_until);
        if let Some(hours) = &hours {
            let (busy, _, starts, _, _, _) =
                AnkarStatsStore::snapshot_hours(hours, &tz, prev_since, prev_until);
            prev.ankar_busy_secs = busy;
            prev.ankar_starts = starts;
        }
        let prev_tasks = prev.sessions.saturating_add(prev.ankar_starts);
        let prev_hours_secs = if prev.ankar_busy_secs > 0 {
            prev.ankar_busy_secs
        } else {
            (prev.opencode_hours * 3600.0) as u64
        };
        let cur_hours_secs = if snap.ankar_busy_secs > 0 {
            snap.ankar_busy_secs
        } else {
            (snap.opencode_hours * 3600.0) as u64
        };
        snap.delta_input_pct = pct_change(snap.tokens.input, prev.tokens.input);
        snap.delta_output_pct = pct_change(snap.tokens.output, prev.tokens.output);
        snap.delta_tasks_pct = pct_change(tasks_total, prev_tasks);
        snap.delta_hours_pct = pct_change(cur_hours_secs, prev_hours_secs);
    }

    snap
}

fn range_label(range: StatsRange, since_ms: i64, _until_ms: i64) -> String {
    if range == StatsRange::All {
        return "all time".into();
    }
    let Ok(ts) = jiff::Timestamp::from_millisecond(since_ms) else {
        return range.label().into();
    };
    let z = ts.to_zoned_datetime(jiff::tz::TimeZone::system());
    let start = z.strftime("%b %d").to_string();
    format!("{start} — today")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_change_while_loading_is_queued_and_stale_dropped() {
        let mut s = Sched::default();
        let g1 = s.submit(StatsRange::Days30).unwrap();
        assert!(s.submit(StatsRange::Days7).is_none());
        assert!(s.submit(StatsRange::Days90).is_none());
        let (keep, next) = s.finish(g1);
        assert!(!keep);
        let (g2, r) = next.unwrap();
        assert_eq!(r, StatsRange::Days90);
        assert!(s.inflight);
        assert_eq!(s.finish(g1), (false, None));
        assert_eq!(s.finish(g2), (true, None));
        assert!(!s.inflight);
    }

    #[test]
    fn abort_clears_inflight_and_pending() {
        let mut s = Sched::default();
        s.submit(StatsRange::All);
        s.submit(StatsRange::Days7);
        s.abort();
        assert!(!s.inflight && s.pending.is_none());
        assert!(s.submit(StatsRange::All).is_some());
    }
}
