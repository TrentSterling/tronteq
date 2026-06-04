//! The right-side signal-chain panel (preamp + compressor / limiter / auto-loudness
//! knobs). Extracted from `main.rs`. Takes `&mut App` directly: `App` lives in the
//! crate root, so this descendant module can read its fields and call `commit()`.

use eframe::egui;

use crate::{knob, presets, theme, App};

pub fn show(app: &mut App, ctx: &egui::Context) {
    // Mutate Copy locals inside the panel closure, then write back + commit after
    // it closes (avoids borrowing app inside nested egui closures).
    let mut preamp = app.preamp_db;
    let mut d = app.dynamics;
    let mut changed = false;
    egui::SidePanel::right("chain")
        .resizable(false)
        .default_width(264.0)
        .show(ctx, |ui| {
            let comp_acc = theme::cyan();
            let lim_acc = egui::Color32::from_rgb(255, 120, 120);
            let agc_acc = theme::ok();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new("SIGNAL CHAIN").color(theme::cyan()).strong());
                ui.separator();

                // Preamp
                ui.horizontal(|ui| {
                    ui.label("Preamp");
                    if ui.button("↺").on_hover_text("reset preamp").clicked() {
                        preamp = 0.0;
                        changed = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    changed |= knob::knob(ui, &mut preamp, -24.0..=24.0, "gain", " dB", 1, false, theme::cyan());
                });
                ui.separator();

                // Compressor
                let mut comp_on = d.comp_enabled != 0;
                if ui.checkbox(&mut comp_on, "Compressor").changed() {
                    d.comp_enabled = comp_on as u32;
                    changed = true;
                }
                ui.horizontal_wrapped(|ui| {
                    for name in presets::COMP_PRESETS {
                        if ui.button(name).clicked() { presets::apply_comp(&mut d, name); changed = true; }
                    }
                    if ui.button("↺").on_hover_text("reset compressor").clicked() { presets::reset_comp(&mut d); changed = true; }
                });
                comp_on = d.comp_enabled != 0;
                ui.add_enabled_ui(comp_on, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        changed |= knob::knob(ui, &mut d.comp_threshold_db, -60.0..=0.0, "thresh", " dB", 0, false, comp_acc);
                        changed |= knob::knob(ui, &mut d.comp_ratio, 1.0..=20.0, "ratio", ":1", 1, false, comp_acc);
                        changed |= knob::knob(ui, &mut d.comp_attack_ms, 0.1..=200.0, "attack", " ms", 1, true, comp_acc);
                        changed |= knob::knob(ui, &mut d.comp_release_ms, 10.0..=1000.0, "release", " ms", 0, true, comp_acc);
                        changed |= knob::knob(ui, &mut d.comp_makeup_db, 0.0..=24.0, "makeup", " dB", 1, false, comp_acc);
                    });
                });
                ui.separator();

                // Limiter
                let mut lim_on = d.limiter_enabled != 0;
                if ui.checkbox(&mut lim_on, "Limiter").changed() {
                    d.limiter_enabled = lim_on as u32;
                    changed = true;
                }
                ui.horizontal_wrapped(|ui| {
                    for name in presets::LIMITER_PRESETS {
                        if ui.button(name).clicked() { presets::apply_limiter(&mut d, name); changed = true; }
                    }
                    if ui.button("↺").on_hover_text("reset limiter").clicked() { presets::reset_limiter(&mut d); changed = true; }
                });
                lim_on = d.limiter_enabled != 0;
                ui.add_enabled_ui(lim_on, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        changed |= knob::knob(ui, &mut d.limiter_ceiling_db, -12.0..=0.0, "ceiling", " dB", 1, false, lim_acc);
                    });
                });
                ui.separator();

                // Auto-loudness
                let mut agc_on = d.agc_enabled != 0;
                if ui.checkbox(&mut agc_on, "Auto-loudness").changed() {
                    d.agc_enabled = agc_on as u32;
                    changed = true;
                }
                ui.horizontal_wrapped(|ui| {
                    for name in presets::AGC_PRESETS {
                        if ui.button(name).clicked() { presets::apply_agc(&mut d, name); changed = true; }
                    }
                    if ui.button("↺").on_hover_text("reset auto-loudness").clicked() { presets::reset_agc(&mut d); changed = true; }
                });
                agc_on = d.agc_enabled != 0;
                ui.add_enabled_ui(agc_on, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        changed |= knob::knob(ui, &mut d.agc_target_db, -30.0..=-6.0, "target", " dB", 0, false, agc_acc);
                        changed |= knob::knob(ui, &mut d.agc_max_gain_db, 0.0..=36.0, "max gain", " dB", 0, false, agc_acc);
                    });
                });
                ui.add_space(8.0);
            });
        });
    if changed {
        app.preamp_db = preamp;
        app.dynamics = d;
        app.commit();
    }
}
