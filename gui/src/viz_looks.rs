//! Named visual looks contain art settings only, never sound or window state.
use crate::{
    show::ShowMode,
    viz_settings::{Effect, EffectSlot, VizSettings},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub xy: crate::viz_xy::Xy,
    pub flash: crate::viz_performance::Flash,
    pub transform: [f32;4],
    pub motion: crate::viz_motion::Motion,
    pub scene_variants: std::collections::BTreeMap<String, u32>,
    pub scene_details: std::collections::BTreeMap<String, [f32; 4]>,
    pub hue: f32,
    pub saturation: f32,
    pub accent: Option<[f32; 3]>,
    pub signal_gains: [f32; 6],
    pub signal_response: crate::viz_signals::Settings,
    pub scene_controls: std::collections::BTreeMap<String, [f32; 4]>,
    pub name: String,
    pub scene: String,
    pub speed: f32,
    pub energy: f32,
    pub brightness: f32,
    pub rainbow: bool,
    pub effects: Vec<EffectSlot>,
    pub effects_enabled: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            xy: Default::default(),
            flash: Default::default(), transform:[1.0,0.0,0.0,0.0],
            motion: Default::default(),
            scene_variants: Default::default(), scene_details: Default::default(),
            hue: 0.0, saturation: 1.0, accent: None,
            signal_gains: [1.0; 6],
            signal_response: Default::default(),
            scene_controls: Default::default(),
            name: String::new(),
            scene: "gl_warp".into(),
            speed: 1.0,
            energy: 1.0,
            brightness: 1.0,
            rainbow: true,
            effects: vec![],
            effects_enabled: true,
        }
    }
}

impl Look {
    pub fn capture(name: &str, scene: ShowMode, visual: &VizSettings) -> Self {
        Self {
            xy: visual.xy.clone(),
            flash: visual.flash.clone(), transform:visual.transform,
            motion: visual.motion.clone(),
            scene_variants: visual.scene_variants.clone(), scene_details: visual.scene_details.clone(),
            hue: visual.hue, saturation: visual.saturation,
            accent: Some(visual.accent.unwrap_or_else(|| {
                let c = crate::theme::viz_accent();
                [c.r() as f32/255.0, c.g() as f32/255.0, c.b() as f32/255.0]
            })),
            signal_gains: visual.signal_gains,
            signal_response: visual.signal_response.clone(),
            scene_controls: visual.scene_controls.clone(),
            name: name.trim().into(),
            scene: scene.as_str().into(),
            speed: visual.speed,
            energy: visual.energy,
            brightness: visual.brightness,
            rainbow: visual.rainbow,
            effects: visual.effects.clone(),
            effects_enabled: visual.effects_enabled,
        }
    }
    pub fn available(&self) -> bool {
        self.scene == "off" || ShowMode::from_str(&self.scene) != ShowMode::Off
    }
    pub fn apply(&self, scene: &mut ShowMode, visual: &mut VizSettings) -> bool {
        if !self.available() {
            return false;
        }
        *scene = ShowMode::from_str(&self.scene);
        visual.xy=self.xy.clone();
        visual.motion = self.motion.clone();
        visual.flash = self.flash.clone(); visual.transform=self.transform;
        visual.scene_variants = self.scene_variants.clone();
        visual.scene_details = self.scene_details.clone();
        visual.hue = self.hue;
        visual.saturation = self.saturation;
        visual.accent = self.accent;
        visual.signal_gains = self.signal_gains;
        visual.signal_response = self.signal_response.clone();
        visual.scene_controls = self.scene_controls.clone();
        visual.speed = self.speed;
        visual.energy = self.energy;
        visual.brightness = self.brightness;
        visual.rainbow = self.rainbow;
        visual.effects = self.effects.clone();
        visual.effects_enabled = self.effects_enabled;
        visual.sanitize();
        true
    }
    pub fn matches(&self, scene: ShowMode, visual: &VizSettings) -> bool {
        let mut expected = self.clone();
        let actual = Self::capture(&self.name, scene, visual);
        if expected.accent.is_none() { expected.accent = actual.accent; }
        expected == actual
    }
}

