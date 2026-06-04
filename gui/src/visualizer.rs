//! Realtime output visualizer via WASAPI loopback capture of the default render
//! endpoint (the post-APO audio). Runs on its own thread. Two products for the GUI:
//!   - `snapshot()`  : a ring of decimated mono samples (scrolling waveform)
//!   - `spectrum()`  : log-spaced FFT magnitude bars (20 Hz..20 kHz), smoothed
//! Failures just end the thread (waveform flat / bars zero) — never fatal to the GUI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

const RING: usize = 2048; // mono samples retained for the waveform
const DECIMATE: usize = 6; // keep 1 of every N frames for the waveform ring
const SILENT_FLAG: u32 = 0x2; // AUDCLNT_BUFFERFLAGS_SILENT
const STEREO_RING: usize = 1024; // recent [L,R] pairs retained for the goniometer
const STEREO_DECIMATE: usize = 4; // keep 1 of every N frames for the stereo ring

const FFT_SIZE: usize = 2048; // FFT window length (every frame, non-decimated)
const FFT_HOP: usize = 512; // recompute the spectrum every N frames (~94 Hz @ 48k)
pub const NUM_BARS: usize = 64; // log-spaced display bars across 20 Hz..20 kHz
const F_LO: f32 = 20.0; // lowest bar edge (Hz)
const F_HI: f32 = 20_000.0; // highest bar edge (Hz)
const DB_FLOOR: f32 = -78.0; // maps to bar value 0.0
const DB_CEIL: f32 = -12.0; // maps to bar value 1.0
const RISE: f32 = 1.0; // instant attack
const FALL: f32 = 0.80; // exponential decay per hop (lower = snappier fall)

