use crate::config::{Config, Launcher};
use crate::layout::Workspace;
use crate::ui_theme;
use egui::{RichText, Ui};
use std::path::PathBuf;

pub fn draw_bar(ui: &mut Ui, config: &Config, workspace: &mut Workspace) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Launch")
                .small()
                .color(ui_theme::TEXT_MUTED),
        );
        let launchers = config.launchers.clone();
        let cwd = workspace.active_tab().and_then(|t| t.cwd.clone());
        for launcher in &launchers {
            if ui.button(&launcher.name).clicked() {
                let _ = run_launcher(ui.ctx(), workspace, launcher, cwd.clone());
            }
        }
    });
}

pub fn run_launcher(
    ctx: &egui::Context,
    workspace: &mut Workspace,
    launcher: &Launcher,
    cwd: Option<PathBuf>,
) -> anyhow::Result<()> {
    // Run via PowerShell so PATH-resolved tools work.
    let ps = crate::shell::ShellProfile::powershell();
    let script = if launcher.args.is_empty() {
        launcher.command.clone()
    } else {
        format!(
            "{} {}",
            launcher.command,
            launcher
                .args
                .iter()
                .map(|a| format!("'{a}'"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let args = vec!["-NoLogo".into(), "-Command".into(), script];
    workspace.open_command_tab(ctx, &launcher.name, &ps.program, &args, cwd)?;
    Ok(())
}
