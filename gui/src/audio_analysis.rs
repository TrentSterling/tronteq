//! Audio-clock analysis. No GUI timing, devices, or system-state writes.
use crate::vizbus::VizBus;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::Arc;

pub const NUM_BARS: usize = 64;
const FFT_SIZE: usize = 2048;
const FFT_HOP: usize = 512;
const WAVE: usize = 2048;
const STEREO: usize = 1024;

struct Ring<T: Copy> {
    data: Vec<T>,
    next: usize,
}
impl<T: Copy> Ring<T> {
    fn new(size: usize, zero: T) -> Self {
        Self {
            data: vec![zero; size],
            next: 0,
        }
    }
    fn push(&mut self, value: T) {
        self.data[self.next] = value;
        self.next = (self.next + 1) % self.data.len();
    }
    fn ordered(&self) -> Vec<T> {
        self.data[self.next..]
            .iter()
            .chain(&self.data[..self.next])
            .copied()
            .collect()
    }
}

#[derive(Clone)]
pub struct Frame {
    pub ticks: u64,
    pub spectrum: [f32; NUM_BARS],
    pub peaks: [f32; NUM_BARS],
    /// Strongest channel, selected per FFT window with hysteresis. Never a phase-cancelling sum.
    pub wave: Vec<f32>,
    pub stereo: Vec<[f32; 2]>,
    pub rms: f32,
    pub peak: f32,
    pub loudness: Vec<f32>,
    /// Oldest first, exactly three seconds at 60 rows per second.
    pub waterfall: Vec<[f32; NUM_BARS]>,
    pub bus: VizBus,
}

pub struct Analyzer {
    rate: u32,
    channels: usize,
    fft: Arc<dyn Fft<f32>>,
    work: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    hann: Vec<f32>,
    fft_ring: Vec<Vec<f32>>,
    pos: usize,
    hop: usize,
    bars: [f32; NUM_BARS],
    peaks: [f32; NUM_BARS],
    wave: Ring<f32>,
    stereo: Ring<[f32; 2]>,
    loudness: Ring<f32>,
    waterfall: Ring<[f32; NUM_BARS]>,
    wave_channel: usize,
    wave_phase: u32,
    stereo_phase: u32,
    tick_phase: u32,
    pub ticks: u64,
    msq: f32,
    peak: f32,
    rms_decay: f32,
    peak_decay: f32,
    bar_decay: f32,
    bus: VizBus,
}

impl Analyzer {
    pub fn new(rate: u32, channels: usize) -> Self {
        assert!((8_000..=384_000).contains(&rate) && (1..=64).contains(&channels));
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        Self {
            rate,
            channels,
            scratch: vec![Complex::default(); fft.get_inplace_scratch_len()],
            fft,
            work: vec![Complex::default(); FFT_SIZE],
            hann: (0..FFT_SIZE)
                .map(|i| {
                    0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos()
                })
                .collect(),
            fft_ring: vec![vec![0.0; FFT_SIZE]; channels],
            pos: 0,
            hop: 0,
            bars: [0.0; NUM_BARS],
            peaks: [0.0; NUM_BARS],
            wave: Ring::new(WAVE, 0.0),
            stereo: Ring::new(STEREO, [0.0; 2]),
            loudness: Ring::new(480, 0.0),
            waterfall: Ring::new(180, [0.0; NUM_BARS]),
            wave_channel: 0,
            wave_phase: 0,
            stereo_phase: 0,
            tick_phase: 0,
            ticks: 0,
            msq: 0.0,
            peak: 0.0,
            rms_decay: 0.9995f32.powf(48_000.0 / rate as f32),
            peak_decay: 0.9999f32.powf(48_000.0 / rate as f32),
            bar_decay: 0.8f32.powf(48_000.0 / rate as f32),
            bus: VizBus::new(),
        }
    }

