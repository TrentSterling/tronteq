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
