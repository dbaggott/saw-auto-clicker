//! The butterfly preset's button: a monarch butterfly, orange when the preset
//! is on and grey when it is off. A click while it is already on flaps its
//! wings, to show the click landed.

use eframe::egui::{self, Color32, Pos2, Shape, Stroke, Vec2, vec2};

const SIZE: f32 = 30.0;
const FLAP_SECONDS: f64 = 0.5;
const FLAPS: f64 = 2.0;

/// One wing on the right of the body, in units of the half-span, with y
/// downwards. Each outline is convex, as the painter needs.
const FOREWING: [(f32, f32); 7] = [
    (0.08, -0.05),
    (0.32, -0.72),
    (0.72, -0.86),
    (0.98, -0.62),
    (0.9, -0.28),
    (0.55, 0.0),
    (0.12, 0.06),
];
const HINDWING: [(f32, f32); 7] = [
    (0.08, 0.06),
    (0.55, 0.04),
    (0.8, 0.3),
    (0.72, 0.64),
    (0.42, 0.8),
    (0.16, 0.56),
    (0.06, 0.22),
];
/// How far each wing's orange reaches towards its edge; the rest is the
/// black margin.
const INSET: f32 = 0.74;
/// Points on each wing's edge that a vein runs to from the body.
const FOREWING_VEINS: [(f32, f32); 2] = [(0.72, -0.86), (0.9, -0.28)];
const HINDWING_VEINS: [(f32, f32); 2] = [(0.8, 0.3), (0.42, 0.8)];
/// White spots in the black margin.
const SPOTS: [(f32, f32); 4] = [(0.86, -0.7), (0.95, -0.47), (0.74, 0.5), (0.55, 0.7)];

struct Colours {
    wing: Color32,
    margin: Color32,
    spot: Color32,
}

/// Shows the button and says whether it was clicked. `flap_started` holds
/// when the last flap began, as egui input time.
pub fn button(
    ui: &mut egui::Ui,
    on: bool,
    flap_started: &mut Option<f64>,
    accent: Color32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(SIZE), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), on, "Butterfly")
    });
    let now = ui.input(|i| i.time);
    if response.clicked() && on {
        *flap_started = Some(now);
    }
    let fold = match *flap_started {
        Some(start) if now - start < FLAP_SECONDS => {
            ui.ctx().request_repaint();
            let phase = (now - start) / FLAP_SECONDS * FLAPS * std::f64::consts::TAU;
            (0.5 - 0.5 * phase.cos()) as f32
        }
        _ => {
            *flap_started = None;
            0.0
        }
    };
    let colours = if on {
        Colours {
            wing: accent,
            margin: Color32::from_gray(20),
            spot: Color32::WHITE,
        }
    } else {
        let dark = ui.visuals().dark_mode;
        Colours {
            wing: Color32::from_gray(if dark { 120 } else { 175 }),
            margin: Color32::from_gray(if dark { 70 } else { 110 }),
            spot: Color32::from_gray(if dark { 150 } else { 225 }),
        }
    };
    let background = if response.hovered() || response.has_focus() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, background);
    paint(ui.painter(), rect.center(), SIZE * 0.45, fold, &colours);
    response
}

/// Paints the butterfly centred on `centre`. `fold` from 0 to 1 narrows the
/// wings towards the body, as in a flap.
fn paint(painter: &egui::Painter, centre: Pos2, half_span: f32, fold: f32, c: &Colours) {
    let width = 1.0 - 0.75 * fold;
    for side in [-1.0, 1.0] {
        let at = |(x, y): (f32, f32)| centre + vec2(side * x * width, y) * half_span;
        for (outline, veins) in [(&FOREWING, &FOREWING_VEINS), (&HINDWING, &HINDWING_VEINS)] {
            let edge: Vec<Pos2> = outline.iter().map(|&p| at(p)).collect();
            let middle =
                edge.iter().fold(Vec2::ZERO, |sum, p| sum + p.to_vec2()) / edge.len() as f32;
            let inner: Vec<Pos2> = edge
                .iter()
                .map(|p| (middle + (p.to_vec2() - middle) * INSET).to_pos2())
                .collect();
            painter.add(Shape::convex_polygon(edge, c.margin, Stroke::NONE));
            painter.add(Shape::convex_polygon(inner, c.wing, Stroke::NONE));
            for &vein in veins {
                painter.line_segment([at((0.06, 0.0)), at(vein)], Stroke::new(0.8, c.margin));
            }
        }
        for &spot in &SPOTS {
            painter.circle_filled(at(spot), 0.9, c.spot);
        }
        let antenna_tip = at((0.28, -0.98));
        painter.line_segment(
            [centre + vec2(0.0, -0.3) * half_span, antenna_tip],
            Stroke::new(0.9, c.margin),
        );
        painter.circle_filled(antenna_tip, 1.1, c.margin);
    }
    painter.add(Shape::ellipse_filled(
        centre + vec2(0.0, 0.1) * half_span,
        vec2(0.09, 0.62) * half_span,
        c.margin,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Twice the signed area of `points`, positive when they turn clockwise
    /// on screen (y down).
    fn turns(points: &[(f32, f32)]) -> Vec<f32> {
        (0..points.len())
            .map(|i| {
                let (a, b, c) = (
                    points[i],
                    points[(i + 1) % points.len()],
                    points[(i + 2) % points.len()],
                );
                (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0)
            })
            .collect()
    }

    #[test]
    fn each_wing_outline_is_convex() {
        for outline in [FOREWING, HINDWING] {
            let turns = turns(&outline);
            assert!(
                turns.iter().all(|&t| t > 0.0) || turns.iter().all(|&t| t < 0.0),
                "{outline:?} turns {turns:?}"
            );
        }
    }
}