    pub fn feed(&mut self, frame: &[f32]) {
        assert!(frame.len() >= self.channels);
        let mut samples = [0.0f32; 64];
        let mut energy = 0.0;
        let mut peak = 0.0f32;
        for c in 0..self.channels {
            let v = if frame[c].is_finite() {
                frame[c].clamp(-16.0, 16.0)
            } else {
                0.0
            };
            samples[c] = v;
            self.fft_ring[c][self.pos] = v;
            energy += v * v;
            peak = peak.max(v.abs());
        }
        self.msq =
            self.msq * self.rms_decay + energy / self.channels as f32 * (1.0 - self.rms_decay);
        self.peak = (self.peak * self.peak_decay).max(peak);
        self.pos = (self.pos + 1) % FFT_SIZE;
        self.hop += 1;
        if self.hop == FFT_HOP {
            self.hop = 0;
            self.compute_spectrum();
        }

        self.wave_phase += 8_000;
        if self.wave_phase >= self.rate {
            self.wave_phase -= self.rate;
            self.wave.push(samples[self.wave_channel]);
        }
        self.stereo_phase += 12_000.min(self.rate);
        if self.stereo_phase >= self.rate {
            self.stereo_phase -= self.rate;
            self.stereo
                .push([samples[0], samples[if self.channels > 1 { 1 } else { 0 }]]);
        }
        self.tick_phase += 60;
        if self.tick_phase >= self.rate {
            self.tick_phase -= self.rate;
            self.ticks += 1;
            for (p, &s) in self.peaks.iter_mut().zip(&self.bars) {
                *p = (*p - 0.012).max(s);
            }
            let rms = self.msq.sqrt();
            // Ring order is irrelevant for covariance; avoid a materialized copy here.
            self.bus.step(&self.bars, &self.stereo.data, rms, self.peak);
            self.loudness.push(rms);
            self.waterfall.push(self.bars);
        }
    }

    pub fn silence(&mut self, frames: usize) {
        let zero = [0.0; 64];
        for _ in 0..frames {
            self.feed(&zero);
        }
    }

