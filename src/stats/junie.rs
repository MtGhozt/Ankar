//! Junie session summaries: timestamps and token totals only (no prompts, no message text).

use super::ankar::{parse_hour_key, AnkarHours};
use super::{window_idx, AgentFilter, AgentRow, DayBucket, Source, StatsSnapshot, Tokens};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const MAX_FILES: usize = 500;
const MAX_BYTES: u64 = 64 * 1024;
const AGENT_NAME: &str = "Junie";

#[allow(clippy::too_many_arguments)]
pub fn load_into(
    snaps: &mut [StatsSnapshot],
    tz: &jiff::tz::TimeZone,
    split_ms: i64,
    scan_since: i64,
    until_ms: i64,
    prev_since: i64,
    hours: Option<&AnkarHours>,
    filter: Option<&AgentFilter>,
) {
    let Some(root) = sessions_root() else {
        return;
    };
    let keep = AgentFilter::allows(filter, Source::Ankar, AGENT_NAME);
    let mut files = 0usize;
    let mut accs: Vec<Acc> = snaps.iter().map(|_| Acc::default()).collect();

    let Ok(sessions) = fs::read_dir(&root) else {
        snaps[0].warnings.push("Junie: cannot read ~/.junie/sessions".into());
        return;
    };
    for entry in sessions.flatten() {
        if files >= MAX_FILES {
            break;
        }
        let path = entry.path().join("summary.json");
        if !path.is_file() {
            continue;
        }
        let Ok(meta) = path.metadata() else {
            continue;
        };
        if meta.len() > MAX_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        files += 1;
        let Ok(summary) = serde_json::from_str::<Summary>(&text) else {
            continue;
        };
        let created = summary.created_at;
        if created <= 0 || created < scan_since || created >= until_ms {
            continue;
        }
        let i = window_idx(split_ms, created, snaps.len());
        let updated = summary.updated_at.max(created);
        let dur_ms = (updated - created).clamp(0, 6 * 60 * 60 * 1000) as u64;
        let dur_secs = dur_ms / 1000;
        let (win_since, win_until) = window_bounds(i, split_ms, prev_since, until_ms);
        let add_secs =
            summary_busy_contribution(created, updated, dur_secs, hours, tz, win_since, win_until);
        let acc = &mut accs[i];
        acc.tasks = acc.tasks.saturating_add(1);
        acc.busy_secs = acc.busy_secs.saturating_add(add_secs);
        for use_ in &summary.main_agent.model_usage {
            acc.tokens.input = acc.tokens.input.saturating_add(use_.input_tokens);
            acc.tokens.output = acc.tokens.output.saturating_add(use_.output_tokens);
            acc.tokens.cache_read = acc.tokens.cache_read.saturating_add(use_.cache_input_tokens);
            acc.tokens.cache_write = acc.tokens.cache_write.saturating_add(use_.cache_create_tokens);
        }
        if !keep {
            continue;
        }
        bump_activity(&mut snaps[i].activity, tz, created);
        let day = local_day(tz, created);
        let d = acc.days.entry(day.clone()).or_insert_with(|| DayBucket {
            day,
            ..DayBucket::default()
        });
        d.tasks = d.tasks.saturating_add(1);
        d.busy_secs = d.busy_secs.saturating_add(add_secs);
    }

    for (snap, acc) in snaps.iter_mut().zip(accs) {
        if acc.tasks == 0 {
            continue;
        }
        finish(snap, acc);
    }
}

fn window_bounds(window: usize, split_ms: i64, prev_since: i64, until_ms: i64) -> (i64, i64) {
    if window > 0 {
        (prev_since, split_ms)
    } else {
        (split_ms, until_ms)
    }
}

const HOUR_MS: i64 = 3_600_000;

/// Busy seconds from a Junie summary when Ankar did not already record Junie busy in the overlap window.
pub(crate) fn summary_busy_contribution(
    created_at: i64,
    updated_at: i64,
    duration_secs: u64,
    hours: Option<&AnkarHours>,
    tz: &jiff::tz::TimeZone,
    since_ms: i64,
    until_ms: i64,
) -> u64 {
    let Some(hours) = hours else {
        return duration_secs;
    };
    if ankar_recorded_junie_busy(hours, tz, created_at, updated_at, since_ms, until_ms) {
        0
    } else {
        duration_secs
    }
}

