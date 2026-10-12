//! IPC contract between the Rust GUI and the C++ APO.
//!
//! Lives as a 224-byte file at `C:\ProgramData\TrontEq\state.bin`.
//! Both processes `CreateFileMapping` the same file, `MapViewOfFile`,
//! and read/write the shared struct using a seqlock on `version`.

#![allow(clippy::missing_safety_doc)]

#[cfg(windows)]
pub mod endpoints;

use std::fs::OpenOptions;
use std::sync::atomic::{AtomicU64, Ordering};

use memmap2::{MmapMut, MmapOptions};
use serde::{Deserialize, Serialize};

pub const STATE_FILE: &str = r"C:\ProgramData\TrontEq\state.bin";
pub const STATE_DIR: &str = r"C:\ProgramData\TrontEq";
/// Full file size: GUI-owned state (seqlock) + APO-owned telemetry block.
pub const STATE_BYTES: usize = 224;
/// State portion only (version + bands + preamp + dynamics + delay_ms +
/// _reserved). The APO can read this much from an older file even before the
/// GUI extends it to STATE_BYTES.
pub const STATE_CORE_BYTES: usize = 200;
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
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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

impl Default for Band {
    fn default() -> Self {
        Band::flat(1000.0)
    }
}

/// Dynamics-processing parameters (compressor, limiter, AGC). Appended to the
/// IPC contract after the EQ section. `*_enabled` are u32 to keep 4-byte
/// alignment clean (no implicit padding). Mirror in `apo/src/EqState.h`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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

impl Default for Dynamics {
    fn default() -> Self {
        Dynamics::default_passive()
    }
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
    pub delay_ms: f32,     // A/V-sync delay, ms; GUI-written, seqlock'd like preamp
    pub _reserved: [u8; 4], // 8-byte align + future headroom
    pub telemetry: Telemetry, // APO-written; not under the seqlock
}

const _: () = {
    assert!(std::mem::size_of::<EqState>() == STATE_BYTES);
    assert!(std::mem::align_of::<EqState>() == 8);
    // delay_ms sits right after dynamics, at the end of the (new) 200-byte
    // core state, and telemetry starts exactly at STATE_CORE_BYTES so an
    // un-extended file keeps working and only the appended bytes are new.
    assert!(std::mem::offset_of!(EqState, delay_ms) == 192);
    assert!(std::mem::offset_of!(EqState, _reserved) == 196);
    assert!(std::mem::offset_of!(EqState, telemetry) == STATE_CORE_BYTES);
    assert!(std::mem::offset_of!(EqState, telemetry) == 200);
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
            delay_ms: 0.0,
            _reserved: [0; 4],
            telemetry: Telemetry::default(),
        }
    }
}

/// Writable mapping. Rust access is serialized locally and with an OS file lock;
/// the APO still reads the unchanged 224-byte seqlock ABI without taking a lock.
pub struct StateHandle {
    _mmap: MmapMut,
    ptr: *mut EqState,
    file: std::sync::Mutex<std::fs::File>,
}
unsafe impl Send for StateHandle {}
unsafe impl Sync for StateHandle {}

#[derive(Debug)]
pub enum StateError {
    Busy { version: u64 },
    Changed { expected: u64, actual: u64 },
    Locked,
    Invalid(String),
    Io(std::io::Error),
}
impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy { version } => write!(f, "The sound state has an unfinished write (version {version})."),
            Self::Changed { expected, actual } => write!(f, "The sound state changed from version {expected} to {actual}. Reload it before restoring."),
            Self::Locked => write!(f, "Another process is accessing the sound state. Try reloading it."),
            Self::Invalid(error) => write!(f, "Invalid sound state: {error}"),
            Self::Io(error) => write!(f, "Sound state access failed: {error}"),
        }
    }
}
impl std::error::Error for StateError {}

struct FileGuard<'a> { file: std::sync::MutexGuard<'a, std::fs::File> }
impl<'a> FileGuard<'a> {
    fn acquire(file: &'a std::sync::Mutex<std::fs::File>, exclusive: bool) -> Result<Self, StateError> {
        let file = match file.try_lock() {
            Ok(file) => file,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Err(StateError::Locked),
        };
        let lock = if exclusive { file.try_lock() } else { file.try_lock_shared() };
        lock.map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => StateError::Locked,
            std::fs::TryLockError::Error(e) => StateError::Io(e),
        })?;
        Ok(Self { file })
    }
}
impl Drop for FileGuard<'_> { fn drop(&mut self) { let _ = self.file.unlock(); } }