pub struct Visualizer {
    samples: Arc<Mutex<Vec<f32>>>,
    spectrum: Arc<Mutex<Vec<f32>>>,
    level: Arc<Mutex<(f32, f32)>>, // (rms, peak), both 0.0..~1.0
    stereo: Arc<Mutex<Vec<[f32; 2]>>>, // recent [L, R] pairs for the goniometer
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Visualizer {
    pub fn start() -> Self {
        let samples = Arc::new(Mutex::new(vec![0.0f32; RING]));
        let spectrum = Arc::new(Mutex::new(vec![0.0f32; NUM_BARS]));
        let level = Arc::new(Mutex::new((0.0f32, 0.0f32)));
        let stereo = Arc::new(Mutex::new(vec![[0.0f32; 2]; STEREO_RING]));
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = samples.clone();
        let sp2 = spectrum.clone();
        let lv2 = level.clone();
        let stx = stereo.clone();
        let stop2 = stop.clone();
        let handle = std::thread::spawn(move || unsafe {
            let _ = capture_loop(&s2, &sp2, &lv2, &stx, &stop2);
        });
        Visualizer { samples, spectrum, level, stereo, stop, handle: Some(handle) }
    }

    /// Decimated mono ring for the scrolling waveform backdrop.
    pub fn snapshot(&self) -> Vec<f32> {
        self.samples.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// `NUM_BARS` log-spaced magnitude bars in 0.0..=1.0 (already smoothed).
    pub fn spectrum(&self) -> Vec<f32> {
        self.spectrum.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Current output (rms, peak) linear levels, ~0.0..1.0.
    pub fn level(&self) -> (f32, f32) {
        self.level.lock().map(|l| *l).unwrap_or((0.0, 0.0))
    }

    /// Recent decimated [L, R] sample pairs for the stereo goniometer.
    pub fn stereo(&self) -> Vec<[f32; 2]> {
        self.stereo.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

impl Drop for Visualizer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn push_stereo(buf: &mut Vec<[f32; 2]>, v: [f32; 2]) {
    buf.push(v);
    let len = buf.len();
    if len > STEREO_RING {
        buf.drain(0..len - STEREO_RING);
    }
}

fn push_ring(buf: &mut Vec<f32>, v: f32) {
    buf.push(v);
    let len = buf.len();
    if len > RING {
        buf.drain(0..len - RING);
    }
}

/// Precompute the log-spaced bar edges (NUM_BARS+1 frequencies, 20 Hz..20 kHz).
fn bar_edges() -> Vec<f32> {
    let ratio = (F_HI / F_LO).log10(); // decades spanned (~3.0)
    (0..=NUM_BARS)
        .map(|b| F_LO * 10f32.powf(ratio * b as f32 / NUM_BARS as f32))
        .collect()
}

/// Hann window of length FFT_SIZE.
fn hann_window() -> Vec<f32> {
    let n = FFT_SIZE as f32;
    (0..FFT_SIZE)
        .map(|k| 0.5 - 0.5 * (std::f32::consts::TAU * k as f32 / (n - 1.0)).cos())
        .collect()
}

/// Read the oldest-first window from the ring, window it, FFT, fold into log bars
/// and smooth (instant rise, exponential fall). `bars` holds the persistent state.
#[allow(clippy::too_many_arguments)]
fn compute_spectrum(
    ring: &[f32],
    pos: usize,
    fft: &Arc<dyn Fft<f32>>,
    work: &mut [Complex<f32>],
    scratch: &mut [Complex<f32>],
    hann: &[f32],
    edges: &[f32],
    sample_rate: f32,
    bars: &mut [f32],
) {
    for k in 0..FFT_SIZE {
        let s = ring[(pos + k) % FFT_SIZE];
        work[k] = Complex { re: s * hann[k], im: 0.0 };
    }
    fft.process_with_scratch(work, scratch);

    // Magnitude reference: a full-scale sine through a Hann window peaks near
    // FFT_SIZE/4, so dividing by that puts 0 dBFS tones at ~0 dB.
    let norm = FFT_SIZE as f32 / 4.0;
    let hz_per_bin = sample_rate / FFT_SIZE as f32;
    let nyq_bin = FFT_SIZE / 2 - 1;

    for b in 0..NUM_BARS {
        let lo = ((edges[b] / hz_per_bin).floor() as usize).max(1);
        let hi = (((edges[b + 1] / hz_per_bin).ceil() as usize).min(nyq_bin)).max(lo);
        let mut peak = 0.0f32;
        for bin in lo..=hi {
            peak = peak.max(work[bin].norm());
        }
        let db = 20.0 * (peak / norm).max(1e-9).log10();
        let target = ((db - DB_FLOOR) / (DB_CEIL - DB_FLOOR)).clamp(0.0, 1.0);
        bars[b] = if target >= bars[b] {
            bars[b] + (target - bars[b]) * RISE
        } else {
            bars[b] * FALL + target * (1.0 - FALL)
        };
    }
}

/// Owns COM for the capture thread and keeps re-acquiring the default endpoint
/// whenever a session ends — a session dies whenever the audio graph is rebuilt
/// (device change, format change, toggling Windows audio enhancements like
/// Loudness Equalization). Without this the visualizer would freeze permanently
/// after any such event; now it recovers within ~0.5s on its own.
unsafe fn capture_loop(
    samples: &Arc<Mutex<Vec<f32>>>,
    spectrum: &Arc<Mutex<Vec<f32>>>,
    level: &Arc<Mutex<(f32, f32)>>,
    stereo: &Arc<Mutex<Vec<[f32; 2]>>>,
    stop: &Arc<AtomicBool>,
) -> windows::core::Result<()> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    while !stop.load(Ordering::Relaxed) {
        let _ = run_session(samples, spectrum, level, stereo, stop);
        // Session ended (error or stop). Flatten the viz so it doesn't show stale
        // bars/levels, then pause before re-acquiring the (possibly new) default.
        if let Ok(mut sp) = spectrum.lock() {
            sp.iter_mut().for_each(|v| *v = 0.0);
        }
        if let Ok(mut lv) = level.lock() {
            *lv = (0.0, 0.0);
        }
        if let Ok(mut st) = stereo.lock() {
            st.iter_mut().for_each(|v| *v = [0.0, 0.0]);
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    CoUninitialize();
    Ok(())
}

/// One capture session against the current default render endpoint. Returns Err
/// (triggering a re-acquire in `capture_loop`) when the device is invalidated.
unsafe fn run_session(
    samples: &Arc<Mutex<Vec<f32>>>,
    spectrum: &Arc<Mutex<Vec<f32>>>,
    level: &Arc<Mutex<(f32, f32)>>,
    stereo: &Arc<Mutex<Vec<[f32; 2]>>>,
    stop: &Arc<AtomicBool>,
) -> windows::core::Result<()> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

    let pfmt = client.GetMixFormat()?;
    let channels = (*pfmt).nChannels as usize;
    let sample_rate = (*pfmt).nSamplesPerSec as f32;
    // Shared-mode mix is virtually always 32-bit float (tag 3, or EXTENSIBLE 0xFFFE).
    let is_float = (*pfmt).wFormatTag == 3 || (*pfmt).wFormatTag == 0xFFFE;
    let bps = ((*pfmt).wBitsPerSample / 8) as usize;

    client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK,
        2_000_000, // 200 ms buffer
        0,
        pfmt,
        None,
    )?;
    let capture: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    CoTaskMemFree(Some(pfmt as *const core::ffi::c_void));

    let usable = is_float && bps == 4 && channels > 0;

    // FFT machinery + persistent ring/state (thread-local, no locks on the hot path).
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
    let mut work = vec![Complex { re: 0.0f32, im: 0.0f32 }; FFT_SIZE];
    let mut scratch = vec![Complex { re: 0.0f32, im: 0.0f32 }; fft.get_inplace_scratch_len()];
    let hann = hann_window();
    let edges = bar_edges();
    let mut fft_ring = vec![0.0f32; FFT_SIZE];
    let mut fft_pos = 0usize;
    let mut hop = 0usize;
    let mut bars = vec![0.0f32; NUM_BARS];
    let mut peak = 0.0f32; // decaying output peak (linear)
    let mut msq = 0.0f32; // one-pole mean-square for RMS

    let mut feed = |mono: f32,
                    fft_ring: &mut [f32],
                    fft_pos: &mut usize,
                    hop: &mut usize,
                    bars: &mut [f32]| {
        fft_ring[*fft_pos] = mono;
        *fft_pos = (*fft_pos + 1) % FFT_SIZE;
        *hop += 1;
        if *hop >= FFT_HOP {
            *hop = 0;
            compute_spectrum(
                fft_ring, *fft_pos, &fft, &mut work, &mut scratch, &hann, &edges, sample_rate,
                bars,
            );
            if let Ok(mut sp) = spectrum.lock() {
                sp.copy_from_slice(bars);
            }
        }
    };

    while !stop.load(Ordering::Relaxed) {
        loop {
            let packet = capture.GetNextPacketSize()?;
            if packet == 0 {
                break;
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            let silent = (flags & SILENT_FLAG) != 0;

            if let Ok(mut buf) = samples.lock() {
                if usable && !silent && !data.is_null() {
                    let f = data as *const f32;
                    let mut st = stereo.lock().ok();
                    for i in 0..frames as usize {
                        let mut acc = 0.0f32;
                        for c in 0..channels {
                            acc += *f.add(i * channels + c);
                        }
                        let mono = acc / channels as f32;
                        feed(mono, &mut fft_ring, &mut fft_pos, &mut hop, &mut bars);
                        peak = (peak * 0.9999).max(mono.abs());
                        msq = msq * 0.9995 + mono * mono * 0.0005;
                        if i % DECIMATE == 0 {
                            push_ring(&mut buf, mono);
                        }
                        if i % STEREO_DECIMATE == 0 {
                            if let Some(st) = st.as_mut() {
                                let l = *f.add(i * channels);
                                let r = if channels > 1 { *f.add(i * channels + 1) } else { l };
                                push_stereo(&mut **st, [l, r]);
                            }
                        }
                    }
                } else {
                    // silence (or unsupported format) -> flatten waveform + decay bars
                    for i in 0..frames as usize {
                        feed(0.0, &mut fft_ring, &mut fft_pos, &mut hop, &mut bars);
                        if i % DECIMATE == 0 {
                            push_ring(&mut buf, 0.0);
                        }
                    }
                    peak *= 0.6;
                    msq *= 0.6;
                }
            }
            capture.ReleaseBuffer(frames)?;
            if let Ok(mut lv) = level.lock() {
                *lv = (msq.sqrt().min(1.5), peak.min(1.5));
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }

    let _ = client.Stop();
    Ok(())
}
