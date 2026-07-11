//! VizBus — the data pipes. One struct, stepped once per frame (16ms gate),
//! holding every derived audio statistic the app can visualize: band energies,
//! spectral centroid + flux, a beat-pulse envelope, a realtime BPM estimate
//! (autocorrelation of the onset strength), momentary loudness, crest factor,
//! and stereo correlation/width. Each signal keeps a fixed sparkline history.
//!
//! The DATA tab renders the bus directly; viz modes (painter + the GL stage)
//! consume the same fields, so what you see in DATA is exactly what a viz can
//! be driven by. Zero steady-state allocation: fixed rings everywhere.

use std::time::Instant;

const STEP: std::time::Duration = std::time::Duration::from_millis(16);
/// Sparkline depth: 4 seconds at 60 Hz.
pub const HIST: usize = 240;
/// Onset ring: 8 seconds at 60 Hz feeds the BPM autocorrelation.
const ONSET: usize = 480;

/// Fixed-size ring for the DATA-tab sparklines.
#[derive(Clone)]
pub struct History {
    buf: [f32; HIST],
    idx: usize,
    len: usize,
}

impl Default for History {
    fn default() -> Self {
        History { buf: [0.0; HIST], idx: 0, len: 0 }
    }
}

impl History {
    fn push(&mut self, v: f32) {
        self.buf[self.idx] = v;
        self.idx = (self.idx + 1) % HIST;
        self.len = (self.len + 1).min(HIST);
    }
    /// Oldest-to-newest iteration for drawing.
    pub fn iter(&self) -> impl Iterator<Item = f32> + '_ {
        let start = if self.len < HIST { 0 } else { self.idx };
        (0..self.len).map(move |i| self.buf[(start + i) % HIST])
    }
    pub fn len(&self) -> usize {
        self.len
    }
    #[allow(dead_code)] // the GL stage reads this next wave
    pub fn last(&self) -> f32 {
        if self.len == 0 { 0.0 } else { self.buf[(self.idx + HIST - 1) % HIST] }
    }
}

/// Every published pipe, in DATA-tab display order.
#[repr(usize)]
#[derive(Clone, Copy)]
pub enum Signal {
    Pulse,
    Flux,
    Bass,
    Mid,
    Treble,
    Centroid,
    Loud,
    Crest,
    Corr,
    Width,
    Count,
}

pub struct VizBus {
    // Current values (also the newest sample of each history).
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub centroid: f32,
    pub flux: f32,
    pub pulse: f32,
    pub bpm: f32,
    pub bpm_conf: f32,
    /// 0..1 sawtooth locked to the BPM grid (soft-resynced on strong beats).
    pub beat_phase: f32,
    /// Momentary loudness, dBFS over a ~400ms RMS window (LUFS-flavored).
    pub loud_db: f32,
    pub crest_db: f32,
    pub corr: f32,
    pub width: f32,

    pub hist: Vec<History>, // Signal::Count entries, allocated once

    // Internals
    prev_spectrum: Vec<f32>,
    onset: [f32; ONSET],
    onset_idx: usize,
    onset_len: usize,
    env: f32,
    rms_win: [f32; 24], // ~400ms of per-step RMS
    rms_idx: usize,
    steps: u32,
    last_step: Instant,
}

impl VizBus {
    pub fn new() -> Self {
        VizBus {
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            centroid: 0.0,
            flux: 0.0,
            pulse: 0.0,
            bpm: 0.0,
            bpm_conf: 0.0,
            beat_phase: 0.0,
            loud_db: -60.0,
            crest_db: 0.0,
            corr: 0.0,
            width: 0.0,
            hist: vec![History::default(); Signal::Count as usize],
            prev_spectrum: Vec::new(),
            onset: [0.0; ONSET],
            onset_idx: 0,
            onset_len: 0,
            env: 0.0,
            rms_win: [0.0; 24],
            rms_idx: 0,
            steps: 0,
            last_step: Instant::now(),
        }
    }

