#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod changes;
mod config;
mod files;
mod launchers;
mod layout;
mod color_check;
mod dir_picker;
mod folder_pick;
mod footer;
mod gitinfo;
mod inbox;
mod logging;
mod logos;
mod notify;
mod palette;
mod persist;
mod procs;
mod shell;
mod sidebar;
mod stats;
mod status;
mod terminal;
mod ui_theme;
mod window_watch;

use anyhow::Context;

/// Bumped whenever we need Matias to confirm he's running a fresh binary.
const BUILD_MARK: &str = "r15";

/// Parse `--window-size WxH` (logical px) for midterm acceptance screenshots.
fn parse_window_size(args: &[String]) -> Option<[f32; 2]> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if let Some(rest) = arg.strip_prefix("--window-size=") {
            return parse_wh(rest);
        }
        if arg == "--window-size" {
            if let Some(val) = iter.next() {
                return parse_wh(val);
            }
        }
    }
    None
}

fn parse_wh(s: &str) -> Option<[f32; 2]> {
    let (w, h) = s.split_once('x').or_else(|| s.split_once('X'))?;
    let w: f32 = w.trim().parse().ok()?;
    let h: f32 = h.trim().parse().ok()?;
    if w >= 320.0 && h >= 240.0 {
        Some([w, h])
    } else {
        None
    }
}

fn main() -> anyhow::Result<()> {
    if let Err(err) = config::migrate_legacy_dir() {
        eprintln!("legacy config migration failed: {err}");
    }
    let filter = logging::filter(std::env::var("RUST_LOG").ok(), config::Config::log_level_on_disk());
    match config::Config::config_dir().and_then(|dir| logging::init(&dir, &filter)) {
        Ok(path) => {
            eprintln!("ankar log: {}", path.display());
            log::info!("log file {} (filter {filter})", path.display());
        }
        Err(err) => {
            env_logger::Builder::new().parse_filters(&filter).init();
            log::warn!("no log file ({err}); logging to stderr only");
        }
    }
    if let Ok(path) = config::Config::config_path() {
        log::info!("config {}", path.display());
    }

    let args: Vec<String> = std::env::args().collect();

    let window_size = match parse_window_size(&args) {
        Some(size) => {
            log::info!("window size override: {}x{} logical px", size[0], size[1]);
            size
        }
        None => [1100.0, 700.0],
    };

    let version = env!("CARGO_PKG_VERSION");
    let exe = std::env::current_exe().ok();
    match &exe {
        Some(exe) => log::info!("Ankar {version} ({BUILD_MARK}) launching from {}", exe.display()),
        None => log::info!("Ankar {version} ({BUILD_MARK}) launching"),
    }
    let exe_dir = exe.as_deref().and_then(std::path::Path::parent);
    match logging::conpty_dll(exe_dir, std::env::var_os("PATH").as_deref()) {
        Some(dll) => log::info!("ConPTY: {} (OpenConsole.exe expected in x64\\ beside it)", dll.display()),
        None => log::info!("ConPTY: inbox Windows API; no conpty.dll beside ankar.exe or on PATH"),
    }

    let title = format!("Ankar ({BUILD_MARK})");
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_size)
            .with_min_inner_size([640.0, 400.0])
            // Chromeless: the app draws its own caption bar (see `draw_tab_bar`).
            .with_decorations(false)
            .with_transparent(false)
            .with_title(title.clone()),
        // Native monitor DPI (e.g. 1.5 at 150%). Do not set_pixels_per_point.
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        &title,
        native_options,
        Box::new(|cc| Ok(Box::new(app::AnkarApp::new(cc)?))),
    )
    .map_err(|e| anyhow::anyhow!("eframe failed: {e}"))
    .context("failed to start Ankar")
}
