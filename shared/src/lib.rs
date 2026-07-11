//! IPC contract between the Rust GUI and the C++ APO.
//!
//! Lives as a 192-byte file at `C:\ProgramData\TrontEq\state.bin`.
//! Both processes `CreateFileMapping` the same file, `MapViewOfFile`,
//! and read/write the shared struct using a seqlock on `version`.

#![allow(clippy::missing_safety_doc)]

use std::fs::OpenOptions;
use std::sync::atomic::{AtomicU64, Ordering};

use memmap2::{MmapMut, MmapOptions};

pub const STATE_FILE: &str = r"C:\ProgramData\TrontEq\state.bin";
pub const STATE_DIR: &str = r"C:\ProgramData\TrontEq";
/// Full file size: GUI-owned state (seqlock) + APO-owned telemetry block.
pub const STATE_BYTES: usize = 216;
/// State portion only (version + bands + preamp + dynamics). The APO can read
/// this much from an older file even before the GUI extends it to STATE_BYTES.
pub const STATE_CORE_BYTES: usize = 192;
pub const NUM_BANDS: usize = 8;

#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BandKind {
    Peak = 0,
    LowShelf = 1,
    HighShelf = 2,
    HighPass = 3,
    LowPass = 4,
    BandPass = 5,
    Notch = 6,
    AllPass = 7,
}

impl BandKind {
    pub fn from_u32(v: u32) -> BandKind {
        match v {
            1 => BandKind::LowShelf,
            2 => BandKind::HighShelf,
            3 => BandKind::HighPass,
            4 => BandKind::LowPass,
            5 => BandKind::BandPass,
            6 => BandKind::Notch,
            7 => BandKind::AllPass,
            _ => BandKind::Peak,
        }
    }

    /// Right-click cycle order through all band types.
    pub fn next(self) -> BandKind {
        BandKind::from_u32((self as u32 + 1) % 8)
    }

    pub fn short_label(self) -> &'static str {
        match self {
            BandKind::Peak => "peak",
            BandKind::LowShelf => "low shelf",
            BandKind::HighShelf => "high shelf",
            BandKind::HighPass => "high-pass",
            BandKind::LowPass => "low-pass",
            BandKind::BandPass => "band-pass",
            BandKind::Notch => "notch",
            BandKind::AllPass => "all-pass",
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Band {
    pub freq: f32,
    pub gain: f32,
    pub q: f32,
    pub kind: u32,
}

impl Band {
    pub const fn flat(freq: f32) -> Band {
        Band { freq, gain: 0.0, q: 1.0, kind: BandKind::Peak as u32 }
    }
}

/// Dynamics-processing parameters (compressor, limiter, AGC). Appended to the
/// IPC contract after the EQ section. `*_enabled` are u32 to keep 4-byte
/// alignment clean (no implicit padding). Mirror in `apo/src/EqState.h`.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct Dynamics {
    pub comp_enabled: u32,
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    pub comp_attack_ms: f32,
    pub comp_release_ms: f32,
    pub comp_knee_db: f32,
    pub comp_makeup_db: f32,
    pub limiter_enabled: u32,
    pub limiter_ceiling_db: f32,
    pub agc_enabled: u32,
    pub agc_target_db: f32,
    pub agc_max_gain_db: f32,
}

impl Dynamics {
    pub const fn default_passive() -> Dynamics {
        Dynamics {
            comp_enabled: 0,
            comp_threshold_db: -18.0,
            comp_ratio: 2.0,
            comp_attack_ms: 10.0,
            comp_release_ms: 120.0,
            comp_knee_db: 6.0,
            comp_makeup_db: 0.0,
            limiter_enabled: 1,
            limiter_ceiling_db: -0.3,
            agc_enabled: 0,
            agc_target_db: -16.0,
            agc_max_gain_db: 18.0,
        }
    }
}

/// Meter telemetry written by the APO and read by the GUI (APO -> GUI, the reverse
/// of the seqlock state). Plain f32s refreshed every audio buffer; no seqlock —
/// a 4-byte aligned f32 read/write is atomic and minor tearing across fields is
/// cosmetic on a meter. `seq` bumps every buffer so the GUI can tell the APO is
/// actively processing (vs a stale / all-zero block from an un-upgraded APO).
/// Mirror in `apo/src/EqState.h`.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct Telemetry {
    pub seq: u32,     // increments per processed buffer (0 = APO never wrote)
    pub in_peak: f32, // pre-chain (input) peak, linear
    pub in_rms: f32,  // pre-chain RMS, linear
    pub out_peak: f32, // post-chain (output) peak, linear
    pub out_rms: f32,  // post-chain RMS, linear
    pub gr_db: f32,    // dynamics gain reduction, dB >= 0
}

