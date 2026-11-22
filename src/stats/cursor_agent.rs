//! cursor-agent local chats: meta.json + prompt_history.json only (no store.db, no auth).

use super::{AgentRow, DayBucket, Source, StatsSnapshot};
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

pub fn load_into(snap: &mut StatsSnapshot, since_ms: i64, until_ms: i64) {
    let Some(root) = chats_root() else {
        snap.warnings
            .push("cursor-agent: no local chats folder (~/.cursor/chats)".into());
        return;
    };

    let mut files = 0usize;
    let mut chats = 0u64;
    let mut prompts = 0u64;
    let mut busy_secs = 0u64;
    let mut days: std::collections::HashMap<String, DayBucket> = std::collections::HashMap::new();

    // Two levels: chats/<hash>/<uuid>/meta.json
    let Ok(level1) = fs::read_dir(&root) else {
        snap.warnings
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
            chats = chats.saturating_add(1);
            bump_activity(&mut snap.activity, created);

            let updated = updated.unwrap_or(created).max(created);
            let dur_ms = (updated - created).clamp(0, 6 * 60 * 60 * 1000) as u64;
            let secs = dur_ms / 1000;
            busy_secs = busy_secs.saturating_add(secs);

            let day = local_day(created);
            let d = days.entry(day.clone()).or_insert_with(|| DayBucket {
                day,
                ..DayBucket::default()
            });
            d.tasks = d.tasks.saturating_add(1);
            d.busy_secs = d.busy_secs.saturating_add(secs);

            let hist = chat_dir.join("prompt_history.json");
            if hist.is_file() {
                if let Ok(md) = hist.metadata() {
                    if md.len() <= MAX_BYTES {
                        if let Ok(ht) = fs::read_to_string(&hist) {
                            if let Ok(arr) = serde_json::from_str::<serde_json::Value>(&ht) {
                                if let Some(a) = arr.as_array() {
                                    prompts = prompts.saturating_add(a.len() as u64);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    snap.sessions = snap.sessions.saturating_add(chats);
    snap.prompts = snap.prompts.saturating_add(prompts);
    snap.cursor_hours = busy_secs as f64 / 3600.0;

    snap.agents.push(AgentRow {
        source: Source::CursorAgent,
        name: "cursor-agent".into(),
        busy_secs,
        waiting_secs: 0,
        tasks: chats,
        tokens: None, // cursor-agent does not store tokens locally
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

fn bump_activity(activity: &mut [[u32; 24]; 7], time_ms: i64) {
    let Ok(ts) = jiff::Timestamp::from_millisecond(time_ms) else {
        return;
    };
    let z = ts.to_zoned_datetime(jiff::tz::TimeZone::system());
    let wd = z.weekday().to_monday_zero_offset() as usize;
    let hour = z.hour() as usize;
    if let Some(row) = activity.get_mut(wd) {
        if let Some(h) = row.get_mut(hour) {
            *h = h.saturating_add(1);
        }
    }
}

fn local_day(time_ms: i64) -> String {
    let Ok(ts) = jiff::Timestamp::from_millisecond(time_ms) else {
        return "1970-01-01".into();
    };
    ts.to_zoned_datetime(jiff::tz::TimeZone::system())
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
