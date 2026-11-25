//! cursor-agent local chats: meta.json timestamps only (no store.db, no auth).

use super::{window_idx, AgentFilter, AgentRow, DayBucket, Source, StatsSnapshot};
use std::fs;
use std::path::PathBuf;

const MAX_FILES: usize = 5000;
const MAX_BYTES: u64 = 64 * 1024;

pub fn chats_root() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let p = home.join(".cursor").join("chats");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// Per-window totals for one chats walk.
#[derive(Default)]
struct Acc {
    chats: u64,
    busy_secs: u64,
    days: std::collections::HashMap<String, DayBucket>,
}

/// One walk of the chats folder; chats created in `[since_ms, until_ms)` go to `snaps[0]`
/// (>= `split_ms`) or `snaps[1]` (before it).
pub fn load_into(
    snaps: &mut [StatsSnapshot],
    tz: &jiff::tz::TimeZone,
    split_ms: i64,
    since_ms: i64,
    until_ms: i64,
    filter: Option<&AgentFilter>,
) {
    let keep = AgentFilter::allows(filter, Source::CursorAgent, AGENT_NAME);
    let Some(root) = chats_root() else {
        snaps[0]
            .warnings
            .push("cursor-agent: no local chats folder (~/.cursor/chats)".into());
        return;
    };

    let mut files = 0usize;
    let mut accs: Vec<Acc> = snaps.iter().map(|_| Acc::default()).collect();

    // Two levels: chats/<hash>/<uuid>/meta.json
    let Ok(level1) = fs::read_dir(&root) else {
        snaps[0]
            .warnings
            .push("cursor-agent: cannot read chats folder".into());
        return;
    };

    for e1 in level1.flatten() {
        if files >= MAX_FILES {
            break;
        }
        let p1 = e1.path();
        if !p1.is_dir() {
            continue;
        }
        let Ok(level2) = fs::read_dir(&p1) else {
            continue;
        };
        for e2 in level2.flatten() {
            if files >= MAX_FILES {
                break;
            }
            let chat_dir = e2.path();
            if !chat_dir.is_dir() {
                continue;
            }
            let meta_path = chat_dir.join("meta.json");
            if !meta_path.is_file() {
                continue;
            }
            files += 1;
            let Ok(meta) = meta_path.metadata() else {
                continue;
            };
            if meta.len() > MAX_BYTES {
                continue;
            }
            let Ok(text) = fs::read_to_string(&meta_path) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let created = as_i64(&v, "createdAtMs").or_else(|| as_i64(&v, "created_at_ms"));
            let updated = as_i64(&v, "updatedAtMs").or_else(|| as_i64(&v, "updated_at_ms"));
            let Some(created) = created else {
                continue;
            };
            if created < since_ms || created >= until_ms {
                continue;
            }
            let i = window_idx(split_ms, created, snaps.len());
            let acc = &mut accs[i];
            acc.chats = acc.chats.saturating_add(1);

            let updated = updated.unwrap_or(created).max(created);
            let dur_ms = (updated - created).clamp(0, 6 * 60 * 60 * 1000) as u64;
            let secs = dur_ms / 1000;
            acc.busy_secs = acc.busy_secs.saturating_add(secs);
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
            d.busy_secs = d.busy_secs.saturating_add(secs);
        }
    }

    for (snap, acc) in snaps.iter_mut().zip(accs) {
        finish(snap, acc, keep);
    }
}

/// The name of the one agent this source reports.
const AGENT_NAME: &str = "cursor-agent";

/// The AGENTS row is always added; sessions, hours and days only when `keep` (filter).
fn finish(snap: &mut StatsSnapshot, acc: Acc, keep: bool) {
    let Acc {
        chats,
        busy_secs,
        days,
    } = acc;
    if keep {
        snap.sessions = snap.sessions.saturating_add(chats);
        snap.cursor_hours = busy_secs as f64 / 3600.0;
    }

    snap.agents.push(AgentRow {
        source: Source::CursorAgent,
        name: AGENT_NAME.into(),
        busy_secs,
        waiting_secs: 0,
        tasks: chats,
        tokens: None, // cursor-agent does not store tokens locally
        modes: String::new(),
    });

    for (_, d) in days {
        if let Some(ex) = snap.days.iter_mut().find(|x| x.day == d.day) {
            ex.tasks = ex.tasks.saturating_add(d.tasks);
            ex.busy_secs = ex.busy_secs.saturating_add(d.busy_secs);
        } else {
            snap.days.push(d);
        }
    }
    snap.days.sort_by(|a, b| a.day.cmp(&b.day));
}

fn as_i64(v: &serde_json::Value, key: &str) -> Option<i64> {
    let x = v.get(key)?;
    if let Some(n) = x.as_i64() {
        return Some(n);
    }
    if let Some(n) = x.as_u64() {
        return Some(n as i64);
    }
    if let Some(n) = x.as_f64() {
        return Some(n as i64);
    }
    None
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
    ts.to_zoned_datetime(tz.clone())
        .strftime("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tolerates_weird_meta() {
        let dir = std::env::temp_dir().join(format!("ankar_cursor_test_{}", std::process::id()));
        let chat = dir.join("abc").join("uuid1");
        fs::create_dir_all(&chat).unwrap();
        let mut f = fs::File::create(chat.join("meta.json")).unwrap();
        writeln!(f, r#"{{"title":"x","createdAtMs":"nope"}}"#).unwrap();
        // Should not panic when parsing via as_i64.
        let v: serde_json::Value = serde_json::from_str(r#"{"createdAtMs":"nope"}"#).unwrap();
        assert!(as_i64(&v, "createdAtMs").is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
