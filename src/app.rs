use crate::terminal::{TerminalSession, TerminalView};
use egui::{Color32, Context, Frame, RichText, Theme, Visuals};

pub struct NacarApp {
    session: TerminalSession,
}

impl NacarApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        apply_theme(&cc.egui_ctx);

        let shell = default_powershell();
        let session = TerminalSession::spawn(
            1,
            cc.egui_ctx.clone(),
            &shell.program,
            &shell.args,
            None,
        )?;

        Ok(Self { session })
    }
}

impl eframe::App for NacarApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("title_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("nacar")
                        .strong()
                        .color(Color32::from_rgb(0xE8, 0xE8, 0xE8)),
                );
                ui.label(
                    RichText::new("PowerShell")
                        .small()
                        .color(Color32::from_rgb(0x88, 0x88, 0x88)),
                );
            });
        });

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::from_rgb(0x12, 0x12, 0x14)))
            .show(ui, |ui| {
                TerminalView::show(ui, &mut self.session);
            });
    }
}

struct ShellCmd {
    program: String,
    args: Vec<String>,
}

fn default_powershell() -> ShellCmd {
    let candidates = [
        r"C:\Program Files\PowerShell\7\pwsh.exe",
        r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
    ];
    for path in candidates {
        if std::path::Path::new(path).exists() {
            return ShellCmd {
                program: path.to_string(),
                args: vec!["-NoLogo".into()],
            };
        }
    }
    ShellCmd {
        program: "powershell.exe".into(),
        args: vec!["-NoLogo".into()],
    }
}

fn apply_theme(ctx: &Context) {
    let mut visuals = Visuals::dark();
    visuals.window_fill = Color32::from_rgb(0x12, 0x12, 0x14);
    visuals.panel_fill = Color32::from_rgb(0x12, 0x12, 0x14);
    visuals.override_text_color = Some(Color32::from_rgb(0xD0, 0xD0, 0xD0));
    visuals.selection.bg_fill = Color32::from_rgb(0x3A, 0x7C, 0xA5);
    ctx.set_visuals_of(Theme::Dark, visuals);

    ctx.style_mut_of(Theme::Dark, |style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    });
}
