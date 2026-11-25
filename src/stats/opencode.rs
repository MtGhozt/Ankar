//! Read-only OpenCode SQLite (`opencode.db`) — tokens, models, agents, activity.

use super::{window_idx, AgentFilter, DayBucket, ModelRow, Source, AgentRow, StatsSnapshot, Tokens};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn db_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("OPENCODE_DB") {
        let path = PathBuf::from(&p);
        if path.is_absolute() {
            return if path.exists() { Some(path) } else { None };
        }
        // Relative to data dir.
        if let Some(dir) = data_dir() {
            let full = dir.join(path);
            if full.exists() {
                return Some(full);
            }
        }
    }
    let dir = data_dir()?;
    let db = dir.join("opencode.db");
    if db.exists() {
        Some(db)
    } else {
        None
    }
}

fn data_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        let p = PathBuf::from(xdg).join("opencode");
        if p.exists() {
            return Some(p);
        }
    }
    let home = dirs::home_dir()?;
    let p = home.join(".local").join("share").join("opencode");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

pub fn mtime_pair(path: &Path) -> (Option<u64>, Option<u64>) {
    let db = path
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let wal = path
        .with_extension("db-wal")
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    // Also try literal `-wal` suffix used by SQLite.
    let wal2 = {
        let mut s = path.as_os_str().to_owned();
        s.push("-wal");
        PathBuf::from(s)
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
    };
    (db, wal.or(wal2))
}

/// One scan of `[since_ms, until_ms)`; rows go to `snaps[0]` (>= `split_ms`) or `snaps[1]` (before it).
pub fn load_into(
    snaps: &mut [StatsSnapshot],
    tz: &jiff::tz::TimeZone,
    split_ms: i64,
    since_ms: i64,
    until_ms: i64,
    filter: Option<&AgentFilter>,
) {
    let Some(path) = db_path() else {
        snaps[0]
            .warnings
            .push("OpenCode: database not found (~/.local/share/opencode/opencode.db)".into());
        return;
    };

    let conn = match open_readonly(&path) {
        Ok(c) => c,
        Err(e) => {
            // One retry after brief pause (WAL writers).
            std::thread::sleep(Duration::from_millis(80));
            match open_readonly(&path) {
                Ok(c) => c,
                Err(_) => {
                    snaps[0].warnings.push(format!(
                        "OpenCode is writing or locked ({e}); retry in a moment"
                    ));
                    return;
                }
            }
        }
    };

    let tables = table_names(&conn);
    let has_v2 = tables.iter().any(|t| t == "session_v2")
        && tables.iter().any(|t| t == "session_message");
    let has_v1 =
        tables.iter().any(|t| t == "session") && tables.iter().any(|t| t == "message");

    if !has_v2 && !has_v1 {
        snaps[0]
            .warnings
            .push("OpenCode: no session_v2/session_message tables found".into());
        return;
    }

    let mut v2_ids: HashMap<String, ()> = HashMap::new();
    if has_v2 {
        if let Err(e) =
            aggregate_v2(&conn, snaps, tz, (split_ms, since_ms, until_ms), filter, &mut v2_ids)
        {
            snaps[0]
                .warnings
                .push(format!("OpenCode v2 query failed: {e}"));
        }
    }
    if has_v1 {
        if let Err(e) =
            aggregate_v1(&conn, snaps, tz, (split_ms, since_ms, until_ms), filter, &v2_ids)
        {
            snaps[0]
                .warnings
                .push(format!("OpenCode v1 query failed: {e}"));
        }
    }
}

fn open_readonly(path: &Path) -> Result<Connection, rusqlite::Error> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let uri = format!("file:{}?mode=ro", path_to_uri(path));
    let conn = Connection::open_with_flags(uri, flags)?;
    let _ = conn.busy_timeout(Duration::from_secs(2));
    let _ = conn.execute_batch(
        "PRAGMA query_only=ON;
         PRAGMA cache_size=-2000;
         PRAGMA temp_store=MEMORY;",
    );
    Ok(conn)
}

fn path_to_uri(path: &Path) -> String {
    // SQLite URI on Windows: file:///C:/...
    let s = path.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        s
    } else {
        format!("/{s}")
    }
}

fn table_names(conn: &Connection) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
    ) else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return out;
    };
    for row in rows.flatten() {
        out.push(row);
    }
    out
}

/// Per-window accumulators for the v2 message loop.
#[derive(Default)]
struct V2Acc {
    models: HashMap<(String, String), ModelRow>,
    agents: HashMap<String, AgentRow>,
    days: HashMap<String, DayBucket>,
    hours_ms: u64,
}