    /// Advance the bus (16ms wall-clock gated; call every frame, cheap no-op
    /// between steps).
    pub fn step(
        &mut self,
        spectrum: &[f32],
        stereo: &[[f32; 2]],
        rms: f32,
        peak: f32,
    ) {
        let now = Instant::now();
        if now.duration_since(self.last_step) < STEP {
            return;
        }
        self.last_step = now;
        self.steps = self.steps.wrapping_add(1);
        let n = spectrum.len();
        if n < 8 {
            return;
        }

        // Band energies over the log-spaced bins: low quarter / mid / top.
        let q = n / 4;
        let mean = |s: &[f32]| s.iter().sum::<f32>() / s.len().max(1) as f32;
        self.bass = mean(&spectrum[..q]);
        self.mid = mean(&spectrum[q..(n - q)]);
        self.treble = mean(&spectrum[(n - q)..]);

        // Spectral centroid (0..1 across the log axis) + flux (onset strength).
        let sum: f32 = spectrum.iter().sum::<f32>().max(1e-6);
        self.centroid =
            spectrum.iter().enumerate().map(|(i, v)| i as f32 * v).sum::<f32>() / (sum * n as f32);
        let mut flux = 0.0;
        if self.prev_spectrum.len() == n {
            for (v, p) in spectrum.iter().zip(self.prev_spectrum.iter()) {
                flux += (v - p).max(0.0);
            }
        }
        self.prev_spectrum.clear();
        self.prev_spectrum.extend_from_slice(spectrum);
        self.flux = (flux / n as f32 * 12.0).min(1.0);

        // Onset envelope (bass-weighted flux) feeds pulse + BPM.
        let onset = (self.flux * 0.6 + self.bass * 0.4).min(1.0);
        self.onset[self.onset_idx] = onset;
        self.onset_idx = (self.onset_idx + 1) % ONSET;
        self.onset_len = (self.onset_len + 1).min(ONSET);

        // Beat pulse: fast rise over a slow envelope (same feel as show.rs).
        self.env = self.env * 0.94 + onset * 0.06;
        let raw = ((onset - self.env * 1.25) * 4.0).clamp(0.0, 1.0);
        self.pulse = (self.pulse * 0.86).max(raw);

        // Beat phase advances on the BPM grid; a strong pulse near wrap softly
        // resyncs it so viz locked to phase stays on the actual beat.
        if self.bpm > 1.0 {
            self.beat_phase = (self.beat_phase + (1.0 / 60.0) * self.bpm / 60.0).fract();
            if self.pulse > 0.6 && self.bpm_conf > 0.25 {
                if self.beat_phase > 0.85 || self.beat_phase < 0.15 {
                    self.beat_phase = 0.0;
                }
            }
        }

        // BPM autocorrelation every half second once the ring has data.
        if self.steps % 30 == 0 && self.onset_len >= ONSET / 2 {
            self.detect_bpm();
        }

        // Loudness (~400ms RMS window, dBFS) + crest factor.
        self.rms_win[self.rms_idx] = rms;
        self.rms_idx = (self.rms_idx + 1) % self.rms_win.len();
        let win_rms =
            (self.rms_win.iter().map(|v| v * v).sum::<f32>() / self.rms_win.len() as f32).sqrt();
        self.loud_db = 20.0 * win_rms.max(1e-5).log10();
        self.crest_db = 20.0 * (peak.max(1e-5) / rms.max(1e-5)).log10();

        // Stereo correlation (Pearson) + width (side/mid energy).
        if stereo.len() > 16 {
            let (mut sl, mut sr, mut sll, mut srr, mut slr) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let (mut m_e, mut s_e) = (0.0f32, 0.0f32);
            for &[l, r] in stereo {
                sl += l;
                sr += r;
                sll += l * l;
                srr += r * r;
                slr += l * r;
                let m = (l + r) * 0.5;
                let s = (l - r) * 0.5;
                m_e += m * m;
                s_e += s * s;
            }
            let k = stereo.len() as f32;
            let cov = slr / k - (sl / k) * (sr / k);
            let vl = (sll / k - (sl / k) * (sl / k)).max(1e-9);
            let vr = (srr / k - (sr / k) * (sr / k)).max(1e-9);
            self.corr = (cov / (vl * vr).sqrt()).clamp(-1.0, 1.0);
            self.width = (s_e / m_e.max(1e-9)).sqrt().min(2.0);
        }

        // Publish histories (normalized 0..1 where the natural range isn't).
        let rows: [(Signal, f32); 10] = [
            (Signal::Pulse, self.pulse),
            (Signal::Flux, self.flux),
            (Signal::Bass, self.bass),
            (Signal::Mid, self.mid),
            (Signal::Treble, self.treble),
            (Signal::Centroid, self.centroid),
            (Signal::Loud, ((self.loud_db + 60.0) / 60.0).clamp(0.0, 1.0)),
            (Signal::Crest, (self.crest_db / 24.0).clamp(0.0, 1.0)),
            (Signal::Corr, (self.corr + 1.0) * 0.5),
            (Signal::Width, (self.width * 0.5).min(1.0)),
        ];
        for (s, v) in rows {
            self.hist[s as usize].push(v);
        }
    }

