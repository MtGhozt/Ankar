use crate::config::Config;
use crate::layout::{SessionStatus, Workspace};
use crate::notify;
use crate::terminal::AgentState;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MIN_WORK_FOR_TOAST: Duration = Duration::from_secs(30);
/// No OSC 7501 for this long while "working" means the agent is gone.
const AGENT_STALE: Duration = Duration::from_secs(5 * 60);

/// A session without OSC 7501 reports works only while its shell runs a child that is still
/// printing; a shell at its prompt is idle however recently it redrew.
fn guess_busy(has_child: bool, bell: bool, idle: Duration, timeout: Duration) -> bool {
    has_child && !bell && idle < timeout
}

/// Track previous statuses so we only notify on transitions.
pub struct StatusTracker {
    prev: HashMap<u64, SessionStatus>,
    /// When each tab's agent started its current run of work (OSC 7501 `working`).
    working_since: HashMap<u64, Instant>,
    /// Ignore toasts while the window is still settling (focus flashes, PATH probes).
    started: Instant,
}

impl Default for StatusTracker {
    fn default() -> Self {
        Self {
            prev: HashMap::new(),
            working_since: HashMap::new(),
            started: Instant::now(),
        }
    }
}

impl StatusTracker {
    pub fn tick(&mut self, workspace: &mut Workspace, config: &Config, window_focused: bool) {
        let timeout = Duration::from_secs(config.waiting_timeout_secs.max(1));
        let notify_ok = self.started.elapsed() >= Duration::from_secs(3);
        for tab in &mut workspace.tabs {
            let mut any_alive = false;
            let mut any_waiting = false;
            let mut any_working = false;
            // Guessed from a running child; shown as Working but never toasts.
            let mut child_working = false;
            let mut all_exited = true;
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
                all_exited = false;
                any_alive = true;
                let agent = session.agent();
                match agent.state {
                    // An agent that reports its own state (OSC 7501) beats the guesswork.
                    // ...but a Working that went silent for 5 min is stale (agent crashed or quit).
                    Some(AgentState::Working)
                        if agent.updated.is_some_and(|t| t.elapsed() < AGENT_STALE) =>
                    {
                        any_working = true
                    }
                    Some(_) => any_waiting = true,
                    None => {
                        let bell = session.saw_bell();
                        if guess_busy(tab.busy_child, bell, session.idle_for(), timeout) {
                            child_working = true;
                        } else {
                            any_waiting = true;
                        }
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

            let new_status = if all_exited && !tab.sessions.is_empty() {
                SessionStatus::Finished
            } else if any_working || child_working {
                SessionStatus::Active
            } else if any_waiting && any_alive {
                SessionStatus::Waiting
            } else {
                SessionStatus::Active
            };

            let prev = self
                .prev
                .get(&tab.id)
                .copied()
                .unwrap_or(SessionStatus::Active);
            if prev != new_status {
                tab.status_changed_at = Instant::now();
            }
            // Toast only for an agent-reported working -> idle after a long job. Quiet prompts,
            // bells and exits never notify.
            if any_working {
                self.working_since.entry(tab.id).or_insert_with(Instant::now);
            } else if let Some(since) = self.working_since.remove(&tab.id) {
                if config.notifications
                    && notify_ok
                    && new_status == SessionStatus::Waiting
                    && since.elapsed() >= MIN_WORK_FOR_TOAST
                {
                    notify::session_status_changed(&tab.session_name, new_status, window_focused);
                }
            }
            self.prev.insert(tab.id, new_status);
            tab.status = new_status;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_shell_waits_and_only_a_running_child_works() {
        let (s, t) = (Duration::from_secs, Duration::from_secs(10));
        assert!(!guess_busy(false, false, s(0), t), "prompt just redrawn: still idle");
        assert!(guess_busy(true, false, s(1), t), "cargo build printing");
        assert!(!guess_busy(true, false, s(10), t), "child quiet past the timeout waits");
        assert!(!guess_busy(true, true, s(0), t), "a bell asks for attention");
    }
}
