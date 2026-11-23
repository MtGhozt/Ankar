use crate::layout::SessionStatus;
use notify_rust::Notification;

pub fn session_status_changed(session_name: &str, status: SessionStatus, window_focused: bool) {
    if window_focused {
        return;
    }
    let (summary, body) = match status {
        SessionStatus::Waiting => (
            "Ankar — waiting",
            format!("{session_name} is waiting for input"),
        ),
        SessionStatus::Finished => (
            "Ankar — finished",
            format!("{session_name} has exited"),
        ),
        SessionStatus::Active => return,
    };
    // TODO(ankar): app_id propio requiere registrar un AUMID (HKCU\Software\Classes\AppUserModelId\MtGhozt.Ankar o un acceso directo con AppUserModelID). Sin eso Windows muestra "Windows PowerShell".
    // WinRT toasts can block for a while; never show them from the UI thread.
    std::thread::spawn(move || {
        let _ = Notification::new()
            .summary(summary)
            .body(&body)
            .appname("Ankar")
            .timeout(5000)
            .show();
    });
}