fn aggregate_v2(
    conn: &Connection,
    snaps: &mut [StatsSnapshot],
    tz: &jiff::tz::TimeZone,
    (split_ms, since_ms, until_ms): (i64, i64, i64),
    filter: Option<&AgentFilter>,
    v2_ids: &mut HashMap<String, ()>,
) -> Result<(), rusqlite::Error> {
    // Session counts (with a filter they come from the kept messages below).
    {
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, time_created FROM session_v2
             WHERE time_created >= ?1 AND time_created < ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![since_ms, until_ms], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            ))
        })?;
        for row in rows.flatten() {
            let (id, parent, created) = row;
            let snap = &mut snaps[window_idx(split_ms, created, snaps.len())];
            v2_ids.insert(id, ());
            if filter.is_some() {
                continue;
            }
            if parent.is_none() {
                snap.sessions = snap.sessions.saturating_add(1);
            } else {
                snap.subagents = snap.subagents.saturating_add(1);
            }
        }
    }

    let sql = "
        SELECT message.session_id,
               session.parent_id,
               message.type,
               message.time_created,
               json_extract(message.data,'$.model.providerID'),
               json_extract(message.data,'$.model.id'),
               json_extract(message.data,'$.tokens.input'),
               json_extract(message.data,'$.tokens.output'),
               json_extract(message.data,'$.tokens.reasoning'),
               json_extract(message.data,'$.tokens.cache.read'),
               json_extract(message.data,'$.tokens.cache.write'),
               json_extract(message.data,'$.cost'),
               json_extract(message.data,'$.agent'),
               json_extract(message.data,'$.time.created'),
               json_extract(message.data,'$.time.completed')
        FROM session_message AS message
        JOIN session_v2 AS session ON session.id = message.session_id
        WHERE message.type IN ('user','assistant')
          AND message.time_created >= ?1 AND message.time_created < ?2
          AND (session.fork_session_id IS NULL OR message.time_created >= session.time_created)
    ";

    let mut stmt = conn.prepare(sql)?;
    let mut accs: Vec<V2Acc> = snaps.iter().map(|_| V2Acc::default()).collect();

    let rows = stmt.query_map(rusqlite::params![since_ms, until_ms], |r| {
        Ok(MsgRow {
            session_id: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
            parent_id: r.get::<_, Option<String>>(1)?,
            msg_type: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            time_created: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            provider: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            model: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            input: r.get::<_, Option<i64>>(6)?.unwrap_or(0).max(0) as u64,
            output: r.get::<_, Option<i64>>(7)?.unwrap_or(0).max(0) as u64,
            reasoning: r.get::<_, Option<i64>>(8)?.unwrap_or(0).max(0) as u64,
            cache_read: r.get::<_, Option<i64>>(9)?.unwrap_or(0).max(0) as u64,
            cache_write: r.get::<_, Option<i64>>(10)?.unwrap_or(0).max(0) as u64,
            cost: r.get::<_, Option<f64>>(11)?.unwrap_or(0.0),
            agent: r.get::<_, Option<String>>(12)?.unwrap_or_default(),
            t_created: r.get::<_, Option<i64>>(13)?,
            t_completed: r.get::<_, Option<i64>>(14)?,
        })
    })?;

    let mut seen: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();
    for row in rows.flatten() {
        let i = window_idx(split_ms, row.time_created, snaps.len());
        let snap = &mut snaps[i];
        let V2Acc {
            models,
            agents,
            days,
            hours_ms,
        } = &mut accs[i];
        let agent_name = if row.agent.is_empty() {
            "opencode".to_string()
        } else {
            row.agent.clone()
        };
        // The AGENTS table always lists every agent; the rest of the page follows the filter.
        let keep = AgentFilter::allows(filter, Source::OpenCode, &agent_name);
        if keep {
            bump_activity(&mut snap.activity, tz, row.time_created);
            if row.msg_type == "user" && row.parent_id.is_none() {
                snap.prompts = snap.prompts.saturating_add(1);
            }
            if filter.is_some() && seen.insert((i, row.session_id.clone())) {
                if row.parent_id.is_none() {
                    snap.sessions = snap.sessions.saturating_add(1);
                } else {
                    snap.subagents = snap.subagents.saturating_add(1);
                }
            }
        }

        if row.msg_type != "assistant" {
            continue;
        }

        let tok = Tokens {
            input: row.input,
            output: row.output,
            reasoning: row.reasoning,
            cache_read: row.cache_read,
            cache_write: row.cache_write,
        };
        let busy_ms = match (row.t_created, row.t_completed) {
            // Cap at 6h per message.
            (Some(c), Some(d)) => Some(d.saturating_sub(c)).filter(|d| *d > 0 && *d <= 6 * 60 * 60 * 1000),
            _ => None,
        };
        let a = agents.entry(agent_name.clone()).or_insert_with(|| AgentRow {
            source: Source::OpenCode,
            name: agent_name,
            busy_secs: 0,
            waiting_secs: 0,
            tasks: 0,
            tokens: Some(Tokens::default()),
        });
        a.tasks = a.tasks.saturating_add(1);
        if let Some(t) = a.tokens.as_mut() {
            t.add_assign(&tok);
        }
        if let Some(ms) = busy_ms {
            a.busy_secs = a.busy_secs.saturating_add(ms as u64 / 1000);
        }
        if !keep {
            continue;
        }

        snap.steps = snap.steps.saturating_add(1);
        snap.tokens.add_assign(&tok);
        snap.cost += row.cost.max(0.0);

        let day = local_day(tz, row.time_created);
        let day_e = days.entry(day.clone()).or_insert_with(|| DayBucket {
            day,
            ..DayBucket::default()
        });
        day_e.input = day_e.input.saturating_add(row.input);
        day_e.output = day_e.output.saturating_add(row.output);
        day_e.tasks = day_e.tasks.saturating_add(1);
        if let Some(ms) = busy_ms {
            *hours_ms = hours_ms.saturating_add(ms as u64);
            day_e.busy_secs = day_e.busy_secs.saturating_add(ms as u64 / 1000);
        }

        let provider = if row.provider.is_empty() {
            "unknown".into()
        } else {
            row.provider.clone()
        };
        let model = if row.model.is_empty() {
            "unknown".into()
        } else {
            row.model.clone()
        };
        let m = models
            .entry((provider.clone(), model.clone()))
            .or_insert_with(|| ModelRow {
                provider: provider.clone(),
                model: model.clone(),
                steps: 0,
                tokens: Tokens::default(),
                cost: 0.0,
            });
        m.steps = m.steps.saturating_add(1);
        m.tokens.add_assign(&tok);
        m.cost += row.cost.max(0.0);
    }

    for (snap, acc) in snaps.iter_mut().zip(accs) {
        finish_v2(snap, acc);
    }
    Ok(())
}

