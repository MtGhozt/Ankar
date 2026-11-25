use crate::config::Config;
use crate::layout::{SessionStatus, Workspace};
use crate::notify;
use crate::terminal::AgentState;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Track previous statuses so we only notify on transitions.
pub struct StatusTracker {
    prev: HashMap<u64, SessionStatus>,
    /// Ignore toasts while the window is still settling (focus flashes, PATH probes).
    started: Instant,
}

impl Default for StatusTracker {
    fn default() -> Self {
        Self {
            prev: HashMap::new(),
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
                    Some(AgentState::Working) => any_working = true,
                    Some(AgentState::Idle | AgentState::Clear) => any_waiting = true,
                    None => {
                        if session.saw_bell() || session.idle_for() >= timeout {
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
            } else if any_working {
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
                if notify_ok
                    && matches!(
                        new_status,
                        SessionStatus::Waiting | SessionStatus::Finished
                    )
                {
                    notify::session_status_changed(&tab.session_name, new_status, window_focused);
                }
            }
            self.prev.insert(tab.id, new_status);
            tab.status = new_status;
        }
    }
}