    /// Autocorrelate the onset ring over the 60..=180 BPM lag range, pick the
    /// strongest lag (parabolic-refined), fold octaves toward 80..160, and
    /// EMA-smooth when confidence holds up.
    fn detect_bpm(&mut self) {
        // Materialize the ring oldest-first into a scratch on the stack.
        let mut o = [0.0f32; ONSET];
        let start = if self.onset_len < ONSET { 0 } else { self.onset_idx };
        for i in 0..self.onset_len {
            o[i] = self.onset[(start + i) % ONSET];
        }
        let len = self.onset_len;
        let m = o[..len].iter().sum::<f32>() / len as f32;
        for v in o[..len].iter_mut() {
            *v -= m;
        }
        let r0: f32 = o[..len].iter().map(|v| v * v).sum::<f32>().max(1e-9);

        // Lags: 60 BPM -> 60 steps, 180 BPM -> 20 steps (at 60 Hz).
        let (mut best_lag, mut best_r) = (0usize, 0.0f32);
        let mut r_at = [0.0f32; 61];
        for lag in 20..=60usize {
            if lag >= len {
                break;
            }
            let mut r = 0.0;
            for i in lag..len {
                r += o[i] * o[i - lag];
            }
            r /= r0;
            r_at[lag] = r;
            if r > best_r {
                best_r = r;
                best_lag = lag;
            }
        }
        if best_lag == 0 || best_r < 0.05 {
            self.bpm_conf *= 0.7; // decay confidence when nothing periodic
            return;
        }
        // Parabolic refinement around the peak lag.
        let lag = best_lag as f32
            + if best_lag > 20 && best_lag < 60 {
                let (a, b, c) = (r_at[best_lag - 1], r_at[best_lag], r_at[best_lag + 1]);
                let d = a - 2.0 * b + c;
                if d.abs() > 1e-6 { 0.5 * (a - c) / d } else { 0.0 }
            } else {
                0.0
            };
        let mut bpm = 3600.0 / lag;
        // Octave fold into the 80..160 sweet spot when the half/double lag
        // correlates comparably.
        if bpm < 80.0 && bpm * 2.0 <= 180.0 {
            bpm *= 2.0;
        } else if bpm > 160.0 && bpm * 0.5 >= 60.0 {
            let half_lag = (lag * 2.0) as usize;
            if half_lag <= 60 && r_at[half_lag] > best_r * 0.6 {
                bpm *= 0.5;
            }
        }
        let conf = best_r.clamp(0.0, 1.0);
        self.bpm_conf = self.bpm_conf * 0.6 + conf * 0.4;
        if self.bpm <= 1.0 || (self.bpm - bpm).abs() > 25.0 {
            self.bpm = bpm; // jump on first lock or a genuinely new tempo
        } else {
            self.bpm = self.bpm * 0.8 + bpm * 0.2;
        }
    }
}
