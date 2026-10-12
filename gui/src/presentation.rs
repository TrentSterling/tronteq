//! Temporary presentation state. It never changes audio or persisted view preferences.
use eframe::egui;

#[derive(Default)]
pub struct Presentation {
    pub active: bool,
    previous_fullscreen: bool,
    reveal_until: f64,
    last_pointer: Option<egui::Pos2>,
}

impl Presentation {
    pub fn enter(&mut self, fullscreen: bool, time: f64) {
        if self.active {
            return;
        }
        self.active = true;
        self.previous_fullscreen = fullscreen;
        self.reveal_until = time + 3.0;
        self.last_pointer = None;
    }
    pub fn leave(&mut self) -> bool {
        self.active = false;
        self.previous_fullscreen
    }
    pub fn input(&mut self, ctx: &egui::Context) {
        let (escape, f11, fullscreen) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::F11),
                i.viewport().fullscreen.unwrap_or(false),
            )
        });
        if self.active && (escape || f11) {
            let restore = self.leave();
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(if f11 {
                false
            } else {
                restore
            }));
        } else if f11 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
        } else if escape && fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }
    fn reveal(&mut self, time: f64, pointer: Option<egui::Pos2>, key: bool) -> bool {
        if pointer != self.last_pointer || key {
            self.reveal_until = time + 3.0;
        }
        self.last_pointer = pointer;
        time < self.reveal_until
    }
    pub fn controls(&mut self, ctx: &egui::Context, scene: &str) {
        if !self.active {
            return;
        }
        let (time, pointer, key) = ctx.input(|i| {
            (
                i.time,
                i.pointer.hover_pos(),
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Key { pressed: true, .. })),
            )
        });
        if self.reveal(time, pointer, key) {
            egui::Area::new(egui::Id::new("presentation-controls"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-18.0, -18.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(scene);
                            if ui.button("Back to controls (Esc)").clicked() {
                                let restore = self.leave();
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(restore));
                            }
                        });
                    });
                });
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};
    #[test]
    fn leave_restores_the_window_state_from_entry() {
        let mut p = Presentation::default();
        p.enter(false, 0.0);
        p.enter(true, 1.0);
        assert!(!p.leave());
        assert!(!p.active);
        p.enter(true, 2.0);
        assert!(p.leave());
    }
    #[test]
    fn controls_hide_on_idle_and_return_on_input() {
        let mut p = Presentation::default();
        p.enter(false, 0.0);
        assert!(p.reveal(0.0, Some(egui::pos2(1.0, 1.0)), false));
        assert!(!p.reveal(3.1, Some(egui::pos2(1.0, 1.0)), false));
        assert!(p.reveal(4.0, Some(egui::pos2(2.0, 1.0)), false));
        assert!(p.reveal(8.0, None, true));
    }
    #[test]
    fn escape_exits_after_controls_are_hidden() {
        let mut p = Presentation::default();
        p.enter(false, -10.0);
        let mut h = Harness::builder().build_state(
            |ctx, p: &mut Presentation| {
                p.input(ctx);
            },
            p,
        );
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(!h.state().active);
    }
    #[test]
    fn visible_return_button_exits_presentation() {
        let mut p = Presentation::default();
        p.enter(false, 0.0);
        let mut h = Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .build_state(
                |ctx, p: &mut Presentation| {
                    p.controls(ctx, "Neon drift");
                },
                p,
            );
        h.get_by_label("Back to controls (Esc)").click();
        h.run();
        assert!(!h.state().active);
    }
}