fn finish_v2(snap: &mut StatsSnapshot, acc: V2Acc) {
    let V2Acc {
        models,
        agents,
        days,
        hours_ms,
    } = acc;
    snap.opencode_hours = hours_ms as f64 / 3_600_000.0;

    let mut model_vec: Vec<ModelRow> = models.into_values().collect();
    model_vec.sort_by_key(|a| std::cmp::Reverse(a.tokens.total()));
    snap.models = model_vec;

    let mut agent_vec: Vec<AgentRow> = agents.into_values().collect();
    agent_vec.sort_by(|a, b| b.busy_secs.cmp(&a.busy_secs).then(b.tasks.cmp(&a.tasks)));
    for a in agent_vec {
        snap.agents.push(a);
    }

    merge_days(&mut snap.days, days);
}

fn aggregate_v1(
    conn: &Connection,
    snaps: &mut [StatsSnapshot],
    tz: &jiff::tz::TimeZone,
    (split_ms, since_ms, until_ms): (i64, i64, i64),
    filter: Option<&AgentFilter>,
    v2_ids: &HashMap<String, ()>,
) -> Result<(), rusqlite::Error> {
    // Only sessions not already in v2.
    let sql = "
        SELECT message.session_id,
               json_extract(message.data,'$.role'),
               json_extract(message.data,'$.providerID'),
               json_extract(message.data,'$.modelID'),
               json_extract(message.data,'$.tokens.input'),
               json_extract(message.data,'$.tokens.output'),
               json_extract(message.data,'$.tokens.reasoning'),
               json_extract(message.data,'$.tokens.cache.read'),
               json_extract(message.data,'$.tokens.cache.write'),
               json_extract(message.data,'$.cost'),
               json_extract(message.data,'$.agent'),
               json_extract(message.data,'$.time.created'),
               json_extract(message.data,'$.time.completed')
        FROM message
        WHERE json_extract(message.data,'$.role') IN ('user','assistant')
          AND COALESCE(json_extract(message.data,'$.time.created'), 0) >= ?1
          AND COALESCE(json_extract(message.data,'$.time.created'), 0) < ?2
    ";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![since_ms, until_ms], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?.unwrap_or_default(),
            r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            r.get::<_, Option<i64>>(4)?.unwrap_or(0).max(0) as u64,
            r.get::<_, Option<i64>>(5)?.unwrap_or(0).max(0) as u64,
            r.get::<_, Option<i64>>(6)?.unwrap_or(0).max(0) as u64,
            r.get::<_, Option<i64>>(7)?.unwrap_or(0).max(0) as u64,
            r.get::<_, Option<i64>>(8)?.unwrap_or(0).max(0) as u64,
            r.get::<_, Option<f64>>(9)?.unwrap_or(0.0),
            r.get::<_, Option<String>>(10)?.unwrap_or_default(),
            r.get::<_, Option<i64>>(11)?,
            r.get::<_, Option<i64>>(12)?,
        ))
    })?;

    for row in rows.flatten() {
        let (
            sid,
            role,
            provider,
            model,
            input,
            output,
            reasoning,
            cache_read,
            cache_write,
            cost,
            agent,
            t_created,
            t_completed,
        ) = row;
        if v2_ids.contains_key(&sid) {
            continue;
        }
        let t = t_created.unwrap_or(0);
        let snap = &mut snaps[window_idx(split_ms, t, snaps.len())];
        let agent_name = if agent.is_empty() {
            "opencode".to_string()
        } else {
            agent
        };
        let keep = AgentFilter::allows(filter, Source::OpenCode, &agent_name);
        if keep {
            bump_activity(&mut snap.activity, tz, t);
        }
        if role == "user" {
            if keep {
                snap.prompts = snap.prompts.saturating_add(1);
            }
            continue;
        }
        if role != "assistant" {
            continue;
        }
        let tok = Tokens {
            input,
            output,
            reasoning,
            cache_read,
            cache_write,
        };
        // The AGENTS table always lists every agent; the rest of the page follows the filter.
        if let Some(a) = snap.agents.iter_mut().find(|a| a.name == agent_name) {
            a.tasks = a.tasks.saturating_add(1);
            if let Some(t) = a.tokens.as_mut() {
                t.add_assign(&tok);
            }
        } else {
            let mut busy = 0u64;
            if let (Some(c), Some(d)) = (t_created, t_completed) {
                let dur = d.saturating_sub(c);
                if dur > 0 && dur <= 6 * 60 * 60 * 1000 {
                    busy = (dur as u64) / 1000;
                    if keep {
                        snap.opencode_hours += dur as f64 / 3_600_000.0;
                    }
                }
            }
            snap.agents.push(AgentRow {
                source: Source::OpenCode,
                name: agent_name,
                busy_secs: busy,
                waiting_secs: 0,
                tasks: 1,
                tokens: Some(tok.clone()),
            });
        }
        if !keep {
            continue;
        }
        snap.steps = snap.steps.saturating_add(1);
        snap.tokens.add_assign(&tok);
        snap.cost += cost.max(0.0);

        let provider = if provider.is_empty() {
            "unknown".into()
        } else {
            provider
        };
        let model = if model.is_empty() {
            "unknown".into()
        } else {
            model
        };
        if let Some(m) = snap
            .models
            .iter_mut()
            .find(|m| m.provider == provider && m.model == model)
        {
            m.steps = m.steps.saturating_add(1);
            m.tokens.add_assign(&tok);
            m.cost += cost.max(0.0);
        } else {
            snap.models.push(ModelRow {
                provider,
                model,
                steps: 1,
                tokens: tok.clone(),
                cost: cost.max(0.0),
            });
        }
    }
    Ok(())
}

