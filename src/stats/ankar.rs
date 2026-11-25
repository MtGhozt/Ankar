//! Ankar-owned busy/waiting seconds, aggregated per local hour in `%APPDATA%\ankar\stats.json`.

use super::{AgentFilter, AgentRow, AnkarHourCell, DayBucket, Source};
use crate::config::Config;
use crate::layout::{SessionStatus, Workspace};
use crate::terminal::AgentState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const VERSION: u32 = 1;
const RETENTION_DAYS: i64 = 180;
const FLUSH_EVERY: Duration = Duration::from_secs(60);

pub type AnkarHours = HashMap<String, HashMap<String, AnkarHourCell>>;

pub type AnkarRangeSnap = (
    u64,
    u64,
    u64,
    Vec<AgentRow>,
    Vec<DayBucket>,
    [[u32; 24]; 7],
);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StatsFile {
    version: u32,
    #[serde(default)]
    hours: AnkarHours,
}

impl Default for StatsFile {
    fn default() -> Self {
        Self {
            version: VERSION,
            hours: HashMap::new(),
        }
    }
}

pub struct AnkarStatsStore {
    data: StatsFile,
    dirty: bool,
    last_flush: Instant,
    last_tick: Instant,
    /// Sub-second time carried between ticks.
    pending_secs: f64,
    /// Launcher tab ids already counted as a start.
    seen: HashSet<u64>,
    path: PathBuf,
}

impl Default for AnkarStatsStore {
    fn default() -> Self {
        let path = Config::config_dir()
            .map(|d| d.join("stats.json"))
            .unwrap_or_else(|_| PathBuf::from("stats.json"));
        let data = load_or_reset(&path);
        Self {
            data,
            dirty: false,
            last_flush: Instant::now(),
            last_tick: Instant::now(),
            pending_secs: 0.0,
            seen: HashSet::new(),
            path,
        }
    }
}

impl AnkarStatsStore {
    pub fn tick(&mut self, workspace: &Workspace) {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        let dt_secs = take_whole_secs(&mut self.pending_secs, dt);
        if dt_secs == 0 {
            return;
        }

        let hour_key = local_hour_key();
        for tab in &workspace.tabs {
            let Some(key) = launcher_key(
                &tab.profile_id,
                tab.command.as_deref(),
                &tab.args,
                tab.detected.as_deref(),
            ) else {
                continue;
            };
            let started = self.seen.insert(tab.id);
            let act = activity(reported_working(tab), tab.status);
            if !started && act == Activity::None {
                continue;
            }

            let cell = self
                .data
                .hours
                .entry(hour_key.clone())
                .or_default()
                .entry(key)
                .or_default();
            if started {
                cell.starts = cell.starts.saturating_add(1);
            }
            match act {
                Activity::Busy => cell.busy = cell.busy.saturating_add(dt_secs),
                Activity::Waiting => cell.waiting = cell.waiting.saturating_add(dt_secs),
                Activity::None => {}
            }
            self.dirty = true;
        }

        if self.dirty && self.last_flush.elapsed() >= FLUSH_EVERY {
            self.flush();
        }
    }

    /// Serialize now, write on a thread (the frame must not wait for the disk).
    pub fn flush(&mut self) {
        if let Some(bytes) = self.take_bytes() {
            let path = self.path.clone();
            std::thread::spawn(move || write_logged(&path, &bytes));
        }
    }

    /// Same, but written before returning (app exit).
    pub fn flush_sync(&mut self) {
        if let Some(bytes) = self.take_bytes() {
            write_logged(&self.path, &bytes);
        }
    }

    fn take_bytes(&mut self) -> Option<Vec<u8>> {
        if !self.dirty {
            return None;
        }
        prune_old(&mut self.data);
        match serde_json::to_vec(&self.data) {
            Ok(bytes) => {
                self.dirty = false;
                self.last_flush = Instant::now();
                Some(bytes)
            }
            Err(e) => {
                log::warn!("stats.json serialize failed: {e}");
                None
            }
        }
    }