#[repr(C)]
pub struct EqState {
    pub version: AtomicU64,
    pub bands: [Band; NUM_BANDS],
    pub preamp_db: f32, // was bass_boost; gain applied at the head of the chain
    pub bypass: u8,
    pub _pad: [u8; 3],
    pub dynamics: Dynamics,
    pub telemetry: Telemetry, // APO-written; not under the seqlock
}

const _: () = {
    assert!(std::mem::size_of::<EqState>() == STATE_BYTES);
    assert!(std::mem::align_of::<EqState>() == 8);
    // Telemetry must start exactly at the end of the legacy 192-byte state so an
    // un-extended file keeps working and only the appended bytes are new.
    assert!(std::mem::size_of::<Telemetry>() == STATE_BYTES - STATE_CORE_BYTES);
};

pub const DEFAULT_FREQS: [f32; NUM_BANDS] =
    [31.25, 62.5, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0];

impl EqState {
    pub fn default_flat() -> Self {
        let mut bands = [Band::flat(0.0); NUM_BANDS];
        for (i, f) in DEFAULT_FREQS.iter().enumerate() {
            bands[i] = Band::flat(*f);
        }
        EqState {
            version: AtomicU64::new(0),
            bands,
            preamp_db: 0.0,
            bypass: 0,
            _pad: [0; 3],
            dynamics: Dynamics::default_passive(),
            telemetry: Telemetry::default(),
        }
    }
}

/// Writable handle on the memory-mapped state file.
pub struct StateHandle {
    _mmap: MmapMut,
    ptr: *mut EqState,
}

unsafe impl Send for StateHandle {}
unsafe impl Sync for StateHandle {}

impl StateHandle {
    /// Create or open the state file. If empty, initialize with a flat default.
    pub fn open_or_init() -> anyhow::Result<Self> {
        std::fs::create_dir_all(STATE_DIR)?;
        Self::open_at(std::path::Path::new(STATE_FILE))
    }

    /// Open or create a state file at an arbitrary path. `open_or_init` uses the
    /// real `STATE_FILE`; tests pass a temp path so they never touch system state.
    pub fn open_at(path: &std::path::Path) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        let meta = file.metadata()?;
        if meta.len() == 0 {
            file.set_len(STATE_BYTES as u64)?;
        } else if meta.len() < STATE_BYTES as u64 {
            file.set_len(STATE_BYTES as u64)?;
        }

        let mut mmap = unsafe { MmapOptions::new().len(STATE_BYTES).map_mut(&file)? };
        let ptr = mmap.as_mut_ptr() as *mut EqState;

        let needs_init = unsafe {
            let hdr_ver = (&(*ptr).version).load(Ordering::Acquire);
            hdr_ver == 0 && (*ptr).bands.iter().all(|b| b.freq == 0.0)
        };
        if needs_init {
            let init = EqState::default_flat();
            unsafe {
                // version is left at 0; we haven't committed anything yet.
                std::ptr::copy_nonoverlapping(
                    &init.bands as *const _,
                    &mut (*ptr).bands as *mut _,
                    1,
                );
                (*ptr).preamp_db = init.preamp_db;
                (*ptr).bypass = init.bypass;
                (*ptr)._pad = init._pad;
                (*ptr).dynamics = init.dynamics;
                // Publish version 2 (even, committed) so readers see the init.
                (&(*ptr).version).store(2, Ordering::Release);
            }
        }

        Ok(StateHandle { _mmap: mmap, ptr })
    }

    /// Snapshot the current state (seqlock reader, GUI-side).
    pub fn snapshot(&self) -> Snapshot {
        loop {
            unsafe {
                let v1 = (&(*self.ptr).version).load(Ordering::Acquire);
                if v1 & 1 != 0 {
                    // Writer mid-update. Spin.
                    std::hint::spin_loop();
                    continue;
                }
                let bands = (*self.ptr).bands;
                let preamp_db = (*self.ptr).preamp_db;
                let bypass = (*self.ptr).bypass;
                let dynamics = (*self.ptr).dynamics;
                std::sync::atomic::fence(Ordering::Acquire);
                let v2 = (&(*self.ptr).version).load(Ordering::Acquire);
                if v1 == v2 {
                    return Snapshot { version: v2, bands, preamp_db, bypass, dynamics };
                }
            }
        }
    }

    /// Read the APO-written meter telemetry. Volatile (another process writes it)
    /// and lock-free — meter tearing is cosmetic, so no seqlock. `seq == 0` means
    /// the APO has never written (e.g. an APO build without telemetry support).
    pub fn telemetry(&self) -> Telemetry {
        unsafe { std::ptr::read_volatile(std::ptr::addr_of!((*self.ptr).telemetry)) }
    }

    /// Write a new state (seqlock writer).
    pub fn write(&self, update: impl FnOnce(&mut EqStateWrite)) {
        unsafe {
            let ver = (&(*self.ptr).version).fetch_add(1, Ordering::AcqRel); // odd now
            debug_assert!(ver & 1 == 0);
            let mut w = EqStateWrite { ptr: self.ptr };
            update(&mut w);
            std::sync::atomic::fence(Ordering::Release);
            (&(*self.ptr).version).fetch_add(1, Ordering::AcqRel); // even now
        }
    }
}

