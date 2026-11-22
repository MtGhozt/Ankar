use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Raster logos from `assets/logos/` (PNG / WebP). SVGs fall back until converted.
pub struct LogoAtlas {
    textures: HashMap<String, TextureHandle>,
    missing: HashMap<String, bool>,
    /// Cached `where.exe` results keyed by command name.
    installed: HashMap<String, bool>,
}

impl Default for LogoAtlas {
    fn default() -> Self {
        Self {
            textures: HashMap::new(),
            missing: HashMap::new(),
            installed: HashMap::new(),
        }
    }
}

impl LogoAtlas {
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

    pub fn is_installed(&mut self, command: &str) -> bool {
        if let Some(v) = self.installed.get(command) {
            return *v;
        }
        let ok = command_installed(command);
        self.installed.insert(command.to_string(), ok);
        ok
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
            format!("{name} — no instalado\nInstalar / ejecutar: {cmd_line}")
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
    let img = image::load_from_memory(&bytes).ok()?.into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(format!("logo-{name}"), color, TextureOptions::LINEAR))
}

/// `where.exe` check — true if the command resolves on PATH.
pub fn command_installed(command: &str) -> bool {
    let exe = if command.contains('.') || command.contains('\\') || command.contains('/') {
        command.to_string()
    } else {
        // Try bare name; Windows where.exe finds .exe/.cmd/.bat
        command.to_string()
    };
    std::process::Command::new("where.exe")
        .arg(&exe)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
