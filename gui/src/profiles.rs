//! Saved sound profiles — the full audible chain (8 EQ bands + preamp +
//! compressor/limiter/AGC incl. enabled flags) as JSON, one file per profile,
//! in `C:\ProgramData\TrontEq\profiles\`. Bypass is deliberately excluded (it's
//! the live A/B toggle), and so is all UI state: profile == sound save data,
//! settings.json == general save data.
//!
//! Factory profiles are seeded on first run and are ordinary files afterward —
//! rename/overwrite/delete like anything the user saved.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tronteq_shared::{Band, Dynamics, DEFAULT_FREQS, NUM_BANDS};

use crate::log_line;
use crate::presets;

pub const PROFILE_DIR: &str = r"C:\ProgramData\TrontEq\profiles";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub schema: u32,
    pub name: String,
    /// Chip position in the toolbar (ties break by name).
    pub order: u32,
    pub bands: [Band; NUM_BANDS],
    pub preamp_db: f32,
    pub dynamics: Dynamics,
}

impl Default for Profile {
    fn default() -> Self {
        let mut bands = [Band::default(); NUM_BANDS];
        for (i, f) in DEFAULT_FREQS.iter().enumerate() {
            bands[i] = Band::flat(*f);
        }
        Profile {
            schema: 1,
            name: String::new(),
            order: 0,
            bands,
            preamp_db: 0.0,
            dynamics: Dynamics::default_passive(),
        }
    }
}

impl Profile {
    /// Does the live state still match this profile exactly? (The dirty check.
    /// Exact f32 compare is right here — values only ever flow through these
    /// same structs, never through lossy math.)
    pub fn matches(&self, bands: &[Band; NUM_BANDS], preamp_db: f32, dynamics: &Dynamics) -> bool {
        self.bands == *bands && self.preamp_db == preamp_db && self.dynamics == *dynamics
    }
}

pub struct Entry {
    pub path: PathBuf,
    pub profile: Profile,
}

pub struct ProfileStore {
    dir: PathBuf,
    pub entries: Vec<Entry>,
}

impl ProfileStore {
    /// Scan the profile dir, creating + seeding it on first run. Corrupt files
    /// are skipped and logged, never deleted.
    pub fn load() -> Self {
        let dir = PathBuf::from(PROFILE_DIR);
        let mut store = ProfileStore { dir, entries: Vec::new() };
        if let Err(e) = std::fs::create_dir_all(&store.dir) {
            log_line(&format!("profiles: create_dir failed: {e}"));
            return store;
        }
        match std::fs::read_dir(&store.dir) {
            Ok(read) => {
                for f in read.flatten() {
                    let path = f.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    let parsed = std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|s| serde_json::from_str::<Profile>(&s).map_err(|e| e.to_string()));
                    match parsed {
                        Ok(p) if !p.name.trim().is_empty() => {
                            store.entries.push(Entry { path, profile: p })
                        }
                        Ok(_) => log_line(&format!("profiles: {} has no name - skipped", path.display())),
                        Err(e) => log_line(&format!("profiles: failed to load {}: {e}", path.display())),
                    }
                }
            }
            Err(e) => {
                log_line(&format!("profiles: read_dir failed: {e}"));
                return store;
            }
        }
        if store.entries.is_empty() {
            store.seed();
        }
        store.sort();
        store
    }

    fn sort(&mut self) {
        self.entries.sort_by(|a, b| {
            a.profile
                .order
                .cmp(&b.profile.order)
                .then_with(|| a.profile.name.to_lowercase().cmp(&b.profile.name.to_lowercase()))
        });
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.entries.iter().map(|e| &e.profile).find(|p| p.name == name)
    }

    pub fn next_order(&self) -> u32 {
        self.entries.iter().map(|e| e.profile.order).max().map_or(0, |m| m + 1)
    }

    /// Insert or overwrite by name. Writes tmp + rename so a crash mid-save
    /// can't leave a half-written profile.
    pub fn save(&mut self, profile: Profile) {
        let idx = self.entries.iter().position(|e| e.profile.name == profile.name);
        let path = match idx {
            Some(i) => self.entries[i].path.clone(),
            None => self.free_path(&profile.name),
        };
        if let Err(e) = write_json(&path, &profile) {
            log_line(&format!("profiles: save {} failed: {e}", path.display()));
            return;
        }
        match idx {
            Some(i) => self.entries[i].profile = profile,
            None => self.entries.push(Entry { path, profile }),
        }
        self.sort();
    }

    pub fn delete(&mut self, name: &str) {
        if let Some(i) = self.entries.iter().position(|e| e.profile.name == name) {
            let e = self.entries.remove(i);
            if let Err(err) = std::fs::remove_file(&e.path) {
                log_line(&format!("profiles: delete {} failed: {err}", e.path.display()));
            }
        }
    }

    /// Rename in place (same file, new `name` field). No-op if the new name is
    /// empty or already taken — the UI disables the button in those cases too.
    pub fn rename(&mut self, old: &str, new: &str) {
        let new = new.trim();
        if new.is_empty() || (new != old && self.get(new).is_some()) {
            return;
        }
        if let Some(i) = self.entries.iter().position(|e| e.profile.name == old) {
            self.entries[i].profile.name = new.to_string();
            let (path, profile) = (self.entries[i].path.clone(), self.entries[i].profile.clone());
            if let Err(e) = write_json(&path, &profile) {
                log_line(&format!("profiles: rename write {} failed: {e}", path.display()));
            }
            self.sort();
        }
    }

    /// First unused `slug.json` / `slug-2.json` / … in the dir.
    fn free_path(&self, name: &str) -> PathBuf {
        let slug = slugify(name);
        let mut path = self.dir.join(format!("{slug}.json"));
        let mut n = 2;
        while path.exists() {
            path = self.dir.join(format!("{slug}-{n}.json"));
            n += 1;
        }
        path
    }

    /// The six factory profiles (order 0..5), written as ordinary files.
    fn seed(&mut self) {
        let names = std::iter::once("Flat").chain(presets::EQ_PRESETS);
        for (i, name) in names.enumerate() {
            let mut p = Profile::default();
            p.name = name.to_string();
            p.order = i as u32;
            presets::apply_eq(&mut p.bands, name); // "Flat" hits the fallback arm -> all-zero gains
            self.save(p);
        }
        log_line("profiles: seeded factory profiles");
    }
}

fn write_json(path: &Path, profile: &Profile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    // std::fs::rename replaces the destination on Windows (MOVEFILE_REPLACE_EXISTING).
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn slugify(name: &str) -> String {
    let mut s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "profile".into() } else { s }
}
