use crate::config::Launcher;
use crate::layout::Workspace;
use std::path::PathBuf;

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
                .map(|a| format!("'{}'", a.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let args = vec!["-NoLogo".into(), "-Command".into(), script];
    workspace.open_command_tab(ctx, &launcher.name, &ps.program, &args, cwd)?;
    Ok(())
}
