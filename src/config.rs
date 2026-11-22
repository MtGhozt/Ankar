use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "ankar";
const LEGACY_APP_NAME: &str = "nacar";

/// Copies config.toml and sessions.json from %APPDATA%\nacar to %APPDATA%\ankar
/// the first time. Never overwrites files that already exist in the new folder
/// and never deletes the old folder.
pub fn migrate_legacy_dir() -> anyhow::Result<()> {
    let Some(base) = dirs::config_dir() else {
        return Ok(());
    };
    let old = base.join(LEGACY_APP_NAME);
    let new = base.join(APP_NAME);
    if !old.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(&new)?;
    for name in ["config.toml", "sessions.json"] {
        let src = old.join(name);
        let dst = new.join(name);
        if src.is_file() && !dst.exists() {
            fs::copy(&src, &dst)?; // binary copy: keeps UTF-8 as-is
            log::info!("migrated {} -> {}", src.display(), dst.display());
        }
    }
    Ok(())
}

fn default_scrollback_lines() -> usize {
    2_000
}

fn default_font_size() -> f32 {
    13.0
}

fn default_accent() -> String {
    "#4D9FFB".into()
}

fn default_sidebar_width() -> f32 {
    260.0
}

fn default_true() -> bool {
    true
}

fn default_icon_bot() -> String {
    "bot".into()
}

fn default_icon_color() -> String {
    "#8A8A8A".into()
}

fn default_logo_key() -> String {
    String::new()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_shell: String,
    pub waiting_timeout_secs: u64,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_accent")]
    pub accent: String,
    pub pinned_projects: Vec<String>,
    pub pinned_sessions: Vec<String>,
    pub launchers: Vec<Launcher>,
    /// Scrollback lines kept per terminal. Hard-capped at 50_000 in the session.
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: usize,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    /// Dim unfocused split panes with a translucent overlay.
    #[serde(default = "default_true")]
    pub dim_inactive_panes: bool,
    /// Collapsed project group keys in the sidebar.
    #[serde(default)]
    pub collapsed_groups: Vec<String>,
    /// Experimental custom title bar (off by default).
    #[serde(default)]
    pub custom_titlebar: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Launcher {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    /// Lucide icon name (fallback when no logo file).
    #[serde(default = "default_icon_bot")]
    pub icon: String,
    /// Hex color for the Lucide fallback.
    #[serde(default = "default_icon_color")]
    pub color: String,
    /// Key under `assets/logos/` (e.g. `opencode`, `openai`). Empty → derive from name.
    #[serde(default = "default_logo_key")]
    pub logo: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_shell: "powershell".into(),
            waiting_timeout_secs: 30,
            font_size: default_font_size(),
            accent: default_accent(),
            pinned_projects: Vec::new(),
            pinned_sessions: Vec::new(),
            launchers: default_launchers(),
            scrollback_lines: default_scrollback_lines(),
            sidebar_width: default_sidebar_width(),
            dim_inactive_panes: true,
            collapsed_groups: Vec::new(),
            custom_titlebar: false,
        }
    }
}

impl Default for Launcher {
    fn default() -> Self {
        Self {
            name: String::new(),
            command: String::new(),
            args: Vec::new(),
            icon: default_icon_bot(),
            color: default_icon_color(),
            logo: String::new(),
        }
    }
}

fn default_launchers() -> Vec<Launcher> {
    vec![
        Launcher {
            name: "opencode".into(),
            command: "opencode".into(),
            args: vec![],
            icon: "sparkles".into(),
            color: "#D97757".into(),
            logo: "opencode".into(),
        },
        Launcher {
            name: "cursor-agent".into(),
            command: "cursor-agent".into(),
            args: vec![],
            icon: "bot".into(),
            color: "#5AA0F2".into(),
            logo: "cursor".into(),
        },
        Launcher {
            name: "claude".into(),
            command: "claude".into(),
            args: vec![],
            icon: "sparkles".into(),
            color: "#D97757".into(),
            logo: "claude".into(),
        },
        Launcher {
            name: "codex".into(),
            command: "codex".into(),
            args: vec![],
            icon: "bot".into(),
            color: "#10A37F".into(),
            logo: "openai".into(),
        },
        Launcher {
            name: "grok".into(),
            command: "grok".into(),
            args: vec![],
            icon: "bot".into(),
            color: "#F0F0F0".into(),
            logo: "grok".into(),
        },
        Launcher {
            name: "antigravity".into(),
            command: "antigravity".into(),
            args: vec![],
            icon: "bot".into(),
            color: "#5AA0F2".into(),
            logo: "antigravity".into(),
        },
        Launcher {
            name: "copilot".into(),
            command: "gh".into(),
            args: vec!["copilot".into()],
            icon: "bot".into(),
            color: "#F0F0F0".into(),
            logo: "copilot".into(),
        },
    ]
}