impl StateHandle {
    pub fn open_or_init() -> anyhow::Result<Self> {
        std::fs::create_dir_all(STATE_DIR)?;
        Self::open_at(std::path::Path::new(STATE_FILE))
    }
    pub fn open_at(path: &std::path::Path) -> anyhow::Result<Self> {
        let file = std::sync::Mutex::new(OpenOptions::new().read(true).write(true).create(true).open(path)?);
        let guard = FileGuard::acquire(&file, true)?;
        let old_len = guard.file.metadata()?.len();
        if old_len < STATE_BYTES as u64 { guard.file.set_len(STATE_BYTES as u64)?; }
        let mut mmap = unsafe { MmapOptions::new().len(STATE_BYTES).map_mut(&*guard.file)? };
        let ptr = mmap.as_mut_ptr() as *mut EqState;
        unsafe {
            let version = (*ptr).version.load(Ordering::Acquire);
            if version == 0 && (*ptr).bands.iter().all(|b| b.freq == 0.0) {
                let initial = Snapshot::flat(false);
                publish(ptr, &initial, 2);
            } else if old_len > 0 && old_len < STATE_BYTES as u64 && version & 1 == 0 {
                // Old telemetry occupied delay_ms. Migrate only a committed state;
                // an interrupted legacy file goes through explicit recovery instead.
                let mut prior = read_snapshot(ptr)?;
                prior.delay_ms = 0.0;
                (*ptr).version.store(version.wrapping_add(1), Ordering::Release);
                std::ptr::write_volatile(std::ptr::addr_of_mut!((*ptr)._reserved), [0; 4]);
                std::ptr::write_volatile(std::ptr::addr_of_mut!((*ptr).telemetry), Telemetry::default());
                publish(ptr, &prior, next_even(version));
            }
        }
        drop(guard);
        Ok(Self { _mmap: mmap, ptr, file })
    }

    /// Bounded, coherent read. A dead writer cannot hang its caller.
    pub fn snapshot(&self) -> Result<Snapshot, StateError> {
        let _guard = FileGuard::acquire(&self.file, false)?;
        unsafe { read_snapshot(self.ptr) }
    }
    /// APO owns this block; no Rust writer lock is involved in meter reads.
    pub fn telemetry(&self) -> Telemetry {
        unsafe { std::ptr::read_volatile(std::ptr::addr_of!((*self.ptr).telemetry)) }
    }

    /// Edit a local draft first. A panicking edit cannot leave an odd version or
    /// partially edited state. Only the short infallible copy is under the seqlock.
    pub fn write(&self, update: impl FnOnce(&mut EqStateWrite)) -> Result<u64, StateError> {
        let _guard = FileGuard::acquire(&self.file, true)?;
        let current = unsafe { read_snapshot(self.ptr)? };
        let mut draft = EqStateWrite { snapshot: current };
        update(&mut draft);
        unsafe { self.replace(current.version, &draft.snapshot) }
    }

    /// Explicit user recovery. Never steals an active writer's OS lock or applies
    /// a recovery decision made against a different state version.
    pub fn recover(&self, expected_version: u64, replacement: &Snapshot) -> Result<u64, StateError> {
        replacement.validate_recovery().map_err(StateError::Invalid)?;
        let _guard = FileGuard::acquire(&self.file, true)?;
        unsafe { self.replace(expected_version, replacement) }
    }
    unsafe fn replace(&self, expected: u64, replacement: &Snapshot) -> Result<u64, StateError> {
        (*self.ptr).version.compare_exchange(expected, expected | 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|actual| StateError::Changed { expected, actual })?;
        let committed = next_even(expected);
        publish(self.ptr, replacement, committed);
        Ok(committed)
    }
}

