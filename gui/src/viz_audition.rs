//! Bounded in-memory replay of analyzer frames, never an audio-output route.
use crate::{audio_analysis::Frame, show::ShowMode, viz_settings::VizSettings};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const DURATION: f64 = 8.0;
const MAX_FRAMES: usize = 481;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating { Keeper, NeedsWork, Boring }

#[derive(Default)]
pub struct Audition {
    frames: Vec<(f64, Arc<Frame>)>,
    recording: Option<(f64, u64)>,
    pub playing: bool,
    pub auto_next: bool,
    started: f64,
    active_scene: String,
    pub epoch: u64,
    pub progress: f32,
    pub message: String,
}
impl Audition {
    pub fn record(&mut self, now: f64) {
        self.frames.clear();
        self.recording = Some((now, u64::MAX));
        self.playing = false;
        self.message.clear();
        self.progress = 0.0;
    }
    pub fn restart(&mut self, now: f64) {
        self.started = now;
        self.progress = 0.0;
        self.epoch = self.epoch.wrapping_add(1);
    }
    pub fn tick(&mut self, live: Arc<Frame>, now: f64, scene: &mut ShowMode, config: &VizSettings) -> Arc<Frame> {
        if let Some((start, first_tick)) = self.recording {
            let first = if first_tick == u64::MAX { live.ticks } else { first_tick };
            self.recording = Some((start, first));
            self.progress = ((now-start)/DURATION).clamp(0.0, 1.0) as f32;
            if live.ticks < first {
                self.recording = None;
                self.frames.clear();
                self.message = "Capture restarted. Record the phrase again.".into();
            } else if self.frames.len() < MAX_FRAMES && self.frames.last().is_none_or(|(_,f)| f.ticks != live.ticks) {
                self.frames.push(((live.ticks-first) as f64/60.0, live.clone()));
            }
            if now-start >= DURATION && self.recording.is_some() {
                self.recording = None;
                if self.frames.len() >= 60 && self.frames.last().is_some_and(|(t,_)| *t >= 7.0) {
                    self.playing = true;
                    self.restart(now);
                    self.message = "Phrase captured. Sound remains live.".into();
                } else {
                    self.frames.clear();
                    self.message = "Not enough live analysis. Start music and record again.".into();
                }
            }
        }
        if !self.playing || self.frames.is_empty() { return live; }
        let key = format!("{}:{}", scene.as_str(), config.scene_variants.get(scene.as_str()).copied().unwrap_or(0));
        if self.active_scene != key {
            self.active_scene = key;
            self.restart(now);
        }
        if now-self.started >= DURATION {
            if self.auto_next {
                next_scene(scene, config);
                self.active_scene = format!("{}:{}", scene.as_str(), config.scene_variants.get(scene.as_str()).copied().unwrap_or(0));
            }
            self.restart(now);
        }
        let elapsed = (now-self.started).max(0.0);
        self.progress = (elapsed/DURATION) as f32;
        let index = self.frames.partition_point(|(t,_)| *t <= elapsed).saturating_sub(1);
        self.frames[index].1.clone()
    }
}
fn next_scene(scene: &mut ShowMode, config: &VizSettings) {
    let pool = config.scene_pool();
    let index = pool.iter().position(|s| s == scene).map_or(0, |i| (i+1)%pool.len());
    *scene = pool[index];
}

