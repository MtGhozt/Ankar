use crate::layout::SessionStatus;
use notify_rust::Notification;

pub fn session_status_changed(session_name: &str, status: SessionStatus, window_focused: bool) {
    if window_focused {
        return;
    }
    let (summary, body) = match status {
        SessionStatus::Waiting => (
            "nacar — waiting",
            format!("{session_name} is waiting for input"),
        ),
        SessionStatus::Finished => (
            "nacar — finished",
            format!("{session_name} has exited"),
        ),
        SessionStatus::Active => return,
    };
    let _ = Notification::new()
        .summary(summary)
        .body(&body)
        .appname("nacar")
        .timeout(5000)
        .show();
}
