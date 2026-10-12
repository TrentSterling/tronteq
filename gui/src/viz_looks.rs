//! Named visual looks contain art settings only, never sound or window state.
use crate::{
    show::ShowMode,
    viz_settings::{Effect, EffectSlot, VizSettings},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
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
        *self == Self::capture(&self.name, scene, visual)
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
    vec![
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
    ]
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
}