pub struct EqStateWrite {
    ptr: *mut EqState,
}

impl EqStateWrite {
    pub fn set_bands(&mut self, bands: &[Band; NUM_BANDS]) {
        unsafe { (*self.ptr).bands = *bands; }
    }
    pub fn set_bypass(&mut self, on: bool) {
        unsafe { (*self.ptr).bypass = if on { 1 } else { 0 }; }
    }
    pub fn set_preamp(&mut self, db: f32) {
        unsafe { (*self.ptr).preamp_db = db; }
    }
    pub fn set_dynamics(&mut self, d: &Dynamics) {
        unsafe { (*self.ptr).dynamics = *d; }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub version: u64,
    pub bands: [Band; NUM_BANDS],
    pub preamp_db: f32,
    pub bypass: u8,
    pub dynamics: Dynamics,
}

impl Snapshot {
    pub fn is_bypassed(&self) -> bool { self.bypass != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_abi() {
        // Mirrored byte-for-byte in apo/src/EqState.h.
        assert_eq!(std::mem::size_of::<EqState>(), STATE_BYTES);
        assert_eq!(std::mem::align_of::<EqState>(), 8);
        assert_eq!(std::mem::size_of::<Band>(), 16);
        assert_eq!(std::mem::size_of::<Dynamics>(), 48);
        assert_eq!(std::mem::size_of::<Telemetry>(), 24);
        assert_eq!(STATE_BYTES - STATE_CORE_BYTES, 24);
    }

    #[test]
    fn bandkind_cycle_and_parse() {
        let mut k = BandKind::Peak;
        for _ in 0..8 {
            k = k.next();
        }
        assert_eq!(k, BandKind::Peak); // full cycle returns to start
        assert_eq!(BandKind::from_u32(99), BandKind::Peak); // out-of-range -> Peak
        assert_eq!(BandKind::from_u32(6), BandKind::Notch);
    }

    #[test]
    fn defaults_are_sane() {
        let d = Dynamics::default_passive();
        assert_eq!(d.comp_enabled, 0);
        assert_eq!(d.limiter_enabled, 1); // limiter on by default (clip safety)
        assert_eq!(d.agc_enabled, 0);
        let b = Band::flat(1000.0);
        assert_eq!(b.gain, 0.0);
        assert_eq!(b.kind, BandKind::Peak as u32);
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tronteq_test_{name}.bin"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn fresh_file_inits_to_flat_default() {
        let p = temp_path("init");
        let h = StateHandle::open_at(&p).unwrap();
        let s = h.snapshot();
        assert_eq!(s.version, 2); // committed init
        assert_eq!(s.bypass, 0);
        assert_eq!(s.bands[0].freq, DEFAULT_FREQS[0]);
        assert_eq!(s.bands[7].freq, DEFAULT_FREQS[7]);
        assert_eq!(s.dynamics.limiter_enabled, 1);
        drop(h);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn seqlock_roundtrip() {
        let p = temp_path("roundtrip");
        let h = StateHandle::open_at(&p).unwrap();
        let v0 = h.snapshot().version;

        let mut bands = [Band::flat(0.0); NUM_BANDS];
        bands[2].gain = 6.5;
        bands[2].kind = BandKind::Notch as u32;
        let mut dynamics = Dynamics::default_passive();
        dynamics.comp_enabled = 1;
        dynamics.comp_makeup_db = 4.0;

        h.write(|w| {
            w.set_bands(&bands);
            w.set_preamp(-3.0);
            w.set_bypass(true);
            w.set_dynamics(&dynamics);
        });

        let s = h.snapshot();
        assert!(s.version > v0 && s.version % 2 == 0); // committed (even), advanced
        assert_eq!(s.bands[2].gain, 6.5);
        assert_eq!(s.bands[2].kind, BandKind::Notch as u32);
        assert_eq!(s.preamp_db, -3.0);
        assert!(s.is_bypassed());
        assert_eq!(s.dynamics.comp_enabled, 1);
        assert_eq!(s.dynamics.comp_makeup_db, 4.0);
        drop(h);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn reopen_preserves_state() {
        let p = temp_path("reopen");
        {
            let h = StateHandle::open_at(&p).unwrap();
            h.write(|w| w.set_preamp(7.5));
        }
        // Reopen the same file: a non-zero committed version must NOT re-init to flat.
        let h2 = StateHandle::open_at(&p).unwrap();
        assert_eq!(h2.snapshot().preamp_db, 7.5);
        drop(h2);
        let _ = std::fs::remove_file(&p);
    }
}
