//! Session-only sound history and A/B slots. Output calibration and bypass stay outside it.
use eframe::egui;
use tronteq_shared::{Band, Dynamics, NUM_BANDS};

#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub bands: [Band; NUM_BANDS],
    pub preamp: f32,
    pub dynamics: Dynamics,
    pub profile: Option<String>,
}
impl Sound {
    fn same_settings(&self, other: &Self) -> bool {
        self.bands == other.bands && self.preamp == other.preamp && self.dynamics == other.dynamics
    }
}

#[derive(Clone, PartialEq)]
struct State {
    slots: [Sound; 2],
    active: usize,
}

pub struct Editor {
    state: State,
    pending: Option<State>,
    undo: Vec<State>,
    redo: Vec<State>,
}

impl Editor {
    pub fn new(sound: Sound) -> Self {
        Self {
            state: State {
                slots: [sound.clone(), sound],
                active: 0,
            },
            pending: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
    pub fn current(&self) -> &Sound {
        &self.state.slots[self.state.active]
    }
    fn begin(&mut self) {
        if self.pending.is_none() {
            self.pending = Some(self.state.clone());
        }
    }
    pub fn observe(&mut self, sound: Sound) {
        if !self.current().same_settings(&sound) {
            self.begin();
        }
        self.state.slots[self.state.active] = sound;
    }
    /// One drag is one undo item; a gesture returning to its starting state is ignored.
    pub fn finish_gesture(&mut self) {
        if let Some(before) = self.pending.take() {
            if before != self.state {
                if self.undo.len() == 128 {
                    self.undo.remove(0);
                }
                self.undo.push(before);
                self.redo.clear();
            }
        }
    }
    pub fn can_undo(&self) -> bool {
        self.pending.is_some() || !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        self.pending.is_none() && !self.redo.is_empty()
    }
    pub fn undo(&mut self) -> Option<Sound> {
        self.finish_gesture();
        let previous = self.undo.pop()?;
        self.redo.push(std::mem::replace(&mut self.state, previous));
        Some(self.current().clone())
    }
    pub fn redo(&mut self) -> Option<Sound> {
        self.finish_gesture();
        let next = self.redo.pop()?;
        self.undo.push(std::mem::replace(&mut self.state, next));
        Some(self.current().clone())
    }
    pub fn select(&mut self, slot: usize) -> Option<Sound> {
        if slot > 1 || slot == self.state.active {
            return None;
        }
        self.finish_gesture();
        self.begin();
        self.state.active = slot;
        self.finish_gesture();
        Some(self.current().clone())
    }
    pub fn copy_to_other(&mut self) {
        self.finish_gesture();
        self.begin();
        self.state.slots[1 - self.state.active] = self.current().clone();
        self.finish_gesture();
    }
    pub fn controls(&mut self, ui: &mut egui::Ui) -> Option<Sound> {
        let mut result = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui.add_enabled(self.can_undo(), egui::Button::new("Undo").small()).on_hover_text("Undo sound edit (Ctrl+Z)").clicked() { result = self.undo(); }
            if ui.add_enabled(self.can_redo(), egui::Button::new("Redo").small()).on_hover_text("Redo sound edit (Ctrl+Y or Ctrl+Shift+Z)").clicked() { result = self.redo(); }
            ui.separator();
            for (slot, label) in [(0, "A"), (1, "B")] {
                if ui.selectable_label(self.state.active == slot, label).on_hover_text("Compare two sounds during this session. Save a profile to keep either sound. Bypass and A/V sync are preserved.").clicked() { result = self.select(slot); }
            }
            let label = if self.state.active == 0 { "A > B" } else { "B > A" };
            if ui.small_button(label).on_hover_text("Copy this sound to the other slot. Undo restores the previous comparison.").clicked() { self.copy_to_other(); }
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sound(gain: f32) -> Sound {
        Sound {
            bands: [Band {
                gain,
                ..Band::flat(1000.0)
            }; NUM_BANDS],
            preamp: 0.0,
            dynamics: Dynamics::default_passive(),
            profile: None,
        }
    }
    #[test]
    fn a_drag_is_one_undo_item_and_new_edits_clear_redo() {
        let mut e = Editor::new(sound(0.0));
        for i in 1..=100 {
            e.observe(sound(i as f32 / 10.0));
        }
        e.finish_gesture();
        assert_eq!(e.undo().unwrap().bands[0].gain, 0.0);
        assert!(!e.can_undo());
        assert_eq!(e.redo().unwrap().bands[0].gain, 10.0);
        e.undo();
        e.observe(sound(-3.0));
        e.finish_gesture();
        assert!(!e.can_redo());
    }
    #[test]
    fn slots_keep_independent_edits_and_undo_restores_slot_selection_and_copy() {
        let mut e = Editor::new(sound(0.0));
        e.observe(sound(3.0));
        e.finish_gesture();
        e.copy_to_other();
        assert_eq!(e.select(1).unwrap().bands[0].gain, 3.0);
        e.observe(sound(-6.0));
        e.finish_gesture();
        assert_eq!(e.select(0).unwrap().bands[0].gain, 3.0);
        assert_eq!(e.undo().unwrap().bands[0].gain, -6.0);
        assert_eq!(e.state.active, 1);
        e.copy_to_other();
        e.undo();
        assert_eq!(e.select(0).unwrap().bands[0].gain, 3.0);
    }
    #[test]
    fn unchanged_gestures_and_profile_markers_do_not_make_sound_undo_entries() {
        let mut e = Editor::new(sound(0.0));
        e.observe(sound(2.0));
        e.observe(sound(0.0));
        e.finish_gesture();
        assert!(!e.can_undo());
        e.observe(Sound {
            profile: Some("Flat".into()),
            ..sound(0.0)
        });
        e.finish_gesture();
        assert!(!e.can_undo());
        e.copy_to_other();
        assert_eq!(e.current().bands[0].gain, 0.0);
    }
    #[test]
    fn history_is_bounded_and_controls_switch_and_undo() {
        use egui_kittest::{kittest::Queryable, Harness};
        let mut e = Editor::new(sound(0.0));
        for i in 1..=200 {
            e.observe(sound(i as f32 / 10.0));
            e.finish_gesture();
        }
        assert_eq!(e.undo.len(), 128);
        let mut h = Harness::builder()
            .with_size(egui::vec2(310.0, 160.0))
            .build_ui_state(
                |ui, e| {
                    e.controls(ui);
                },
                e,
            );
        assert!(h.get_by_label("A > B").rect().right() <= 310.0);
        h.get_by_label("B").click();
        h.run();
        assert_eq!(h.state().current().bands[0].gain, 0.0);
        h.get_by_label("Undo").click();
        h.run();
        assert_eq!(h.state().current().bands[0].gain, 20.0);
        assert_eq!(h.state().state.active, 0);
    }
}