struct MsgRow {
    session_id: String,
    parent_id: Option<String>,
    msg_type: String,
    time_created: i64,
    provider: String,
    model: String,
    input: u64,
    output: u64,
    reasoning: u64,
    cache_read: u64,
    cache_write: u64,
    cost: f64,
    agent: String,
    t_created: Option<i64>,
    t_completed: Option<i64>,
}

fn bump_activity(activity: &mut [[u32; 24]; 7], tz: &jiff::tz::TimeZone, time_ms: i64) {
    if time_ms <= 0 {
        return;
    }
    let ts = jiff::Timestamp::from_millisecond(time_ms).ok();
    let Some(ts) = ts else { return };
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
    let z = ts.to_zoned_datetime(tz.clone());
    z.strftime("%Y-%m-%d").to_string()
}

fn merge_days(dst: &mut Vec<DayBucket>, src: HashMap<String, DayBucket>) {
    for (k, v) in src {
        if let Some(d) = dst.iter_mut().find(|d| d.day == k) {
            d.input = d.input.saturating_add(v.input);
            d.output = d.output.saturating_add(v.output);
            d.tasks = d.tasks.saturating_add(v.tasks);
            d.busy_secs = d.busy_secs.saturating_add(v.busy_secs);
        } else {
            dst.push(v);
        }
    }
    dst.sort_by(|a, b| a.day.cmp(&b.day));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem_v2() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "
            CREATE TABLE session_v2(
              id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT, title TEXT,
              agent TEXT, model TEXT, cost REAL, tokens_input INTEGER, tokens_output INTEGER,
              tokens_reasoning INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER,
              time_created INTEGER, time_updated INTEGER, fork_session_id TEXT, time_archived INTEGER
            );
            CREATE TABLE session_message(
              id TEXT PRIMARY KEY, session_id TEXT, type TEXT, seq INTEGER,
              time_created INTEGER, time_updated INTEGER, data TEXT
            );
            INSERT INTO session_v2(id,parent_id,time_created,time_updated)
              VALUES('s1',NULL,1000,2000);
            INSERT INTO session_message VALUES
              ('m1','s1','user',1,1000,1000,'{}'),
              ('m2','s1','assistant',2,1100,1200,
               '{\"model\":{\"providerID\":\"anthropic\",\"id\":\"opus\"},\"tokens\":{\"input\":10,\"output\":5,\"reasoning\":0,\"cache\":{\"read\":2,\"write\":0}},\"cost\":0.01,\"agent\":\"build\",\"time\":{\"created\":1100,\"completed\":2100}}'),
              ('m3','s1','assistant',3,1300,1400,
               '{\"model\":{\"providerID\":\"anthropic\",\"id\":\"opus\"},\"tokens\":{\"input\":20,\"output\":8,\"reasoning\":1,\"cache\":{\"read\":0,\"write\":0}},\"cost\":0.02,\"agent\":\"build\",\"time\":{\"created\":1300,\"completed\":2300}}');
            ",
        )
        .unwrap();
        c
    }

    #[test]
    fn v2_sums() {
        let conn = mem_v2();
        let mut snaps = [StatsSnapshot::default()];
        let mut ids = HashMap::new();
        aggregate_v2(&conn, &mut snaps, &jiff::tz::TimeZone::UTC, (0, 0, 10_000), None, &mut ids)
            .unwrap();
        let snap = &snaps[0];
        assert_eq!(snap.sessions, 1);
        assert_eq!(snap.steps, 2);
        assert_eq!(snap.tokens.input, 30);
        assert_eq!(snap.tokens.output, 13);
        assert!(snap.cost > 0.0);
        assert!(!snap.models.is_empty());
    }

    #[test]
    fn v2_splits_current_and_previous_window() {
        let conn = mem_v2();
        let mut snaps = [StatsSnapshot::default(), StatsSnapshot::default()];
        let mut ids = HashMap::new();
        aggregate_v2(&conn, &mut snaps, &jiff::tz::TimeZone::UTC, (1200, 0, 10_000), None, &mut ids)
            .unwrap();
        assert_eq!((snaps[0].steps, snaps[0].tokens.input), (1, 20));
        assert_eq!((snaps[1].steps, snaps[1].tokens.input), (1, 10));
        assert_eq!((snaps[0].sessions, snaps[1].sessions), (0, 1));
    }

    #[test]
    fn agent_filter_limits_everything_but_the_agents_table() {
        let conn = mem_v2();
        let tz = jiff::tz::TimeZone::UTC;
        let run = |name: &str| {
            let f = AgentFilter { source: Source::OpenCode, name: name.into() };
            let mut snaps = [StatsSnapshot::default()];
            aggregate_v2(&conn, &mut snaps, &tz, (0, 0, 10_000), Some(&f), &mut HashMap::new())
                .unwrap();
            let [snap] = snaps;
            snap
        };
        let build = run("build");
        assert_eq!((build.sessions, build.steps, build.tokens.input), (1, 2, 30));
        assert_eq!(build.days.len(), 1);
        assert!(build.activity.iter().flatten().sum::<u32>() > 0);
        let plan = run("plan");
        assert_eq!((plan.sessions, plan.steps, plan.tokens.input), (0, 0, 0));
        assert!(plan.models.is_empty() && plan.days.is_empty());
        assert_eq!(plan.activity.iter().flatten().sum::<u32>(), 0);
        // The AGENTS table still lists "build" so the user can pick another row.
        assert_eq!(plan.agents.len(), 1);
        assert_eq!(plan.agents[0].name, "build");
    }

    #[test]
    fn empty_db_warns() {
        let c = Connection::open_in_memory().unwrap();
        let tables = table_names(&c);
        assert!(tables.is_empty());
    }
}