pub fn factory() -> Vec<Look> {
    let effects = |entries: &[(Effect, f32)]| {
        entries
            .iter()
            .map(|&(effect, mix)| EffectSlot {
                effect,
                enabled: true,
                mix,
            })
            .collect()
    };
    let mut looks = vec![
        Look {
            name: "Neon drift".into(),
            scene: "gl_warp".into(),
            speed: 0.75,
            effects: effects(&[(Effect::Aberration, 0.35), (Effect::Grain, 0.15)]),
            ..Default::default()
        },
        Look {
            name: "Solar tide".into(),
            scene: "gl_flame".into(),
            speed: 0.7,
            energy: 1.2,
            effects: effects(&[(Effect::Thermal, 0.45), (Effect::ZoomBlur, 0.3)]),
            ..Default::default()
        },
        Look {
            name: "Phosphor".into(),
            scene: "scope_trails".into(),
            rainbow: false,
            effects: effects(&[(Effect::EdgeGlow, 0.35), (Effect::Scanlines, 0.6)]),
            ..Default::default()
        },
        Look {
            name: "Night drive".into(),
            scene: "gl_outrun".into(),
            energy: 1.1,
            effects: effects(&[(Effect::Aberration, 0.2), (Effect::Grain, 0.2)]),
            ..Default::default()
        },
        Look {
            name: "Still water".into(),
            scene: "gl_ocean".into(),
            speed: 0.4,
            energy: 0.7,
            brightness: 1.05,
            ..Default::default()
        },
    ];
    use crate::viz_motion::{Motion, Route, Source, MacroRange, scene_key, detail_key};
    for (variant, name, speed, controls, details) in [
        (1, "Molten metal", 0.45, [0.6,0.65,0.3,1.5], [0.55,1.45,0.4,0.7]),
        (2, "Electric ink", 0.7, [1.3,0.8,0.4,1.1], [1.4,0.65,1.4,0.6]),
        (3, "Solar surface", 0.55, [0.8,0.6,1.4,1.5], [0.7,1.3,0.6,1.5]),
        (4, "Bioluminescence", 0.4, [0.55,1.1,1.5,1.35], [0.65,0.8,0.7,0.6]),
    ] {
        let mut motion = Motion::default();
        motion.routes.insert(scene_key("gl_flame",0), Route { source: Source::Mids, depth: 0.16, smoothing: 0.24, ..Default::default() });
        motion.routes.insert(scene_key("gl_flame",2), Route { source: Source::Highs, depth: 0.18, ..Default::default() });
        motion.routes.insert(detail_key("gl_flame",0), Route { source: Source::Width, depth: 0.1, smoothing: 0.4, ..Default::default() });
        motion.ranges.insert("speed".into(), MacroRange { low: speed, high: 1.4 });
        motion.ranges.insert("brightness".into(), MacroRange { low: 0.8, high: 1.3 });
        looks.push(Look { name: name.into(), scene: "gl_flame".into(), speed,
            scene_variants: [("gl_flame".into(), variant)].into(),
            scene_controls: [("gl_flame".into(), controls)].into(),
            scene_details: [("gl_flame".into(), details)].into(),
            brightness: 0.8, motion, ..Default::default() });
    }
    let mut motion = Motion::default();
    motion.routes.insert("speed".into(), Route { source: Source::Bass, depth: 0.22, smoothing: 0.09, ..Default::default() });
    motion.routes.insert(scene_key("tunnel",1), Route { source: Source::Mids, depth: 0.3, ..Default::default() });
    motion.ranges.insert("speed".into(), MacroRange { low: 0.65, high: 1.4 });
    motion.ranges.insert(scene_key("tunnel",3), MacroRange { low: 0.5, high: 1.6 });
    looks.push(Look { name: "Pulse passage".into(), scene: "tunnel".into(), speed: 0.65,
        scene_controls: [("tunnel".into(), [1.0,0.8,0.65,0.5])].into(), motion, ..Default::default() });
    looks.push(Look { name: "Stereo sculpture".into(), scene: "stereo_scope".into(),
        scene_variants: [("stereo_scope".into(),1)].into(),
        scene_controls: [("stereo_scope".into(),[1.25,0.7,0.3,1.5])].into(),
        ..Default::default() });
    looks.push(Look { name: "Ridge flight".into(), scene: "gl_terrain".into(),
        scene_controls: [("gl_terrain".into(),[1.4,0.7,1.2,0.65])].into(),
        speed: 0.75, ..Default::default() });
    for (name,scene,controls) in [("Afterhours district","spectral_city",[1.2,0.9,1.35,0.7]),("Resonant gold","cymatics",[0.6,0.7,0.8,0.5]),("Binary stars","stereo_orbit",[1.3,1.25,1.2,1.2])] {
        let mut motion=Motion::default();
        motion.routes.insert(scene_key(scene,0),Route{source:Source::Bass,secondary:Source::Beat,combine:crate::viz_motion::Combine::Add,weight:0.25,depth:0.12,..Default::default()});
        motion.routes.insert("hue".into(),Route{source:Source::Width,depth:0.12,smoothing:0.4,..Default::default()});
        looks.push(Look{name:name.into(),scene:scene.into(),scene_controls:[(scene.into(),controls)].into(),rainbow:scene!="cymatics",accent:if scene=="cymatics"{Some([1.0,0.68,0.2])}else{None},motion,speed:0.8,..Default::default()});
    }
    let mut flame=Look{name:"Flame instrument".into(),scene:"gl_flame".into(),speed:0.75,..Default::default()};
    flame.xy.enabled=true;flame.xy.scene="gl_flame".into();
    flame.xy.corners=std::array::from_fn(|i|{
        let (name,controls,brightness)=match i {
            0=>("Ember",[0.6,0.65,0.5,0.75],0.8),
            1=>("Ribbon",[1.1,1.45,0.8,1.0],1.0),
            2=>("Storm",[1.6,0.8,1.5,1.2],0.85),
            _=>("Eruption",[1.4,1.6,1.7,1.35],1.1),
        };
        let mut values:std::collections::BTreeMap<String,f32>=controls.into_iter().enumerate().map(|(axis,value)|(scene_key("gl_flame",axis),value)).collect();
        values.insert("brightness".into(),brightness);
        Some(crate::viz_xy::Corner{name:name.into(),values})
    });
    flame.motion.routes.insert("xy_x".into(),Route{source:Source::Bass,secondary:Source::Beat,weight:0.7,depth:1.0,smoothing:0.25,..Default::default()});
    flame.motion.routes.insert("xy_y".into(),Route{source:Source::Width,depth:1.0,smoothing:0.5,..Default::default()});
    looks.push(flame);
    looks
}

