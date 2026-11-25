use crate::config::Config;
use crate::inbox::{Inbox, Kind};
use crate::layout::{SessionStatus, Workspace};
use crate::notify;
use crate::terminal::AgentState;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MIN_WORK_FOR_TOAST: Duration = Duration::from_secs(30);
/// No OSC 7501 for this long while "working" means the agent is gone.
pub(crate) const AGENT_STALE: Duration = Duration::from_secs(5 * 60);

/// A session without OSC 7501 reports looks busy only while its shell runs a child that is still
/// printing; a shell at its prompt is idle however recently it redrew.
fn guess_busy(has_child: bool, bell: bool, idle: Duration, timeout: Duration) -> bool {
    has_child && !bell && idle < timeout
}

/// What a tab's sessions are doing this frame.
#[derive(Clone, Copy, Default)]
struct Signals {
    exited: bool,
    /// OSC 7501: `Some(true)` while the agent says it works. It beats the guesswork.
    reported: Option<bool>,
    /// Child running and printing (`guess_busy`).
    looks_busy: bool,
    /// The user pressed Enter since the status last changed.
    submitted: bool,
}

/// Idle until the first piece of work, Working while busy, Waiting once that work stops.
/// Without OSC 7501 only a submitted command starts work: startup output and idle redraws of a
/// TUI never turn Idle or Waiting into Working.
fn next_status(prev: SessionStatus, s: Signals) -> SessionStatus {
    if s.exited {
        return SessionStatus::Finished;
    }
    let busy = match s.reported {
        Some(working) => working,
        None => s.looks_busy && (prev == SessionStatus::Active || s.submitted),
    };
    match (busy, prev) {
        (true, _) => SessionStatus::Active,
        (false, SessionStatus::Active | SessionStatus::Waiting) => SessionStatus::Waiting,
        (false, SessionStatus::Idle | SessionStatus::Finished) => SessionStatus::Idle,
    }
}

/// Track when agent work started, so only long jobs toast.
pub struct StatusTracker {
    /// When each tab's agent started its current run of work (OSC 7501 `working`).
    working_since: HashMap<u64, Instant>,
    /// Ignore toasts while the window is still settling (focus flashes, PATH probes).
    started: Instant,
}

impl Default for StatusTracker {
    fn default() -> Self {
        Self {
            working_since: HashMap::new(),
            started: Instant::now(),
        }
    }
}

impl StatusTracker {
    pub fn tick(&mut self, workspace: &mut Workspace, config: &Config, window_focused: bool, inbox: &mut Inbox) {
        let timeout = Duration::from_secs(config.waiting_timeout_secs.max(1));
        let notify_ok = self.started.elapsed() >= Duration::from_secs(3);
        for tab in &mut workspace.tabs {
            let mut s = Signals { exited: !tab.sessions.is_empty(), ..Signals::default() };
            let mut exit_code = None;
            let mut activity = None;

            for session in tab.sessions.values() {
                if let Some(a) = session.activity() {
                    activity = Some(a);
                }
                if session.is_exited() {
                    if exit_code.is_none() {
                        exit_code = session.exit_code();
                    }
                    continue;
                }
                s.exited = false;
                s.submitted |= session.last_submit().is_some_and(|t| t > tab.status_changed_at);
                let agent = session.agent();
                match agent.state {
                    // ...but a Working that went silent for 5 min is stale (agent crashed or quit).
                    Some(state) => {
                        let working = state == AgentState::Working
                            && agent.updated.is_some_and(|t| t.elapsed() < AGENT_STALE);
                        s.reported = Some(s.reported.unwrap_or(false) || working);
                    }
                    None => {
                        let bell = session.saw_bell();
                        s.looks_busy |= guess_busy(tab.busy_child, bell, session.idle_for(), timeout);
                    }
                }
                if let Some(title) = agent.title {
                    activity = Some(title);
                }
            }

            if let Some(a) = activity {
                tab.activity = Some(a);
            }
            if let Some(code) = exit_code {
                tab.exit_code = Some(code);
            }

            let new_status = next_status(tab.status, s);
            if tab.status != new_status {
                tab.status_changed_at = Instant::now();
            }
            // Toast only for an agent-reported working -> idle after a long job. Quiet prompts,
            // bells and exits never notify.
            if s.reported == Some(true) {
                self.working_since.entry(tab.id).or_insert_with(Instant::now);
            } else if let Some(since) = self.working_since.remove(&tab.id) {
                if notify_ok && new_status == SessionStatus::Waiting && since.elapsed() >= MIN_WORK_FOR_TOAST {
                    inbox.push(Kind::Done, tab.id, &tab.session_name, format!("{} finished working", tab.session_name));
                    if config.notifications {
                        notify::session_status_changed(&tab.session_name, new_status, window_focused);
                    }
                }
            }
            if notify_ok && new_status == SessionStatus::Finished && tab.status != SessionStatus::Finished {
                if let Some(code) = tab.exit_code.filter(|&c| c != 0) {
                    inbox.push(Kind::Error, tab.id, &tab.session_name, format!("{} exited with code {code}", tab.session_name));
                }
            }
            tab.status = new_status;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionStatus::*;

    #[test]
    fn an_idle_shell_waits_and_only_a_running_child_works() {
        let (s, t) = (Duration::from_secs, Duration::from_secs(10));
        assert!(!guess_busy(false, false, s(0), t), "prompt just redrawn: still idle");
        assert!(guess_busy(true, false, s(1), t), "cargo build printing");
        assert!(!guess_busy(true, false, s(10), t), "child quiet past the timeout waits");
        assert!(!guess_busy(true, true, s(0), t), "a bell asks for attention");
    }

    #[test]
    fn a_fresh_agent_is_idle_even_while_it_draws_its_first_screen() {
        let printing = Signals { looks_busy: true, ..Signals::default() };
        assert_eq!(next_status(Idle, printing), Idle, "Junie/Copilot startup output is not work");
        assert_eq!(next_status(Idle, Signals::default()), Idle);
        let reported_idle = Signals { reported: Some(false), ..Signals::default() };
        assert_eq!(next_status(Idle, reported_idle), Idle, "OpenCode says idle at start");
    }

    #[test]
    fn idle_to_working_to_waiting_and_back_after_input() {
        let submitted_busy = Signals { looks_busy: true, submitted: true, ..Signals::default() };
        let quiet = Signals::default();
        assert_eq!(next_status(Idle, submitted_busy), Active, "Idle -> Working");
        assert_eq!(next_status(Active, Signals { looks_busy: true, ..quiet }), Active, "still printing");
        assert_eq!(next_status(Active, quiet), Waiting, "Working -> Waiting when it stops");
        assert_eq!(
            next_status(Waiting, Signals { looks_busy: true, ..quiet }),
            Waiting,
            "a redraw without input stays Waiting"
        );
        assert_eq!(next_status(Waiting, submitted_busy), Active, "Waiting -> Working after input");
    }

    #[test]
    fn agent_reports_drive_the_status_and_exits_finish() {
        let working = Signals { reported: Some(true), ..Signals::default() };
        let idle = Signals { reported: Some(false), ..Signals::default() };
        assert_eq!(next_status(Idle, working), Active);
        assert_eq!(next_status(Active, idle), Waiting);
        assert_eq!(next_status(Waiting, working), Active);
        assert_eq!(next_status(Active, Signals { exited: true, ..working }), Finished);
    }
}
