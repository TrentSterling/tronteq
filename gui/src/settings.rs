//! App-wide UI settings persisted across restarts (`settings.json`). Sound
//! state lives in profiles + state.bin; this is everything else: theme, viz
//! layer mix, rainbow, zoom, which profile is active, which inspector tab is
//! open. Saved on change from `update()` — there is no clean shutdown hook
//! (tray Quit is `process::exit`), so save-on-change is the only reliable path.

use serde::{Deserialize, Serialize};

use crate::curve::Layers;
use crate::log_line;

pub const SETTINGS_FILE: &str = r"C:\ProgramData\TrontEq\settings.json";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub schema: u32,
    pub dark_mode: bool,
    pub rainbow: bool,
    pub layers: Layers,
    pub zoom: f32,
    pub active_profile: Option<String>,
    pub inspector_tab: String,
    pub show_mode: String,
    /// Auto-cycle through the GL SHOW modes every ~18s while one is active.
    #[serde(default)]
    pub show_shuffle: bool,
    /// Stackable post-fx bitmask (see `glstage::FX_*`). 0 = off.
    #[serde(default)]
    pub show_fx: u32,
    /// Theme name; built-ins resolve by name, derived themes rebuild from
    /// `theme_colors`. Empty = pre-theme settings file -> honor `dark_mode`.
    pub theme_name: String,
    pub theme_colors: Vec<String>,
    /// Discord-style background wash across the chrome panels. Default ON.
    /// No field-level `#[serde(default)]`: an old settings.json missing this
    /// key must fall back to the container-level default below (true), not
    /// bool's own default (false).
    pub gradient: bool,
    /// Gradient v2 knobs (Discord parity): direction dial, color intensity,
    /// number of pegs (1..=4), harmony rule index, preset index
    /// (-1=harmony -2=custom else index into `theme::GRADIENT_PRESETS`).
    /// None of these get a field-level `#[serde(default)]` either, for the
    /// same reason as `gradient` above: their sensible defaults (0.45
    /// intensity, -1 preset, 0.85/0.59 frost, true sync) are NOT the same as
    /// the field type's own `Default::default()` (0.0/0/false), so a
    /// field-level attribute would silently zero them out for anyone
    /// upgrading from a settings.json that predates this feature. The
    /// container-level `#[serde(default)]` above already does the right
    /// thing (missing keys fall back to `AppSettings::default()` below).
    pub gradient_angle: f32,
    pub gradient_intensity: f32,
    pub gradient_pegs: u8,
    pub gradient_harmony: u8,
    pub gradient_preset: i16,
    /// Custom-mode pegs as comma-joined hex ("" = defaults).
    pub gradient_custom: String,
    /// Frost = panel opacity over the wash, per mode (0..1).
    pub frost_dark: f32,
    pub frost_light: f32,
    /// Picking a gradient preset also rethemes the app (accent from its most
    /// saturated stop). Off = the preset only drives the wash, independent
    /// of the chrome accent.
    pub gradient_preset_sync: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            schema: 1,
            dark_mode: true,
            rainbow: true,
            layers: Layers::default(),
            zoom: 1.0,
            active_profile: None,
            inspector_tab: "chain".into(),
            show_mode: "off".into(),
            show_shuffle: false,
            show_fx: 0,
            theme_name: String::new(),
            theme_colors: Vec::new(),
            gradient: true,
            // Mirrors `theme::GradientCfg::default()` + the frost/sync
            // defaults in `theme.rs` — kept as separate literals (not a
            // cross-module call) so this file has no compile-time dependency
            // on the theme module's internals, same as the rest of this struct.
            gradient_angle: 135.0,
            gradient_intensity: 0.45,
            gradient_pegs: 3,
            gradient_harmony: 0,
            gradient_preset: -1,
            gradient_custom: String::new(),
            frost_dark: 0.85,
            frost_light: 0.59,
            gradient_preset_sync: true,
        }
    }
}

impl AppSettings {
    pub fn load() -> Self {
        match std::fs::read_to_string(SETTINGS_FILE) {
            Ok(s) => match serde_json::from_str::<AppSettings>(&s) {
                Ok(mut a) => {
                    a.zoom = a.zoom.clamp(0.5, 2.0);
                    a
                }
                Err(e) => {
                    log_line(&format!("settings: parse failed ({e}) - using defaults"));
                    AppSettings::default()
                }
            },
            Err(_) => AppSettings::default(), // first run
        }
    }

    pub fn save(&self) {
        let json = match serde_json::to_string_pretty(self) {
            Ok(j) => j,
            Err(_) => return,
        };
        let tmp = format!("{SETTINGS_FILE}.tmp");
        let write = std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, SETTINGS_FILE));
        if let Err(e) = write {
            log_line(&format!("settings: save failed: {e}"));
        }
    }
}