    /// Copy of the hour buckets, so the lock can be released before aggregating.
    pub fn hours_clone(&self) -> AnkarHours {
        self.data.hours.clone()
    }

    /// Aggregate cloned hour buckets for `[since_ms, until_ms)`; no lock needed.
    pub fn snapshot_hours(
        hours: &AnkarHours,
        tz: &jiff::tz::TimeZone,
        since_ms: i64,
        until_ms: i64,
        filter: Option<&AgentFilter>,
    ) -> AnkarRangeSnap {
        let mut busy = 0u64;
        let mut waiting = 0u64;
        let mut starts = 0u64;
        let mut by_agent: HashMap<String, (u64, u64, u64)> = HashMap::new();
        let mut by_day: HashMap<String, DayBucket> = HashMap::new();
        let mut activity = [[0u32; 24]; 7];

        for (hour_key, agents) in hours {
            let Some((day, hour, weekday, ts_ms)) = parse_hour_key(hour_key, tz) else {
                continue;
            };
            if ts_ms < since_ms || ts_ms >= until_ms {
                continue;
            }
            for (raw, cell) in agents {
                // The AGENTS table always lists every tool; the rest follows the filter.
                let name = super::canonical_tool(raw).name;
                let e = by_agent.entry(name.to_string()).or_default();
                e.0 = e.0.saturating_add(cell.busy);
                e.1 = e.1.saturating_add(cell.waiting);
                e.2 = e.2.saturating_add(cell.starts);
                if !AgentFilter::allows(filter, Source::Ankar, name) {
                    continue;
                }
                busy = busy.saturating_add(cell.busy);
                waiting = waiting.saturating_add(cell.waiting);
                starts = starts.saturating_add(cell.starts);

                let day_e = by_day.entry(day.clone()).or_insert_with(|| DayBucket {
                    day: day.clone(),
                    ..DayBucket::default()
                });
                day_e.busy_secs = day_e.busy_secs.saturating_add(cell.busy);
                day_e.tasks = day_e.tasks.saturating_add(cell.starts);

                if cell.busy > 0 || cell.waiting > 0 || cell.starts > 0 {
                    if let Some(row) = activity.get_mut(weekday) {
                        if let Some(h) = row.get_mut(hour) {
                            *h = h.saturating_add(1);
                        }
                    }
                }
            }
        }

        let mut agents: Vec<AgentRow> = by_agent
            .into_iter()
            .filter(|(name, (b, w, t))| visible_agent(name, *b, *w, *t))
            .map(|(name, (b, w, t))| AgentRow {
                source: Source::Ankar,
                name,
                busy_secs: b,
                waiting_secs: w,
                tasks: t,
                tokens: None,
                modes: String::new(),
            })
            .collect();
        agents.sort_by_key(|a| std::cmp::Reverse(a.busy_secs));

        let mut days: Vec<DayBucket> = by_day.into_values().collect();
        days.sort_by_key(|a| a.day.clone());

        (busy, waiting, starts, agents, days, activity)
    }
}

/// Add `dt` (ignored unless `0 < dt <= 10`, e.g. sleep) to `pending`; return the whole seconds
/// now due and keep the fraction.
fn take_whole_secs(pending: &mut f64, dt: f64) -> u64 {
    if dt > 0.0 && dt <= 10.0 {
        *pending += dt;
    }
    if *pending < 1.0 {
        return 0;
    }
    let whole = pending.floor();
    *pending -= whole;
    whole as u64
}

