//! Tabbed right-side inspector: CHAIN (signal chain), VIZ (analyze layers),
//! SETUP (output device, zoom, resets, readouts). Takes `&mut App` — App lives
//! in the crate root, so this module reads its fields and calls its methods
//! directly. Interactive state is mutated through Copy locals inside the panel
//! closure and written back after it closes (the proven main.rs pattern).

use eframe::egui;
use tronteq_shared::Dynamics;

use crate::{curve, devices, knob, presets, profiles, theme, App};

/// Which inspector tab is open. Persisted in settings.json by name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Chain,
    Viz,
    Setup,
}

impl Tab {
    pub fn from_str(s: &str) -> Tab {
        match s {
            "viz" => Tab::Viz,
            "setup" => Tab::Setup,
            _ => Tab::Chain,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Tab::Chain => "chain",
            Tab::Viz => "viz",
            Tab::Setup => "setup",
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    // Copy locals mutated inside the closure, written back after it closes
    // (avoids borrowing app inside nested egui closures).
    let mut tab = app.tab;
    let mut preamp = app.preamp_db;
    let mut d = app.dynamics;
    let mut chain_changed = false;
    let mut layers = app.layers;
    let mut rainbow = app.rainbow;
    let mut sel_device = app.selected_device;
    let mut do_apply = false;
    let mut do_refresh = false;
    let mut do_reset_all = false;

    egui::SidePanel::right("chain")
        .resizable(false)
        .default_width(264.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut tab, Tab::Chain, "CHAIN");
                ui.selectable_value(&mut tab, Tab::Viz, "VIZ");
                ui.selectable_value(&mut tab, Tab::Setup, "SETUP");
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                match tab {
                    Tab::Chain => chain_tab(ui, &mut preamp, &mut d, &mut chain_changed),
                    Tab::Viz => viz_tab(ui, &mut layers, &mut rainbow),
                    Tab::Setup => setup_tab(
                        ui,
                        ctx,
                        app,
                        &mut sel_device,
                        &mut do_apply,
                        &mut do_refresh,
                        &mut do_reset_all,
                    ),
                }
                ui.add_space(8.0);
            });
        });

    app.tab = tab;
    app.layers = layers;
    app.rainbow = rainbow;
    app.selected_device = sel_device;
    if do_refresh {
        app.devices = devices::list().unwrap_or_default();
        app.selected_device = app.selected_device.min(app.devices.len().saturating_sub(1));
    }
    if do_apply {
        app.start_apply();
    }
    if do_reset_all {
        app.reset_all();
        app.active_profile = None;
    }
    if chain_changed {
        app.preamp_db = preamp;
        app.dynamics = d;
        app.commit();
    }
}

/// The signal chain: preamp + compressor / limiter / auto-loudness knobs and
/// their per-component presets. Content unchanged from the pre-tab panel.
fn chain_tab(ui: &mut egui::Ui, preamp: &mut f32, d: &mut Dynamics, changed: &mut bool) {
    let comp_acc = theme::cyan();
    let lim_acc = egui::Color32::from_rgb(255, 120, 120);
    let agc_acc = theme::ok();

    ui.label(egui::RichText::new("SIGNAL CHAIN").color(theme::cyan()).strong());
    ui.separator();

    // Preamp
    ui.horizontal(|ui| {
        ui.label("Preamp");
        if ui.button("↺").on_hover_text("reset preamp").clicked() {
            *preamp = 0.0;
            *changed = true;
        }
    });
    ui.horizontal_wrapped(|ui| {
        *changed |= knob::knob(ui, preamp, -24.0..=24.0, "gain", " dB", 1, false, theme::cyan());
    });
    ui.separator();

    // Compressor
    let mut comp_on = d.comp_enabled != 0;
    if ui.checkbox(&mut comp_on, "Compressor").changed() {
        d.comp_enabled = comp_on as u32;
        *changed = true;
    }
    ui.horizontal_wrapped(|ui| {
        for name in presets::COMP_PRESETS {
            if ui.button(name).clicked() {
                presets::apply_comp(d, name);
                *changed = true;
            }
        }
        if ui.button("↺").on_hover_text("reset compressor").clicked() {
            presets::reset_comp(d);
            *changed = true;
        }
    });
    let comp_on = d.comp_enabled != 0;
    ui.add_enabled_ui(comp_on, |ui| {
        ui.horizontal_wrapped(|ui| {
            *changed |= knob::knob(ui, &mut d.comp_threshold_db, -60.0..=0.0, "thresh", " dB", 0, false, comp_acc);
            *changed |= knob::knob(ui, &mut d.comp_ratio, 1.0..=20.0, "ratio", ":1", 1, false, comp_acc);
            *changed |= knob::knob(ui, &mut d.comp_attack_ms, 0.1..=200.0, "attack", " ms", 1, true, comp_acc);
            *changed |= knob::knob(ui, &mut d.comp_release_ms, 10.0..=1000.0, "release", " ms", 0, true, comp_acc);
            *changed |= knob::knob(ui, &mut d.comp_makeup_db, 0.0..=24.0, "makeup", " dB", 1, false, comp_acc);
        });
    });
    ui.separator();

    // Limiter
    let mut lim_on = d.limiter_enabled != 0;
    if ui.checkbox(&mut lim_on, "Limiter").changed() {
        d.limiter_enabled = lim_on as u32;
        *changed = true;
    }
    ui.horizontal_wrapped(|ui| {
        for name in presets::LIMITER_PRESETS {
            if ui.button(name).clicked() {
                presets::apply_limiter(d, name);
                *changed = true;
            }
        }
        if ui.button("↺").on_hover_text("reset limiter").clicked() {
            presets::reset_limiter(d);
            *changed = true;
        }
    });
    let lim_on = d.limiter_enabled != 0;
    ui.add_enabled_ui(lim_on, |ui| {
        ui.horizontal_wrapped(|ui| {
            *changed |= knob::knob(ui, &mut d.limiter_ceiling_db, -12.0..=0.0, "ceiling", " dB", 1, false, lim_acc);
        });
    });
    ui.separator();

    // Auto-loudness
    let mut agc_on = d.agc_enabled != 0;
    if ui.checkbox(&mut agc_on, "Auto-loudness").changed() {
        d.agc_enabled = agc_on as u32;
        *changed = true;
    }
    ui.horizontal_wrapped(|ui| {
        for name in presets::AGC_PRESETS {
            if ui.button(name).clicked() {
                presets::apply_agc(d, name);
                *changed = true;
            }
        }
        if ui.button("↺").on_hover_text("reset auto-loudness").clicked() {
            presets::reset_agc(d);
            *changed = true;
        }
    });
    let agc_on = d.agc_enabled != 0;
    ui.add_enabled_ui(agc_on, |ui| {
        ui.horizontal_wrapped(|ui| {
            *changed |= knob::knob(ui, &mut d.agc_target_db, -30.0..=-6.0, "target", " dB", 0, false, agc_acc);
            *changed |= knob::knob(ui, &mut d.agc_max_gain_db, 0.0..=36.0, "max gain", " dB", 0, false, agc_acc);
        });
    });
}

