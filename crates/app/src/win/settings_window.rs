//! The settings window (R1, R14, R15, Q8): an eframe window opened on demand
//! on the main thread. While it is open, eframe's event loop pumps the main
//! thread's messages, so the tray keeps working; the window polls for tray
//! clicks and the clicker's reports as the main loop does.

use std::time::Duration;

use eframe::egui;
use saw_core::{BUTTERFLY_CPS, CPS_RANGE, ClickButtons, Hotkey, Key, Modifiers};

use crate::form::{self, Draft};
use crate::hotkey_text::{self, KEYS, key_name};
use crate::icon;
use crate::status::hotkey_missing;
use crate::win::app::{App, Next};
use crate::win::butterfly;

/// How often the open window checks for tray clicks and the clicker's
/// reports.
const POLL: Duration = Duration::from_millis(100);
/// How long the hotkey controls must sit untouched before the hotkey they
/// show is registered, so the combinations passed through while ticking
/// modifiers are never registered themselves.
const HOTKEY_SETTLE: f64 = 1.5;

/// The icon's hub orange: its blade orange is too light to carry white text.
const ACCENT: egui::Color32 = egui::Color32::from_rgb(
    icon::ENABLED_HUB[0],
    icon::ENABLED_HUB[1],
    icon::ENABLED_HUB[2],
);

const WIDTH: f32 = 540.0;
const MARGIN: i8 = 16;

const CPS_HELP: &str = "The target clicks per second when auto-clicking.";
const DELAY_HELP: &str =
    "The amount of time you must hold down a click before auto-clicking starts.";
const JITTER_HELP: &str = "Adds natural variation to the click rate.";
const CLICKS_HELP: &str = "Which mouse buttons auto-click when held down. A \
button that is left out always clicks normally.";
const HOTKEY_HELP: &str = "An optional keyboard shortcut that enables and disables auto-clicking.";
const ENABLE_ON_STARTUP_HELP: &str = "By default, the app starts disabled to prevent \
accidental auto-clicking. Select this if you want the auto-clicker to \
automatically enable itself on startup.";

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
    let enable_on_startup = app.enable_on_startup;
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
                saved: draft,
                hotkey_edited: None,
                modifiers,
                enable_on_startup,
                hotkey_unusable: None,
                flap_started: None,
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
    /// What the controls show. Its hotkey is `None` while no key is chosen.
    draft: Draft,
    /// The draft last put into effect; any difference from it is saved.
    saved: Draft,
    /// When the hotkey controls last changed, as egui input time.
    hotkey_edited: Option<f64>,
    /// The hotkey's modifiers as ticked, kept while no key is chosen.
    modifiers: Modifiers,
    enable_on_startup: bool,
    /// Why the hotkey the controls show can't be used, while the one before
    /// it stays in effect.
    hotkey_unusable: Option<String>,
    flap_started: Option<f64>,
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
                    self.notices(ui);
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Changes are saved as you make them.")
                            .small()
                            .weak(),
                    );
                })
                .response
                .rect
                .bottom()
            })
            .inner;
        self.save_changes(ui.ctx());
        self.fit_height(ui.ctx(), bottom + f32::from(MARGIN));
        self.frames += 1;
        if self.close_after_frames.is_some_and(|n| self.frames >= n) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Saves what a drag or the hotkey's settling time was still holding
    /// back.
    fn on_exit(&mut self) {
        if self.draft != self.saved || self.enable_on_startup != self.app.enable_on_startup {
            self.save(self.draft);
        }
    }
}

