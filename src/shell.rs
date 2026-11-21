use std::path::PathBuf;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellKind {
    PowerShell,
    Cmd,
    Wsl { distro: Option<String> },
}

#[derive(Clone, Debug)]
pub struct ShellProfile {
    pub id: String,
    pub name: String,
    #[allow(dead_code)]
    pub kind: ShellKind,
    pub program: String,
    pub args: Vec<String>,
}

impl ShellProfile {
    pub fn powershell() -> Self {
        let candidates = [
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
        ];
        let program = candidates
            .iter()
            .find(|p| PathBuf::from(p).exists())
            .unwrap_or(&"powershell.exe")
            .to_string();
        Self {
            id: "powershell".into(),
            name: "PowerShell".into(),
            kind: ShellKind::PowerShell,
            program,
            args: vec!["-NoLogo".into()],
        }
    }

    pub fn cmd() -> Self {
        Self {
            id: "cmd".into(),
            name: "Command Prompt".into(),
            kind: ShellKind::Cmd,
            program: r"C:\Windows\System32\cmd.exe".into(),
            args: vec![],
        }
    }

    pub fn wsl(distro: Option<String>) -> Self {
        let (id, name, args) = match &distro {
            Some(d) => (
                format!("wsl:{d}"),
                format!("WSL ({d})"),
                vec!["-d".into(), d.clone()],
            ),
            None => ("wsl".into(), "WSL".into(), vec![]),
        };
        Self {
            id,
            name,
            kind: ShellKind::Wsl { distro },
            program: "wsl.exe".into(),
            args,
        }
    }
}

pub fn list_profiles() -> Vec<ShellProfile> {
    let mut profiles = vec![ShellProfile::powershell(), ShellProfile::cmd()];
    if which_exists("wsl.exe") {
        let distros = list_wsl_distros();
        if distros.is_empty() {
            profiles.push(ShellProfile::wsl(None));
        } else {
            for d in distros {
                profiles.push(ShellProfile::wsl(Some(d)));
            }
        }
    }
    profiles
}

pub fn default_profile() -> ShellProfile {
    ShellProfile::powershell()
}

fn which_exists(name: &str) -> bool {
    if name.eq_ignore_ascii_case("wsl.exe") {
        return PathBuf::from(r"C:\Windows\System32\wsl.exe").exists();
    }
    #[cfg(windows)]
    {
        Command::new("where.exe")
            .arg(name)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("which")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// `wsl -l -q` returns UTF-16LE on Windows.
pub fn list_wsl_distros() -> Vec<String> {
    let mut cmd = Command::new("wsl.exe");
    cmd.args(["-l", "-q"]);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = match cmd.output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };

    let text = decode_wsl_list(&output);
    text.lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn decode_wsl_list(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes.len().is_multiple_of(2) {
        // Likely UTF-16LE (with or without BOM).
        let u16s: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = String::from_utf16_lossy(&u16s);
        if s.chars().any(|c| c == '\n' || c.is_alphanumeric()) {
            return s;
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}