fn next_even(version: u64) -> u64 { (version | 1).wrapping_add(1).max(2) }
unsafe fn read_snapshot(ptr: *mut EqState) -> Result<Snapshot, StateError> {
    for attempt in 0..64 {
        let before = (*ptr).version.load(Ordering::Acquire);
        if before & 1 == 0 {
            let s = Snapshot {
                version: before,
                bands: std::ptr::read_volatile(std::ptr::addr_of!((*ptr).bands)),
                preamp_db: std::ptr::read_volatile(std::ptr::addr_of!((*ptr).preamp_db)),
                bypass: std::ptr::read_volatile(std::ptr::addr_of!((*ptr).bypass)),
                dynamics: std::ptr::read_volatile(std::ptr::addr_of!((*ptr).dynamics)),
                delay_ms: std::ptr::read_volatile(std::ptr::addr_of!((*ptr).delay_ms)),
            };
            std::sync::atomic::fence(Ordering::Acquire);
            if (*ptr).version.load(Ordering::Acquire) == before { return Ok(s); }
        }
        if attempt % 8 == 7 { std::thread::yield_now(); } else { std::hint::spin_loop(); }
    }
    Err(StateError::Busy { version: (*ptr).version.load(Ordering::Acquire) })
}
unsafe fn publish(ptr: *mut EqState, s: &Snapshot, version: u64) {
    std::ptr::write_volatile(std::ptr::addr_of_mut!((*ptr).bands), s.bands);
    std::ptr::write_volatile(std::ptr::addr_of_mut!((*ptr).preamp_db), s.preamp_db);
    std::ptr::write_volatile(std::ptr::addr_of_mut!((*ptr).bypass), s.bypass);
    std::ptr::write_volatile(std::ptr::addr_of_mut!((*ptr).dynamics), s.dynamics);
    std::ptr::write_volatile(std::ptr::addr_of_mut!((*ptr).delay_ms), s.delay_ms);
    // Preserve padding, reserved bytes, and APO-owned telemetry during normal writes/recovery.
    (*ptr).version.store(version, Ordering::Release);
}