impl Launcher {
    pub fn logo_key(&self) -> &str {
        if self.logo.is_empty() {
            self.name.as_str()
        } else {
            self.logo.as_str()
        }
    }
}

impl Config {
    /// Midterm-like narrow sidebar. Never let a bad save lock it at half the window.
    pub fn clamp_sidebar_width(width: f32) -> f32 {
        width.clamp(200.0, 320.0)
    }

    pub fn config_dir() -> anyhow::Result<PathBuf> {
        let base = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
        Ok(base.join(APP_NAME))
    }

    pub fn config_path() -> anyhow::Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn sessions_path() -> anyhow::Result<PathBuf> {
        Ok(Self::config_dir()?.join("sessions.json"))
    }

    /// Returns `(config, load_failed)`. When `load_failed` is true the on-disk
    /// TOML was unreadable/unparseable — callers must not overwrite it on exit.
    pub fn load_or_create() -> anyhow::Result<(Self, bool)> {
        let dir = Self::config_dir()?;
        fs::create_dir_all(&dir)?;
        let path = Self::config_path()?;
        if path.exists() {
            let text = fs::read_to_string(&path)?;
            match toml::from_str::<Config>(&text) {
                Ok(mut cfg) => {
                    cfg.sidebar_width = Self::clamp_sidebar_width(cfg.sidebar_width);
                    cfg.merge_default_launchers();
                    Ok((cfg, false))
                }
                Err(e) => {
                    log::warn!("config.toml parse failed: {e}");
                    let bad = dir.join("config.toml.bad");
                    if let Err(copy_err) = fs::copy(&path, &bad) {
                        log::warn!("could not back up broken config.toml: {copy_err}");
                    } else {
                        log::warn!("broken config copied to {}", bad.display());
                    }
                    Ok((Config::default(), true))
                }
            }
        } else {
            let cfg = Config::default();
            cfg.write_default_file(&path)?;
            Ok((cfg, false))
        }
    }

    /// Append any built-in launchers the user doesn't already have (by name).
    fn merge_default_launchers(&mut self) {
        for builtin in default_launchers() {
            let exists = self
                .launchers
                .iter()
                .any(|l| l.name.eq_ignore_ascii_case(&builtin.name));
            if !exists {
                self.launchers.push(builtin);
            } else {
                // Fill empty logo keys from builtins.
                if let Some(existing) = self
                    .launchers
                    .iter_mut()
                    .find(|l| l.name.eq_ignore_ascii_case(&builtin.name))
                {
                    if existing.logo.is_empty() && !builtin.logo.is_empty() {
                        existing.logo = builtin.logo;
                    }
                }
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path()?;
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }

    fn write_default_file(&self, path: &Path) -> anyhow::Result<()> {
        let body = format!(
            r##"# Ankar configuration
# Location: %APPDATA%\ankar\config.toml

# Default shell profile id: powershell | cmd | wsl | wsl:<distro>
default_shell = "{default_shell}"

# Seconds without PTY output before a live session is "waiting"
waiting_timeout_secs = {waiting_timeout_secs}

# Terminal font size (points) — Hack at 13 with ~1.3 line height
font_size = {font_size}

# Accent color (hex) — cursor + selection
accent = "{accent}"

# Scrollback lines per terminal (default 2000; hard cap 50000)
scrollback_lines = {scrollback_lines}

# Sidebar width in logical px (min 220, max 40% of window)
sidebar_width = {sidebar_width}

# Dim unfocused split panes
dim_inactive_panes = {dim_inactive_panes}

# Experimental custom title bar (Windows chrome risks — leave false)
custom_titlebar = false

# Pinned project folders (absolute paths)
pinned_projects = []

# Pinned session ids (stable names)
pinned_sessions = []

# Collapsed project group keys in the sidebar
collapsed_groups = []

# Launchers — icon is a Lucide name; color is hex
[[launchers]]
name = "opencode"
command = "opencode"
args = []
icon = "sparkles"
color = "#D97757"

[[launchers]]
name = "cursor-agent"
command = "cursor-agent"
args = []
icon = "bot"
color = "#8A8A8A"
"##,
            default_shell = self.default_shell,
            waiting_timeout_secs = self.waiting_timeout_secs,
            font_size = self.font_size,
            accent = self.accent,
            scrollback_lines = self.scrollback_lines,
            sidebar_width = self.sidebar_width,
            dim_inactive_panes = self.dim_inactive_panes,
        );
        fs::write(path, body)?;
        Ok(())
    }
}
