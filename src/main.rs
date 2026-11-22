#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod files;
mod launchers;
mod layout;
mod logos;
mod notify;
mod palette;
mod persist;
mod shell;
mod sidebar;
mod status;
mod terminal;
mod ui_theme;

use anyhow::Context;

/// Bumped whenever we need Matias to confirm he's running a fresh binary.
const BUILD_MARK: &str = "midterm-r8";

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
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if let Err(err) = config::migrate_legacy_dir() {
        log::warn!("legacy config migration failed: {err}");
    }

    let args: Vec<String> = std::env::args().collect();
    let window_size = match parse_window_size(&args) {
        Some(size) => {
            log::info!("window size override: {}x{} logical px", size[0], size[1]);
            size
        }
        None => [1100.0, 700.0],
    };

    if let Ok(exe) = std::env::current_exe() {
        log::info!(
            "Ankar {BUILD_MARK} launching from {}",
            exe.display()
        );
    } else {
        log::info!("Ankar {BUILD_MARK} launching");
    }

    let title = format!("Ankar ({BUILD_MARK})");
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_size)
            .with_min_inner_size([640.0, 400.0])
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
