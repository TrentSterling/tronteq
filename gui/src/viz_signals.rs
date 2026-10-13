//! Per-look signal conditioning. Learned ranges belong to the runtime, never a preset.
use crate::{viz_motion::Source, vizbus::VizBus};
use serde::{Deserialize, Serialize};

pub const COUNT: usize = 19;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Channel {
    pub adapt: f32,
    pub attack: f32,
    pub release: f32,
}
impl Default for Channel {
    fn default() -> Self {
        Self {
            adapt: 0.75,
            attack: 0.025,
            release: 0.18,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub memory: f32,
    pub floor_db: f32,
    pub channels: [Channel; COUNT],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            memory: 6.0,
            floor_db: -55.0,
            channels: [Channel::default(); COUNT],
        }
    }
}
pub fn adaptive(source: Source) -> bool {
    matches!(
        source,
        Source::Bass
            | Source::Mids
            | Source::Highs
            | Source::SubBass
            | Source::Level
            | Source::Beat
            | Source::Change
            | Source::Kick
            | Source::LowOnset
            | Source::MidOnset
            | Source::HighOnset
    )
}
fn transient(source: Source) -> bool {
    matches!(
        source,
        Source::Beat
            | Source::Change
            | Source::Kick
            | Source::LowOnset
            | Source::MidOnset
            | Source::HighOnset
    )
}
fn finite(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        fallback
    }
}
impl Settings {
    pub fn sanitize(&mut self) {
        self.memory = finite(self.memory, 1.0, 30.0, 6.0);
        self.floor_db = finite(self.floor_db, -80.0, -25.0, -55.0);
        for c in &mut self.channels {
            c.adapt = finite(c.adapt, 0.0, 1.0, 0.75);
            c.attack = finite(c.attack, 0.0, 2.0, 0.025);
            c.release = finite(c.release, 0.0, 3.0, 0.18);
        }
    }
    pub fn preset(attack: f32, release: f32) -> Self {
        Self {
            enabled: true,
            channels: [Channel {
                attack,
                release,
                ..Default::default()
            }; COUNT],
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Tracker {
    low: f32,
    high: f32,
    envelope: f32,
    age: f32,
}

#[derive(Default)]
pub struct Runtime {
    pub raw: [f32; COUNT],
    pub learned: [[f32; 2]; COUNT],
    pub active: bool,
    pub selected: Source,
    trackers: [Tracker; COUNT],
    enabled: bool,
}
impl Runtime {
    pub fn reset(&mut self) {
        let selected = self.selected;
        *self = Self::default();
        self.selected = selected;
    }
    pub fn update(&mut self, c: &Settings, bus: &VizBus, gains: [f32; 6], dt: f32) -> [f32; COUNT] {
        let raw = [
            0.0,
            bus.bass,
            bus.mid,
            bus.treble,
            bus.pulse,
            bus.flux,
            bus.width,
            bus.centroid,
            (bus.loud_db + 60.0) / 60.0,
            bus.crest_db / 24.0,
            (bus.corr + 1.0) * 0.5,
            bus.sub_bass,
            bus.kick,
            bus.band_onsets[0],
            bus.band_onsets[1],
            bus.band_onsets[2],
            (bus.balance + 1.0) * 0.5,
            bus.tonality,
            if bus.loud_db > -60.0 {
                1.0 - bus.tonality
            } else {
                0.0
            },
        ]
        .map(|v| finite(v, 0.0, 1.0, 0.0));
        let gain = |i: usize| {
            if (1..=6).contains(&i) {
                finite(gains[i - 1], 0.0, 2.0, 1.0)
            } else {
                1.0
            }
        };
        self.raw = std::array::from_fn(|i| (raw[i] * gain(i)).min(1.0));
        self.active = bus.loud_db.is_finite() && bus.loud_db > c.floor_db;
        if c.enabled != self.enabled {
            self.trackers = [Tracker::default(); COUNT];
            self.enabled = c.enabled;
        }
        if !c.enabled {
            return self.raw;
        }
        let dt = finite(dt, 0.0, 0.1, 0.0);
        let memory = finite(c.memory, 1.0, 30.0, 6.0);
        let learn = 1.0 - (-dt / memory).exp();
        std::array::from_fn(|i| {
            if i == 0 {
                return 0.0;
            }
            let source = Source::ALL[i];
            let channel = c.channels[i];
            let t = &mut self.trackers[i];
            let x = raw[i];
            let mut target = x;
            if adaptive(source) {
                if self.active {
                    if t.age == 0.0 {
                        t.low = x;
                        t.high = x;
                        t.envelope = x;
                    }
                    t.low = x.min(t.low + (x - t.low) * learn);
                    t.high = x.max(t.high + (x - t.high) * learn);
                    t.age += dt;
                    // Keep a minimum range so steady tones/noise cannot grow into violent motion.
                    // Onsets keep a zero baseline; sustained bands expand around their moving center.
                    let normalized = if transient(source) {
                        x / t.high.max(0.18)
                    } else {
                        (0.5 + (x - (t.low + t.high) * 0.5) * 0.85 / (t.high - t.low).max(0.12))
                            * (x / 0.12).min(1.0)
                    }
                    .clamp(0.0, 1.0);
                    target += (normalized - x) * channel.adapt * (t.age / 1.0).min(1.0);
                } else {
                    target = 0.0;
                }
            }
            self.learned[i] = [t.low, t.high];
            let tau = if target > t.envelope {
                channel.attack
            } else {
                channel.release
            };
            let alpha = if tau <= 0.0 {
                1.0
            } else {
                1.0 - (-dt / tau).exp()
            };
            t.envelope += (target - t.envelope) * alpha;
            finite(t.envelope * gain(i), 0.0, 1.0, 0.0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_is_exact_and_gain_zero_stays_zero() {
        let mut rt = Runtime::default();
        let mut b = VizBus::new();
        b.bass = 0.96;
        b.mid = 0.4;
        b.loud_db = -18.0;
        let mut c = Settings::default();
        assert_eq!(rt.update(&c, &b, [1.0; 6], 1.0 / 60.0)[1], 0.96);
        c.enabled = true;
        for _ in 0..600 {
            assert_eq!(rt.update(&c, &b, [0.0; 6], 1.0 / 60.0)[1], 0.0);
        }
    }
    #[test]
    fn compressed_band_expands_without_amplifying_steady_noise_and_silence_releases() {
        let mut rt = Runtime::default();
        let mut c = Settings::preset(0.0, 0.0);
        c.channels[1].adapt = 1.0;
        let mut b = VizBus::new();
        b.loud_db = -18.0;
        let mut lo = 1.0f32;
        let mut hi = 0.0f32;
        for i in 0..900 {
            b.bass = 0.90 + 0.08 * (i as f32 * 0.1).sin().abs();
            let v = rt.update(&c, &b, [1.0; 6], 1.0 / 60.0)[1];
            if i > 600 {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        assert!(hi - lo > 0.4, "expanded range {lo}..{hi}");
        c.channels[1].release = 0.15;
        b.loud_db = -90.0;
        for _ in 0..120 {
            rt.update(&c, &b, [1.0; 6], 1.0 / 60.0);
        }
        assert!(rt.update(&c, &b, [1.0; 6], 1.0 / 60.0)[1] < 0.0001);
        b.loud_db = -30.0;
        b.bass = 0.001;
        for _ in 0..2400 {
            rt.update(&c, &b, [1.0; 6], 1.0 / 60.0);
        }
        let baseline = rt.update(&c, &b, [1.0; 6], 1.0 / 60.0)[1];
        b.bass = 0.002;
        assert!((rt.update(&c, &b, [1.0; 6], 1.0 / 60.0)[1] - baseline).abs() < 0.02);
    }
    #[test]
    fn envelope_timing_is_frame_rate_independent_and_stereo_coordinates_keep_meaning() {
        let mut values = vec![];
        for fps in [30, 60, 144] {
            let mut rt = Runtime::default();
            let mut c = Settings::preset(0.1, 0.5);
            c.channels[1].adapt = 0.0;
            let mut b = VizBus::new();
            b.loud_db = -18.0;
            b.balance = -1.0;
            b.corr = 0.0;
            for _ in 0..fps {
                rt.update(&c, &b, [1.0; 6], 1.0 / fps as f32);
            }
            b.bass = 1.0;
            for _ in 0..fps {
                rt.update(&c, &b, [1.0; 6], 1.0 / fps as f32);
            }
            b.bass = 0.0;
            let mut v = [0.0; COUNT];
            for _ in 0..fps {
                v = rt.update(&c, &b, [1.0; 6], 1.0 / fps as f32);
            }
            assert_eq!(v[Source::Balance as usize], 0.0);
            assert!((v[Source::Correlation as usize] - 0.5).abs() < 0.0001);
            values.push(v[1]);
        }
        assert!((values[0] - values[2]).abs() < 0.00001);
    }
}