pub fn panel(ui: &mut egui::Ui, scene: &mut ShowMode, config: &mut VizSettings, audition: &mut Audition, selected: &mut Option<String>, shuffle: &mut bool) {
    let now = ui.input(|i| i.time);
    ui.label(egui::RichText::new("Audition the same phrase").strong());
    ui.label("Capture eight seconds of visual analysis and compare scenes against the same passage.");
    ui.small("This loops the visuals only. Your music and EQ continue normally. The capture stays in memory.");
    if audition.recording.is_some() {
        ui.add(egui::ProgressBar::new(audition.progress).text("Recording analysis..."));
        if ui.button("Cancel recording").clicked() { audition.recording = None; audition.frames.clear(); }
    } else if ui.button("Record 8-second phrase").clicked() {
        audition.record(now); *shuffle = false;
    }
    if !audition.frames.is_empty() && audition.recording.is_none() {
        ui.horizontal(|ui| {
            if ui.selectable_label(audition.playing, "Loop phrase").clicked() { audition.playing = true; audition.restart(now); *shuffle = false; }
            if ui.selectable_label(!audition.playing, "Live input").clicked() { audition.playing = false; audition.restart(now); }
        });
        if audition.playing { ui.add(egui::ProgressBar::new(audition.progress).text("Phrase playback")); }
        ui.checkbox(&mut audition.auto_next, "Next scene after each phrase");
        ui.horizontal(|ui| {
            if ui.button("Restart phrase").clicked() { audition.restart(now); }
            if ui.button("Next scene").clicked() { next_scene(scene, config); audition.restart(now); }
        });
        if ui.small_button("Discard capture").clicked() {
            audition.frames.clear(); audition.playing = false; audition.restart(now); audition.message.clear();
        }
    }
    if !audition.message.is_empty() { ui.small(&audition.message); }
    ui.separator();
    let looks: Vec<_> = crate::viz_looks::factory().into_iter().chain(config.saved_looks.iter().cloned()).collect();
    egui::ComboBox::from_id_salt("audition-look").selected_text(selected.as_deref().unwrap_or("Compare a look")).show_ui(ui, |ui| {
        for look in &looks {
            if ui.selectable_label(selected.as_deref() == Some(&look.name), &look.name).clicked() {
                look.apply(scene, config); *selected = Some(look.name.clone()); audition.restart(now);
            }
        }
    });
    let key = selected.as_ref().filter(|name| looks.iter().any(|l| &l.name == *name && l.matches(*scene, config)))
        .map(|name| format!("look:{name}"))
        .unwrap_or_else(|| format!("scene:{}:{}", scene.as_str(), config.scene_variants.get(scene.as_str()).copied().unwrap_or(0)));
    ui.small("Rate this look / scene variation");
    ui.horizontal_wrapped(|ui| {
        for (rating, label) in [(Rating::Keeper,"Keeper"),(Rating::NeedsWork,"Needs work"),(Rating::Boring,"Boring")] {
            let active = config.review_ratings.get(&key) == Some(&rating);
            if ui.selectable_label(active, label).clicked() {
                if active { config.review_ratings.remove(&key); } else { config.review_ratings.insert(key.clone(), rating); }
            }
        }
    });
    egui::CollapsingHeader::new(format!("Review notes ({})",config.review_ratings.len())).show(ui, |ui| {
        for (key, rating) in &config.review_ratings {
            ui.small(format!("{}: {}",key,match rating { Rating::Keeper=>"Keeper", Rating::NeedsWork=>"Needs work", Rating::Boring=>"Boring" }));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_is_bounded_replays_same_ticks_and_restarts_when_scene_changes() {
        let mut audition = Audition::default();
        let config = VizSettings::default();
        let mut scene = ShowMode::Tunnel;
        let mut frame = crate::audio_analysis::Analyzer::new(48000, 2).snapshot();
        audition.record(0.0);
        for i in 0..=480 { frame.ticks=i; audition.tick(Arc::new(frame.clone()), i as f64/60.0, &mut scene, &config); }
        assert!(audition.playing && audition.frames.len() == MAX_FRAMES);
        let live = Arc::new(frame);
        assert_eq!(audition.tick(live.clone(), 9.0, &mut scene, &config).ticks, 60);
        scene = ShowMode::GlFlame;
        assert_eq!(audition.tick(live.clone(), 9.1, &mut scene, &config).ticks, 0);
        assert_eq!(audition.tick(live.clone(), 10.1, &mut scene, &config).ticks, 60);
        audition.playing = false;
        assert!(Arc::ptr_eq(&audition.tick(live.clone(), 11.0, &mut scene, &config), &live));
    }
    #[test]
    fn stalled_capture_does_not_replay_a_frozen_frame() {
        let mut audition = Audition::default();
        let frame = Arc::new(crate::audio_analysis::Analyzer::new(48000,2).snapshot());
        audition.record(0.0);
        audition.tick(frame.clone(),0.0,&mut ShowMode::Tunnel,&VizSettings::default());
        audition.tick(frame,8.1,&mut ShowMode::Tunnel,&VizSettings::default());
        assert!(!audition.playing && audition.frames.is_empty());
        assert!(!audition.message.is_empty());
    }
}