/// ANALYZE = the data layers stacked behind the EQ curve. SHOW (full-canvas
/// eye-candy modes) slots in under its own header next session.
fn viz_tab(ui: &mut egui::Ui, layers: &mut curve::Layers, rainbow: &mut bool) {
    ui.label(egui::RichText::new("ANALYZE").color(theme::cyan()).strong());
    ui.checkbox(&mut layers.spectrum, "Spectrum bars");
    ui.checkbox(&mut layers.peak_hold, "Peak-hold caps");
    ui.checkbox(&mut layers.analyzer, "Analyzer line");
    ui.checkbox(&mut layers.waterfall, "Spectrogram waterfall");
    ui.checkbox(&mut layers.waveform, "Waveform");
    ui.checkbox(&mut layers.goniometer, "Stereo goniometer");
    ui.checkbox(&mut layers.loudness, "Loudness history");
    ui.separator();
    ui.checkbox(rainbow, "Rainbow mode");
    ui.separator();
    ui.label(egui::RichText::new("SHOW").color(theme::cyan()).strong());
    ui.label(
        egui::RichText::new(
            "Full-canvas eye-candy modes land here next: bars XL, scope trails, spectrum tunnel. Winamp energy.",
        )
        .color(theme::muted())
        .small(),
    );
}

/// Output device install flow + UI prefs + maintenance + status readouts.
#[allow(clippy::too_many_arguments)]
fn setup_tab(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    app: &App,
    sel_device: &mut usize,
    do_apply: &mut bool,
    do_refresh: &mut bool,
    do_reset_all: &mut bool,
) {
    let busy = app.apply_rx.is_some();

    ui.label(egui::RichText::new("OUTPUT").color(theme::cyan()).strong());
    let selected = app
        .devices
        .get(*sel_device)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "-".to_string());
    egui::ComboBox::from_id_salt("device_picker")
        .selected_text(selected)
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            for (i, dev) in app.devices.iter().enumerate() {
                let label = if dev.is_default {
                    format!("{}  (default)", dev.name)
                } else {
                    dev.name.clone()
                };
                ui.selectable_value(sel_device, i, label);
            }
        });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!busy, egui::Button::new("Apply EQ here"))
            .on_hover_text("Install the APO onto this output (elevated)")
            .clicked()
        {
            *do_apply = true;
        }
        if ui
            .add_enabled(!busy, egui::Button::new("⟳"))
            .on_hover_text("Refresh device list")
            .clicked()
        {
            *do_refresh = true;
        }
        if busy {
            ui.spinner();
        }
    });
    if let Some((ok, msg)) = &app.device_status {
        let color = if *ok {
            egui::Color32::from_rgb(140, 220, 140)
        } else {
            egui::Color32::LIGHT_RED
        };
        ui.colored_label(color, msg);
    }
    ui.separator();

    ui.label(egui::RichText::new("UI").color(theme::cyan()).strong());
    ui.horizontal(|ui| {
        ui.label("Zoom");
        let zoom = ctx.zoom_factor();
        if ui.button("-").on_hover_text("Zoom out").clicked() {
            ctx.set_zoom_factor((zoom - 0.1).max(0.5));
        }
        if ui
            .button(format!("{}%", (zoom * 100.0).round() as i32))
            .on_hover_text("Reset zoom to 100%")
            .clicked()
        {
            ctx.set_zoom_factor(1.0);
        }
        if ui.button("+").on_hover_text("Zoom in").clicked() {
            ctx.set_zoom_factor((zoom + 0.1).min(2.0));
        }
    });
    ui.separator();

    ui.label(egui::RichText::new("MAINTENANCE").color(theme::cyan()).strong());
    if ui
        .button("Reset whole chain")
        .on_hover_text("Flat EQ + 0 preamp + passive dynamics")
        .clicked()
    {
        *do_reset_all = true;
    }
    if ui
        .button("Open profiles folder")
        .on_hover_text(profiles::PROFILE_DIR)
        .clicked()
    {
        let _ = std::process::Command::new("explorer").arg(profiles::PROFILE_DIR).spawn();
    }
    ui.separator();

    ui.label(egui::RichText::new("STATUS").color(theme::cyan()).strong());
    ui.label(format!("{} Hz  ·  state v{}", app.sample_rate as u32, app.state.version()));
}
