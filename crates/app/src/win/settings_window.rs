//! The settings window (R1, R14, R15, Q8): an eframe window opened on demand
//! on the main thread. While it is open, eframe's event loop pumps the main
//! thread's messages, so the tray keeps working; the window polls for tray
//! clicks and the clicker's reports as the main loop does.

use std::time::Duration;

use eframe::egui;
use saw_core::{CPS_RANGE, Hotkey, Key, Modifiers};

use crate::form::{self, Draft};
use crate::hotkey_text::{self, KEYS, key_name};
use crate::icon;
use crate::status::hotkey_missing;
use crate::win::app::{App, Next};

/// How often the open window checks for tray clicks and the clicker's
/// reports.
const POLL: Duration = Duration::from_millis(100);

/// The icon's hub orange: its blade orange is too light to carry white text.
const ACCENT: egui::Color32 =
    egui::Color32::from_rgb(icon::ARMED_HUB[0], icon::ARMED_HUB[1], icon::ARMED_HUB[2]);

const WIDTH: f32 = 540.0;
const MARGIN: i8 = 16;

const CPS_HELP: &str = "How fast it clicks while you hold a mouse button down, \
from 1 to 50 clicks per second.";
const DELAY_HELP: &str = "How long you hold a mouse button before auto-clicking \
starts. Anything shorter is an ordinary press, so normal clicks, double-clicks \
and quick drags still work.\n\nWhile armed, any longer hold auto-clicks, \
including drags and text selection. Disarm first for that kind of work.";
const JITTER_HELP: &str = "Makes the time between clicks vary at random, so \
they aren't perfectly even. The average rate stays the same.\n\nAt 0% every \
click is evenly spaced. At 50%, each gap can be anywhere from half to one and \
a half times its usual length.";
const HOTKEY_HELP: &str = "A keyboard shortcut that arms and disarms \
auto-clicking from anywhere. Choose a key, then pick the keys to hold with \
it. A letter or digit also needs Ctrl, Alt or Win, so the key still types \
normally.\n\nIf another app already uses the shortcut, this window says so \
and the previous one keeps working. With None, arm and disarm from the tray \
icon instead.";
const ARM_ON_STARTUP_HELP: &str = "Start armed whenever the app starts, \
including when Windows starts it at login. When off, the app starts disarmed, \
except the first time after it's installed: arm it with the hotkey or from \
the tray icon.";

/// Shows the window until it closes, and says whether Quit was chosen from
/// the tray meanwhile. `close_after_frames` closes it by itself once it has
/// drawn that many frames, for the self-test.
pub fn show(app: &mut App, close_after_frames: Option<u32>) -> Result<Next, String> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("saw-auto-clicker settings")
        .with_inner_size([WIDTH, 360.0])
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
    let arm_on_startup = app.arm_on_startup;
    let modifiers = draft.hotkey.map_or(
        // What a newly chosen key starts with.
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::default()
        },
        |h| h.modifiers,
    );
    eframe::run_native(
        "saw-auto-clicker",
        options,
        Box::new(|cc| {
            style(&cc.egui_ctx);
            Ok(Box::new(Window {
                app,
                draft,
                modifiers,
                arm_on_startup,
                applied: false,
                apply_problem: None,
                next: &mut next,
                requested_height: None,
                frames: 0,
                close_after_frames,
            }))
        }),
    )
    .map_err(|e| format!("the settings window failed: {e}"))?;
    Ok(next)
}

/// Roomier spacing, rounder corners and the icon's orange as the accent, in
/// both the light and dark themes.
fn style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        for (text_style, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 14.0),
            (egui::TextStyle::Heading, 15.0),
            (egui::TextStyle::Small, 11.5),
        ] {
            if let Some(font) = style.text_styles.get_mut(&text_style) {
                font.size = size;
            }
        }
        let spacing = &mut style.spacing;
        spacing.item_spacing = egui::vec2(8.0, 8.0);
        spacing.button_padding = egui::vec2(14.0, 6.0);
        spacing.interact_size.y = 26.0;
        spacing.slider_width = 190.0;
        spacing.tooltip_width = 300.0;

        let visuals = &mut style.visuals;
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke.color = egui::Color32::WHITE;
        visuals.slider_trailing_fill = true;
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(6);
        }
        // egui's default label grey is faint, most of all in the dark theme.
        visuals.widgets.noninteractive.fg_stroke.color = if visuals.dark_mode {
            egui::Color32::from_gray(210)
        } else {
            egui::Color32::from_gray(40)
        };
        visuals.window_corner_radius = egui::CornerRadius::same(10);
        visuals.menu_corner_radius = egui::CornerRadius::same(8);
    });
}

