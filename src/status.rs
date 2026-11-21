use crate::config::Config;
use crate::layout::{SessionStatus, Workspace};
use crate::notify;
use std::collections::HashMap;
use std::time::Duration;

/// Track previous statuses so we only notify on transitions.
pub struct StatusTracker {
    prev: HashMap<u64, SessionStatus>,
}

impl Default for StatusTracker {
    fn default() -> Self {
        Self {
            prev: HashMap::new(),
        }
    }
}

impl StatusTracker {
    pub fn tick(&mut self, workspace: &mut Workspace, config: &Config, window_focused: bool) {
        let timeout = Duration::from_secs(config.waiting_timeout_secs.max(1));
        for tab in &mut workspace.tabs {
            let mut any_alive = false;
            let mut any_waiting = false;
            let mut all_exited = true;

            for session in tab.sessions.values() {
                if session.is_exited() {
                    continue;
                }
                all_exited = false;
                any_alive = true;
                if session.saw_bell() || session.idle_for() >= timeout {
                    any_waiting = true;
                }
            }

            let new_status = if all_exited && !tab.sessions.is_empty() {
                SessionStatus::Finished
            } else if any_waiting && any_alive {
                SessionStatus::Waiting
            } else {
                SessionStatus::Active
            };

            let prev = self.prev.get(&tab.id).copied().unwrap_or(SessionStatus::Active);
            if prev != new_status
                && matches!(
                    new_status,
                    SessionStatus::Waiting | SessionStatus::Finished
                )
            {
                notify::session_status_changed(&tab.session_name, new_status, window_focused);
            }
            self.prev.insert(tab.id, new_status);
            tab.status = new_status;
        }
    }
}
