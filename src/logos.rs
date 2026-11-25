use crate::config::Config;
use crate::layout::Tab;
use crate::ui_theme::{self, Icon};
use egui::{Color32, ColorImage, Context, TextureFilter, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Hide the console window that `where.exe` would otherwise flash on Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Agent logos embedded from `assets/logos/` (64×64 RGBA PNG), resampled to the drawn size.
#[derive(Default)]
pub struct LogoAtlas {
    /// Keyed by logo and physical pixel size; `None` if decoding failed.
    textures: HashMap<(&'static str, u32), Option<TextureHandle>>,
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

    /// Logo for `key` drawn at `size` logical px. Unknown keys have no logo.
    pub fn get(&mut self, ctx: &Context, key: &str, size: f32) -> Option<TextureHandle> {
        let (name, bytes) = embedded_logo(key)?;
        let px = (size * ctx.pixels_per_point()).ceil().max(1.0) as u32;
        self.textures
            .entry((name, px))
            .or_insert_with(|| load_texture(ctx, name, bytes, px))
            .clone()
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

fn embedded_logo(key: &str) -> Option<(&'static str, &'static [u8])> {
    Some(match key.to_ascii_lowercase().as_str() {
        "opencode" => ("opencode", &include_bytes!("../assets/logos/opencode.png")[..]),
        "openai" | "codex" => ("openai", &include_bytes!("../assets/logos/openai.png")[..]),
        "claude" | "anthropic" => ("claude", &include_bytes!("../assets/logos/claude.png")[..]),
        "grok" => ("grok", &include_bytes!("../assets/logos/grok.png")[..]),
        "antigravity" => (
            "antigravity",
            &include_bytes!("../assets/logos/antigravity.png")[..],
        ),
        "cursor" | "cursor-agent" => ("cursor", &include_bytes!("../assets/logos/cursor.png")[..]),
        "copilot" | "github-copilot" => {
            ("copilot", &include_bytes!("../assets/logos/copilot.png")[..])
        }
        "junie" => ("junie", &include_bytes!("../assets/logos/junie.png")[..]),
        _ => return None,
    })
}

fn load_texture(ctx: &Context, name: &str, bytes: &[u8], px: u32) -> Option<TextureHandle> {
    let mut img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    // Resample premultiplied so transparent (black) pixels don't darken the edges.
    for p in img.pixels_mut() {
        let a = p[3] as u16;
        for c in 0..3 {
            p[c] = ((p[c] as u16 * a + 127) / 255) as u8;
        }
    }
    if img.width() != px || img.height() != px {
        img = image::imageops::resize(&img, px, px, image::imageops::FilterType::Lanczos3);
    }
    // Lanczos can overshoot; keep the premultiplied invariant (color <= alpha).
    for p in img.pixels_mut() {
        let a = p[3];
        for c in 0..3 {
            p[c] = p[c].min(a);
        }
    }
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_premultiplied(size, img.as_raw());
    let options = TextureOptions {
        magnification: TextureFilter::Linear,
        minification: TextureFilter::Linear,
        mipmap_mode: Some(TextureFilter::Linear),
        ..Default::default()
    };
    Some(ctx.load_texture(format!("logo-{name}-{px}"), color, options))
}

/// Lucide fallback icon, its color, and the embedded logo key for a tab.
pub fn agent_visual(tab: &Tab, config: &Config) -> (Icon, Color32, Option<&'static str>) {
    let name = tab.shown_name().to_lowercase();
    let cmd = tab.command.as_deref().unwrap_or("").to_lowercase();
    let hay = format!("{name} {cmd} {}", tab.profile_id);

    for launcher in &config.launchers {
        let ln = launcher.name.to_lowercase();
        if hay.contains(&ln) || name == ln {
            let ic = Icon::from_name(&launcher.icon).unwrap_or(Icon::Bot);
            let color =
                ui_theme::parse_hex_color(&launcher.color).unwrap_or(ui_theme::TEXT_MUTED);
            let key = embedded_logo(launcher.logo_key()).map(|(key, _)| key);
            return (ic, color, key);
        }
    }
    if hay.contains("claude") {
        return (Icon::Sparkles, ui_theme::rgb(0xD9, 0x77, 0x57), Some("claude"));
    }
    if hay.contains("opencode") {
        return (Icon::Sparkles, ui_theme::rgb(0xD9, 0x77, 0x57), Some("opencode"));
    }
    if hay.contains("codex") || hay.contains("openai") {
        return (Icon::Bot, ui_theme::rgb(0x10, 0xA3, 0x7F), Some("openai"));
    }
    if hay.contains("cursor") {
        return (Icon::Bot, ui_theme::rgb(0x5A, 0xA0, 0xF2), Some("cursor"));
    }
    if hay.contains("grok") {
        return (Icon::Bot, ui_theme::TEXT, Some("grok"));
    }
    if hay.contains("antigravity") {
        return (Icon::Bot, ui_theme::rgb(0x5A, 0xA0, 0xF2), Some("antigravity"));
    }
    if hay.contains("copilot") {
        return (Icon::Bot, ui_theme::TEXT, Some("copilot"));
    }
    if tab.command.is_some() || tab.detected.is_some() {
        return (Icon::Bot, ui_theme::TEXT_MUTED, None);
    }
    (Icon::Terminal, ui_theme::TEXT_MUTED, None)
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