fn ankar_recorded_junie_busy(
    hours: &AnkarHours,
    tz: &jiff::tz::TimeZone,
    created_at: i64,
    updated_at: i64,
    since_ms: i64,
    until_ms: i64,
) -> bool {
    for (hour_key, agents) in hours {
        let Some((_, _, _, ts_ms)) = parse_hour_key(hour_key, tz) else {
            continue;
        };
        if ts_ms < since_ms || ts_ms >= until_ms {
            continue;
        }
        let hour_end = ts_ms.saturating_add(HOUR_MS);
        if created_at >= hour_end || updated_at <= ts_ms {
            continue;
        }
        for (raw, cell) in agents {
            if super::canonical_tool(raw).name == AGENT_NAME && cell.busy > 0 {
                return true;
            }
        }
    }
    false
}

fn sessions_root() -> Option<PathBuf> {
    let p = dirs::home_dir()?.join(".junie").join("sessions");
    p.is_dir().then_some(p)
}

#[derive(Default)]
struct Acc {
    tasks: u64,
    busy_secs: u64,
    tokens: Tokens,
    days: HashMap<String, DayBucket>,
}

fn finish(snap: &mut StatsSnapshot, acc: Acc) {
    let Acc { tasks, busy_secs, tokens, days } = acc;
    snap.agents.push(AgentRow {
        source: Source::Ankar,
        name: AGENT_NAME.into(),
        busy_secs,
        waiting_secs: 0,
        tasks,
        tokens: Some(tokens),
        modes: String::new(),
    });
    for d in days.into_values() {
        if let Some(ex) = snap.days.iter_mut().find(|x| x.day == d.day) {
            ex.tasks = ex.tasks.saturating_add(d.tasks);
            ex.busy_secs = ex.busy_secs.saturating_add(d.busy_secs);
        } else {
            snap.days.push(d);
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    #[serde(default)]
    created_at: i64,
    #[serde(default)]
    updated_at: i64,
    #[serde(default)]
    main_agent: MainAgent,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MainAgent {
    #[serde(default)]
    model_usage: Vec<ModelUse>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelUse {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_input_tokens: u64,
    #[serde(default)]
    cache_create_tokens: u64,
}

fn bump_activity(activity: &mut [[u32; 24]; 7], tz: &jiff::tz::TimeZone, time_ms: i64) {
    let Ok(ts) = jiff::Timestamp::from_millisecond(time_ms) else {
        return;
    };
    let z = ts.to_zoned_datetime(tz.clone());
    let wd = z.weekday().to_monday_zero_offset() as usize;
    let hour = z.hour() as usize;
    if let Some(row) = activity.get_mut(wd) {
        if let Some(h) = row.get_mut(hour) {
            *h = h.saturating_add(1);
        }
    }
}

fn local_day(tz: &jiff::tz::TimeZone, time_ms: i64) -> String {
    let Ok(ts) = jiff::Timestamp::from_millisecond(time_ms) else {
        return "1970-01-01".into();
    };
    ts.to_zoned_datetime(tz.clone()).strftime("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::AnkarHourCell;
    use std::collections::HashMap;

    #[test]
    fn summary_busy_skips_when_ankar_timed_junie_in_overlap() {
        let tz = jiff::tz::TimeZone::UTC;
        let hour_start = jiff::civil::Date::strptime("%Y-%m-%d", "2026-06-01")
            .unwrap()
            .at(12, 0, 0, 0)
            .to_zoned_datetime(tz.clone())
            .unwrap()
            .timestamp()
            .as_millisecond();
        let key = "2026-06-01T12".to_string();
        let mut hours = AnkarHours::new();
        hours.insert(
            key,
            HashMap::from([("junie".into(), AnkarHourCell { busy: 30, waiting: 0, starts: 1 })]),
        );
        let created = hour_start + 1_000;
        let updated = created + 600_000;
        assert_eq!(
            summary_busy_contribution(
                created,
                updated,
                600,
                Some(&hours),
                &tz,
                hour_start,
                hour_start + HOUR_MS
            ),
            0
        );
        assert_eq!(
            summary_busy_contribution(
                hour_start + HOUR_MS + 1_000,
                hour_start + HOUR_MS + 601_000,
                600,
                Some(&hours),
                &tz,
                hour_start,
                hour_start + 2 * HOUR_MS
            ),
            600
        );
    }
}