struct Window<'a> {
    app: &'a mut App,
    /// Its hotkey is `None` while no key is chosen.
    draft: Draft,
    /// The hotkey's modifiers as ticked, kept while no key is chosen.
    modifiers: Modifiers,
    arm_on_startup: bool,
    /// Whether the last Apply went through, for its confirmation line.
    applied: bool,
    /// Why the last Apply was refused before reaching the app.
    apply_problem: Option<String>,
    next: &'a mut Next,
    /// The height last asked of the window, so a size the OS won't grant is
    /// asked for once rather than every frame.
    requested_height: Option<f32>,
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
        let panel = egui::Frame::central_panel(ui.style()).inner_margin(MARGIN);
        let bottom = egui::CentralPanel::default()
            .frame(panel)
            .show(ui, |ui| {
                // Measured around the content: the panel itself always fills
                // the window.
                ui.vertical(|ui| {
                    self.settings(ui);
                    let changed = self.changed();
                    self.notices(ui, changed);
                    ui.add_space(12.0);
                    self.buttons(ui, changed);
                })
                .response
                .rect
                .bottom()
            })
            .inner;
        self.fit_height(ui.ctx(), bottom + f32::from(MARGIN));
        self.frames += 1;
        if self.close_after_frames.is_some_and(|n| self.frames >= n) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl Window<'_> {
    fn settings(&mut self, ui: &mut egui::Ui) {
        let (delay_ms, jitter_percent) = (form::delay_ms_range(), form::jitter_percent_range());
        let d = &mut self.draft;
        card(ui, "Clicking", |ui| {
            setting(ui, "Clicks per second", CPS_HELP);
            ui.add(egui::Slider::new(&mut d.cps, CPS_RANGE));
            ui.end_row();
            setting(ui, "Activation delay", DELAY_HELP);
            ui.add(egui::Slider::new(&mut d.delay_ms, delay_ms).suffix(" ms"));
            ui.end_row();
            setting(ui, "Jitter", JITTER_HELP);
            ui.add(egui::Slider::new(&mut d.jitter_percent, jitter_percent).suffix("%"));
            ui.end_row();
        });
        ui.add_space(4.0);
        card(ui, "Arming", |ui| {
            setting(ui, "Toggle hotkey", HOTKEY_HELP);
            ui.horizontal(|ui| {
                let mut key: Option<Key> = d.hotkey.map(|h| h.key);
                let m = &mut self.modifiers;
                ui.add_enabled_ui(key.is_some(), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.spacing_mut().button_padding.x = 8.0;
                    for (on, name) in [
                        (&mut m.ctrl, "Ctrl"),
                        (&mut m.alt, "Alt"),
                        (&mut m.shift, "Shift"),
                        (&mut m.meta, "Win"),
                    ] {
                        if ui.add(egui::Button::new(name).selected(*on)).clicked() {
                            *on = !*on;
                        }
                    }
                });
                egui::ComboBox::from_id_salt("key")
                    .width(64.0)
                    .selected_text(key.map_or("None", key_name))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut key, None, "None");
                        for (k, name) in KEYS {
                            ui.selectable_value(&mut key, Some(k), name);
                        }
                    });
                d.hotkey = key.map(|key| Hotkey { modifiers: *m, key });
            });
            ui.end_row();
            setting(ui, "Arm on startup", ARM_ON_STARTUP_HELP);
            toggle(ui, &mut self.arm_on_startup, "Arm on startup");
            ui.end_row();
        });
    }

    fn changed(&self) -> bool {
        self.draft != Draft::from(&self.app.settings)
            || self.arm_on_startup != self.app.arm_on_startup
    }

    fn buttons(&mut self, ui: &mut egui::Ui, changed: bool) {
        // With the chosen hotkey refused, applying unchanged settings retries
        // it.
        let can_apply = changed || hotkey_missing(&self.app.settings, self.app.active_hotkey);
        // A fixed height: a right-to-left layout otherwise fills the rest of
        // the panel, and the window could never shrink to fit.
        let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(row, layout, |ui| {
            let apply = egui::Button::new(egui::RichText::new("Apply").color(egui::Color32::WHITE))
                .fill(ACCENT)
                .min_size(egui::vec2(84.0, 0.0));
            if ui.add_enabled(can_apply, apply).clicked() {
                self.apply();
                // The notices above are already drawn for this frame.
                ui.ctx().request_repaint();
            }
            if ui
                .add(egui::Button::new("Close").min_size(egui::vec2(84.0, 0.0)))
                .clicked()
            {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn notices(&self, ui: &mut egui::Ui, changed: bool) {
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
            notice(ui, error, problem);
        }
        if let Some(problem) = &self.app.config_problem {
            notice(ui, warning, problem);
        }
        if self.applied && !changed && self.app.hotkey_problem.is_none() {
            let done = ui.visuals().text_color();
            notice(
                ui,
                done,
                "Applied. New settings take effect from the next press.",
            );
        }
    }

    /// Grows or shrinks the window to `height`, so a message never needs
    /// scrolling to and no space sits empty below the buttons.
    fn fit_height(&mut self, ctx: &egui::Context, height: f32) {
        let current = ctx.input(|i| i.viewport().inner_rect.map(|r| r.height()));
        let off = |h: Option<f32>| h.is_none_or(|h| (h - height).abs() > 1.0);
        if off(current) && off(self.requested_height) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(WIDTH, height)));
            self.requested_height = Some(height);
        }
    }

    fn apply(&mut self) {
        let checked = self
            .draft
            .hotkey
            .map_or(Ok(()), |h| hotkey_text::usable(h).map(|_| ()))
            .and_then(|()| self.draft.to_settings().map_err(|e| e.to_string()));
        match checked {
            Ok(settings) => {
                self.app.apply_settings(settings, self.arm_on_startup);
                self.apply_problem = None;
                self.applied = true;
                // A refused hotkey shows the one still in effect.
                self.draft = Draft::from(&self.app.settings);
                if let Some(hotkey) = self.draft.hotkey {
                    self.modifiers = hotkey.modifiers;
                }
            }
            Err(why) => {
                self.apply_problem = Some(why);
                self.applied = false;
            }
        }
    }
}

