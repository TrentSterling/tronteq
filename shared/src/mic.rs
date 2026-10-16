//! Mic stack: the input-side twin of the output EQ.
//!
//! Lives as a 256-byte file at `C:\ProgramData\TrontEq\mic.bin`. The first 224
//! bytes are exactly the output `EqState` layout (seqlock version, 8 bands,
//! preamp, bypass, dynamics, telemetry), so the APO reuses the same reader and
//! DSP. A noise-gate block follows at offset 224 and commits under the same
//! seqlock version. Mirrored in `apo/src/EqState.h` (`Gate`, `MicState`).
//!
//! Chain inside the capture APO: preamp -> 8-band EQ -> gate -> AGC ->
//! compressor -> limiter. Any program reading the mic in shared mode (Axela,
//! Discord, OBS) hears the processed signal.

use serde::{Deserialize, Serialize};

use crate::{
    read_coherent, Band, BandKind, Dynamics, EqState, Snapshot, StateError, StateHandle, Telemetry,
    NUM_BANDS, STATE_BYTES, STATE_DIR,
};

pub const MIC_STATE_FILE: &str = r"C:\ProgramData\TrontEq\mic.bin";
/// 224-byte `EqState` + 32-byte `Gate`.
pub const MIC_STATE_BYTES: usize = 256;

/// Downward expander / noise gate. Opens above `threshold_db`, closes below
/// `threshold_db - hysteresis_db` once `hold_ms` has passed, and attenuates by
/// `range_db` (not to silence) while closed. Mirror in `apo/src/EqState.h`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gate {
    pub enabled: u32,
    pub threshold_db: f32,
    pub hysteresis_db: f32,
    pub range_db: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
    pub _reserved: u32,
}

impl Default for Gate {
    fn default() -> Self {
        Gate {
            enabled: 0,
            threshold_db: -50.0,
            hysteresis_db: 6.0,
            range_db: -24.0,
            attack_ms: 2.0,
            hold_ms: 200.0,
            release_ms: 250.0,
            _reserved: 0,
        }
    }
}

#[repr(C)]
pub struct MicState {
    pub eq: EqState,
    pub gate: Gate,
}

const _: () = {
    assert!(std::mem::size_of::<Gate>() == 32);
    assert!(std::mem::size_of::<MicState>() == MIC_STATE_BYTES);
    assert!(std::mem::offset_of!(MicState, gate) == STATE_BYTES);
};

/// Everything the mic chain needs, as one value the GUI and CLI edit whole.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MicSettings {
    pub bypass: bool,
    pub preamp_db: f32,
    pub bands: [Band; NUM_BANDS],
    pub gate: Gate,
    pub dynamics: Dynamics,
}

impl Default for MicSettings {
    fn default() -> Self {
        preset("whisper").expect("whisper preset")
    }
}

impl MicSettings {
    fn from_parts(s: &Snapshot, gate: Gate) -> Self {
        MicSettings { bypass: s.bypass != 0, preamp_db: s.preamp_db, bands: s.bands, gate, dynamics: s.dynamics }
    }
    fn core(&self, version: u64) -> Snapshot {
        Snapshot {
            version,
            bands: self.bands,
            preamp_db: self.preamp_db,
            bypass: u8::from(self.bypass),
            dynamics: self.dynamics,
            delay_ms: 0.0, // no A/V delay on a microphone
        }
    }

    /// Reject values the APO would have to clamp, so a bad preset file fails
    /// loudly instead of silently sounding different.
    pub fn validate(&self) -> Result<(), String> {
        let mut core = self.core(2);
        core.bypass = 0;
        core.validate_recovery()?;
        let g = &self.gate;
        let valid = |x: f32, min: f32, max: f32| x.is_finite() && (min..=max).contains(&x);
        if g.enabled > 1 || !valid(g.threshold_db, -90.0, 0.0) || !valid(g.hysteresis_db, 0.0, 30.0)
            || !valid(g.range_db, -90.0, 0.0) || !valid(g.attack_ms, 0.1, 500.0)
            || !valid(g.hold_ms, 0.0, 5000.0) || !valid(g.release_ms, 1.0, 5000.0)
        {
            return Err("invalid gate settings".into());
        }
        Ok(())
    }
}

