//! Visual settings are independent of the sound profile and editor colors.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Mirror,
    ZoomBlur,
    Aberration,
    Pixelate,
    Halftone,
    Scanlines,
    Grain,
    Strobe,
    EdgeGlow,
    Thermal,
}

impl Effect {
    pub const ALL: [Self; 10] = [
        Self::Mirror,
        Self::ZoomBlur,
        Self::Aberration,
        Self::Pixelate,
        Self::Halftone,
        Self::Scanlines,
        Self::Grain,
        Self::Strobe,
        Self::EdgeGlow,
        Self::Thermal,
    ];
    pub fn bit(self) -> u32 {
        use crate::glstage::*;
        match self {
            Self::Mirror => FX_MIRROR,
            Self::ZoomBlur => FX_ZOOMBLUR,
            Self::Aberration => FX_ABERRATION,
            Self::Pixelate => FX_PIXELATE,
            Self::Halftone => FX_HALFTONE,
            Self::Scanlines => FX_SCANLINES,
            Self::Grain => FX_GRAIN,
            Self::Strobe => FX_STROBE,
            Self::EdgeGlow => FX_EDGEGLOW,
            Self::Thermal => FX_THERMAL,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Mirror => "Mirror",
            Self::ZoomBlur => "Zoom blur",
            Self::Aberration => "Color split",
            Self::Pixelate => "Pixelate",
            Self::Halftone => "Halftone",
            Self::Scanlines => "CRT scanlines",
            Self::Grain => "Film grain",
            Self::Strobe => "Beat flash",
            Self::EdgeGlow => "Edge glow",
            Self::Thermal => "Thermal",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Mirror => "Reflect the scene across its center into four quadrants.",
            Self::ZoomBlur => "Smear toward the center, with stronger blur on audio pulses.",
            Self::Aberration => {
                "Separate red and blue along the image radius; audio pulses widen the split."
            }
            Self::Pixelate => "Turn the image into blocks that grow with the bass.",
            Self::Halftone => "Render brightness as colored dots on a rotated grid.",
            Self::Scanlines => "Add CRT scanlines, a slight screen curve, and an RGB mask.",
            Self::Grain => "Add moving film grain and darken the corners.",
            Self::Strobe => {
                "Lift brightness briefly at the detected beat phase, scaled by audio level."
            }
            Self::EdgeGlow => "Outline contrast edges using the current theme accent.",
            Self::Thermal => "Remap brightness through a blue, magenta, orange, and white palette.",
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectSlot {
    pub effect: Effect,
    pub enabled: bool,
    pub mix: f32,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    #[default]
    Full,
    Balanced,
    Low,
}
impl Quality {
    pub fn scale(self) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::Balanced => 0.75,
            Self::Low => 0.5,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VizSettings {
    pub xy: crate::viz_xy::Xy,
    pub pulse_control: crate::viz_compositor::Pulse,
    pub performance: crate::viz_performance::Performance,
    pub flash: crate::viz_performance::Flash,
    pub transform: [f32;4],
    pub saved_racks: Vec<crate::viz_performance::Rack>,
    pub motion: crate::viz_motion::Motion,
    pub scene_variants: std::collections::BTreeMap<String, u32>,
    pub scene_details: std::collections::BTreeMap<String, [f32; 4]>,
    pub hue: f32,
    pub saturation: f32,
    pub accent: Option<[f32; 3]>,
    /// Independent input gains: bass, mids, treble, beat, spectral flux, stereo width.
    pub signal_gains: [f32; 6],
    pub signal_response: crate::viz_signals::Settings,
    /// Scene-local controls survive switching scenes and are included in saved looks.
    pub scene_controls: std::collections::BTreeMap<String, [f32; 4]>,
    pub review_ratings: std::collections::BTreeMap<String, crate::viz_audition::Rating>,
    pub stage_view: bool,
    pub overlays_in_stage: bool,
    pub speed: f32,
    pub energy: f32,
    pub brightness: f32,
    pub rainbow: bool,
    pub shuffle_seconds: f32,
    pub shuffle_favorites: bool,
    pub favorites: Vec<String>,
    pub effects: Vec<EffectSlot>,
    pub effects_enabled: bool,
    pub saved_looks: Vec<crate::viz_looks::Look>,
    pub quality: Quality,
    pub transition_seconds: f32,
}

impl Default for VizSettings {
    fn default() -> Self {
        Self {
            xy: Default::default(),
            pulse_control:Default::default(),
            performance: Default::default(), flash: Default::default(), transform:[1.0,0.0,0.0,0.0], saved_racks:vec![],
            motion: Default::default(),
            scene_variants: Default::default(), scene_details: Default::default(),
            hue: 0.0,
            saturation: 1.0,
            accent: None,
            signal_gains: [1.0; 6],
            signal_response: Default::default(),
            scene_controls: Default::default(),
            review_ratings: Default::default(),
            stage_view: false,
            overlays_in_stage: false,
            speed: 1.0,
            energy: 1.0,
            brightness: 1.0,
            rainbow: true,
            shuffle_seconds: 18.0,
            shuffle_favorites: false,
            favorites: vec![],
            effects: vec![],
            effects_enabled: true,
            saved_looks: vec![],
            quality: Quality::Full,
            transition_seconds: 0.65,
        }
    }
}

impl VizSettings {
    pub fn from_legacy(fx: u32, rainbow: bool) -> Self {
        Self {
            rainbow,
            effects: Effect::ALL
                .into_iter()
                .filter(|e| fx & e.bit() != 0)
                .map(|effect| EffectSlot {
                    effect,
                    enabled: true,
                    mix: 1.0,
                })
                .collect(),
            ..Default::default()
        }
    }
    pub fn sanitize(&mut self) {
        let finite = |x: f32, min: f32, max: f32, default: f32| {
            if x.is_finite() {
                x.clamp(min, max)
            } else {
                default
            }
        };
        self.motion.sanitize();
        self.signal_response.sanitize();
        self.xy.sanitize();
        self.pulse_control.sanitize();
        self.performance.sanitize();self.flash.sanitize();
        for (i,key) in ["zoom","rotation","pan_x","pan_y"].iter().enumerate() { let (lo,hi)=crate::viz_motion::bounds(key).unwrap();self.transform[i]=finite(self.transform[i],lo,hi,if i==0 {1.0} else {0.0}); }
        self.scene_variants.retain(|key, value| {
            let max = match key.as_str() { "gl_flame" => 4, "stereo_scope" => 1, _ => return false };
            *value = (*value).min(max); true
        });
        self.scene_details.retain(|key, controls| {
            for v in controls { *v = finite(*v, 0.0, 2.0, 1.0); }
            key == "gl_flame" || key == "stereo_scope"
        });
        self.hue = finite(self.hue, 0.0, 1.0, 0.0);
        self.saturation = finite(self.saturation, 0.0, 2.0, 1.0);
        if let Some(accent) = &mut self.accent {
            for v in accent { *v = finite(*v, 0.0, 1.0, 0.5); }
        }
        self.speed = finite(self.speed, 0.0, 2.0, 1.0);
        for gain in &mut self.signal_gains { *gain = finite(*gain, 0.0, 2.0, 1.0); }
        self.scene_controls.retain(|id, controls| {
            for value in controls { *value = finite(*value, 0.0, 2.0, 1.0); }
            crate::show::ShowMode::from_str(id) != crate::show::ShowMode::Off
        });
        self.energy = finite(self.energy, 0.0, 2.0, 1.0);
        self.brightness = finite(self.brightness, 0.1, 1.5, 1.0);
        self.shuffle_seconds = finite(self.shuffle_seconds, 5.0, 120.0, 18.0);
        self.transition_seconds = finite(self.transition_seconds, 0.0, 2.0, 0.65);
        let mut seen = 0;
        self.effects.retain_mut(|slot| {
            slot.mix = finite(slot.mix, 0.0, 1.0, 1.0);
            let fresh = seen & slot.effect.bit() == 0;
            seen |= slot.effect.bit();
            fresh
        });
        self.favorites
            .retain(|s| crate::show::ShowMode::from_str(s) != crate::show::ShowMode::Off);
    }
    pub fn effect_uniforms(&self) -> ([u32; 10], [f32; 10], usize) {
        let mut order = [0; 10];
        let mut mix = [0.0; 10];
        let mut count = 0;
        if !self.effects_enabled {
            return (order, mix, count);
        }
        for slot in self
            .effects
            .iter()
            .filter(|s| s.enabled && s.mix > 0.0)
            .take(10)
        {
            order[count] = slot.effect.bit();
            mix[count] = slot.mix;
            count += 1;
        }
        (order, mix, count)
    }
    pub fn legacy_mask(&self) -> u32 {
        self.effects
            .iter()
            .filter(|s| s.enabled)
            .fold(0, |m, s| m | s.effect.bit())
    }
    pub fn scene_pool(&self) -> Vec<crate::show::ShowMode> {
        let favorites: Vec<_> = crate::show::ShowMode::ALL
            .into_iter()
            .filter(|m| {
                *m != crate::show::ShowMode::Off && self.favorites.iter().any(|s| s == m.as_str())
            })
            .collect();
        if !self.shuffle_favorites || favorites.is_empty() {
            crate::show::ShowMode::ALL[1..].to_vec()
        } else {
            favorites
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playback_defaults_migrate_and_settings_roundtrip() {
        let mut settings: VizSettings = serde_json::from_str("{\"speed\":0.5}").unwrap();
        assert_eq!(settings.quality.scale(), 1.0);
        assert_eq!(settings.transition_seconds, 0.65);
        settings.quality = Quality::Low;
        settings.transition_seconds = 1.25;
        let loaded: VizSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(loaded.quality.scale(), 0.5);
        assert_eq!(loaded.transition_seconds, 1.25);
        settings.transition_seconds = f32::NAN;
        settings.sanitize();
        assert_eq!(settings.transition_seconds, 0.65);
    }
    #[test]
    fn migration_preserves_effects_and_order_through_roundtrip() {
        let mut v = VizSettings::from_legacy((1 << 4) | (1 << 5), false);
        v.effects.swap(0, 1);
        let decoded: VizSettings =
            serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(decoded.effect_uniforms().0[..2], [1 << 5, 1 << 4]);
        assert!(!decoded.rainbow);
    }
    #[test]
    fn disabled_effects_do_not_consume_render_passes() {
        let mut v = VizSettings::from_legacy(7, true);
        v.effects[0].enabled = false;
        v.effects[1].mix = 0.0;
        assert_eq!(v.effect_uniforms().2, 1);
        assert_eq!(v.effect_uniforms().0[0], 4);
    }
    #[test]
    fn shuffle_includes_all_scenes_and_honors_favorites() {
        let mut v = VizSettings::default();
        assert_eq!(v.scene_pool().len(), crate::glstage::MODE_COUNT);
        v.favorites = vec!["particles".into(), "gl_ocean".into(), "missing".into()];
        v.shuffle_favorites = true;
        v.sanitize();
        assert_eq!(v.scene_pool().len(), 2);
        assert_eq!(v.favorites.len(), 2);
    }
    #[test]
    fn driver_and_scene_controls_bound_corrupt_values() {
        let mut v = VizSettings::default();
        v.signal_gains = [f32::NAN, -2.0, 4.0, 0.0, 1.3, f32::INFINITY];
        v.scene_controls.insert("tunnel".into(), [f32::NAN, -1.0, 9.0, 0.5]);
        v.scene_controls.insert("missing".into(), [1.0; 4]);
        v.sanitize();
        assert_eq!(v.signal_gains, [1.0, 0.0, 2.0, 0.0, 1.3, 1.0]);
        assert_eq!(v.scene_controls["tunnel"], [1.0, 0.0, 2.0, 0.5]);
        assert!(!v.scene_controls.contains_key("missing"));
    }
    #[test]
    fn effect_comparison_migrates_enabled_and_roundtrips_without_losing_the_stack() {
        let old: VizSettings = serde_json::from_str(
            "{\"effects\":[{\"effect\":\"grain\",\"mix\":0.2,\"enabled\":true}]}",
        )
        .unwrap();
        assert!(old.effects_enabled);
        let mut v = old.clone();
        v.effects_enabled = false;
        let restored: VizSettings =
            serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert!(restored.effects == old.effects);
        assert_eq!(restored.effect_uniforms().2, 0);
        let look = crate::viz_looks::Look::capture(
            "Without effects",
            crate::show::ShowMode::GlWarp,
            &restored,
        );
        let look: crate::viz_looks::Look =
            serde_json::from_str(&serde_json::to_string(&look).unwrap()).unwrap();
        let mut mode = crate::show::ShowMode::Off;
        assert!(look.apply(&mut mode, &mut v));
        assert!(!v.effects_enabled && v.effects == old.effects);
        v.effects_enabled = true;
        assert_eq!(v.effect_uniforms(), old.effect_uniforms());
        let legacy_look: crate::viz_looks::Look =
            serde_json::from_str("{\"name\":\"Old look\",\"scene\":\"gl_warp\"}").unwrap();
        assert!(legacy_look.effects_enabled);
    }
}
