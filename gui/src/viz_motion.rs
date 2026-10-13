//! Audio modulation and a performance macro. Authored settings never contain
//! frame values, so playback cannot dirty saved looks or flood undo history.
use crate::{viz_settings::VizSettings, vizbus::VizBus};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Off,
    Bass,
    Mids,
    Highs,
    Beat,
    Change,
    Width,
    Brightness,
    Level,
    Crest,
    Correlation,
    SubBass, Kick, LowOnset, MidOnset, HighOnset, Balance, Tonal, Noisy,
}
impl Source {
    pub const ALL: [Self; 19] = [Self::Off, Self::Bass, Self::Mids, Self::Highs,
        Self::Beat, Self::Change, Self::Width, Self::Brightness, Self::Level,
        Self::Crest, Self::Correlation, Self::SubBass, Self::Kick, Self::LowOnset, Self::MidOnset, Self::HighOnset, Self::Balance, Self::Tonal, Self::Noisy];
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off", Self::Bass => "Bass", Self::Mids => "Mids",
            Self::Highs => "Highs", Self::Beat => "Beat", Self::Change => "Spectral change",
            Self::Width => "Stereo width", Self::Brightness => "Spectral brightness",
            Self::Level => "Level", Self::Crest => "Crest", Self::Correlation => "Correlation",
            Self::SubBass => "Sub-bass", Self::Kick => "Kick onset", Self::LowOnset => "Low transient", Self::MidOnset => "Mid transient", Self::HighOnset => "High transient", Self::Balance => "Stereo balance", Self::Tonal => "Tonal character", Self::Noisy => "Noisy character",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Route {
    pub source: Source,
    pub secondary: Source,
    pub combine: Combine,
    pub weight: f32,
    pub curve: [f32;5],
    pub depth: f32,
    pub smoothing: f32,
    pub invert: bool,
}
impl Default for Route {
    fn default() -> Self { Self { source: Source::Off, secondary:Source::Off, combine:Combine::Mix, weight:0.5, curve:[0.0,0.25,0.5,0.75,1.0], depth: 0.25, smoothing: 0.12, invert: false } }
}

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
pub enum Combine { #[default] Mix, Add, Multiply, Maximum, Difference }
impl Combine {
    pub const ALL:[Self;5]=[Self::Mix,Self::Add,Self::Multiply,Self::Maximum,Self::Difference];
    pub fn label(self)->&'static str {match self {Self::Mix=>"Mix",Self::Add=>"Add",Self::Multiply=>"Multiply",Self::Maximum=>"Maximum",Self::Difference=>"Difference"}}
}
impl Route {
    pub fn input(&self,signals:&[f32;19])->f32 {
        let a=signals[self.source as usize];let b=signals[self.secondary as usize];
        if self.secondary==Source::Off {return a;}
        match self.combine {Combine::Mix=>a+(b-a)*self.weight,Combine::Add=>(a+b*self.weight).min(1.0),Combine::Multiply=>a*(1.0-self.weight+b*self.weight),Combine::Maximum=>a.max(b*self.weight),Combine::Difference=>(a-b*self.weight).abs()}.clamp(0.0,1.0)
    }
    pub fn shaped(&self,x:f32)->f32 {let p=x.clamp(0.0,1.0)*4.0;let i=(p as usize).min(3);self.curve[i]+(self.curve[i+1]-self.curve[i])*(p-i as f32)}
}

#[cfg(test)]mod combined_tests{
    use super::*;
    #[test]fn combining_and_shaping(){let mut signals=[0.;19];signals[Source::Bass as usize]=0.8;signals[Source::Beat as usize]=0.2;let mut r=Route{source:Source::Bass,secondary:Source::Beat,weight:0.5,..Default::default()};for (op,want) in [(Combine::Mix,0.5),(Combine::Add,0.9),(Combine::Multiply,0.48),(Combine::Maximum,0.8),(Combine::Difference,0.7)]{r.combine=op;assert!((r.input(&signals)-want).abs()<0.00001);}r.curve=[0.,1.,0.,1.,0.];assert_eq!(r.shaped(0.25),1.);assert_eq!(r.shaped(0.5),0.);assert_eq!(r.shaped(0.375),0.5);}
    #[test]fn old_route_remains_linear(){let r:Route=serde_json::from_str(r#"{"source":"bass","depth":0.4}"#).unwrap();assert_eq!(r.secondary,Source::Off);assert_eq!(r.shaped(0.42),0.42);}
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroRange { pub low: f32, pub high: f32 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Motion {
    pub intensity: f32,
    pub routes: BTreeMap<String, Route>,
    pub ranges: BTreeMap<String, MacroRange>,
}

pub fn scene_key(scene: &str, index: usize) -> String { format!("scene:{scene}:{index}") }
pub fn detail_key(scene: &str, index: usize) -> String { format!("detail:{scene}:{index}") }
pub fn effect_key(bit: u32) -> String { format!("effect:{bit}") }
pub fn bounds(key: &str) -> Option<(f32, f32)> {
    match key {
        "speed" | "energy" => Some((0.0, 2.0)),
        "brightness" => Some((0.1, 1.5)),
        "hue" => Some((0.0, 1.0)),
        "saturation" => Some((0.0, 2.0)),
        "zoom" => Some((0.5, 2.0)),
        "deck_scale" => Some((0.25, 4.0)),
        "deck_distortion" | "deck_softness" => Some((0.0,1.0)),
        "rotation" | "deck_angle" => Some((-3.1415927, 3.1415927)),
        "pan_x" | "pan_y" => Some((-0.5, 0.5)),
        "xy_x" | "xy_y" | "crossfade" | "flash_amount" | "pad:0" | "pad:1" | "pad:2" | "pad:3" | "pad:4" => Some((0.0, 1.0)),
        _ => {
            if let Some(key) = key.strip_prefix("detail:") {
                let (scene, index) = key.rsplit_once(':')?;
                if (scene == "gl_flame" || scene == "stereo_scope") && index.parse::<usize>().ok()? < 4 { return Some((0.0, 2.0)); }
            }
            if let Some(key) = key.strip_prefix("scene:") {
                let (scene, index) = key.rsplit_once(':')?;
                let index = index.parse::<usize>().ok()?;
                if index < 4 && crate::viz_panel::scene_control_labels(crate::show::ShowMode::from_str(scene)).is_some() {
                    return Some((0.0, 2.0));
                }
            }
            if let Some(bit) = key.strip_prefix("effect:").and_then(|s| s.parse::<u32>().ok()) {
                if crate::viz_settings::Effect::ALL.iter().any(|e| e.bit() == bit) { return Some((0.0, 1.0)); }
            }
            None
        }
    }
}
fn finite(x: f32, min: f32, max: f32, default: f32) -> f32 {
    if x.is_finite() { x.clamp(min, max) } else { default }
}
impl Motion {
    pub fn sanitize(&mut self) {
        self.intensity = finite(self.intensity, 0.0, 1.0, 0.0);
        self.routes.retain(|key, route| {
            route.weight=finite(route.weight,0.0,1.0,0.5);
            for (i,v) in route.curve.iter_mut().enumerate(){*v=finite(*v,0.0,1.0,i as f32/4.0);}
            route.depth = finite(route.depth, 0.0, 1.0, 0.25);
            route.smoothing = finite(route.smoothing, 0.0, 2.0, 0.12);
            bounds(key).is_some() && route.source != Source::Off
        });
        self.ranges.retain(|key, range| {
            if let Some((min, max)) = bounds(key) {
                range.low = finite(range.low, min, max, min);
                range.high = finite(range.high, min, max, max);
                true
            } else { false }
        });
    }
}

#[derive(Default)]
pub struct Runtime {
    pub xy: crate::viz_xy::Runtime,
    pub signals: [f32; 19],
    pub conditioning: crate::viz_signals::Runtime,
    envelopes: BTreeMap<String, ((Source,Source), f32)>,
    pub values: BTreeMap<String, f32>,
}
impl Runtime {
    pub fn update(&mut self, motion: &Motion, bus: &VizBus, gains: [f32; 6], response: &crate::viz_signals::Settings, dt: f32) {
        self.signals = self.conditioning.update(response, bus, gains, dt);
        self.envelopes.retain(|key, _| motion.routes.contains_key(key));
        self.values.clear();
        let dt = finite(dt, 0.0, 0.25, 0.0);
        for (key, route) in &motion.routes {
            let pair=(route.source,route.secondary);
            let entry = self.envelopes.entry(key.clone()).or_insert((pair, 0.0));
            if entry.0 != pair { *entry = (pair, 0.0); }
            let alpha = if route.smoothing <= 0.0 { 1.0 } else { 1.0 - (-dt / route.smoothing).exp() };
            entry.1 += (route.shaped(route.input(&self.signals)) - entry.1) * alpha;
        }
    }
    pub fn value(&mut self, motion: &Motion, key: &str, base: f32) -> f32 {
        let Some((min, max)) = bounds(key) else { return base; };
        let mut value = motion.ranges.get(key).map_or(base, |r| r.low + (r.high-r.low)*motion.intensity);
        if let Some(route) = motion.routes.get(key) {
            let signal = self.envelopes.get(key).map_or(0.0, |e| e.1);
            value += signal * route.depth * (max-min) * if route.invert { -1.0 } else { 1.0 };
        }
        let value = finite(value, min, max, min);
        self.values.insert(key.into(), value);
        value
    }
    pub fn resolve(&mut self, config: &VizSettings, scene: &str, bus: &VizBus, dt: f32) -> Resolved {
        self.update(&config.motion, bus, config.signal_gains, &config.signal_response, dt);
        self.resolve_values(config,scene)
    }
    pub fn resolve_values(&mut self,config:&VizSettings,scene:&str)->Resolved {
        let m = &config.motion;
        let controls = config.scene_controls.get(scene).copied().unwrap_or([1.0; 4]);
        let details = config.scene_details.get(scene).copied().unwrap_or([1.0; 4]);
        let mut order = [0; 10];
        let mut mixes = [0.0; 10];
        let mut count = 0;
        if config.effects_enabled {
            for slot in config.effects.iter().filter(|s| s.enabled).take(10) {
                let mix = self.value(m, &effect_key(slot.effect.bit()), slot.mix);
                if mix > 0.0 { order[count] = slot.effect.bit(); mixes[count] = mix; count += 1; }
            }
        }
        Resolved {
            speed: self.value(m, "speed", config.speed),
            energy: self.value(m, "energy", config.energy),
            brightness: self.value(m, "brightness", config.brightness),
            hue: self.value(m, "hue", config.hue),
            saturation: self.value(m, "saturation", config.saturation),
            controls: std::array::from_fn(|i| self.value(m, &scene_key(scene, i), controls[i])),
            details: std::array::from_fn(|i| self.value(m, &detail_key(scene, i), details[i])),
            order, mixes, count,
        }
    }
}
pub struct Resolved {
    pub speed: f32, pub energy: f32, pub brightness: f32,
    pub hue: f32, pub saturation: f32, pub controls: [f32; 4],
    pub details: [f32; 4],
    pub order: [u32; 10], pub mixes: [f32; 10], pub count: usize,
}

pub fn binding(ui: &mut egui::Ui, key: &str, base: f32, motion: &mut Motion, live: &Runtime) {
    binding_editor(ui,key,base,motion,live,false);
}
pub fn binding_editor(ui: &mut egui::Ui, key: &str, base: f32, motion: &mut Motion, live: &Runtime, expanded:bool) {
    let Some((min, max)) = bounds(key) else { return; };
    let mut route = motion.routes.get(key).cloned().unwrap_or_default();
    let mut macro_on = motion.ranges.contains_key(key);
    let caption = if route.source == Source::Off { "React to...".into() } else { format!("React to {}", route.source.label()) };
    ui.push_id(key, |ui| {
        egui::CollapsingHeader::new(caption).default_open(expanded).show(ui, |ui| {
            egui::ComboBox::from_id_salt("source").selected_text(route.source.label()).show_ui(ui, |ui| {
                for source in Source::ALL { ui.selectable_value(&mut route.source, source, source.label()); }
            });
            if route.source != Source::Off {
                egui::CollapsingHeader::new("Combine and shape").default_open(expanded).show(ui,|ui| {
                    egui::ComboBox::from_id_salt("second").selected_text(format!("Second: {}",route.secondary.label())).show_ui(ui,|ui|{for source in Source::ALL {ui.selectable_value(&mut route.secondary,source,source.label());}});
                    if route.secondary!=Source::Off {
                        egui::ComboBox::from_id_salt("combine").selected_text(route.combine.label()).show_ui(ui,|ui|{for op in Combine::ALL{ui.selectable_value(&mut route.combine,op,op.label());}});
                        ui.add(egui::Slider::new(&mut route.weight,0.0..=1.0).text("Second weight"));
                    }
                    let input=route.input(&live.signals);curve_editor(ui,&mut route.curve,input);
                });
                ui.add(egui::ProgressBar::new(live.signals[route.source as usize]).desired_height(5.0));
                ui.add(egui::Slider::new(&mut route.depth, 0.0..=1.0).text("Depth"));
                ui.add(egui::Slider::new(&mut route.smoothing, 0.0..=2.0).text("Smoothing").suffix(" s"));
                ui.checkbox(&mut route.invert, "Move downward");
                ui.small("Depth adds a fraction of the slider's range to its base value.");
            }
            ui.checkbox(&mut macro_on, "Follow Intensity");
            if macro_on {
                let range = motion.ranges.entry(key.into()).or_insert(MacroRange { low: base, high: max });
                ui.add(egui::Slider::new(&mut range.low, min..=max).text("At 0%"));
                ui.add(egui::Slider::new(&mut range.high, min..=max).text("At 100%"));
            }
            if let Some(v) = live.values.get(key) { ui.small(format!("Live value: {v:0.2}")); }
        });
    });
    if !macro_on { motion.ranges.remove(key); }
    if route.source == Source::Off { motion.routes.remove(key); } else { motion.routes.insert(key.into(), route); }
}

pub fn curve_editor(ui:&mut egui::Ui,points:&mut [f32;5],input:f32) {
    ui.small("Drag the curve. Input runs left to right; output runs bottom to top.");
    let (r,response)=ui.allocate_exact_size(egui::vec2(ui.available_width().min(270.0),96.0),egui::Sense::click_and_drag());
    let graph=r.shrink(8.0);let pos=|i:usize,y:f32| egui::pos2(graph.left()+graph.width()*i as f32/4.0,graph.bottom()-graph.height()*y);
    if response.is_pointer_button_down_on(){if let Some(p)=response.interact_pointer_pos(){let i=(((p.x-graph.left())/graph.width())*4.0).round().clamp(0.0,4.0) as usize;points[i]=((graph.bottom()-p.y)/graph.height()).clamp(0.0,1.0);}}
    let painter=ui.painter();painter.rect_filled(r,4,ui.visuals().extreme_bg_color);
    for i in 0..5 {painter.line_segment([pos(i,0.0),pos(i,1.0)],egui::Stroke::new(0.5,ui.visuals().weak_text_color().gamma_multiply(0.3)));}
    painter.add(egui::Shape::line((0..5).map(|i|pos(i,points[i])).collect(),egui::Stroke::new(2.0,crate::theme::cyan())));
    for (i,v) in points.iter().enumerate(){painter.circle_filled(pos(i,*v),3.5,ui.visuals().text_color());}
    let x=graph.left()+input.clamp(0.0,1.0)*graph.width();painter.line_segment([egui::pos2(x,graph.top()),egui::pos2(x,graph.bottom())],egui::Stroke::new(1.0,egui::Color32::GOLD));
    ui.horizontal_wrapped(|ui|{for (name,values) in [("Linear",[0.0,0.25,0.5,0.75,1.0]),("Soft",[0.0,0.5,0.72,0.88,1.0]),("Gate",[0.0,0.0,0.0,0.9,1.0]),("Peak",[0.0,0.5,1.0,0.5,0.0])]{if ui.small_button(name).clicked(){*points=values;}}});
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modulation_is_frame_rate_independent_and_authored_settings_do_not_change() {
        let mut config = VizSettings::default();
        config.motion.routes.insert("speed".into(), Route { source: Source::Beat, depth: 0.25, smoothing: 0.3, invert: false, ..Default::default() });
        let before = serde_json::to_string(&config).unwrap();
        let mut bus = VizBus::new(); bus.pulse = 1.0;
        let mut values = vec![];
        for rate in [30,60,144] {
            let mut runtime = Runtime::default();
            let mut value = 0.0;
            for _ in 0..rate { value = runtime.resolve(&config, "tunnel", &bus, 1.0/rate as f32).speed; }
            values.push(value);
        }
        assert!((values[0]-values[2]).abs() < 0.00001);
        assert!(values[0] > 1.45 && values[0] < 1.5);
        assert_eq!(before, serde_json::to_string(&config).unwrap());
    }
    #[test]
    fn macro_routes_clamp_invert_and_effect_identity_survives_reordering() {
        use crate::viz_settings::{Effect, EffectSlot};
        let mut config = VizSettings::default();
        config.motion.intensity = 0.5;
        config.motion.ranges.insert("brightness".into(), MacroRange { low: 0.2, high: 1.4 });
        let key = effect_key(Effect::Grain.bit());
        config.motion.routes.insert(key, Route { source: Source::Bass, depth: 1.0, smoothing: 0.0, invert: false, ..Default::default() });
        config.effects = vec![EffectSlot { effect: Effect::Grain, mix: 0.0, enabled: true }, EffectSlot { effect: Effect::Mirror, mix: 0.3, enabled: true }];
        let mut bus = VizBus::new(); bus.bass = 1.0;
        let mut runtime = Runtime::default();
        let resolved = runtime.resolve(&config, "tunnel", &bus, 0.016);
        assert!((resolved.brightness-0.8).abs() < 0.00001);
        assert_eq!(resolved.mixes[0], 1.0);
        config.effects.swap(0, 1);
        let resolved = runtime.resolve(&config, "tunnel", &bus, 0.016);
        assert_eq!(resolved.mixes[1], 1.0);
        config.motion.routes.get_mut(&effect_key(Effect::Grain.bit())).unwrap().invert = true;
        assert_eq!(runtime.resolve(&config, "tunnel", &bus, 0.016).count, 1);
    }
    #[test]
    fn settings_reject_unknown_targets_and_nonfinite_ranges() {
        let mut motion = Motion::default();
        motion.intensity = f32::NAN;
        motion.ranges.insert("brightness".into(), MacroRange { low: f32::NAN, high: f32::INFINITY });
        motion.ranges.insert("scene:off:0".into(), MacroRange { low: 0.0, high: 1.0 });
        motion.sanitize();
        assert_eq!(motion.intensity, 0.0);
        assert_eq!(motion.ranges.len(), 1);
        assert_eq!(motion.ranges["brightness"].high, 1.5);
    }
}