/// Built-in starting points. `whisper` (the default) is for speech recognition
/// (Axela on the Yeti): low cut, a small trim for a hot mic, a slow soft gate
/// that never clips word onsets, and no compression. `voice` adds tone and light
/// compression for people listening; `broadcast` is the tighter streaming sound.
pub const PRESETS: [&str; 4] = ["whisper", "voice", "broadcast", "clean"];

pub fn preset(name: &str) -> Option<MicSettings> {
    let band = |freq: f32, gain: f32, q: f32, kind: BandKind| Band { freq, gain, q, kind: kind as u32 };
    let tone = |mud: f32, presence: f32, air: f32| [
        band(80.0, 0.0, 0.707, BandKind::HighPass),   // rumble, desk thumps
        band(120.0, 0.0, 0.707, BandKind::LowShelf),  // proximity warmth (spare)
        band(250.0, mud, 1.0, BandKind::Peak),        // boxiness / mud
        band(500.0, 0.0, 1.0, BandKind::Peak),
        band(1_000.0, 0.0, 1.0, BandKind::Peak),
        band(2_500.0, 0.0, 1.0, BandKind::Peak),
        band(4_500.0, presence, 1.0, BandKind::Peak), // consonants / intelligibility
        band(10_000.0, air, 0.707, BandKind::HighShelf),
    ];
    let mut dynamics = Dynamics::default_passive();
    dynamics.limiter_ceiling_db = -1.0;
    let settings = match name {
        "whisper" => {
            // Whisper wants an untouched spectrum and natural dynamics; the Yeti
            // already peaks hot (speech RMS 0.15 to 0.4), so trim rather than boost.
            let gate = Gate { enabled: 1, threshold_db: -50.0, range_db: -18.0, attack_ms: 5.0,
                hold_ms: 250.0, release_ms: 300.0, ..Gate::default() };
            MicSettings { bypass: false, preamp_db: -3.0, bands: tone(0.0, 0.0, 0.0), gate, dynamics }
        }
        "voice" => {
            dynamics.comp_enabled = 1;
            dynamics.comp_threshold_db = -24.0;
            dynamics.comp_ratio = 3.0;
            dynamics.comp_attack_ms = 5.0;
            dynamics.comp_release_ms = 120.0;
            dynamics.comp_knee_db = 6.0;
            dynamics.comp_makeup_db = 6.0;
            MicSettings { bypass: false, preamp_db: 0.0, bands: tone(-2.0, 3.0, 0.0),
                gate: Gate { enabled: 1, ..Gate::default() }, dynamics }
        }
        "broadcast" => {
            dynamics.comp_enabled = 1;
            dynamics.comp_threshold_db = -20.0;
            dynamics.comp_ratio = 4.0;
            dynamics.comp_attack_ms = 3.0;
            dynamics.comp_release_ms = 100.0;
            dynamics.comp_knee_db = 4.0;
            dynamics.comp_makeup_db = 8.0;
            let gate = Gate { enabled: 1, threshold_db: -45.0, range_db: -60.0, hold_ms: 150.0, release_ms: 150.0, ..Gate::default() };
            MicSettings { bypass: false, preamp_db: 0.0, bands: tone(-3.0, 4.0, 2.0), gate, dynamics }
        }
        "clean" => MicSettings { bypass: false, preamp_db: 0.0, bands: tone(0.0, 0.0, 0.0),
            gate: Gate::default(), dynamics },
        _ => return None,
    };
    Some(settings)
}

/// Writable mapping of `mic.bin`, with the same locking and seqlock rules as
/// the output `StateHandle`.
pub struct MicHandle {
    inner: StateHandle,
}

impl MicHandle {
    pub fn open_or_init() -> anyhow::Result<Self> {
        std::fs::create_dir_all(STATE_DIR)?;
        Self::open_at(std::path::Path::new(MIC_STATE_FILE))
    }

    pub fn open_at(path: &std::path::Path) -> anyhow::Result<Self> {
        let inner = StateHandle::open_layout(path, MIC_STATE_BYTES, |ptr| unsafe {
            let initial = MicSettings::default();
            std::ptr::write_volatile(gate_ptr(ptr), initial.gate);
            crate::publish(ptr, &initial.core(2), 2);
        })?;
        Ok(Self { inner })
    }

