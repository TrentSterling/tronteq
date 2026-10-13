//! A live overview of the same routes used by each slider.
use crate::{
    show::ShowMode,
    viz_motion::{self, Route, Runtime, Source},
    viz_settings::VizSettings,
};
use eframe::egui;

#[derive(Default)]
pub struct Panel {
    selected: String,
    target: String,
}

pub fn targets(mode: ShowMode, c: &VizSettings) -> Vec<(String, String, f32)> {
    let mut out = vec![
        ("xy_x".into(), "XY horizontal".into(), c.xy.position[0]),
        ("xy_y".into(), "XY vertical".into(), c.xy.position[1]),
        ("speed".into(), "Speed".into(), c.speed),
        ("energy".into(), "Audio response".into(), c.energy),
        ("brightness".into(), "Brightness".into(), c.brightness),
        ("hue".into(), "Color shift".into(), c.hue),
        ("saturation".into(), "Saturation".into(), c.saturation),
        (
            "crossfade".into(),
            "Deck crossfade".into(),
            c.performance.crossfade,
        ),
        (
            "flash_amount".into(),
            "Flash strength".into(),
            c.flash.amount,
        ),
    ];
    for (key,label,value) in [
        ("deck_distortion","Deck distortion",c.performance.interaction.amount),
        ("deck_scale","Distortion field scale",c.performance.interaction.scale),
        ("deck_angle","Distortion angle",c.performance.interaction.angle),
        ("deck_softness","Distortion softness",c.performance.interaction.softness),
    ] { out.push((key.into(),label.into(),value)); }
    for (i, (key, label)) in [
        ("zoom", "Zoom"),
        ("rotation", "Rotation"),
        ("pan_x", "Horizontal"),
        ("pan_y", "Vertical"),
    ]
    .into_iter()
    .enumerate()
    {
        out.push((key.into(), label.into(), c.transform[i]));
    }
    if let Some(labels) = crate::viz_panel::scene_control_labels(mode) {
        let v = c
            .scene_controls
            .get(mode.as_str())
            .copied()
            .unwrap_or([1.0; 4]);
        for (i, label) in labels.into_iter().enumerate() {
            out.push((
                viz_motion::scene_key(mode.as_str(), i),
                format!("{}: {label}", mode.label()),
                v[i],
            ));
        }
    }
    if mode == ShowMode::GlFlame || mode == ShowMode::StereoScope {
        let values = c
            .scene_details
            .get(mode.as_str())
            .copied()
            .unwrap_or([1.0; 4]);
        for (i, v) in values.into_iter().enumerate() {
            out.push((
                viz_motion::detail_key(mode.as_str(), i),
                format!("{} detail {}", mode.label(), i + 1),
                v,
            ));
        }
    }
    for s in &c.effects {
        out.push((
            viz_motion::effect_key(s.effect.bit()),
            s.effect.label().into(),
            s.mix,
        ));
    }
    for (i, label) in crate::viz_performance::PADS.into_iter().enumerate() {
        out.push((format!("pad:{i}"), format!("Pad: {label}"), 0.0));
    }
    out
}
impl Panel {
    pub fn show(&mut self, ui: &mut egui::Ui, mode: ShowMode, c: &mut VizSettings, live: &Runtime) {
        ui.strong("Patch bay");
        ui.small("Connect one or two signals to a control. Click a cable to edit its response.");
        let targets = targets(mode, c);
        if self.target.is_empty() {
            self.target = "zoom".into();
        }
        egui::ComboBox::from_id_salt("new-route-target")
            .selected_text(
                targets
                    .iter()
                    .find(|(k, _, _)| k == &self.target)
                    .map_or(self.target.as_str(), |(_, n, _)| n),
            )
            .show_ui(ui, |ui| {
                for (key, label, _) in &targets {
                    ui.selectable_value(&mut self.target, key.clone(), label);
                }
            });
        if ui.button("Connect signal").clicked() {
            c.motion.routes.entry(self.target.clone()).or_insert(Route {
                source: Source::Bass,
                ..Default::default()
            });
            self.selected = self.target.clone();
        }
        if !c.motion.routes.contains_key(&self.selected) {
            self.selected = c.motion.routes.keys().next().cloned().unwrap_or_default();
        }
        let mut remove = None;
        for (key, route) in &c.motion.routes {
            let label = targets
                .iter()
                .find(|(k, _, _)| k == key)
                .map_or(key.as_str(), |(_, label, _)| label);
            ui.push_id(key, |ui| {
                let (r, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 76.0),
                    egui::Sense::click(),
                );
                if response.clicked() {
                    self.selected = key.clone();
                }
                let p = ui.painter();
                p.rect_filled(
                    r,
                    4,
                    if self.selected == *key {
                        ui.visuals().selection.bg_fill.gamma_multiply(0.45)
                    } else {
                        ui.visuals().extreme_bg_color
                    },
                );
                let left = r.left() + 10.0;
                let right = r.right() - 10.0;
                let top = r.top() + 14.0;
                p.text(
                    egui::pos2(left, top),
                    egui::Align2::LEFT_CENTER,
                    route.source.label(),
                    egui::FontId::proportional(12.0),
                    ui.visuals().text_color(),
                );
                if route.secondary != Source::Off {
                    p.text(
                        egui::pos2(left, top + 16.0),
                        egui::Align2::LEFT_CENTER,
                        format!("+ {}", route.secondary.label()),
                        egui::FontId::proportional(11.0),
                        ui.visuals().weak_text_color(),
                    );
                }
                p.text(
                    egui::pos2(right, r.bottom() - 12.0),
                    egui::Align2::RIGHT_CENTER,
                    label,
                    egui::FontId::proportional(11.0),
                    ui.visuals().text_color(),
                );
                let a = egui::pos2(left, r.bottom() - 28.0);
                let b = egui::pos2(right, r.bottom() - 28.0);
                let signal = route.shaped(route.input(&live.signals));
                p.line_segment(
                    [a, b],
                    egui::Stroke::new(
                        2.0,
                        crate::theme::cyan().gamma_multiply(0.25 + signal * 0.75),
                    ),
                );
                let travel = (ui.input(|i| i.time) as f32 * 0.6).fract();
                p.circle_filled(a.lerp(b, travel), 2.0 + signal * 2.0, crate::theme::cyan());
                p.text(
                    egui::pos2(right, top),
                    egui::Align2::RIGHT_CENTER,
                    format!("{:.0}%", signal * 100.0),
                    egui::FontId::monospace(11.0),
                    crate::theme::cyan(),
                );
            });
        }
        if c.motion.routes.is_empty() {
            ui.label("No connections yet. Pick a control above.");
        }
        if c.motion.routes.contains_key(&self.selected) {
            ui.separator();
            let label = targets
                .iter()
                .find(|(k, _, _)| k == &self.selected)
                .map_or(self.selected.as_str(), |(_, n, _)| n);
            ui.strong(label);
            let base = targets
                .iter()
                .find(|(k, _, _)| k == &self.selected)
                .map_or(0.0, |(_, _, v)| *v);
            viz_motion::binding_editor(ui, &self.selected, base, &mut c.motion, live, true);
            if ui.small_button("Disconnect").clicked() {
                remove = Some(self.selected.clone());
            }
        }
        if let Some(key) = remove {
            c.motion.routes.remove(&key);
        }
    }
}
