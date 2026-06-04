//! IPC contract between the Rust GUI and the C++ APO.
//!
//! Lives as a 144-byte file at `C:\ProgramData\TrontEq\state.bin`.
//! Both processes `CreateFileMapping` the same file, `MapViewOfFile`,
//! and read/write the shared struct using a seqlock on `version`.

#![allow(clippy::missing_safety_doc)]

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use memmap2::{MmapMut, MmapOptions};

pub const STATE_FILE: &str = r"C:\ProgramData\TrontEq\state.bin";
pub const STATE_DIR: &str = r"C:\ProgramData\TrontEq";
pub const STATE_BYTES: usize = 192;
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

#[repr(C)]
pub struct EqState {
    pub version: AtomicU64,
    pub bands: [Band; NUM_BANDS],
    pub preamp_db: f32, // was bass_boost; gain applied at the head of the chain
    pub bypass: u8,
    pub _pad: [u8; 3],
    pub dynamics: Dynamics,
}

const _: () = {
    assert!(std::mem::size_of::<EqState>() == STATE_BYTES);
    assert!(std::mem::align_of::<EqState>() == 8);
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
        let path = PathBuf::from(STATE_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
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