    pub fn snapshot(&self) -> Result<MicSettings, StateError> {
        let _guard = self.inner.lock(false)?;
        let ptr = self.inner.raw();
        unsafe { read_coherent(ptr, |s| MicSettings::from_parts(&s, std::ptr::read_volatile(gate_ptr(ptr)))) }
    }

    /// Replace the whole mic chain in one committed write.
    pub fn write(&self, settings: &MicSettings) -> Result<u64, StateError> {
        settings.validate().map_err(StateError::Invalid)?;
        let _guard = self.inner.lock(true)?;
        let ptr = self.inner.raw();
        let current = unsafe { read_coherent(ptr, |s| s.version)? };
        let gate = settings.gate;
        unsafe {
            self.inner.replace_with(current, &settings.core(current), |p| {
                std::ptr::write_volatile(gate_ptr(p), gate);
            })
        }
    }

    /// Meter levels the capture APO writes back (input, output, gain reduction).
    pub fn telemetry(&self) -> Telemetry {
        self.inner.telemetry()
    }
}

unsafe fn gate_ptr(ptr: *mut EqState) -> *mut Gate {
    std::ptr::addr_of_mut!((*(ptr as *mut MicState)).gate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tronteq_mic_test_{name}.bin"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn layout_matches_abi() {
        // Mirrored byte-for-byte in apo/src/EqState.h.
        assert_eq!(std::mem::size_of::<Gate>(), 32);
        assert_eq!(std::mem::size_of::<MicState>(), 256);
        assert_eq!(std::mem::offset_of!(MicState, gate), 224);
        assert_eq!(std::mem::offset_of!(Gate, threshold_db), 4);
        assert_eq!(std::mem::offset_of!(Gate, release_ms), 24);
    }

    #[test]
    fn presets_are_valid() {
        for name in PRESETS {
            let p = preset(name).unwrap();
            p.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        assert!(preset("nope").is_none());
        assert_eq!(MicSettings::default(), preset("whisper").unwrap());
        let w = preset("whisper").unwrap();
        assert_eq!(w.dynamics.comp_enabled, 0); // no compression for speech recognition
        assert!(w.preamp_db < 0.0 && w.gate.release_ms >= 200.0);
    }

    #[test]
    fn fresh_file_starts_on_whisper_preset() {
        let p = temp_path("init");
        let h = MicHandle::open_at(&p).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().len(), MIC_STATE_BYTES as u64);
        assert_eq!(h.snapshot().unwrap(), preset("whisper").unwrap());
        drop(h);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn write_roundtrips_gate_with_core_and_survives_reopen() {
        let p = temp_path("roundtrip");
        {
            let h = MicHandle::open_at(&p).unwrap();
            let mut s = preset("broadcast").unwrap();
            s.gate.threshold_db = -38.5;
            s.preamp_db = 4.0;
            s.bypass = true;
            h.write(&s).unwrap();
            assert_eq!(h.snapshot().unwrap(), s);
        }
        let h = MicHandle::open_at(&p).unwrap();
        let s = h.snapshot().unwrap();
        assert_eq!(s.gate.threshold_db, -38.5);
        assert_eq!(s.preamp_db, 4.0);
        assert!(s.bypass);
        drop(h);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn invalid_gate_is_rejected_without_touching_state() {
        let p = temp_path("invalid");
        let h = MicHandle::open_at(&p).unwrap();
        let before = h.snapshot().unwrap();
        let mut bad = before;
        bad.gate.range_db = f32::NAN;
        assert!(matches!(h.write(&bad), Err(StateError::Invalid(_))));
        assert_eq!(h.snapshot().unwrap(), before);
        drop(h);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn settings_json_fills_missing_fields() {
        let s: MicSettings = serde_json::from_str(r#"{"preamp_db": 3.0, "gate": {"threshold_db": -40.0}}"#).unwrap();
        assert_eq!(s.preamp_db, 3.0);
        assert_eq!(s.gate.threshold_db, -40.0);
        assert_eq!(s.gate.hold_ms, Gate::default().hold_ms);
    }
}
