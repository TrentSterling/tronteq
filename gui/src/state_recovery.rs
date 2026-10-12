//! Recovery is an explicit choice, separate from the live sound editor.
use crate::state_writer::{Problem, RecoveryAction};
use eframe::egui;
use tronteq_shared::Snapshot;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decision {
    Apply(RecoveryAction),
    Later,
}

pub fn show(
    ctx: &egui::Context,
    problem: &Problem,
    saved: Option<Snapshot>,
    saved_label: &str,
) -> Option<Decision> {
    let mut decision = None;
    let response = egui::Modal::new(egui::Id::new("sound-recovery")).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 64.0).clamp(260.0, 480.0));
        ui.heading("Sound settings need recovery");
        egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 180.0).max(110.0)).show(ui, |ui| {
            ui.colored_label(crate::theme::readable_ink(egui::Color32::from_rgb(255, 170, 90)), &problem.message);
            ui.label("EQ editing is paused. Reload the current state or explicitly restore a confirmed sound. The visualizer can stay open.");
            ui.separator();
            ui.strong(saved_label);
            if let Some(s) = saved {
                ui.label(format!("Preamp {:+.1} dB · Delay {:.1} ms · Bypass {}", s.preamp_db, s.delay_ms, if s.is_bypassed() { "ON" } else { "OFF" }));
                ui.small(format!("Compressor {} · Limiter {} · Auto-level {}", if s.dynamics.comp_enabled != 0 { "ON" } else { "OFF" }, if s.dynamics.limiter_enabled != 0 { "ON" } else { "OFF" }, if s.dynamics.agc_enabled != 0 { "ON" } else { "OFF" }));
                ui.collapsing("Saved EQ bands", |ui| {
                    for (i, b) in s.bands.iter().enumerate() {
                        ui.small(format!("{}: {:.1} Hz · {:+.1} dB · Q {:.2} · {}", i + 1, b.freq, b.gain, b.q, tronteq_shared::BandKind::from_u32(b.kind).short_label()));
                    }
                });
            }
            if ui.add_enabled(saved.is_some() && problem.version.is_some(), egui::Button::new("Restore confirmed sound")).clicked() {
                decision = Some(Decision::Apply(RecoveryAction::Restore));
            }
            ui.collapsing("Start from flat", |ui| {
                ui.label("Resets EQ, preamp and dynamics, clears A/V delay, and turns Bypass ON. This replaces the interrupted sound.");
                if ui.add_enabled(problem.version.is_some(), egui::Button::new("Reset to flat, bypassed")).clicked() {
                    decision = Some(Decision::Apply(RecoveryAction::ResetBypassed));
                }
            });
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Reload current state").clicked() { decision = Some(Decision::Apply(RecoveryAction::Reload)); }
            if ui.button("Keep visualizer open").clicked() { decision = Some(Decision::Later); }
        });
    });
    if decision.is_none() && response.should_close() {
        decision = Some(Decision::Later);
    }
    decision
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};
    #[test]
    fn restore_is_explicit_and_unavailable_without_a_confirmed_sound() {
        let p = Problem {
            version: Some(3),
            message: "An unfinished write".into(),
        };
        let mut h = Harness::builder()
            .with_size(egui::vec2(800.0, 500.0))
            .build_state(
                |ctx, s| {
                    if let Some(d) = show(ctx, &p, s.0, "Saved a minute ago") {
                        s.1 = Some(d);
                    }
                },
                (Some(Snapshot::flat(true)), None),
            );
        assert_eq!(h.state().1, None);
        h.get_by_label("Restore confirmed sound").click();
        h.run();
        assert_eq!(h.state().1, Some(Decision::Apply(RecoveryAction::Restore)));
        h.state_mut().0 = None;
        h.state_mut().1 = None;
        h.run();
        h.get_by_label("Restore confirmed sound").click();
        h.run();
        assert_eq!(h.state().1, None);
        h.get_by_label("Keep visualizer open").click();
        h.run();
        assert_eq!(h.state().1, Some(Decision::Later));
    }
}
