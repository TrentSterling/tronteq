//! Built-in per-component presets + per-component reset-to-default.
//! Each apply fn mutates only its component's fields, leaving the rest alone.

use tronteq_shared::{Band, BandKind, Dynamics, DEFAULT_FREQS, NUM_BANDS};

fn def() -> Dynamics {
    Dynamics::default_passive()
}

// ---- Compressor (3 presets) ------------------------------------------------

pub const COMP_PRESETS: [&str; 3] = ["Gentle", "Punchy", "Crush"];

pub fn apply_comp(d: &mut Dynamics, name: &str) {
    d.comp_enabled = 1;
    match name {
        "Gentle" => set_comp(d, -24.0, 2.0, 30.0, 250.0, 6.0, 2.0),
        "Punchy" => set_comp(d, -18.0, 4.0, 5.0, 120.0, 3.0, 4.0),
        "Crush" => set_comp(d, -30.0, 10.0, 1.0, 80.0, 2.0, 8.0),
        _ => {}
    }
}

fn set_comp(d: &mut Dynamics, thr: f32, ratio: f32, atk: f32, rel: f32, knee: f32, makeup: f32) {
    d.comp_threshold_db = thr;
    d.comp_ratio = ratio;
    d.comp_attack_ms = atk;
    d.comp_release_ms = rel;
    d.comp_knee_db = knee;
    d.comp_makeup_db = makeup;
}

pub fn reset_comp(d: &mut Dynamics) {
    let z = def();
    d.comp_enabled = 0;
    set_comp(d, z.comp_threshold_db, z.comp_ratio, z.comp_attack_ms, z.comp_release_ms, z.comp_knee_db, z.comp_makeup_db);
}

// ---- Limiter (3 ceiling presets) -------------------------------------------

pub const LIMITER_PRESETS: [&str; 3] = ["Transparent", "Loud", "Brickwall"];

pub fn apply_limiter(d: &mut Dynamics, name: &str) {
    d.limiter_enabled = 1;
    d.limiter_ceiling_db = match name {
        "Transparent" => -1.0,
        "Loud" => -0.3,
        "Brickwall" => -0.1,
        _ => d.limiter_ceiling_db,
    };
}

pub fn reset_limiter(d: &mut Dynamics) {
    let z = def();
    d.limiter_enabled = z.limiter_enabled;
    d.limiter_ceiling_db = z.limiter_ceiling_db;
}

// ---- AGC (3 presets) -------------------------------------------------------

pub const AGC_PRESETS: [&str; 3] = ["Subtle", "Movie night", "Everything loud"];

pub fn apply_agc(d: &mut Dynamics, name: &str) {
    d.agc_enabled = 1;
    match name {
        "Subtle" => { d.agc_target_db = -20.0; d.agc_max_gain_db = 9.0; }
        "Movie night" => { d.agc_target_db = -16.0; d.agc_max_gain_db = 18.0; }
        "Everything loud" => { d.agc_target_db = -12.0; d.agc_max_gain_db = 30.0; }
        _ => {}
    }
}

pub fn reset_agc(d: &mut Dynamics) {
    let z = def();
    d.agc_enabled = 0;
    d.agc_target_db = z.agc_target_db;
    d.agc_max_gain_db = z.agc_max_gain_db;
}

// ---- EQ curve presets ------------------------------------------------------

pub const EQ_PRESETS: [&str; 5] = ["Bass", "Vocal", "V-shape", "Treble", "Tinny"];

pub fn apply_eq(bands: &mut [Band; NUM_BANDS], name: &str) {
    // gains per band, low→high (31, 62, 125, 250, 500, 1k, 2k, 4k Hz)
    let gains: [f32; NUM_BANDS] = match name {
        "Bass" => [8.0, 6.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        "Vocal" => [-2.0, -1.0, 0.0, 1.0, 3.0, 4.0, 2.0, 0.0],
        "V-shape" => [6.0, 4.0, 1.0, -3.0, -4.0, -2.0, 2.0, 5.0],
        "Treble" => [0.0, 0.0, 0.0, 0.0, 1.0, 3.0, 5.0, 7.0],
        // intentionally harsh/tinny: gut the low end, jack the mids/presence
        "Tinny" => [-24.0, -24.0, -14.0, 0.0, 14.0, 18.0, 12.0, 4.0],
        _ => [0.0; NUM_BANDS],
    };
    for (i, f) in DEFAULT_FREQS.iter().enumerate() {
        bands[i] = Band {
            freq: *f,
            gain: gains[i],
            q: 1.0,
            kind: BandKind::Peak as u32,
        };
    }
}
