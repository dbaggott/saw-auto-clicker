//! The settings window (R1, R14, R15, Q8): an eframe window opened on demand
//! on the main thread. While it is open, eframe's event loop pumps the main
//! thread's messages, so the tray keeps working; the window polls for tray
//! clicks and the clicker's reports as the main loop does.

use std::time::Duration;

use eframe::egui;
use saw_core::CPS_RANGE;

use crate::form::{self, Draft};
use crate::hotkey_text::{self, KEYS, key_name};
use crate::icon;
use crate::win::app::{App, Next};

/// How often the open window checks for tray clicks and the clicker's
/// reports.
const POLL: Duration = Duration::from_millis(100);

/// Shows the window until it closes, and says whether Quit was chosen from
/// the tray meanwhile. `close_after_frames` closes it by itself once it has
/// drawn that many frames, for the self-test.
pub fn show(app: &mut App, close_after_frames: Option<u32>) -> Result<Next, String> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("saw-auto-clicker settings")
        .with_inner_size([400.0, 330.0])
        .with_resizable(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_icon(egui::IconData {
            rgba: icon::rgba(true),
            width: icon::SIZE,
            height: icon::SIZE,
        });
    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..eframe::NativeOptions::default()
    };
    let mut next = Next::Wait;
    let draft = Draft::from(&app.settings);
    eframe::run_native(
        "saw-auto-clicker",
        options,
        Box::new(|_| {
            Ok(Box::new(Window {
                app,
                draft,
                applied: false,
                apply_problem: None,
                next: &mut next,
                frames: 0,
                close_after_frames,
            }))
        }),
    )
    .map_err(|e| format!("the settings window failed: {e}"))?;
    Ok(next)
}

struct Window<'a> {
    app: &'a mut App,
    draft: Draft,
    /// Whether the last Apply went through, for its confirmation line.
    applied: bool,
    /// Why the last Apply was refused before reaching the app.
    apply_problem: Option<String>,
    next: &'a mut Next,
    frames: u32,
    close_after_frames: Option<u32>,
}

impl eframe::App for Window<'_> {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.app.handle_arrivals() {
            Next::Wait => {}
            Next::OpenSettings => ctx.send_viewport_cmd(egui::ViewportCommand::Focus),
            Next::Quit => {
                *self.next = Next::Quit;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint_after(POLL);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| self.form(ui));
        self.frames += 1;
        if self.close_after_frames.is_some_and(|n| self.frames >= n) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl Window<'_> {
    fn form(&mut self, ui: &mut egui::Ui) {
        let (delay_ms, jitter_percent) = (form::delay_ms_range(), form::jitter_percent_range());
        let d = &mut self.draft;
        egui::Grid::new("settings")
            .num_columns(2)
            .spacing([12.0, 10.0])
            .show(ui, |ui| {
                ui.label("Clicks per second");
                ui.add(egui::Slider::new(&mut d.cps, CPS_RANGE));
                ui.end_row();
                ui.label("Activation delay");
                ui.add(egui::Slider::new(&mut d.delay_ms, delay_ms).suffix(" ms"));
                ui.end_row();
                ui.label("Jitter");
                ui.add(egui::Slider::new(&mut d.jitter_percent, jitter_percent).suffix("%"));
                ui.end_row();
                ui.label("Toggle hotkey");
                ui.horizontal(|ui| {
                    let m = &mut d.hotkey.modifiers;
                    ui.checkbox(&mut m.ctrl, "Ctrl");
                    ui.checkbox(&mut m.alt, "Alt");
                    ui.checkbox(&mut m.shift, "Shift");
                    ui.checkbox(&mut m.meta, "Win");
                    egui::ComboBox::from_id_salt("key")
                        .width(56.0)
                        .selected_text(key_name(d.hotkey.key))
                        .show_ui(ui, |ui| {
                            for (key, name) in KEYS {
                                ui.selectable_value(&mut d.hotkey.key, key, name);
                            }
                        });
                });
                ui.end_row();
            });

        ui.add_space(12.0);
        let changed = self.draft != Draft::from(&self.app.settings);
        // With no hotkey active, applying unchanged settings retries it.
        let can_apply = changed || self.app.active_hotkey.is_none();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(can_apply, egui::Button::new("Apply"))
                .clicked()
            {
                self.apply();
            }
            if ui.button("Close").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        ui.add_space(8.0);
        let error = ui.visuals().error_fg_color;
        let warning = ui.visuals().warn_fg_color;
        for problem in [
            &self.apply_problem,
            &self.app.hotkey_problem,
            &self.app.save_problem,
        ]
        .into_iter()
        .flatten()
        {
            ui.colored_label(error, problem);
        }
        if let Some(problem) = &self.app.config_problem {
            ui.colored_label(warning, problem);
        }
        if self.applied && !changed && self.app.hotkey_problem.is_none() {
            ui.label("Applied. New settings take effect from the next press.");
        }
    }

    fn apply(&mut self) {
        let checked = hotkey_text::usable(self.draft.hotkey)
            .and_then(|_| self.draft.to_settings().map_err(|e| e.to_string()));
        match checked {
            Ok(settings) => {
                self.app.apply_settings(settings);
                self.apply_problem = None;
                self.applied = true;
                // A refused hotkey shows the one still in effect.
                self.draft = Draft::from(&self.app.settings);
            }
            Err(why) => {
                self.apply_problem = Some(why);
                self.applied = false;
            }
        }
    }
}
