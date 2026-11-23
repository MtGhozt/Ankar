use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Hide the console window that `where.exe` would otherwise flash on Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Raster logos from `assets/logos/` (PNG / WebP). SVGs fall back until converted.
#[derive(Default)]
pub struct LogoAtlas {
    textures: HashMap<String, TextureHandle>,
    missing: HashMap<String, bool>,
    /// `where.exe` results keyed by command name, filled by the startup thread.
    installed: HashMap<String, bool>,
    installed_rx: Option<Receiver<(String, bool)>>,
}

impl LogoAtlas {
    /// Checks every launcher command once, off the UI thread.
    pub fn new(ctx: &Context, mut commands: Vec<String>) -> Self {
        commands.sort();
        commands.dedup();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for command in commands {
                let ok = command_installed(&command);
                if tx.send((command, ok)).is_err() {
                    return;
                }
            }
            ctx.request_repaint();
        });
        Self {
            installed_rx: Some(rx),
            ..Default::default()
        }
    }

    pub fn logos_dir() -> PathBuf {
        // Dev: next to Cargo.toml. Release: next to the exe, then cwd.
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/logos"),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("assets/logos")))
                .unwrap_or_default(),
            PathBuf::from("assets/logos"),
        ];
        candidates
            .into_iter()
            .find(|p| p.is_dir())
            .unwrap_or_else(|| PathBuf::from("assets/logos"))
    }

    pub fn get(&mut self, ctx: &Context, key: &str) -> Option<TextureHandle> {
        if let Some(t) = self.textures.get(key) {
            return Some(t.clone());
        }
        if self.missing.get(key).copied().unwrap_or(false) {
            return None;
        }
        let dir = Self::logos_dir();
        let path = resolve_logo_path(&dir, key);
        match path.and_then(|p| load_texture(ctx, key, &p)) {
            Some(tex) => {
                self.textures.insert(key.to_string(), tex.clone());
                Some(tex)
            }
            None => {
                self.missing.insert(key.to_string(), true);
                None
            }
        }
    }

    /// Unknown until the startup check reports back; treated as installed meanwhile.
    pub fn is_installed(&mut self, command: &str) -> bool {
        self.poll_installed();
        self.installed.get(command).copied().unwrap_or(true)
    }

    fn poll_installed(&mut self) {
        let Some(rx) = &self.installed_rx else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok((command, ok)) => {
                    self.installed.insert(command, ok);
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        self.installed_rx = None;
    }

    /// Hover / tooltip line for a launcher icon.
    pub fn launcher_tooltip(&mut self, name: &str, command: &str, args: &[String]) -> String {
        let cmd_line = if args.is_empty() {
            command.to_string()
        } else {
            format!("{command} {}", args.join(" "))
        };
        if self.is_installed(command) {
            format!("{name}\n{cmd_line}")
        } else {
            format!("{name} — not installed\nInstall or run: {cmd_line}")
        }
    }
}

fn resolve_logo_path(dir: &Path, key: &str) -> Option<PathBuf> {
    let stems = [
        key,
        &format!("{key}-logo"),
        &format!("{key}-ai-icon"),
        &format!("{key}-ai"),
    ];
    let exts = ["png", "webp", "jpg", "jpeg"];
    for stem in stems {
        for ext in exts {
            let p = dir.join(format!("{stem}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // Known file aliases
    let aliases: &[(&str, &str)] = &[
        ("opencode", "opencode-logo.webp"),
        ("openai", "openai.png"),
        ("claude", "claude-ai.png"),
        ("anthropic", "claude-ai.png"),
        ("grok", "grok-ai-icon.webp"),
        ("antigravity", "google-antigravity.png"),
        ("cursor", "cursor.png"),
        ("cursor-agent", "cursor.png"),
        ("copilot", "github-copilot.webp"),
        ("github-copilot", "github-copilot.webp"),
        ("codex", "openai.png"),
        ("default", "default.png"),
    ];
    for (k, file) in aliases {
        if key.eq_ignore_ascii_case(k) {
            let p = dir.join(file);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let fallback = dir.join("default.png");
    if fallback.is_file() {
        Some(fallback)
    } else {
        None
    }
}

fn load_texture(ctx: &Context, name: &str, path: &Path) -> Option<TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    let mut img = image::load_from_memory(&bytes).ok()?.into_rgba8();
    let ppp = ctx.pixels_per_point();
  // Target ~18 logical px icons at native density when the asset is larger.
    let target = (18.0 * ppp).round().max(1.0) as u32;
    let (w, h) = (img.width(), img.height());
    if w > target + 2 || h > target + 2 {
        img = image::imageops::resize(
            &img,
            target,
            target,
            image::imageops::FilterType::Lanczos3,
        );
    }
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(format!("logo-{name}"), color, TextureOptions::LINEAR))
}

/// `where.exe` check — true if the command resolves on PATH.
/// Must use `CREATE_NO_WINDOW` on Windows or each probe flashes a console ("pum pum").
pub fn command_installed(command: &str) -> bool {
    let mut cmd = Command::new("where.exe");
    cmd.arg(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.status().map(|s| s.success()).unwrap_or(false)
}