impl Window<'_> {
    fn settings(&mut self, ui: &mut egui::Ui) {
        let delay_ms = form::delay_ms_range();
        let d = &mut self.draft;
        card(ui, "Clicking", |ui| {
            setting(ui, "Clicks per second", CPS_HELP);
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut d.cps, CPS_RANGE));
                let on = d.butterfly();
                let butterfly = butterfly::button(ui, on, &mut self.flap_started, ACCENT);
                show_help(&butterfly, &butterfly, &butterfly_help(on));
                if butterfly.clicked() && !on {
                    d.set_butterfly();
                }
            });
            ui.end_row();
            setting(ui, "Activation delay", DELAY_HELP);
            ui.add(egui::Slider::new(&mut d.delay_ms, delay_ms).suffix(" ms"));
            ui.end_row();
            setting(ui, "Jitter", JITTER_HELP);
            toggle(ui, &mut d.jitter, "Jitter");
            ui.end_row();
            setting(ui, "Clicks", CLICKS_HELP);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (choice, name) in [
                    (ClickButtons::Left, "Left only"),
                    (ClickButtons::Both, "Both"),
                    (ClickButtons::Right, "Right only"),
                ] {
                    let button = egui::Button::new(name)
                        .selected(d.buttons == choice)
                        .min_size(egui::vec2(84.0, 0.0));
                    if ui.add(button).clicked() {
                        d.buttons = choice;
                    }
                }
            });
            ui.end_row();
        });
        ui.add_space(4.0);
        card(ui, "Enabling and disabling", |ui| {
            setting(ui, "Toggle hotkey", HOTKEY_HELP);
            let before = d.hotkey;
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
            if d.hotkey != before {
                self.hotkey_edited = Some(ui.input(|i| i.time));
            }
            ui.end_row();
            setting(ui, "Enable on startup", ENABLE_ON_STARTUP_HELP);
            toggle(ui, &mut self.enable_on_startup, "Enable on startup");
            ui.end_row();
        });
    }

    fn notices(&mut self, ui: &mut egui::Ui) {
        let error = ui.visuals().error_fg_color;
        let warning = ui.visuals().warn_fg_color;
        if let Some(problem) = &self.hotkey_unusable {
            notice(ui, error, problem);
        }
        let mut retry = false;
        if let Some(problem) = &self.app.hotkey_problem {
            notice(ui, error, problem);
            // A hotkey refused at startup has no change of the user's to
            // retry it.
            retry = hotkey_missing(&self.app.settings, self.app.active_hotkey)
                && ui.button("Try the hotkey again").clicked();
        }
        if retry {
            self.save(self.saved);
        }
        if let Some(problem) = &self.app.save_problem {
            notice(ui, error, problem);
        }
        if let Some(problem) = &self.app.config_problem {
            notice(ui, warning, problem);
        }
    }

    /// Saves whatever the controls changed. A slider's change is saved once
    /// it is let go, not at every step of the drag, and a hotkey once its
    /// controls have settled.
    fn save_changes(&mut self, ctx: &egui::Context) {
        let mut ready = self.draft;
        if let Some(edited) = self.hotkey_edited {
            let waited = ctx.input(|i| i.time) - edited;
            if waited < HOTKEY_SETTLE {
                ready.hotkey = self.saved.hotkey;
                ctx.request_repaint_after(Duration::from_secs_f64(HOTKEY_SETTLE - waited));
            } else {
                self.hotkey_edited = None;
            }
        }
        let changed = ready != self.saved || self.enable_on_startup != self.app.enable_on_startup;
        if changed && ctx.dragged_id().is_none() {
            self.save(ready);
            // The notices above are already drawn for this frame.
            ctx.request_repaint();
        }
    }

    /// Grows or shrinks the window to `height`, so a message never needs
    /// scrolling to and no space sits empty below the settings.
    fn fit_height(&mut self, ctx: &egui::Context, height: f32) {
        let current = ctx.input(|i| i.viewport().inner_rect.map(|r| r.height()));
        let off = |h: Option<f32>| h.is_none_or(|h| (h - height).abs() > 1.0);
        if off(current) && off(self.requested_height) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(WIDTH, height)));
            self.requested_height = Some(height);
        }
    }

    /// Puts `draft` into effect, keeping the hotkey in effect when the
    /// chosen one can't be used. A refused hotkey stays in the controls, with
    /// the notice naming the one still in effect, so the user can carry on
    /// editing it.
    fn save(&mut self, draft: Draft) {
        let hotkey = match draft.hotkey.map(hotkey_text::usable).transpose() {
            Ok(hotkey) => {
                self.hotkey_unusable = None;
                hotkey
            }
            Err(why) => {
                self.hotkey_unusable = Some(why);
                self.app.settings.hotkey()
            }
        };
        let requested = Draft { hotkey, ..draft }
            .to_settings()
            .expect("the controls keep every value in range");
        self.app.apply_settings(requested, self.enable_on_startup);
        self.saved = draft;
    }
}

fn butterfly_help(on: bool) -> String {
    let what = format!(
        "Simulates the human-achievable clicks per second from butterfly \
         clicking: {BUTTERFLY_CPS} clicks per second, with Jitter on."
    );
    if on {
        format!("{what}\n\nButterfly clicking is on.")
    } else {
        what
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
    show_help(&row.response, &row.inner, help);
}

/// Shows `help` under `hovered` while the pointer is on it or `focused` has
/// the keyboard focus. Unlike egui's hover text, a click doesn't hide it:
/// the info icon looks clickable, so people click it.
fn show_help(hovered: &egui::Response, focused: &egui::Response, help: &str) {
    if hovered.contains_pointer() || focused.has_focus() {
        hovered.show_tooltip_text(help);
    }
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

/// One message below the settings, on a tint of its colour.
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
