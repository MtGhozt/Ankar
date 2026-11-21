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
    3_000
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_shell: String,
    pub waiting_timeout_secs: u64,
    pub font_size: f32,
    pub accent: String,
    pub pinned_projects: Vec<String>,
    pub pinned_sessions: Vec<String>,
    pub launchers: Vec<Launcher>,
    /// Scrollback lines kept per terminal. Hard-capped at 50_000 in the session.
    #[serde(default = "default_scrollback_lines")]
    pub scrollback_lines: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Launcher {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_shell: "powershell".into(),
            waiting_timeout_secs: 30,
            font_size: 14.0,
            accent: "#3A7CA5".into(),
            pinned_projects: Vec::new(),
            pinned_sessions: Vec::new(),
            launchers: vec![
                Launcher {
                    name: "opencode".into(),
                    command: "opencode".into(),
                    args: vec![],
                },
                Launcher {
                    name: "cursor-agent".into(),
                    command: "cursor-agent".into(),
                    args: vec![],
                },
            ],
            scrollback_lines: default_scrollback_lines(),
        }
    }
}

impl Config {
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
                Ok(cfg) => Ok((cfg, false)),
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

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path()?;
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }

    fn write_default_file(&self, path: &Path) -> anyhow::Result<()> {
        let body = format!(
            r#"# Ankar configuration
# Location: %APPDATA%\ankar\config.toml

# Default shell profile id: powershell | cmd | wsl | wsl:<distro>
default_shell = "{default_shell}"

# Seconds without PTY output before a live session is "waiting"
waiting_timeout_secs = {waiting_timeout_secs}

# Terminal font size (points)
font_size = {font_size}

# Accent color (hex)
accent = "{accent}"

# Scrollback lines per terminal (default 3000; hard cap 50000)
scrollback_lines = {scrollback_lines}

# Pinned project folders (absolute paths)
pinned_projects = []

# Pinned session ids (stable names)
pinned_sessions = []

# Bottom launcher buttons — run command in a new tab for the active project
[[launchers]]
name = "opencode"
command = "opencode"
args = []

[[launchers]]
name = "cursor-agent"
command = "cursor-agent"
args = []
"#,
            default_shell = self.default_shell,
            waiting_timeout_secs = self.waiting_timeout_secs,
            font_size = self.font_size,
            accent = self.accent,
            scrollback_lines = self.scrollback_lines,
        );
        fs::write(path, body)?;
        Ok(())
    }
}