    fn compute_spectrum(&mut self) {
        let mut powers = [0.0f32; FFT_SIZE / 2];
        let mut channel_energy = [0.0f32; 64];
        for (c, ring) in self.fft_ring.iter().enumerate() {
            for k in 0..FFT_SIZE {
                let s = ring[(self.pos + k) % FFT_SIZE];
                self.work[k] = Complex {
                    re: s * self.hann[k],
                    im: 0.0,
                };
                channel_energy[c] += s * s;
            }
            self.fft
                .process_with_scratch(&mut self.work, &mut self.scratch);
            for (power, bin) in powers.iter_mut().zip(&self.work) {
                *power += bin.norm_sqr() / self.channels as f32;
            }
        }
        let strongest = (0..self.channels)
            .max_by(|&a, &b| channel_energy[a].total_cmp(&channel_energy[b]))
            .unwrap();
        if channel_energy[strongest] > channel_energy[self.wave_channel] * 1.5 {
            self.wave_channel = strongest;
        }
        let hz_per_bin = self.rate as f32 / FFT_SIZE as f32;
        let nyquist = self.rate as f32 * 0.5;
        for b in 0..NUM_BARS {
            let low = 20.0 * 1000f32.powf(b as f32 / NUM_BARS as f32);
            let high = 20.0 * 1000f32.powf((b + 1) as f32 / NUM_BARS as f32);
            let target = if low >= nyquist {
                0.0
            } else {
                let lo = ((low / hz_per_bin).floor() as usize).clamp(1, FFT_SIZE / 2 - 1);
                let hi = ((high / hz_per_bin).ceil() as usize).clamp(lo, FFT_SIZE / 2 - 1);
                let magnitude = powers[lo..=hi]
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max)
                    .sqrt()
                    / (FFT_SIZE as f32 / 4.0);
                ((20.0 * magnitude.max(1e-9).log10() + 78.0) / 66.0).clamp(0.0, 1.0)
            };
            self.bars[b] = if target >= self.bars[b] {
                target
            } else {
                self.bars[b] * self.bar_decay + target * (1.0 - self.bar_decay)
            };
        }
    }

    pub fn snapshot(&self) -> Frame {
        Frame {
            ticks: self.ticks,
            spectrum: self.bars,
            peaks: std::array::from_fn(|i| self.peaks[i].max(self.bars[i])),
            wave: self.wave.ordered(),
            stereo: self.stereo.ordered(),
            rms: self.msq.sqrt(),
            peak: self.peak,
            loudness: self.loudness.ordered(),
            waterfall: self.waterfall.ordered(),
            bus: self.bus.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn right_only_audio_still_drives_the_waveform_and_all_channel_level() {
        let mut stereo = Analyzer::new(48_000, 2);
        let mut mono = Analyzer::new(48_000, 1);
        for i in 0..24_000 {
            let v = 0.1 * (i as f32 * 0.2).sin();
            stereo.feed(&[0.0, v]);
            mono.feed(&[v]);
        }
        let s = stereo.snapshot();
        let m = mono.snapshot();
        assert!(s.wave.iter().any(|v| v.abs() > 0.09));
        assert!((s.rms * 2f32.sqrt() - m.rms).abs() < 0.0001);
        assert_eq!(s.peak, m.peak);
        assert!(m.stereo.iter().all(|pair| pair[0] == pair[1]));
    }
    fn tone(a: &mut Analyzer, frames: usize, right_phase: f32) {
        for i in 0..frames {
            let s = 0.1 * (std::f32::consts::TAU * 1000.0 * i as f32 / a.rate as f32).sin();
            a.feed(&[s, s * right_phase]);
        }
    }
    #[test]
    fn opposite_phase_has_the_same_spectrum_and_levels_as_in_phase() {
        let mut a = Analyzer::new(48_000, 2);
        let mut b = Analyzer::new(48_000, 2);
        tone(&mut a, 24_000, 1.0);
        tone(&mut b, 24_000, -1.0);
        let a = a.snapshot();
        let b = b.snapshot();
        assert_eq!(a.spectrum, b.spectrum);
        assert_eq!(a.rms, b.rms);
        assert_eq!(a.peak, b.peak);
        assert!((b.rms - 0.1 / 2f32.sqrt()).abs() < 0.002);
        assert!(a.bus.corr > 0.99 && b.bus.corr < -0.99);
        assert!(b.wave.iter().any(|v| v.abs() > 0.09));
    }
    #[test]
    fn rate_changes_preserve_frequency_axis_and_history_duration() {
        for rate in [8_000, 44_100, 48_000, 96_000] {
            let mut a = Analyzer::new(rate, 2);
            tone(&mut a, rate as usize, 1.0);
            let s = a.snapshot();
            assert_eq!(s.ticks, 60);
            assert_eq!(s.bus.hist[0].len(), 60);
            let peak = s
                .spectrum
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .unwrap()
                .0;
            let hz = 20.0 * 1000f32.powf((peak as f32 + 0.5) / NUM_BARS as f32);
            assert!((800.0..1250.0).contains(&hz), "rate {rate}: peak at {hz}");
            for (i, v) in s.spectrum.iter().enumerate() {
                if 20.0 * 1000f32.powf(i as f32 / NUM_BARS as f32) >= rate as f32 / 2.0 {
                    assert_eq!(*v, 0.0);
                }
            }
        }
    }
    #[test]
    fn surround_only_and_silent_packets_update_all_products() {
        let mut a = Analyzer::new(48_000, 6);
        for i in 0..24_000 {
            let v = 0.2 * (i as f32 * 0.2).sin();
            a.feed(&[0.0, 0.0, 0.0, 0.0, v, -v]);
        }
        let s = a.snapshot();
        assert!(s.rms > 0.07 && s.peak > 0.19);
        assert!(s.wave.iter().any(|v| v.abs() > 0.19));
        assert!(s.spectrum.iter().any(|v| *v > 0.5));
        a.silence(144_000);
        let s = a.snapshot();
        assert!(s.rms < 0.0001 && s.peak < 0.0001);
        assert!(s.wave.iter().all(|v| *v == 0.0));
        assert!(s.stereo.iter().all(|v| *v == [0.0; 2]));
        assert!(s.spectrum.iter().all(|v| *v < 0.0001));
    }
    #[test]
    fn snapshots_at_different_gui_rates_do_not_change_analysis() {
        let mut baseline = None;
        for fps in [20, 60, 144] {
            let mut a = Analyzer::new(48_000, 2);
            for i in 0..96_000 {
                let v = if i % 24_000 < 2400 {
                    0.1 * (i as f32 * 0.1).sin()
                } else {
                    0.0
                };
                a.feed(&[v, -v]);
                if i % (48_000 / fps) == 0 {
                    let _ = a.snapshot();
                }
            }
            let s = a.snapshot();
            assert_eq!(s.ticks, 120);
            let values = (
                s.spectrum,
                s.peaks,
                s.loudness,
                s.waterfall,
                s.bus.hist[0].iter().collect::<Vec<_>>(),
            );
            if let Some(expected) = &baseline {
                assert_eq!(&values, expected);
            } else {
                baseline = Some(values);
            }
        }
    }
}
