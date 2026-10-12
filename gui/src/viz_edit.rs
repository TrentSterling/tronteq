//! Session history for artwork. Sound and saved-library preferences never enter it.
use crate::{show::ShowMode, viz_looks::Look, viz_settings::VizSettings};
use eframe::egui;

#[derive(Clone)]
pub struct Edit {
    look: Look,
    selected: Option<String>,
}
impl Edit {
    fn capture(mode: ShowMode, visual: &VizSettings, selected: &Option<String>) -> Self {
        Self {
            look: Look::capture("", mode, visual),
            selected: selected.clone(),
        }
    }
    pub fn apply(
        self,
        mode: &mut ShowMode,
        visual: &mut VizSettings,
        selected: &mut Option<String>,
    ) {
        if self.look.apply(mode, visual) {
            *selected = self.selected;
        }
    }
}

#[derive(Default)]
pub struct Editor {
    current: Option<Edit>,
    pending: Option<Edit>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}
impl Editor {
    pub fn observe(&mut self, mode: ShowMode, visual: &VizSettings, selected: &Option<String>) {
        let next = Edit::capture(mode, visual, selected);
        if let Some(current) = &self.current {
            if current.look != next.look && self.pending.is_none() {
                self.pending = Some(current.clone());
            }
        }
        self.current = Some(next);
    }
    pub fn finish_gesture(&mut self) {
        if let Some(before) = self.pending.take() {
            if self.current.as_ref().is_some_and(|s| s.look != before.look) {
                if self.undo.len() == 128 {
                    self.undo.remove(0);
                }
                self.undo.push(before);
                self.redo.clear();
            }
        }
    }
    /// Automatic cycling follows playback without producing undo entries.
    pub fn follow_scene(&mut self, mode: ShowMode) {
        if let Some(current) = &mut self.current {
            current.look.scene = mode.as_str().into();
        }
        if let Some(pending) = &mut self.pending {
            pending.look.scene = mode.as_str().into();
        }
    }
    pub fn undo(&mut self) -> Option<Edit> {
        self.finish_gesture();
        let previous = self.undo.pop()?;
        if let Some(current) = self.current.replace(previous.clone()) {
            self.redo.push(current);
        }
        Some(previous)
    }
    pub fn redo(&mut self) -> Option<Edit> {
        self.finish_gesture();
        let next = self.redo.pop()?;
        if let Some(current) = self.current.replace(next.clone()) {
            self.undo.push(current);
        }
        Some(next)
    }
    pub fn controls(&mut self, ui: &mut egui::Ui) -> Option<Edit> {
        let mut result = None;
        // This row uses a right-to-left layout, with Undo closest to the title.
        if ui
            .add_enabled(
                !self.redo.is_empty() && self.pending.is_none(),
                egui::Button::new("Redo").small(),
            )
            .on_hover_text("Redo visual edit (Ctrl+Y or Ctrl+Shift+Z). Sound is unchanged.")
            .clicked()
        {
            result = self.redo();
        }
        if ui
            .add_enabled(
                !self.undo.is_empty() || self.pending.is_some(),
                egui::Button::new("Undo").small(),
            )
            .on_hover_text("Undo visual edit (Ctrl+Z). Sound is unchanged.")
            .clicked()
        {
            result = self.undo();
        }
        result
    }
}