pub struct EqStateWrite { snapshot: Snapshot }
impl EqStateWrite {
    pub fn set_bands(&mut self, bands: &[Band; NUM_BANDS]) { self.snapshot.bands = *bands; }
    pub fn set_bypass(&mut self, on: bool) { self.snapshot.bypass = u8::from(on); }
    pub fn set_preamp(&mut self, db: f32) { self.snapshot.preamp_db = db; }
    pub fn set_dynamics(&mut self, d: &Dynamics) { self.snapshot.dynamics = *d; }
    pub fn set_delay(&mut self, ms: f32) { self.snapshot.delay_ms = ms; }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u64,
    pub bands: [Band; NUM_BANDS],
    pub preamp_db: f32,
    pub bypass: u8,
    pub dynamics: Dynamics,
    pub delay_ms: f32,
}
impl Snapshot {
    pub fn is_bypassed(&self) -> bool { self.bypass != 0 }
    pub fn flat(bypass: bool) -> Self {
        Self { version: 2, bands: std::array::from_fn(|i| Band::flat(DEFAULT_FREQS[i])),
            preamp_db: 0.0, bypass: u8::from(bypass), dynamics: Dynamics::default_passive(), delay_ms: 0.0 }
    }
    pub fn validate_recovery(&self) -> Result<(), String> {
        let valid = |x: f32, min: f32, max: f32| x.is_finite() && (min..=max).contains(&x);
        if self.version == 0 || self.version & 1 != 0 || self.bypass > 1 || !valid(self.preamp_db, -60.0, 60.0) || !valid(self.delay_ms, 0.0, 2000.0) {
            return Err("invalid version, preamp, bypass, or delay".into());
        }
        if self.bands.iter().any(|b| !valid(b.freq, 1.0, 192_000.0) || !valid(b.gain, -60.0, 60.0) || !valid(b.q, 0.01, 100.0) || b.kind > 7) {
            return Err("invalid EQ band".into());
        }
        let d = &self.dynamics;
        if d.comp_enabled > 1 || d.limiter_enabled > 1 || d.agc_enabled > 1 ||
            !valid(d.comp_threshold_db, -120.0, 24.0) || !valid(d.comp_ratio, 1.0, 100.0) ||
            !valid(d.comp_attack_ms, 0.0, 10_000.0) || !valid(d.comp_release_ms, 0.0, 10_000.0) ||
            !valid(d.comp_knee_db, 0.0, 60.0) || !valid(d.comp_makeup_db, -60.0, 60.0) ||
            !valid(d.limiter_ceiling_db, -120.0, 12.0) || !valid(d.agc_target_db, -120.0, 12.0) || !valid(d.agc_max_gain_db, 0.0, 60.0) {
            return Err("invalid dynamics settings".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_abi() {
        // Mirrored byte-for-byte in apo/src/EqState.h.
        assert_eq!(STATE_BYTES, 224);
        assert_eq!(STATE_CORE_BYTES, 200);
        assert_eq!(std::mem::size_of::<EqState>(), STATE_BYTES);
        assert_eq!(std::mem::align_of::<EqState>(), 8);
        assert_eq!(std::mem::size_of::<Band>(), 16);
        assert_eq!(std::mem::size_of::<Dynamics>(), 48);
        assert_eq!(std::mem::size_of::<Telemetry>(), 24);
        assert_eq!(STATE_BYTES - STATE_CORE_BYTES, 24);

        // Exact offset math from the spec's layout table.
        assert_eq!(std::mem::offset_of!(EqState, dynamics), 144);
        assert_eq!(std::mem::offset_of!(EqState, delay_ms), 192);
        assert_eq!(std::mem::offset_of!(EqState, _reserved), 196);
        assert_eq!(std::mem::offset_of!(EqState, telemetry), 200);
        assert_eq!(std::mem::offset_of!(EqState, telemetry), STATE_CORE_BYTES);
    }

    #[test]
    fn band_dynamics_serde_roundtrip() {
        let mut b = Band::flat(123.0);
        b.gain = -4.5;
        b.kind = BandKind::Notch as u32;
        let mut d = Dynamics::default_passive();
        d.comp_enabled = 1;
        d.comp_ratio = 3.5;

        let bs = serde_json::to_string(&b).unwrap();
        let ds = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Band>(&bs).unwrap(), b);
        assert_eq!(serde_json::from_str::<Dynamics>(&ds).unwrap(), d);

        // Forward-compat: fields missing from an old file fall back to defaults
        // instead of failing the whole parse.
        let d2: Dynamics = serde_json::from_str(r#"{"comp_ratio": 9.0}"#).unwrap();
        assert_eq!(d2.comp_ratio, 9.0);
        assert_eq!(d2.limiter_enabled, Dynamics::default_passive().limiter_enabled);
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
        let s = h.snapshot().unwrap();
        assert_eq!(s.version, 2); // committed init
        assert_eq!(s.bypass, 0);
        assert_eq!(s.bands[0].freq, DEFAULT_FREQS[0]);
        assert_eq!(s.bands[7].freq, DEFAULT_FREQS[7]);
        assert_eq!(s.dynamics.limiter_enabled, 1);
        assert_eq!(s.delay_ms, 0.0);
        drop(h);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn seqlock_roundtrip() {
        let p = temp_path("roundtrip");
        let h = StateHandle::open_at(&p).unwrap();
        let v0 = h.snapshot().unwrap().version;

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
            w.set_delay(437.5);
        }).unwrap();

        let s = h.snapshot().unwrap();
        assert!(s.version > v0 && s.version % 2 == 0); // committed (even), advanced
        assert_eq!(s.bands[2].gain, 6.5);
        assert_eq!(s.bands[2].kind, BandKind::Notch as u32);
        assert_eq!(s.preamp_db, -3.0);
        assert!(s.is_bypassed());
        assert_eq!(s.dynamics.comp_enabled, 1);
        assert_eq!(s.dynamics.comp_makeup_db, 4.0);
        assert_eq!(s.delay_ms, 437.5);
        drop(h);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn reopen_preserves_state() {
        let p = temp_path("reopen");
        {
            let h = StateHandle::open_at(&p).unwrap();
            h.write(|w| w.set_preamp(7.5)).unwrap();
        }
        // Reopen the same file: a non-zero committed version must NOT re-init to flat.
        let h2 = StateHandle::open_at(&p).unwrap();
        assert_eq!(h2.snapshot().unwrap().preamp_db, 7.5);
        drop(h2);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn migration_from_pre_delay_file_zeroes_phantom_delay() {
        let p = temp_path("migration");
        // Simulate a pre-224 (216-byte) state.bin: committed version (so
        // open_at doesn't treat it as a fresh/empty file), zeroed bands/preamp/
        // bypass/pad/dynamics, and NONZERO bytes at offset 192 -- exactly
        // where the old telemetry block lived and where delay_ms now sits.
        // A naive grow-in-place would let those stale bytes leak in as a
        // phantom delay (and a stale telemetry `seq` misread as "APO live").
        const OLD_STATE_BYTES: usize = 216;
        let mut raw = vec![0u8; OLD_STATE_BYTES];
        raw[0..8].copy_from_slice(&2u64.to_le_bytes()); // committed, not fresh
        for (i, b) in raw[192..OLD_STATE_BYTES].iter_mut().enumerate() {
            *b = (i as u8) + 1; // nonzero old-telemetry bytes at the old offset
        }
        std::fs::write(&p, &raw).unwrap();

        let h = StateHandle::open_at(&p).unwrap();
        let s = h.snapshot().unwrap();
        assert_eq!(s.delay_ms, 0.0);
        // The relocated telemetry block must be cleared too, not carry stale
        // bytes forward (a nonzero seq would look like "the APO is live").
        assert_eq!(h.telemetry().seq, 0);
        drop(h);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn abandoned_write_is_bounded_and_recovery_preserves_telemetry() {
        let p = temp_path("abandoned");
        let h = StateHandle::open_at(&p).unwrap();
        let mut good = h.snapshot().unwrap(); good.preamp_db = -7.0; good.delay_ms = 250.0; good.bypass = 1;
        unsafe {
            (*h.ptr).version.store(3, Ordering::Release);
            (*h.ptr).preamp_db = 42.0; // partial interrupted update, not a committed sound
            (*h.ptr).telemetry.seq = 1234;
            (*h.ptr).telemetry.out_peak = 0.7;
            (*h.ptr)._reserved = [1, 2, 3, 4];
        }
        let start = std::time::Instant::now();
        assert!(matches!(h.snapshot(), Err(StateError::Busy { version: 3 })));
        assert!(start.elapsed() < std::time::Duration::from_millis(100));
        assert!(h.write(|w| w.set_preamp(0.0)).is_err());
        assert!(matches!(h.recover(5, &good), Err(StateError::Changed { .. })));
        assert_eq!(h.recover(3, &good).unwrap(), 4);
        let fixed = h.snapshot().unwrap(); assert_eq!(fixed.preamp_db, -7.0); assert_eq!(fixed.delay_ms, 250.0); assert!(fixed.is_bypassed());
        assert_eq!(h.telemetry().seq, 1234); assert_eq!(h.telemetry().out_peak, 0.7);
        assert_eq!(unsafe { (*h.ptr)._reserved }, [1, 2, 3, 4]);
        drop(h); let _ = std::fs::remove_file(p);
    }
    #[test]
    fn a_panicking_draft_changes_nothing_and_releases_the_writer_lock() {
        let p = temp_path("panic-draft"); let h = StateHandle::open_at(&p).unwrap(); let before = h.snapshot().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = h.write(|w| { w.set_preamp(8.0); w.set_delay(999.0); panic!("fixture"); });
        }));
        assert!(result.is_err()); assert_eq!(h.snapshot().unwrap(), before);
        h.write(|w| w.set_preamp(-2.0)).unwrap(); assert_eq!(h.snapshot().unwrap().preamp_db, -2.0);
        drop(h); let _ = std::fs::remove_file(p);
    }
    #[test]
    fn an_active_writer_cannot_be_stolen_by_recovery_or_another_writer() {
        let p = temp_path("writer-lock"); let first = StateHandle::open_at(&p).unwrap(); let second = StateHandle::open_at(&p).unwrap();
        let prior = first.snapshot().unwrap(); let guard = FileGuard::acquire(&first.file, true).unwrap();
        assert!(matches!(second.snapshot(), Err(StateError::Locked)));
        assert!(matches!(second.write(|w| w.set_bypass(true)), Err(StateError::Locked)));
        assert!(matches!(second.recover(prior.version, &Snapshot::flat(true)), Err(StateError::Locked)));
        drop(guard); assert_eq!(second.snapshot().unwrap(), prior);
        second.write(|w| w.set_preamp(-3.0)).unwrap(); assert_eq!(first.snapshot().unwrap().preamp_db, -3.0);
        drop(first); drop(second); let _ = std::fs::remove_file(p);
    }
    #[test]
    fn invalid_recovery_data_is_rejected_without_touching_state() {
        let p = temp_path("invalid-recovery"); let h = StateHandle::open_at(&p).unwrap(); let before = h.snapshot().unwrap();
        let mut bad = before; bad.bands[0].gain = f32::NAN;
        assert!(matches!(h.recover(before.version, &bad), Err(StateError::Invalid(_))));
        assert_eq!(h.snapshot().unwrap(), before); drop(h); let _ = std::fs::remove_file(p);
    }
}
