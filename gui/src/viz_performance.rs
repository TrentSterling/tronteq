//! Performance state: persisted controls and a separate clock/envelope runtime.
use crate::{
    glstage::Uniforms,
    show::ShowMode,
    viz_looks::Look,
    viz_motion::{Runtime as MotionRuntime, Source},
    viz_settings::{Effect, EffectSlot, VizSettings},
    vizbus::VizBus,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flash {
    pub source: Source,
    pub amount: f32,
    pub threshold: f32,
    pub decay: f32,
    pub max_rate: f32,
}
impl Default for Flash {
    fn default() -> Self {
        Self {
            source: Source::Beat,
            amount: 0.35,
            threshold: 0.2,
            decay: 0.22,
            max_rate: 4.0,
        }
    }
}
impl Flash {
    pub fn sanitize(&mut self) {
        self.amount = finite(self.amount, 0.0, 1.0, 0.35);
        self.threshold = finite(self.threshold, 0.0, 1.0, 0.2);
        self.decay = finite(self.decay, 0.03, 2.0, 0.22);
        self.max_rate = finite(self.max_rate, 0.25, 12.0, 4.0);
    }
}
pub fn finite(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        default
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub look: Look,
    pub hold_beats: f32,
    pub fade_beats: f32,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Performance {
    pub blend:crate::viz_compositor::Blend,
    pub blend_reverse:bool,
    pub interaction: crate::viz_compositor::Interaction,
    pub enabled: bool,
    pub deck_b: Option<Look>,
    pub crossfade: f32,
    pub fade_beats: f32,
    pub manual_bpm: f32,
    pub follow_bpm: bool,
    pub pad_strength: f32,
    pub release: f32,
    pub setlist: Vec<Cue>,
    pub loop_setlist: bool,
}
impl Default for Performance {
    fn default() -> Self {
        Self {
            blend:Default::default(),blend_reverse:false,interaction:Default::default(),
            enabled: false,
            deck_b: None,
            crossfade: 0.0,
            fade_beats: 8.0,
            manual_bpm: 120.0,
            follow_bpm: true,
            pad_strength: 1.0,
            release: 0.3,
            setlist: vec![],
            loop_setlist: true,
        }
    }
}
impl Performance {
    pub fn sanitize(&mut self) {
        self.interaction.sanitize();
        self.crossfade = finite(self.crossfade, 0.0, 1.0, 0.0);
        self.fade_beats = finite(self.fade_beats, 0.0, 64.0, 8.0);
        self.manual_bpm = finite(self.manual_bpm, 40.0, 240.0, 120.0);
        self.pad_strength = finite(self.pad_strength, 0.0, 1.0, 1.0);
        self.release = finite(self.release, 0.03, 2.0, 0.3);
        self.setlist.truncate(128);
        for cue in &mut self.setlist {
            cue.hold_beats = finite(cue.hold_beats, 1.0, 256.0, 32.0);
            cue.fade_beats = finite(cue.fade_beats, 0.0, 64.0, 8.0);
        }
        self.setlist
            .retain(|c| c.look.available() && c.look.scene != "off");
        if self
            .deck_b
            .as_ref()
            .is_some_and(|l| !l.available() || l.scene == "off")
        {
            self.deck_b = None;
        }
    }
}

#[derive(Default)]
pub struct FlashEnvelope {
    last: f32,
    since: f32,
    value: f32,
    source: Option<Source>,
}
impl FlashEnvelope {
    pub fn tick(&mut self, flash: &Flash, source: f32, amount: f32, dt: f32) -> f32 {
        if self.source != Some(flash.source) {
            *self = Self {
                source: Some(flash.source),
                since: 100.0,
                ..Default::default()
            };
        }
        self.since += dt;
        self.value *= (-dt / flash.decay.max(0.03)).exp();
        if flash.source != Source::Off
            && source > flash.threshold
            && self.last <= flash.threshold
            && self.since >= 1.0 / flash.max_rate.max(0.25)
        {
            self.value = 1.0;
            self.since = 0.0;
        }
        self.last = source;
        if flash.source == Source::Off {
            self.value = 0.0;
        }
        self.value * amount
    }
}

pub const PADS: [&str; 5] = ["Punch", "Scatter", "Freeze", "Sweep", "Color burst"];
#[derive(Default)]
pub struct Runtime {
    pub swap_decks: bool,
    pub held: [bool; 5],
    pub pads: [f32; 5],
    pub motion_b: MotionRuntime,
    pub clock: f32,
    pub fade: Option<Fade>,
    pub playing: bool,
    pub cue: usize,
    pub held_beats: f32,
    pub flash_a: FlashEnvelope,
    pub flash_b: FlashEnvelope,
    pub rack_name: String,
    pub mutation: f32,
    pub locks: [bool; 4],
    pub rng: u64,
    pub notice: String,
}
pub struct Fade {
    from: f32,
    to: f32,
    elapsed: f32,
    beats: f32,
    commit: bool,
}
impl Runtime {
    pub fn tempo(config: &Performance, bus: &VizBus) -> f32 {
        if config.follow_bpm && bus.bpm_conf >= 0.25 && bus.bpm.is_finite() && bus.bpm >= 40.0 {
            bus.bpm.clamp(40.0, 240.0)
        } else {
            config.manual_bpm
        }
    }
    pub fn fade_to(&mut self, config: &Performance, to: f32) {
        self.playing = false;
        self.fade = Some(Fade {
            from: config.crossfade,
            to,
            elapsed: 0.0,
            beats: config.fade_beats,
            commit: false,
        });
    }
    pub fn start(&mut self, mode: &mut ShowMode, config: &mut VizSettings) {
        if let Some(cue) = config.performance.setlist.first().cloned() {
            cue.look.apply(mode, config);
            config.performance.enabled = false;
            config.performance.crossfade = 0.0;
            self.cue = 0;
            self.held_beats = 0.0;
            self.playing = true;
            self.fade = None;
        }
    }
    pub fn next(&mut self, config: &mut VizSettings) {
        if self.fade.is_some() || config.performance.setlist.is_empty() {
            return;
        }
        let next = self.cue + 1;
        if next >= config.performance.setlist.len() && !config.performance.loop_setlist {
            self.playing = false;
            return;
        }
        self.cue = next % config.performance.setlist.len();
        let cue = config.performance.setlist[self.cue].clone();
        config.performance.deck_b = Some(cue.look);
        config.performance.enabled = true;
        config.performance.crossfade = 0.0;
        self.fade = Some(Fade {
            from: 0.0,
            to: 1.0,
            elapsed: 0.0,
            beats: cue.fade_beats,
            commit: true,
        });
        self.held_beats = 0.0;
    }
    pub fn tick(
        &mut self,
        mode: &mut ShowMode,
        config: &mut VizSettings,
        bus: &VizBus,
        dt: f32,
    ) -> f32 {
        let dt = dt.clamp(0.0, 0.1);
        self.clock += dt;
        let beat_step = dt * Self::tempo(&config.performance, bus) / 60.0;
        if self.playing && self.fade.is_none() {
            self.held_beats += beat_step;
            if let Some(cue) = config.performance.setlist.get(self.cue) {
                if self.held_beats >= cue.hold_beats {
                    self.next(config);
                }
            } else {
                self.playing = false;
            }
        }
        let mut mix = config.performance.crossfade;
        if let Some(fade) = &mut self.fade {
            let already_rendered = fade.elapsed > 0.0;
            fade.elapsed += beat_step;
            let t = if fade.beats <= 0.0 {
                1.0
            } else {
                (fade.elapsed / fade.beats).min(1.0)
            };
            mix = fade.from + (fade.to - fade.from) * t;
            config.performance.crossfade = mix;
            if t >= 1.0 && (!fade.commit || already_rendered) {
                let commit = fade.commit;
                self.fade = None;
                config.performance.crossfade = mix;
                if commit {
                    if let Some(look) = config.performance.deck_b.clone() {
                        look.apply(mode, config);
                    }
                    self.swap_decks = true;
                    config.performance.crossfade = 0.0;
                    config.performance.enabled = false;
                    mix = 0.0;
                }
            }
        }
        mix
    }
    pub fn tick_pads(&mut self, config: &VizSettings, motion: &mut MotionRuntime, dt: f32) {
        for i in 0..5 {
            let driven = motion.value(&config.motion, &format!("pad:{i}"), 0.0);
            let target = (if self.held[i] { 1.0f32 } else { 0.0 }).max(driven)
                * config.performance.pad_strength;
            let tau = if target > self.pads[i] {
                0.04
            } else {
                config.performance.release
            };
            self.pads[i] += (target - self.pads[i]) * (1.0 - (-dt.max(0.0) / tau.max(0.01)).exp());
            if self.pads[i] < 0.0001 {
                self.pads[i] = 0.0;
            }
        }
    }
    pub fn apply_pads(&self, u: &mut Uniforms) {
        u.brightness = (u.brightness * (1.0 + self.pads[0] * 0.35)).min(1.5);
        u.transform[0] = (u.transform[0] + self.pads[0] * 0.22).min(2.0);
        add_effect(u, Effect::Pixelate, self.pads[1] * 0.55);
        add_effect(u, Effect::Aberration, self.pads[1] * 0.7);
        u.freeze = self.pads[2] > 0.5;
        u.transform[1] += self.pads[3] * (self.clock * 1.8).sin() * 0.45;
        u.transform[2] += self.pads[3] * (self.clock * 1.2).cos() * 0.12;
        u.hue = (u.hue + self.pads[4] * 0.3).fract();
        u.saturation = (u.saturation + self.pads[4] * 0.7).min(2.0);
    }
}
fn add_effect(u: &mut Uniforms, effect: Effect, amount: f32) {
    if amount < 0.0001 {
        return;
    }
    if let Some(i) = u.fx_order[..u.fx_count]
        .iter()
        .position(|b| *b == effect.bit())
    {
        u.fx_mix[i] = u.fx_mix[i].max(amount);
    } else if u.fx_count < 10 {
        u.fx_order[u.fx_count] = effect.bit();
        u.fx_mix[u.fx_count] = amount;
        u.fx_count += 1;
    }
}

/// Shared by both decks; source frames remain immutable.
pub fn uniforms(
    config: &VizSettings,
    mode: ShowMode,
    frame: &crate::audio_analysis::Frame,
    motion: &mut MotionRuntime,
    flash: &mut FlashEnvelope,
    dt: f32,
    epoch: u64,
) -> Uniforms {
    motion.update(&config.motion,&frame.bus,config.signal_gains,&config.signal_response,dt);
    motion.xy.tick(&config.xy,dt);
    motion.xy.display_position=[motion.value(&config.motion,"xy_x",motion.xy.position[0]),motion.value(&config.motion,"xy_y",motion.xy.position[1])];
    let mut morphed;
    let config=if config.xy.enabled && config.xy.scene==mode.as_str(){morphed=config.clone();motion.xy.apply(mode,&mut morphed);&morphed}else{config};
    let b = &frame.bus;
    let r = motion.resolve_values(config, mode.as_str());
    let energy = r.energy;
    let acc = crate::theme::viz_accent();
    let flash_amount = motion.value(&config.motion, "flash_amount", config.flash.amount);
    let flash_value = flash.tick(
        &config.flash,
        motion.signals[config.flash.source as usize],
        flash_amount,
        dt.clamp(0.0, 0.1),
    );
    Uniforms {
        blend:config.performance.blend as u32,blend_reverse:config.performance.blend_reverse,global_pulse:config.pulse_control.array(),
        interaction_mode: config.performance.interaction.mode as u32,
        interaction_direction: config.performance.interaction.direction as u32,
        interaction: [motion.value(&config.motion,"deck_distortion",config.performance.interaction.amount),
            motion.value(&config.motion,"deck_scale",config.performance.interaction.scale),
            motion.value(&config.motion,"deck_angle",config.performance.interaction.angle),
            motion.value(&config.motion,"deck_softness",config.performance.interaction.softness)],
        mode: mode.gl_mode().unwrap_or(crate::glstage::GlMode::Warp),
        epoch,
        bass: ((if config.signal_response.enabled { motion.signals[Source::Bass as usize] } else { b.bass * config.signal_gains[0] }) * energy).min(1.0),
        mid: ((if config.signal_response.enabled { motion.signals[Source::Mids as usize] } else { b.mid * config.signal_gains[1] }) * energy).min(1.0),
        treble: ((if config.signal_response.enabled { motion.signals[Source::Highs as usize] } else { b.treble * config.signal_gains[2] }) * energy).min(1.0),
        pulse: ((if config.signal_response.enabled { motion.signals[Source::Beat as usize] } else { b.pulse * config.signal_gains[3] }) * energy).min(1.0),
        beat_phase: b.beat_phase,
        bright: motion.signals[Source::Brightness as usize],
        rainbow: config.rainbow,
        accent: config.accent.unwrap_or([
            acc.r() as f32 / 255.0,
            acc.g() as f32 / 255.0,
            acc.b() as f32 / 255.0,
        ]),
        hue: r.hue,
        saturation: r.saturation,
        controls: r.controls,
        details: r.details,
        variant: config
            .scene_variants
            .get(mode.as_str())
            .copied()
            .unwrap_or(0),
        spec: std::array::from_fn(|i| (frame.spectrum[i] * energy).clamp(0.0, 1.0)),
        wave: std::array::from_fn(|i| {
            (frame
                .wave
                .get(i * frame.wave.len().max(1) / crate::glstage::WAVE_W)
                .copied()
                .unwrap_or(0.0)
                * energy)
                .clamp(-1.0, 1.0)
        }),
        stereo: std::array::from_fn(|i| {
            let s = frame
                .stereo
                .get(i * frame.stereo.len().max(1) / crate::glstage::STEREO_W)
                .copied()
                .unwrap_or([0.0; 2]);
            [s[0] * energy, s[1] * energy]
        }),
        bpm: b.bpm,
        bpm_conf: b.bpm_conf,
        flux: ((if config.signal_response.enabled { motion.signals[Source::Change as usize] } else { b.flux * config.signal_gains[4] }) * energy).min(1.0),
        loud: (motion.signals[Source::Level as usize] * energy).min(1.0),
        crest: motion.signals[Source::Crest as usize],
        corr: if config.signal_response.enabled { motion.signals[Source::Correlation as usize] * 2.0 - 1.0 } else { b.corr },
        width: (motion.signals[Source::Width as usize]).min(1.0),
        fx: 0,
        fx_order: r.order,
        fx_mix: r.mixes,
        fx_count: r.count,
        speed: r.speed,
        brightness: r.brightness,
        transform: std::array::from_fn(|i| {
            motion.value(
                &config.motion,
                ["zoom", "rotation", "pan_x", "pan_y"][i],
                config.transform[i],
            )
        }),
        flash: flash_value,
        render_scale: config.quality.scale(),
        transition_seconds: config.transition_seconds,
        ..Default::default()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Rack {
    pub name: String,
    pub effects: Vec<EffectSlot>,
    pub motion: crate::viz_motion::Motion,
    pub flash: Flash,
}
impl Rack {
    pub fn capture(name: String, c: &VizSettings) -> Self {
        let mut motion = c.motion.clone();
        motion
            .routes
            .retain(|k, _| k.starts_with("effect:") || k == "flash_amount");
        motion
            .ranges
            .retain(|k, _| k.starts_with("effect:") || k == "flash_amount");
        Self {
            name,
            effects: c.effects.clone(),
            motion,
            flash: c.flash.clone(),
        }
    }
    pub fn apply(&self, c: &mut VizSettings) {
        c.effects = self.effects.clone();
        c.effects_enabled = true;
        c.flash = self.flash.clone();
        c.motion
            .routes
            .retain(|k, _| !k.starts_with("effect:") && k != "flash_amount");
        c.motion
            .ranges
            .retain(|k, _| !k.starts_with("effect:") && k != "flash_amount");
        c.motion.routes.extend(self.motion.routes.clone());
        c.motion.ranges.extend(self.motion.ranges.clone());
        c.sanitize();
    }
}
pub fn factory_racks() -> Vec<Rack> {
    [
        (
            "Dirty CRT",
            vec![
                (Effect::Scanlines, 0.65),
                (Effect::Grain, 0.25),
                (Effect::Aberration, 0.2),
            ],
        ),
        (
            "Liquid neon",
            vec![
                (Effect::ZoomBlur, 0.35),
                (Effect::EdgeGlow, 0.5),
                (Effect::Aberration, 0.25),
            ],
        ),
        (
            "Print crush",
            vec![(Effect::Halftone, 0.65), (Effect::Thermal, 0.4)],
        ),
    ]
    .into_iter()
    .map(|(name, fx)| Rack {
        name: name.into(),
        effects: fx
            .into_iter()
            .map(|(effect, mix)| EffectSlot {
                effect,
                mix,
                enabled: true,
            })
            .collect(),
        motion: Default::default(),
        flash: Default::default(),
    })
    .collect()
}
pub fn mutate(
    mode: &mut ShowMode,
    c: &mut VizSettings,
    strength: f32,
    locks: [bool; 4],
    seed: &mut u64,
) {
    let mut random = || {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 32) as u32 as f32) / (u32::MAX as f32)
    };
    let amount = strength.clamp(0.0, 1.0);
    if amount == 0.0 {
        return;
    }
    if !locks[0] && random() < amount {
        let pool = c.scene_pool();
        *mode = pool[(random() * pool.len() as f32) as usize % pool.len()];
    }
    if !locks[1] {
        c.hue = (c.hue + (random() - 0.5) * amount).rem_euclid(1.0);
        c.saturation = (c.saturation + (random() - 0.5) * amount).clamp(0.0, 2.0);
    }
    if !locks[2] {
        c.speed = (c.speed + (random() - 0.5) * amount * 1.5).clamp(0.0, 2.0);
        c.energy = (c.energy + (random() - 0.5) * amount).clamp(0.0, 2.0);
        let controls = c
            .scene_controls
            .entry(mode.as_str().into())
            .or_insert([1.0; 4]);
        for v in controls {
            *v = (*v + (random() - 0.5) * amount * 1.5).clamp(0.0, 2.0);
        }
    }
    if !locks[3] {
        for slot in &mut c.effects {
            slot.mix = (slot.mix + (random() - 0.5) * amount).clamp(0.0, 1.0);
        }
        if random() < amount {
            let effects: Vec<_> = Effect::ALL
                .into_iter()
                .filter(|e| *e != Effect::Strobe && !c.effects.iter().any(|s| s.effect == *e))
                .collect();
            if !effects.is_empty() {
                c.effects.push(EffectSlot {
                    effect: effects[(random() * effects.len() as f32) as usize % effects.len()],
                    mix: amount * 0.45,
                    enabled: true,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_scene_gains_keep_energy_before_clipping() {
        let mut c=VizSettings::default();c.energy=0.5;c.signal_gains=[2.0;6];
        let mut frame=crate::audio_analysis::Analyzer::new(48000,2).snapshot();
        frame.bus.bass=0.7;frame.bus.mid=0.8;frame.bus.treble=0.9;frame.bus.pulse=0.75;frame.bus.flux=0.6;
        let u=uniforms(&c,ShowMode::GlFlame,&frame,&mut MotionRuntime::default(),&mut FlashEnvelope::default(),1.0/60.0,0);
        assert_eq!([u.bass,u.mid,u.treble,u.pulse,u.flux],[0.7,0.8,0.9,0.75,0.6]);
    }
    #[test]
    fn conditioned_signals_drive_scene_routes_and_distortion_without_mutating_looks() {
        let mut c=VizSettings::default();c.signal_response=crate::viz_signals::Settings::preset(0.0,0.0);
        c.motion.routes.insert("deck_distortion".into(),crate::viz_motion::Route{source:Source::Bass,depth:0.5,smoothing:0.0,..Default::default()});
        c.performance.interaction.amount=0.1;c.performance.interaction.mode=crate::viz_compositor::InteractionMode::Flow;
        let before=serde_json::to_string(&c).unwrap();
        let mut frame=crate::audio_analysis::Analyzer::new(48000,2).snapshot();frame.bus.loud_db=-18.0;frame.bus.bass=0.96;
        let mut motion=MotionRuntime::default();let mut flash=FlashEnvelope::default();
        let mut u=Uniforms::default();
        for _ in 0..180 {u=uniforms(&c,ShowMode::GlFlame,&frame,&mut motion,&mut flash,1.0/60.0,0);}
        assert!(u.bass<0.7 && u.bass>0.5,"steady bass should regain headroom");
        assert!((u.bass-motion.signals[Source::Bass as usize]).abs()<0.00001);
        assert!((u.interaction[0]-(0.1+u.bass*0.5)).abs()<0.00001);
        assert_eq!(serde_json::to_string(&c).unwrap(),before);
        frame.bus.loud_db=-90.0;
        let quiet=uniforms(&c,ShowMode::GlFlame,&frame,&mut motion,&mut flash,1.0/60.0,0);
        assert_eq!(quiet.bass,0.0);assert_eq!(quiet.interaction[0],0.1);
    }
    #[test]
    fn flash_requires_an_edge_obeys_rate_and_turns_off() {
        let mut f = Flash {
            max_rate: 2.0,
            decay: 0.1,
            ..Default::default()
        };
        let mut rt = FlashEnvelope::default();
        assert_eq!(rt.tick(&f, 1.0, 0.6, 0.01), 0.6);
        let held = rt.tick(&f, 1.0, 0.6, 0.1);
        assert!(held < 0.3);
        rt.tick(&f, 0.0, 0.6, 0.01);
        assert!(rt.tick(&f, 1.0, 0.6, 0.01) < held);
        rt.tick(&f, 0.0, 0.6, 0.5);
        assert_eq!(rt.tick(&f, 1.0, 0.6, 0.01), 0.6);
        f.source = Source::Off;
        assert_eq!(rt.tick(&f, 1.0, 0.6, 0.01), 0.0);
    }
    #[test]
    fn reversing_a_fade_uses_the_visible_mix_and_tempo_fallback() {
        let mut c = VizSettings::default();
        c.performance.fade_beats = 2.0;
        let mut rt = Runtime::default();
        let mut mode = ShowMode::GlFlame;
        let bus = VizBus::new();
        rt.fade_to(&c.performance, 1.0);
        for _ in 0..5 {
            rt.tick(&mut mode, &mut c, &bus, 0.1);
        }
        assert!((c.performance.crossfade - 0.5).abs() < 0.001);
        rt.fade_to(&c.performance, 0.0);
        assert!((rt.tick(&mut mode, &mut c, &bus, 0.1) - 0.45).abs() < 0.001);
        assert_eq!(Runtime::tempo(&c.performance, &bus), 120.0);
        let mut bus = bus;
        bus.bpm = 90.0;
        bus.bpm_conf = 0.9;
        assert_eq!(Runtime::tempo(&c.performance, &bus), 90.0);
    }
    #[test]
    fn setlist_commits_incoming_deck_and_stops_at_end() {
        let mut c = VizSettings::default();
        let mut mode = ShowMode::GlFlame;
        c.performance.loop_setlist = false;
        c.performance.setlist = [ShowMode::GlFlame, ShowMode::GlTerrain]
            .into_iter()
            .map(|m| Cue {
                look: Look::capture(m.label(), m, &c),
                hold_beats: 1.0,
                fade_beats: 1.0,
            })
            .collect();
        let mut rt = Runtime::default();
        rt.start(&mut mode, &mut c);
        for _ in 0..11 {
            rt.tick(&mut mode, &mut c, &VizBus::new(), 0.1);
        }
        assert!(mode == ShowMode::GlTerrain);
        assert!(rt.swap_decks);
        assert!(!c.performance.enabled);
        for _ in 0..6 {
            rt.tick(&mut mode, &mut c, &VizBus::new(), 0.1);
        }
        assert!(!rt.playing);
        c.performance.setlist[1].fade_beats = 0.0;
        rt.start(&mut mode, &mut c);
        rt.next(&mut c);
        assert_eq!(rt.tick(&mut mode, &mut c, &VizBus::new(), 0.1), 1.0);
        assert!(mode == ShowMode::GlFlame);
        rt.tick(&mut mode, &mut c, &VizBus::new(), 0.1);
        assert!(mode == ShowMode::GlTerrain);
    }
    #[test]
    fn pads_return_to_base_without_changing_the_authored_look() {
        let c = VizSettings::default();
        let before = serde_json::to_string(&c).unwrap();
        let mut rt = Runtime::default();
        let mut motion = MotionRuntime::default();
        rt.held.fill(true);
        for _ in 0..30 {
            rt.tick_pads(&c, &mut motion, 1.0 / 60.0);
        }
        let mut u = Uniforms::default();
        rt.apply_pads(&mut u);
        assert!(u.freeze && u.transform[0] > 1.1 && u.fx_count >= 2);
        rt.held.fill(false);
        for _ in 0..240 {
            rt.tick_pads(&c, &mut motion, 1.0 / 60.0);
        }
        assert_eq!(rt.pads, [0.0; 5]);
        let mut u = Uniforms::default();
        rt.apply_pads(&mut u);
        assert!(!u.freeze);
        assert_eq!(u.transform, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(serde_json::to_string(&c).unwrap(), before);
    }
    #[test]
    fn racks_preserve_scene_palette_and_non_effect_routes() {
        let mut c = VizSettings::default();
        c.hue = 0.27;
        c.motion.routes.insert(
            "zoom".into(),
            crate::viz_motion::Route {
                source: Source::Bass,
                ..Default::default()
            },
        );
        let mut rack = factory_racks().remove(0);
        rack.motion.routes.insert(
            "effect:4".into(),
            crate::viz_motion::Route {
                source: Source::Kick,
                ..Default::default()
            },
        );
        rack.apply(&mut c);
        assert_eq!(c.hue, 0.27);
        assert!(c.motion.routes.contains_key("zoom"));
        assert!(c.motion.routes.contains_key("effect:4"));
        assert!(Rack::capture("Saved".into(), &c)
            .motion
            .routes
            .keys()
            .all(|k| k.starts_with("effect:")));
        c.saved_racks.push(rack);
        let decoded: VizSettings =
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(decoded.saved_racks.len(), 1);
    }
    #[test]
    fn mutation_honors_locks_and_is_undoable() {
        let mut mode = ShowMode::GlFlame;
        let mut c = VizSettings::default();
        let before = Look::capture("", mode, &c);
        let mut seed = 42;
        mutate(&mut mode, &mut c, 1.0, [true; 4], &mut seed);
        assert!(before.matches(mode, &c));
        let mut editor = crate::viz_edit::Editor::default();
        editor.observe(mode, &c, &None);
        mutate(
            &mut mode,
            &mut c,
            1.0,
            [true, true, false, false],
            &mut seed,
        );
        assert!(mode == ShowMode::GlFlame);
        assert_eq!(c.hue, before.hue);
        assert!(!before.matches(mode, &c));
        editor.observe(mode, &c, &None);
        editor.finish_gesture();
        assert!(editor.undo().is_some());
    }
}