/// Command tabs count under the tool inside `command`/`args` (or `Shell`). A plain shell counts
/// only when `detected` is a known tool. Tab titles are not keys.
fn launcher_key(
    profile_id: &str,
    command: Option<&str>,
    args: &[String],
    detected: Option<&str>,
) -> Option<String> {
    if profile_id == "command" {
        let mut hay = String::new();
        if let Some(c) = command {
            hay.push_str(c);
        }
        for a in args {
            if !hay.is_empty() {
                hay.push(' ');
            }
            hay.push_str(a);
        }
        return Some(super::canonical_tool(&hay).name.to_string());
    }
    let tool = super::canonical_tool(detected?);
    (tool.id != "shell").then(|| tool.name.to_string())
}

/// Shell rows with no busy/waiting time are hidden. Other tools stay if they have tasks or time.
fn visible_agent(name: &str, busy: u64, waiting: u64, tasks: u64) -> bool {
    let time = busy > 0 || waiting > 0;
    if name == "Shell" {
        time
    } else {
        time || tasks > 0
    }
}

/// Fold old stats.json keys that name the same tool into one hour bucket.
fn merge_hours(hours: &mut AnkarHours) {
    for bucket in hours.values_mut() {
        let raw = std::mem::take(bucket);
        for (key, cell) in raw {
            let name = super::canonical_tool(&key).name.to_string();
            let dst = bucket.entry(name).or_default();
            dst.busy = dst.busy.saturating_add(cell.busy);
            dst.waiting = dst.waiting.saturating_add(cell.waiting);
            dst.starts = dst.starts.saturating_add(cell.starts);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Activity {
    Busy,
    Waiting,
    None,
}

/// `reported`: the tab's OSC 7501 verdict (`Some(true)` = working, `Some(false)` = idle), if any
/// session sends one; otherwise the `tab.status` heuristic decides.
fn activity(reported: Option<bool>, status: SessionStatus) -> Activity {
    match (reported, status) {
        (Some(true), _) => Activity::Busy,
        (Some(false), _) => Activity::Waiting,
        (None, SessionStatus::Active) => Activity::Busy,
        (None, SessionStatus::Waiting) => Activity::Waiting,
        (None, _) => Activity::None,
    }
}

/// Same rule as `status::tick`: a `Working` silent for 5 minutes is stale.
fn reported_working(tab: &crate::layout::Tab) -> Option<bool> {
    let mut reported = None;
    for session in tab.sessions.values().filter(|s| !s.is_exited()) {
        let agent = session.agent();
        if let Some(state) = agent.state {
            let working = state == AgentState::Working
                && agent.updated.is_some_and(|t| t.elapsed() < crate::status::AGENT_STALE);
            reported = Some(reported.unwrap_or(false) || working);
        }
    }
    reported
}

fn local_hour_key() -> String {
    jiff::ZonedDateTime::now()
        .strftime("%Y-%m-%dT%H")
        .to_string()
}

fn parse_hour_key(
    key: &str,
    tz: &jiff::tz::TimeZone,
) -> Option<(String, usize, usize, i64)> {
    // YYYY-MM-DDTHH
    if key.len() < 13 {
        return None;
    }
    let day = key.get(0..10)?.to_string();
    let hour: usize = key.get(11..13)?.parse().ok()?;
    if hour > 23 {
        return None;
    }
    let date = jiff::civil::Date::strptime("%Y-%m-%d", &day).ok()?;
    let dt = date.at(hour as i8, 0, 0, 0);
    let z = dt.to_zoned_datetime(tz.clone()).ok()?;
    let weekday = z.weekday().to_monday_zero_offset() as usize;
    let ts_ms = z.timestamp().as_millisecond();
    Some((day, hour, weekday, ts_ms))
}

fn prune_old(data: &mut StatsFile) {
    let cutoff = jiff::ZonedDateTime::now()
        .saturating_sub(jiff::Span::new().days(RETENTION_DAYS))
        .strftime("%Y-%m-%dT%H")
        .to_string();
    data.hours.retain(|k, _| k.as_str() >= cutoff.as_str());
}

fn load_or_reset(path: &PathBuf) -> StatsFile {
    if !path.exists() {
        return StatsFile::default();
    }
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<StatsFile>(&text) {
            Ok(mut f) if f.version == VERSION => {
                merge_hours(&mut f.hours);
                f
            }
            Ok(_) | Err(_) => {
                let bad = path.with_extension("json.bad");
                let _ = fs::rename(path, &bad);
                log::warn!(
                    "stats.json invalid or wrong version; moved to {}",
                    bad.display()
                );
                StatsFile::default()
            }
        },
        Err(e) => {
            log::warn!("stats.json read failed: {e}");
            StatsFile::default()
        }
    }
}

fn write_logged(path: &Path, bytes: &[u8]) {
    if let Err(e) = atomic_write(path, bytes) {
        log::warn!("stats.json write failed: {e}");
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_ticks_accumulate_and_gaps_are_ignored() {
        let mut p = 0.0;
        let total: u64 = (0..10).map(|_| take_whole_secs(&mut p, 0.1)).sum();
        // 0.1 * 10 may land a hair under 1.0 in f64; the next tick must still pay out.
        assert_eq!(total + take_whole_secs(&mut p, 0.1), 1);
        assert_eq!(take_whole_secs(&mut p, 60.0), 0);
        assert_eq!(take_whole_secs(&mut p, -1.0), 0);
        assert_eq!(take_whole_secs(&mut p, 2.5), 2);
        assert!((p - 0.6).abs() < 0.2);
    }

    #[test]
    fn command_tabs_key_by_tool_and_plain_shells_only_when_detected() {
        let args = Vec::<String>::new();
        assert_eq!(
            launcher_key("command", Some("powershell.exe -NoLogo -Command opencode"), &args, None).as_deref(),
            Some("OpenCode")
        );
        assert_eq!(launcher_key("command", Some("matias"), &args, None).as_deref(), Some("Shell"));
        assert_eq!(launcher_key("powershell", None, &args, None), None);
        assert_eq!(launcher_key("powershell", None, &args, Some("claude")).as_deref(), Some("Claude"));
        assert_eq!(launcher_key("powershell", Some("matias"), &args, Some("notepad")), None);
    }

    #[test]
    fn old_keys_merge_into_one_tool_bucket() {
        let cell = |busy, starts| AnkarHourCell { busy, waiting: 0, starts };
        let mut hours = AnkarHours::new();
        hours.insert(
            "2026-01-05T10".into(),
            HashMap::from([
                ("build".into(), cell(10, 1)),
                ("opencode".into(), AnkarHourCell { busy: 4, waiting: 3, starts: 2 }),
                ("powershell.exe -NoLogo -Command opencode".into(), cell(6, 1)),
                ("matias".into(), cell(8, 1)),
            ]),
        );
        merge_hours(&mut hours);
        let bucket = hours.get("2026-01-05T10").unwrap();
        let oc = bucket.get("OpenCode").unwrap();
        assert_eq!((oc.busy, oc.waiting, oc.starts), (20, 3, 4));
        assert_eq!(bucket.get("Shell").unwrap().busy, 8);
        assert_eq!(bucket.len(), 2);
        assert_eq!(super::super::canonical_tool("build").name, "OpenCode");
        assert_eq!(super::super::canonical_tool("matias").name, "Shell");

        let row = |source, name, busy, tasks, tokens| AgentRow {
            source,
            name,
            busy_secs: busy,
            waiting_secs: 0,
            tasks,
            tokens,
            modes: String::new(),
        };
        let folded = super::super::fold_agent_rows(vec![
            row(Source::Ankar, "OpenCode".into(), 20, 4, None),
            row(Source::OpenCode, "build".into(), 5, 2, Some(super::super::Tokens { input: 10, ..Default::default() })),
            row(Source::Ankar, "Junie".into(), 1, 1, None),
        ]);
        assert_eq!(folded.len(), 2);
        let open = folded.iter().find(|r| r.name == "OpenCode").unwrap();
        assert_eq!((open.busy_secs, open.tasks), (25, 6));
        assert_eq!(open.tokens.as_ref().map(|t| t.input), Some(10));
        assert_eq!(folded.iter().find(|r| r.name == "Junie").unwrap().busy_secs, 1);
    }

    #[test]
    fn osc_state_beats_the_status_heuristic() {
        use SessionStatus::*;
        assert_eq!(activity(Some(true), Waiting), Activity::Busy);
        assert_eq!(activity(Some(false), Active), Activity::Waiting);
        assert_eq!(activity(None, Active), Activity::Busy);
        assert_eq!(activity(None, Waiting), Activity::Waiting);
        assert_eq!(activity(None, Finished), Activity::None);
    }

    #[test]
    fn flush_sync_writes_compact_json_and_clears_dirty() {
        let dir = std::env::temp_dir().join(format!("ankar_stats_flush_{}", std::process::id()));
        let path = dir.join("stats.json");
        let mut s = AnkarStatsStore {
            data: StatsFile::default(),
            dirty: true,
            last_flush: Instant::now(),
            last_tick: Instant::now(),
            pending_secs: 0.0,
            seen: HashSet::new(),
            path: path.clone(),
        };
        s.flush_sync();
        assert!(!s.dirty);
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains('\n'));
        assert_eq!(load_or_reset(&path).version, VERSION);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_keeps_recent() {
        let mut f = StatsFile::default();
        f.hours.insert("2099-01-01T00".into(), HashMap::new());
        f.hours.insert("2000-01-01T00".into(), HashMap::new());
        prune_old(&mut f);
        assert!(f.hours.contains_key("2099-01-01T00"));
        assert!(!f.hours.contains_key("2000-01-01T00"));
    }

    #[test]
    fn agent_filter_limits_totals_days_and_heatmap_but_keeps_every_agent_row() {
        let cell = |busy| AnkarHourCell { busy, waiting: 0, starts: 1 };
        let mut hours = AnkarHours::new();
        hours.insert(
            "2026-01-05T10".into(),
            HashMap::from([("claude".to_string(), cell(60)), ("codex".to_string(), cell(30))]),
        );
        let tz = jiff::tz::TimeZone::UTC;
        let all = AnkarStatsStore::snapshot_hours(&hours, &tz, 0, i64::MAX, None);
        assert_eq!((all.0, all.2, all.3.len()), (90, 2, 2));
        let f = AgentFilter { source: Source::Ankar, name: "codex".into() };
        let (busy, _, starts, agents, days, activity) =
            AnkarStatsStore::snapshot_hours(&hours, &tz, 0, i64::MAX, Some(&f));
        assert_eq!((busy, starts, agents.len()), (30, 1, 2));
        assert_eq!(days[0].busy_secs, 30);
        assert_eq!(activity.iter().flatten().sum::<u32>(), 1);
        // Same tool name counts even when the click stored the other source.
        let other = AgentFilter { source: Source::OpenCode, name: "codex".into() };
        let same = AnkarStatsStore::snapshot_hours(&hours, &tz, 0, i64::MAX, Some(&other));
        assert_eq!((same.0, same.4.len(), same.3.len()), (30, 1, 2));
        let junie = AgentFilter { source: Source::Ankar, name: "Junie".into() };
        let none = AnkarStatsStore::snapshot_hours(&hours, &tz, 0, i64::MAX, Some(&junie));
        assert_eq!((none.0, none.4.len(), none.3.len()), (0, 0, 2));
    }

    #[test]
    fn bad_version_resets() {
        let dir = std::env::temp_dir().join(format!("ankar_stats_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("stats.json");
        fs::write(&path, r#"{"version":99,"hours":{}}"#).unwrap();
        let loaded = load_or_reset(&path);
        assert_eq!(loaded.version, VERSION);
        assert!(path.with_extension("json.bad").exists() || !path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
