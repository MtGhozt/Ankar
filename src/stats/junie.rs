//! Junie session summaries: timestamps and token totals only (no prompts, no message text).

use super::{window_idx, AgentFilter, AgentRow, DayBucket, Source, StatsSnapshot, Tokens};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const MAX_FILES: usize = 500;
const MAX_BYTES: u64 = 64 * 1024;
const AGENT_NAME: &str = "Junie";

pub fn load_into(
    snaps: &mut [StatsSnapshot],
    tz: &jiff::tz::TimeZone,
    split_ms: i64,
    since_ms: i64,
    until_ms: i64,
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
        if created <= 0 || created < since_ms || created >= until_ms {
            continue;
        }
        let i = window_idx(split_ms, created, snaps.len());
        let updated = summary.updated_at.max(created);
        let dur_ms = (updated - created).clamp(0, 6 * 60 * 60 * 1000) as u64;
        let acc = &mut accs[i];
        acc.tasks = acc.tasks.saturating_add(1);
        acc.busy_secs = acc.busy_secs.saturating_add(dur_ms / 1000);
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
        d.busy_secs = d.busy_secs.saturating_add(dur_ms / 1000);
    }

    for (snap, acc) in snaps.iter_mut().zip(accs) {
        if acc.tasks == 0 {
            continue;
        }
        finish(snap, acc);
    }
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