/// A titled, rounded panel holding a two-column grid of settings.
fn card(ui: &mut egui::Ui, title: &str, rows: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).heading().strong());
            ui.add_space(2.0);
            egui::Grid::new(title)
                .num_columns(2)
                .spacing([16.0, 10.0])
                .min_col_width(150.0)
                .show(ui, rows);
        });
}

/// A setting's name, followed by an info icon. Hovering either, or tabbing
/// to the icon, shows `help`.
fn setting(ui: &mut egui::Ui, name: &str, help: &str) {
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(name);
        info_icon(ui)
    });
    if row.inner.has_focus() {
        row.inner.show_tooltip_text(help);
    }
    row.response.on_hover_text(help);
}

/// A small circled "i".
fn info_icon(ui: &mut egui::Ui) -> egui::Response {
    let size = 15.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(size, size),
        egui::Sense::focusable_noninteractive(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, "More information"));
    let colour = if response.hovered() || response.has_focus() {
        ACCENT
    } else {
        ui.visuals().weak_text_color()
    };
    let painter = ui.painter();
    painter.circle_stroke(
        rect.center(),
        size / 2.0 - 0.75,
        egui::Stroke::new(1.25, colour),
    );
    painter.text(
        rect.center() + egui::vec2(0.0, 0.5),
        egui::Align2::CENTER_CENTER,
        "i",
        egui::FontId::proportional(10.5),
        colour,
    );
    response
}

/// An on/off switch in place of a checkbox.
fn toggle(ui: &mut egui::Ui, on: &mut bool, name: &str) -> egui::Response {
    let size = egui::vec2(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, ui.is_enabled(), *on, name));
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let visuals = ui.style().interact_selectable(&response, *on);
    let radius = rect.height() / 2.0;
    let track = if *on { ACCENT } else { visuals.bg_fill };
    let painter = ui.painter();
    painter.rect(
        rect,
        radius,
        track,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    painter.circle_filled(
        egui::pos2(x, rect.center().y),
        radius - 3.0,
        egui::Color32::WHITE,
    );
    response
}

/// One message between the settings and the buttons, on a tint of its
/// colour.
fn notice(ui: &mut egui::Ui, colour: egui::Color32, text: &str) {
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.1))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.colored_label(colour, text);
        });
}