pub fn validate_name(name: &str, saved: &[Look]) -> Result<(), &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name for this look.");
    }
    if name.chars().count() > 48 {
        return Err("Use 48 characters or fewer.");
    }
    if saved
        .iter()
        .chain(factory().iter())
        .any(|look| look.name.eq_ignore_ascii_case(name))
    {
        return Err("That name is taken. Choose another name or update the existing look.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_apply_preserve_non_art_preferences() {
        let mut v = VizSettings {
            speed: 0.5,
            energy: 1.5,
            ..Default::default()
        };
        v.effects = vec![
            EffectSlot {
                effect: Effect::Scanlines,
                mix: 0.2,
                enabled: false,
            },
            EffectSlot {
                effect: Effect::Mirror,
                mix: 0.8,
                enabled: true,
            },
        ];
        let saved: Look = serde_json::from_str(
            &serde_json::to_string(&Look::capture("My glow", ShowMode::Particles, &v)).unwrap(),
        )
        .unwrap();
        v.stage_view = true;
        v.quality = crate::viz_settings::Quality::Low;
        v.transition_seconds = 1.25;
        v.overlays_in_stage = true;
        v.shuffle_seconds = 42.0;
        v.favorites.push("gl_ocean".into());
        v.saved_looks.push(saved.clone());
        v.effects.clear();
        v.speed = 1.0;
        let mut mode = ShowMode::Off;
        assert!(saved.apply(&mut mode, &mut v));
        assert!(saved.matches(mode, &v));
        assert!(v.stage_view && v.overlays_in_stage);
        assert_eq!(v.quality.scale(), 0.5);
        assert_eq!(v.transition_seconds, 1.25);
        assert_eq!(v.shuffle_seconds, 42.0);
        assert_eq!(v.favorites, ["gl_ocean"]);
        assert_eq!(v.saved_looks.len(), 1);
    }
    #[test]
    fn names_cannot_silently_overwrite_a_look() {
        let saved = vec![Look {
            name: "My glow".into(),
            ..Default::default()
        }];
        assert!(validate_name(" MY GLOW ", &saved).is_err());
        assert!(validate_name("Phosphor", &saved).is_err());
        assert!(validate_name("", &saved).is_err());
        assert!(validate_name("My second glow", &saved).is_ok());
    }
    #[test]
    fn every_factory_look_resolves_to_a_scene() {
        for look in factory() {
            let mut visual = VizSettings::default();
            let mut scene = ShowMode::Off;
            assert!(look.apply(&mut scene, &mut visual));
            assert!(scene != ShowMode::Off);
            assert!(look.matches(scene, &visual));
        }
    }
    #[test]
    fn scene_controls_and_audio_drivers_survive_saved_looks_and_legacy_loads() {
        let old: Look = serde_json::from_str(r#"{"name":"Legacy","scene":"gl_flame"}"#).unwrap();
        assert_eq!(old.signal_gains, [1.0; 6]);
        assert!(!old.signal_response.enabled);
        assert!(old.scene_controls.is_empty());
        let mut visual = VizSettings::default();
        visual.signal_gains = [0.0, 0.5, 1.0, 1.5, 2.0, 0.8];
        visual.signal_response = crate::viz_signals::Settings::preset(0.01,0.4);
        visual.scene_controls.insert("tunnel".into(), [0.5, 1.2, 0.3, 1.7]);
        visual.scene_controls.insert("gl_flame".into(), [1.0, 1.1, 0.9, 1.0]);
        visual.scene_variants.insert("gl_flame".into(), 3);
        visual.scene_details.insert("gl_flame".into(), [0.3, 0.8, 1.4, 1.7]);
        visual.motion.intensity = 0.42;
        visual.motion.routes.insert("speed".into(), crate::viz_motion::Route {
            source: crate::viz_motion::Source::Beat, depth: 0.6, smoothing: 0.2, invert: true, ..Default::default()
        });
        visual.motion.ranges.insert("brightness".into(), crate::viz_motion::MacroRange { low: 0.3, high: 1.4 });
        visual.hue = 0.3; visual.saturation = 0.7; visual.accent = Some([0.8, 0.2, 0.4]);
        let look = Look::capture("Reactive tunnel", ShowMode::Tunnel, &visual);
        let restored: Look = serde_json::from_str(&serde_json::to_string(&look).unwrap()).unwrap();
        let mut target = VizSettings::default();
        target.favorites.push("gl_flame".into());
        let mut mode = ShowMode::Off;
        assert!(restored.apply(&mut mode, &mut target));
        assert!(mode == ShowMode::Tunnel);
        assert_eq!(target.scene_controls, visual.scene_controls);
        assert_eq!(target.signal_gains, visual.signal_gains);
        assert_eq!(target.signal_response, visual.signal_response);
        assert_eq!(target.scene_variants, visual.scene_variants);
        assert_eq!(target.scene_details, visual.scene_details);
        assert_eq!(target.motion, visual.motion);
        assert_eq!((target.hue, target.saturation, target.accent), (visual.hue, visual.saturation, visual.accent));
        assert_eq!(target.favorites, ["gl_flame"]);
    }
}
