mod app;
mod config;
mod layout;
mod notify;
mod persist;
mod shell;
mod sidebar;
mod status;
mod terminal;

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([640.0, 400.0])
            .with_title("nacar"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "nacar",
        native_options,
        Box::new(|cc| Ok(Box::new(app::NacarApp::new(cc)?))),
    )
    .map_err(|e| anyhow::anyhow!("eframe failed: {e}"))
    .context("failed to start nacar")
}
