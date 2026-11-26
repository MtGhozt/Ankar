//! The log always goes to `%APPDATA%\ankar\ankar-log.txt`: release builds have no console.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const LOG_FILE: &str = "ankar-log.txt";

pub fn log_path(dir: &Path) -> PathBuf {
    dir.join(LOG_FILE)
}

/// `RUST_LOG` wins, then `log_level` from config.toml, else `info`. Same syntax as `RUST_LOG`.
pub fn filter(rust_log: Option<String>, config_level: Option<String>) -> String {
    [rust_log, config_level]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_owned())
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "info".into())
}

/// The `conpty.dll` alacritty_terminal will load: beside the exe first, then on PATH. `None`
/// means the inbox Windows ConPTY, which swallows OSC 10/11 colour queries.
pub fn conpty_dll(exe_dir: Option<&Path>, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let on_path = path_var.map(|p| std::env::split_paths(p).collect::<Vec<_>>()).unwrap_or_default();
    exe_dir
        .map(Path::to_path_buf)
        .into_iter()
        .chain(on_path)
        .map(|dir| dir.join("conpty.dll"))
        .find(|dll| dll.is_file())
}

/// Every record goes to the file; stderr too when there is a console (debug builds).
struct Tee(File);

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buf);
        self.0.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Truncates the log file and installs the global logger on it. Returns the file path.
pub fn init(dir: &Path, filter: &str) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = log_path(dir);
    let file = File::create(&path)?;
    env_logger::Builder::new()
        .parse_filters(filter)
        .target(env_logger::Target::Pipe(Box::new(Tee(file))))
        .try_init()?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_lives_in_the_config_dir_and_rust_log_beats_config() {
        let dir = Path::new(r"C:\Users\m\AppData\Roaming\ankar");
        assert_eq!(log_path(dir), dir.join("ankar-log.txt"));
        assert_eq!(filter(None, None), "info");
        assert_eq!(filter(None, Some("debug".into())), "debug");
        assert_eq!(filter(Some("ankar=debug".into()), Some("warn".into())), "ankar=debug");
        assert_eq!(filter(Some("  ".into()), Some("".into())), "info", "blank means unset");
    }

    #[test]
    fn conpty_dll_is_found_beside_the_exe_before_path() {
        let base = std::env::temp_dir().join(format!("ankar-conpty-{}", std::process::id()));
        let (exe_dir, on_path, empty) = (base.join("exe"), base.join("path"), base.join("empty"));
        for d in [&exe_dir, &on_path, &empty] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(on_path.join("conpty.dll"), b"").unwrap();
        let path_var = std::env::join_paths([&empty, &on_path]).unwrap();
        assert_eq!(conpty_dll(Some(&exe_dir), Some(&path_var)), Some(on_path.join("conpty.dll")));
        std::fs::write(exe_dir.join("conpty.dll"), b"").unwrap();
        assert_eq!(conpty_dll(Some(&exe_dir), Some(&path_var)), Some(exe_dir.join("conpty.dll")));
        assert_eq!(conpty_dll(Some(&empty), None), None, "inbox ConPTY");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn init_truncates_the_file_and_writes_records_to_it() {
        let dir = std::env::temp_dir().join(format!("ankar-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(LOG_FILE), "stale line from the last run\n").unwrap();
        let path = init(&dir, "info").expect("logger installs");
        assert_eq!(path, dir.join(LOG_FILE));
        log::info!("ankar log probe ñ");
        let text = std::fs::read_to_string(&path).expect("UTF-8 log file");
        assert!(text.contains("ankar log probe ñ"), "record written: {text:?}");
        assert!(!text.contains("stale line"), "truncated on start");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
