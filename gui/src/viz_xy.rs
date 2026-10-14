//! Four parameter snapshots; the live morph never writes the authored look.
use crate::{show::ShowMode, viz_settings::VizSettings};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Corner {
    pub name: String,
    pub values: BTreeMap<String, f32>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Xy {
    pub enabled: bool,
    pub scene: String,
    pub corners: [Option<Corner>; 4],
    pub position: [f32; 2],
    pub home: [f32; 2],
    pub spring: bool,
    pub glide: f32,
}
impl Default for Xy {
    fn default() -> Self {
        Self {
            enabled: false,
            scene: String::new(),
            corners: Default::default(),
            position: [0.0, 0.0],
            home: [0.0, 0.0],
            spring: false,
            glide: 0.3,
        }
    }
}
impl Xy {
    pub fn sanitize(&mut self) {
        for p in self.position.iter_mut().chain(self.home.iter_mut()) {
            *p = crate::viz_performance::finite(*p, 0.0, 1.0, 0.0);
        }
        self.glide = crate::viz_performance::finite(self.glide, 0.02, 3.0, 0.3);
        for corner in self.corners.iter_mut().flatten() {
            corner.values.retain(|key, v| {
                if let Some((lo, hi)) = crate::viz_motion::bounds(key) {
                    *v = crate::viz_performance::finite(*v, lo, hi, lo);
                    true
                } else {
                    false
                }
            });
        }
    }
    pub fn capture(mode: ShowMode, c: &VizSettings, name: String) -> Corner {
        Corner {
            name,
            values: crate::patch_bay::targets(mode, c)
                .into_iter()
                .filter(|(k, _, _)| {
                    !k.starts_with("xy_")
                        && k != "crossfade"
                        && k != "flash_amount"
                        && !k.starts_with("pad:")
                })
                .map(|(k, _, v)| (k, v))
                .collect(),
        }
    }
    pub fn weights(p: [f32; 2]) -> [f32; 4] {
        let [x, y] = p.map(|v| v.clamp(0.0, 1.0));
        [(1.0 - x) * (1.0 - y), x * (1.0 - y), (1.0 - x) * y, x * y]
    }
    pub fn values(&self, mode: ShowMode, p: [f32; 2]) -> BTreeMap<String, f32> {
        if !self.enabled || self.scene != mode.as_str() || self.corners.iter().any(Option::is_none)
        {
            return BTreeMap::new();
        }
        let weights = Self::weights(p);
        let mut out = BTreeMap::new();
        let mut totals = BTreeMap::new();
        for (i, c) in self.corners.iter().enumerate() {
            for (key, value) in &c.as_ref().unwrap().values {
                *out.entry(key.clone()).or_insert(0.0) += value * weights[i];
                *totals.entry(key.clone()).or_insert(0.0) += weights[i];
            }
        }
        out.retain(|k, v| {
            let w = totals[k];
            if w > 0.00001 {
                *v /= w;
                true
            } else {
                false
            }
        });
        out
    }
}
#[derive(Default)]
pub struct Runtime {
    pub position: [f32; 2],
    pub display_position: [f32; 2],
    pub held: bool,
    initialized: bool,
}
impl Runtime {
    pub fn tick(&mut self, c: &Xy, dt: f32) {
        if !self.initialized {
            self.position = c.position;
            self.initialized = true;
        }
        let target = if c.spring && !self.held {
            c.home
        } else {
            c.position
        };
        let alpha = if self.held {
            1.0
        } else {
            1.0 - (-dt.clamp(0.0, 0.1) / c.glide.max(0.02)).exp()
        };
        for (i, v) in self.position.iter_mut().enumerate() {
            *v += (target[i] - *v) * alpha;
        }
    }
    pub fn apply(&self, mode: ShowMode, c: &mut VizSettings) {
        for (key, v) in c.xy.values(mode, self.display_position) {
            match key.as_str() {
                "speed" => c.speed = v,
                "energy" => c.energy = v,
                "brightness" => c.brightness = v,
                "hue" => c.hue = v,
                "saturation" => c.saturation = v,
                "zoom" => c.transform[0] = v,
                "rotation" => c.transform[1] = v,
                "pan_x" => c.transform[2] = v,
                "pan_y" => c.transform[3] = v,
                _ => {
                    if let Some(bit) = key
                        .strip_prefix("effect:")
                        .and_then(|s| s.parse::<u32>().ok())
                    {
                        if let Some(s) = c.effects.iter_mut().find(|s| s.effect.bit() == bit) {
                            s.mix = v;
                        }
                    } else if let Some(k) = key.strip_prefix("scene:") {
                        if let Some((id, i)) = k.rsplit_once(':') {
                            if let Ok(i) = i.parse::<usize>() {
                                if i < 4 {
                                    c.scene_controls.entry(id.into()).or_insert([1.0; 4])[i] = v;
                                }
                            }
                        }
                    } else if let Some(k) = key.strip_prefix("detail:") {
                        if let Some((id, i)) = k.rsplit_once(':') {
                            if let Ok(i) = i.parse::<usize>() {
                                if i < 4 {
                                    c.scene_details.entry(id.into()).or_insert([1.0; 4])[i] = v;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
pub fn panel(ui: &mut egui::Ui, mode: ShowMode, c: &mut VizSettings, rt: &mut Runtime) {
    ui.strong("XY performance pad");
    ui.small("Four parameter snapshots of one scene. Capture your corners, then drag between them. Signal routes stay active on top.");
    ui.checkbox(&mut c.xy.enabled, "Enable XY morph");
    if ui.button("Seed four corners from current look").clicked() {
        let base = Xy::capture(mode, c, "Home".into());
        c.xy.scene = mode.as_str().into();
        c.xy.corners = std::array::from_fn(|i| {
            let mut p = base.clone();
            p.name = ["Home", "Flow", "Tangle", "Burst"][i].into();
            if i > 0 {
                for (key, value) in &mut p.values {
                    if let Some((lo, hi)) = crate::viz_motion::bounds(key) {
                        let delta = match i {
                            1 => 0.12,
                            2 => -0.15,
                            _ => 0.3,
                        };
                        if key.starts_with("scene:") || key == "speed" || key == "hue" {
                            *value = (*value + (hi - lo) * delta).clamp(lo, hi);
                        }
                    }
                }
            }
            Some(p)
        });
        c.xy.enabled = true;
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                c.xy.corners.iter().all(Option::is_some) && c.xy.scene == mode.as_str(),
                egui::Button::new("Audio pilot"),
            )
            .on_hover_text(
                "Bass + beats move X; stereo width moves Y. Tune these connections in Patch bay.",
            )
            .clicked()
        {
            use crate::viz_motion::{Combine, Route, Source};
            c.xy.enabled = true;
            c.xy.position = [0.0, 0.0];
            c.xy.home = [0.0, 0.0];
            c.motion.routes.insert(
                "xy_x".into(),
                Route {
                    source: Source::Bass,
                    secondary: Source::Beat,
                    combine: Combine::Mix,
                    weight: 0.7,
                    depth: 1.0,
                    smoothing: 0.25,
                    ..Default::default()
                },
            );
            c.motion.routes.insert(
                "xy_y".into(),
                Route {
                    source: Source::Width,
                    depth: 1.0,
                    smoothing: 0.5,
                    ..Default::default()
                },
            );
        }
        if ui
            .small_button("Manual XY")
            .on_hover_text("Disconnect signal routes from the two XY axes.")
            .clicked()
        {
            c.xy.position = rt.display_position;
            rt.position = rt.display_position;
            c.motion.routes.remove("xy_x");
            c.motion.routes.remove("xy_y");
        }
    });
    if !c.xy.scene.is_empty() {
        ui.small(format!(
            "Snapshots: {}",
            ShowMode::from_str(&c.xy.scene).label()
        ));
        if c.xy.scene != mode.as_str() {
            ui.label("Switch to the snapshot scene or seed a new pad.");
        }
    }
    let side = ui.available_width().min(280.0);
    let (r, response) =
        ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click_and_drag());
    let pad = r.shrink(12.0);
    rt.held = response.is_pointer_button_down_on();
    if rt.held {
        if let Some(p) = response.interact_pointer_pos() {
            c.xy.position = [
                ((p.x - pad.left()) / pad.width()).clamp(0.0, 1.0),
                ((p.y - pad.top()) / pad.height()).clamp(0.0, 1.0),
            ];
        }
    }
    let p = ui.painter();
    p.rect_filled(r, 6, ui.visuals().extreme_bg_color);
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        p.line_segment(
            [
                egui::pos2(pad.left() + t * pad.width(), pad.top()),
                egui::pos2(pad.left() + t * pad.width(), pad.bottom()),
            ],
            egui::Stroke::new(0.5, ui.visuals().weak_text_color().gamma_multiply(0.3)),
        );
        p.line_segment(
            [
                egui::pos2(pad.left(), pad.top() + t * pad.height()),
                egui::pos2(pad.right(), pad.top() + t * pad.height()),
            ],
            egui::Stroke::new(0.5, ui.visuals().weak_text_color().gamma_multiply(0.3)),
        );
    }
    for (i, point) in [
        pad.left_top(),
        pad.right_top(),
        pad.left_bottom(),
        pad.right_bottom(),
    ]
    .into_iter()
    .enumerate()
    {
        p.circle_filled(point, 4.0, crate::theme::cyan());
        p.text(
            point
                + egui::vec2(
                    if i % 2 == 0 { 8.0 } else { -8.0 },
                    if i < 2 { 10.0 } else { -10.0 },
                ),
            if i % 2 == 0 {
                egui::Align2::LEFT_CENTER
            } else {
                egui::Align2::RIGHT_CENTER
            },
            format!("{}", i + 1),
            egui::FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
    }
    let cursor = pad.min
        + egui::vec2(
            rt.display_position[0] * pad.width(),
            rt.display_position[1] * pad.height(),
        );
    p.circle_filled(cursor, 8.0, crate::theme::cyan().gamma_multiply(0.3));
    p.circle_stroke(cursor, 5.0, egui::Stroke::new(2.0, crate::theme::cyan()));
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("Set home here").clicked() {
            c.xy.home = c.xy.position;
        }
        if ui.small_button("Go home").clicked() {
            c.xy.position = c.xy.home;
        }
        ui.checkbox(&mut c.xy.spring, "Glide home on release");
    });
    ui.add(
        egui::Slider::new(&mut c.xy.glide, 0.02..=3.0)
            .text("Glide")
            .suffix(" s"),
    );
    for i in 0..4 {
        ui.push_id(i, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{}.", i + 1));
                let mut name = c.xy.corners[i]
                    .as_ref()
                    .map_or_else(|| format!("Corner {}", i + 1), |p| p.name.clone());
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut name)
                            .desired_width(110.0)
                            .char_limit(40),
                    )
                    .changed()
                {
                    if let Some(p) = &mut c.xy.corners[i] {
                        p.name = name.clone();
                    }
                }
                if ui
                    .add_enabled(
                        c.xy.scene.is_empty() || c.xy.scene == mode.as_str(),
                        egui::Button::new("Capture base").small(),
                    )
                    .clicked()
                {
                    if c.xy.scene.is_empty() {
                        c.xy.scene = mode.as_str().into();
                    }
                    if c.xy.scene == mode.as_str() {
                        c.xy.corners[i] = Some(Xy::capture(mode, c, name));
                    }
                }
            });
        });
    }
    ui.small(
        "Capture base stores the sliders you authored, without the current morph or signal motion.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bilinear_corners_and_missing_keys() {
        let mut xy = Xy {
            enabled: true,
            scene: "gl_warp".into(),
            ..Default::default()
        };
        xy.corners = std::array::from_fn(|i| {
            Some(Corner {
                name: i.to_string(),
                values: BTreeMap::from([("speed".into(), i as f32)]),
            })
        });
        assert_eq!(xy.values(ShowMode::GlWarp, [0., 0.])["speed"], 0.);
        assert_eq!(xy.values(ShowMode::GlWarp, [1., 1.])["speed"], 3.);
        assert_eq!(xy.values(ShowMode::GlWarp, [0.5, 0.5])["speed"], 1.5);
        xy.corners[0]
            .as_mut()
            .unwrap()
            .values
            .insert("hue".into(), 0.8);
        assert_eq!(xy.values(ShowMode::GlWarp, [0.5, 0.5])["hue"], 0.8);
        assert!(xy.values(ShowMode::GlFlame, [0.5, 0.5]).is_empty());
    }
    #[test]
    fn glide_is_time_based_and_returns_home() {
        let c = Xy {
            spring: true,
            position: [1., 1.],
            home: [0., 0.],
            glide: 0.3,
            ..Default::default()
        };
        let mut results = vec![];
        for fps in [20, 60, 144] {
            let mut r = Runtime::default();
            for _ in 0..fps {
                r.tick(&c, 1. / fps as f32);
            }
            results.push(r.position[0]);
        }
        for v in results {
            assert!((v - (-1.0f32 / 0.3).exp()).abs() < 0.00001);
        }
    }
    #[test]
    fn snapshot_is_preserved_in_saved_look() {
        let mut c = VizSettings::default();
        c.xy.enabled = true;
        c.xy.scene = "gl_warp".into();
        c.xy.corners =
            std::array::from_fn(|i| Some(Xy::capture(ShowMode::GlWarp, &c, i.to_string())));
        let look = crate::viz_looks::Look::capture("XY", ShowMode::GlWarp, &c);
        let json = serde_json::to_string(&look).unwrap();
        let loaded: crate::viz_looks::Look = serde_json::from_str(&json).unwrap();
        let mut restored = VizSettings::default();
        loaded.apply(&mut ShowMode::GlWarp, &mut restored);
        assert!(restored.xy == c.xy);
    }
    #[test]
    fn live_morph_applies_routes_after_snapshot_without_editing_the_look() {
        let mut c = VizSettings::default();
        c.xy.enabled = true;
        c.xy.scene = "gl_warp".into();
        c.xy.position = [0.5, 0.5];
        c.xy.corners = std::array::from_fn(|i| {
            Some(Corner {
                name: i.to_string(),
                values: BTreeMap::from([("speed".into(), i as f32 * 0.4)]),
            })
        });
        c.motion.routes.insert(
            "speed".into(),
            crate::viz_motion::Route {
                source: crate::viz_motion::Source::Bass,
                depth: 0.1,
                smoothing: 0.0,
                ..Default::default()
            },
        );
        let before = serde_json::to_string(&c).unwrap();
        let mut frame = crate::audio_analysis::Analyzer::new(48000, 2).snapshot();
        frame.bus.bass = 1.0;
        let u = crate::viz_performance::uniforms(
            &c,
            ShowMode::GlWarp,
            &frame,
            &mut Default::default(),
            &mut Default::default(),
            1.0 / 60.0,
            0,
        );
        assert!((u.speed - 0.8).abs() < 0.00001);
        assert_eq!(before, serde_json::to_string(&c).unwrap());
    }
    #[test]
    fn signal_axes_move_through_snapshots_before_parameter_routes() {
        let mut c = VizSettings::default();
        c.xy.enabled = true;
        c.xy.scene = "gl_warp".into();
        c.xy.corners = std::array::from_fn(|i| {
            Some(Corner {
                name: i.to_string(),
                values: BTreeMap::from([("speed".into(), i as f32 * 0.4)]),
            })
        });
        for (key, source) in [
            ("xy_x", crate::viz_motion::Source::Beat),
            ("xy_y", crate::viz_motion::Source::Width),
        ] {
            c.motion.routes.insert(
                key.into(),
                crate::viz_motion::Route {
                    source,
                    depth: 1.0,
                    smoothing: 0.0,
                    ..Default::default()
                },
            );
        }
        let mut frame = crate::audio_analysis::Analyzer::new(48000, 2).snapshot();
        frame.bus.pulse = 0.25;
        frame.bus.width = 0.75;
        let mut runtime = crate::viz_motion::Runtime::default();
        let u = crate::viz_performance::uniforms(
            &c,
            ShowMode::GlWarp,
            &frame,
            &mut runtime,
            &mut Default::default(),
            1.0 / 60.0,
            0,
        );
        assert_eq!(runtime.xy.display_position, [0.25, 0.75]);
        assert!((u.speed - 0.7).abs() < 0.00001);
        assert_eq!(c.xy.position, [0.0, 0.0]);
    }
}