/// The focused inspector determines the history. Never fall back to sound history
/// when the visual history is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Visual,
    Sound,
}
pub fn shortcut(ctx: &egui::Context, visual_tab: bool, blocked: bool) -> Option<(Target, bool)> {
    if blocked || ctx.wants_keyboard_input() {
        return None;
    }
    ctx.input_mut(|i| {
        let redo = i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
            || i.consume_key(egui::Modifiers::CTRL, egui::Key::Y);
        let undo = if redo {
            false
        } else {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)
        };
        if redo || undo {
            Some((
                if visual_tab {
                    Target::Visual
                } else {
                    Target::Sound
                },
                redo,
            ))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viz_settings::{Effect, EffectSlot};
    #[test]
    fn scene_style_and_stack_drag_undo_together_without_replacing_preferences() {
        let mut editor = Editor::default();
        let mut mode = ShowMode::GlFlame;
        let mut visual = VizSettings::default();
        let mut selected = Some("Initial look".into());
        editor.observe(mode, &visual, &selected);
        let initial = Look::capture("", mode, &visual);
        mode = ShowMode::Particles;
        visual.effects.push(EffectSlot {
            effect: Effect::Scanlines,
            enabled: true,
            mix: 0.7,
        });
        for i in 1..100 {
            visual.speed = i as f32 / 100.0;
            editor.observe(mode, &visual, &None);
        }
        editor.finish_gesture();
        visual.favorites.push("gl_ocean".into());
        visual.stage_view = true;
        visual.shuffle_seconds = 40.0;
        visual
            .saved_looks
            .push(Look::capture("Kept", mode, &visual));
        editor
            .undo()
            .unwrap()
            .apply(&mut mode, &mut visual, &mut selected);
        assert!(initial.matches(mode, &visual));
        assert_eq!(selected.as_deref(), Some("Initial look"));
        assert!(visual.stage_view && visual.favorites == ["gl_ocean"]);
        assert_eq!(visual.shuffle_seconds, 40.0);
        assert_eq!(visual.saved_looks.len(), 1);
        assert!(editor.undo().is_none(), "One drag must produce one edit");
        editor
            .redo()
            .unwrap()
            .apply(&mut mode, &mut visual, &mut selected);
        assert!(mode == ShowMode::Particles && visual.speed == 0.99);
        assert_eq!(visual.effects.len(), 1);
    }
    #[test]
    fn shuffle_and_noop_gestures_do_not_fill_history_or_drop_redo() {
        let mut e = Editor::default();
        let mut v = VizSettings::default();
        e.observe(ShowMode::GlWarp, &v, &None);
        v.brightness = 0.5;
        e.observe(ShowMode::GlWarp, &v, &None);
        v.brightness = 1.0;
        e.observe(ShowMode::GlWarp, &v, &None);
        e.finish_gesture();
        assert!(e.undo().is_none());
        v.brightness = 0.6;
        e.observe(ShowMode::GlWarp, &v, &None);
        e.finish_gesture();
        e.undo()
            .unwrap()
            .apply(&mut ShowMode::GlWarp, &mut v, &mut None);
        e.follow_scene(ShowMode::GlOcean);
        e.observe(ShowMode::GlOcean, &v, &None);
        e.finish_gesture();
        assert!(e.undo().is_none());
        assert!(e.redo().is_some());
        for i in 1..200 {
            v.speed = i as f32 / 100.0;
            e.observe(ShowMode::GlOcean, &v, &None);
            e.finish_gesture();
        }
        assert_eq!(e.undo.len(), 128);
        assert!(e.redo().is_none());
    }
    #[test]
    fn shortcuts_route_by_context_and_are_consumed_once() {
        use egui_kittest::Harness;
        let mut h = Harness::builder().build_state(
            |ctx, s: &mut (bool, bool, Vec<(Target, bool)>)| {
                if let Some(action) = shortcut(ctx, s.0, s.1) {
                    s.2.push(action);
                }
                assert_eq!(shortcut(ctx, s.0, s.1), None);
            },
            (true, false, vec![]),
        );
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
        h.run();
        assert_eq!(h.state().2, [(Target::Visual, false)]);
        h.state_mut().0 = false;
        h.key_press_modifiers(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z);
        h.run();
        assert_eq!(h.state().2[1], (Target::Sound, true));
        h.state_mut().1 = true;
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
        h.run();
        assert_eq!(h.state().2.len(), 2);
    }
    #[test]
    fn focused_text_fields_keep_their_own_undo_shortcut() {
        use egui_kittest::{kittest::Queryable, Harness};
        let mut h = Harness::builder().build_ui_state(
            |ui, s: &mut (String, usize)| {
                if shortcut(ui.ctx(), true, false).is_some() {
                    s.1 += 1;
                }
                ui.text_edit_singleline(&mut s.0);
            },
            (String::from("My look"), 0),
        );
        h.get_by_role(egui::accesskit::Role::TextInput).click();
        h.run();
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
        h.run();
        assert_eq!(h.state().1, 0);
    }
}
