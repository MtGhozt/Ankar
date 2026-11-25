//! Which known agent CLI runs under a session's shell, from a Toolhelp32 process snapshot.

use crate::layout::Workspace;
use std::collections::HashSet;

/// (executable stem, launcher name shown for the session).
/// shortcut: matches the exe name only, so CLIs run through a node shim (`claude.cmd` ->
/// `node.exe`) are not recognised; read the process command line to cover them.
const KNOWN: &[(&str, &str)] = &[
    ("opencode", "opencode"),
    ("copilot", "copilot"),
    ("cursor-agent", "cursor-agent"),
    ("claude", "claude"),
    ("codex", "codex"),
    ("gemini", "gemini"),
    ("junie", "junie"),
    ("grok", "grok"),
    ("antigravity", "antigravity"),
];

pub struct ProcEntry {
    pub pid: u32,
    pub ppid: u32,
    /// File name, e.g. `opencode.exe`.
    pub exe: String,
}

#[cfg(windows)]
pub fn snapshot() -> Vec<ProcEntry> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut out = Vec::new();
    // SAFETY: plain Win32 calls; `entry` is initialised with its size as the API requires, and
    // the snapshot handle is closed on every path.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == -1isize as HANDLE {
            return out;
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            out.push(ProcEntry {
                pid: entry.th32ProcessID,
                ppid: entry.th32ParentProcessID,
                exe: String::from_utf16_lossy(&entry.szExeFile[..len]),
            });
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(not(windows))]
pub fn snapshot() -> Vec<ProcEntry> {
    Vec::new()
}

/// Launcher name of the first known agent anywhere below `shell_pid`.
pub fn agent_under(shell_pid: u32, procs: &[ProcEntry]) -> Option<&'static str> {
    let mut seen = HashSet::from([shell_pid]);
    let mut queue = vec![shell_pid];
    while let Some(parent) = queue.pop() {
        for p in procs.iter().filter(|p| p.ppid == parent && p.pid != parent) {
            if !seen.insert(p.pid) {
                continue; // reused pids can make the "tree" loop
            }
            let lower = p.exe.to_ascii_lowercase();
            let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
            if let Some((_, name)) = KNOWN.iter().find(|(exe, _)| *exe == stem) {
                return Some(name);
            }
            queue.push(p.pid);
        }
    }
    None
}

/// Plain-terminal tabs take the identity of the agent running inside them, and drop it again
/// when it exits. Only the active tab is probed; launcher-started tabs never change.
pub fn detect_active(workspace: &mut Workspace) {
    let Some(tab) = workspace.active_tab_mut() else {
        return;
    };
    if tab.command.is_some() {
        return;
    }
    let pids: Vec<u32> = tab
        .sessions
        .values()
        .filter(|s| !s.is_exited())
        .filter_map(|s| s.shell_pid())
        .collect();
    let procs = if pids.is_empty() { Vec::new() } else { snapshot() };
    let found = pids.iter().find_map(|&pid| agent_under(pid, &procs));
    if tab.detected.as_deref() != found {
        tab.detected = found.map(str::to_owned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, ppid: u32, exe: &str) -> ProcEntry {
        ProcEntry { pid, ppid, exe: exe.into() }
    }

    #[test]
    fn finds_a_known_agent_below_the_shell_and_only_there() {
        let procs = [
            p(1, 0, "explorer.exe"),
            p(10, 1, "pwsh.exe"),
            p(11, 10, "node.exe"),
            p(12, 11, "OpenCode.EXE"),
            p(20, 1, "pwsh.exe"),
            p(21, 20, "claude.exe"),
        ];
        assert_eq!(agent_under(10, &procs), Some("opencode"));
        assert_eq!(agent_under(20, &procs), Some("claude"));
        assert_eq!(agent_under(11, &procs), Some("opencode"));
        assert_eq!(agent_under(12, &procs), None, "the agent itself has no agent below it");
        assert_eq!(agent_under(1, &[p(2, 1, "git.exe")]), None);
    }

    #[test]
    fn a_pid_loop_terminates() {
        let procs = [p(5, 6, "a.exe"), p(6, 5, "b.exe")];
        assert_eq!(agent_under(5, &procs), None);
    }

    #[test]
    fn the_live_snapshot_sees_this_process() {
        if cfg!(windows) {
            let me = std::process::id();
            assert!(snapshot().iter().any(|e| e.pid == me));
        }
    }
}
