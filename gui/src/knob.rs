//! Painter-drawn rotary knob — the "audio gear" control. Vertical drag to turn
//! (Shift = fine), scroll wheel to nudge, optional log taper. Dims when the
//! surrounding `ui` is disabled. Returns true when the value changed this frame.

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use std::ops::RangeInclusive;

use crate::theme;

const W: f32 = 58.0;
const H: f32 = 70.0;
const R: f32 = 18.0; // ring radius
const SWEEP: f32 = 135.0; // degrees either side of straight-up (270 total)

#[allow(clippy::too_many_arguments)]
pub fn knob(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    label: &str,
    suffix: &str,
    decimals: usize,
    log: bool,
    accent: Color32,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(W, H), Sense::click_and_drag());
    let enabled = ui.is_enabled();
    let (min, max) = (*range.start(), *range.end());
    let span = (max - min).max(1e-9);
    let log = log && min > 0.0;

    // value <-> normalized t (0..1), with optional log taper.
    let to_t = |v: f32| {
        if log {
            (v / min).ln() / (max / min).ln()
        } else {
            (v - min) / span
        }
    };
    let from_t = |t: f32| {
        if log {
            min * (max / min).powf(t)
        } else {
            min + t * span
        }
    };

    let mut t = to_t(*value).clamp(0.0, 1.0);
    let mut changed = false;
    if enabled {
        if resp.dragged() {
            let px = if ui.input(|i| i.modifiers.shift) { 600.0 } else { 170.0 };
            t = (t - resp.drag_delta().y / px).clamp(0.0, 1.0);
            *value = from_t(t);
            changed = true;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                t = (t + scroll.signum() * 0.03).clamp(0.0, 1.0);
                *value = from_t(t);
                changed = true;
            }
        }
    }

    let center = Pos2::new(rect.center().x, rect.top() + R + 4.0);
    let a_min = (-SWEEP).to_radians();
    let a_val = (-SWEEP + t * 2.0 * SWEEP).to_radians();
    let a_max = SWEEP.to_radians();

    let fade = |c: Color32, a: u8| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
    let acc = if enabled { accent } else { theme::muted() };
    let painter = ui.painter_at(rect);

    // angle is clockwise from straight up; screen y points down.
    let dir = |a: f32| Vec2::new(a.sin(), -a.cos());
    let arc = |from: f32, to: f32, rad: f32, w: f32, col: Color32| {
        const STEPS: usize = 26;
        let mut prev = center + dir(from) * rad;
        for s in 1..=STEPS {
            let a = from + (to - from) * (s as f32 / STEPS as f32);
            let cur = center + dir(a) * rad;
            painter.line_segment([prev, cur], Stroke::new(w, col));
            prev = cur;
        }
    };

    arc(a_min, a_max, R, 3.0, fade(theme::muted(), 70)); // background track
    if enabled {
        arc(a_min, a_val, R, 8.0, fade(acc, 38)); // glow
    }
    arc(a_min, a_val, R, 3.0, acc); // value arc

    // knob body + pointer
    painter.circle_filled(center, R * 0.64, theme::panel2());
    painter.circle_stroke(center, R * 0.64, Stroke::new(1.0, fade(acc, 130)));
    let p_out = center + dir(a_val) * (R * 0.58);
    let p_in = center + dir(a_val) * (R * 0.20);
    let ptr = if enabled { Color32::WHITE } else { theme::muted() };
    painter.line_segment([p_in, p_out], Stroke::new(2.5, ptr));

    // value + label text
    painter.text(
        Pos2::new(center.x, center.y + R * 0.66 + 3.0),
        egui::Align2::CENTER_TOP,
        format!("{:.*}{}", decimals, *value, suffix),
        egui::FontId::proportional(11.0),
        if enabled { theme::text() } else { theme::muted() },
    );
    painter.text(
        Pos2::new(center.x, rect.bottom() - 1.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.0),
        theme::muted(),
    );

    let _ = resp.on_hover_text(format!("{}: {:.*}{}", label, decimals.max(2), *value, suffix));
    changed
}
