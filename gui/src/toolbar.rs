//! The top toolbar (wordmark, EQ presets, bypass, viz toggles, theme + zoom) and
//! the output device-picker row. Extracted from `main.rs`; takes `&mut App`.

use eframe::egui;

use crate::{devices, presets, theme, App};

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top").show(ctx, |ui| {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let title_color = if app.rainbow {
                theme::hsv((ui.input(|i| i.time) as f32 * 0.2).fract(), 0.9, 1.0)
            } else {
                theme::cyan()
            };
            ui.label(
                egui::RichText::new("TrontEQ")
                    .family(egui::FontFamily::Name("display".into()))
                    .color(title_color)
                    .strong()
                    .size(23.0),
            );
            ui.separator();
            if ui.button("Flat").clicked() {
                app.reset_flat();
            }
            if ui
                .button("Reset")
                .on_hover_text("Reset the whole chain (EQ + preamp + dynamics) to defaults")
                .clicked()
            {
                app.reset_all();
            }
            for name in presets::EQ_PRESETS {
                if ui.button(name).clicked() {
                    presets::apply_eq(&mut app.bands, name);
                    app.commit();
                }
            }
            let mut bypass = app.bypass;
            if ui.checkbox(&mut bypass, "Bypass").changed() {
                app.bypass = bypass;
                app.commit();
            }
            ui.checkbox(&mut app.rainbow, "Rainbow");
            ui.label(egui::RichText::new("Viz").color(theme::muted()));
            let g = &mut app.layers;
            ui.toggle_value(&mut g.spectrum, "Spec").on_hover_text("Spectrum bars");
            ui.toggle_value(&mut g.peak_hold, "Peak").on_hover_text("Peak-hold caps");
            ui.toggle_value(&mut g.analyzer, "Line").on_hover_text("Analyzer line");
            ui.toggle_value(&mut g.waterfall, "Fall").on_hover_text("Spectrogram waterfall");
            ui.toggle_value(&mut g.waveform, "Wave").on_hover_text("Waveform");
            ui.toggle_value(&mut g.goniometer, "Gonio").on_hover_text("Stereo goniometer");
            ui.toggle_value(&mut g.loudness, "Loud").on_hover_text("Loudness history");
            if ui
                .button(if theme::dark_mode() { "Light" } else { "Dark" })
                .on_hover_text("Toggle light / dark theme")
                .clicked()
            {
                theme::set_mode(ctx, !theme::dark_mode());
            }
            ui.separator();
            ui.label(format!("{} Hz", app.sample_rate as u32));
            ui.label(format!("v{}", app.state.version()));
            if ui.button("About").clicked() {
                app.show_about = true;
            }
            ui.separator();
            // UI zoom: -, current %, + (middle resets to 100%).
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

        // Output device picker: pick where the EQ runs and apply it there.
        ui.horizontal(|ui| {
            let busy = app.apply_rx.is_some();
            ui.label("Output:");
            let selected = app
                .devices
                .get(app.selected_device)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "—".to_string());
            egui::ComboBox::from_id_salt("device_picker")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (i, d) in app.devices.iter().enumerate() {
                        let label = if d.is_default {
                            format!("{}  (default)", d.name)
                        } else {
                            d.name.clone()
                        };
                        ui.selectable_value(&mut app.selected_device, i, label);
                    }
                });
            if ui
                .add_enabled(!busy, egui::Button::new("Apply EQ here"))
                .on_hover_text("Install the APO onto this output (elevated)")
                .clicked()
            {
                app.start_apply();
            }
            if ui.add_enabled(!busy, egui::Button::new("⟳")).on_hover_text("Refresh device list").clicked() {
                app.devices = devices::list().unwrap_or_default();
                app.selected_device =
                    app.selected_device.min(app.devices.len().saturating_sub(1));
            }
            if busy {
                ui.spinner();
            }
            if let Some((ok, msg)) = &app.device_status {
                let color = if *ok {
                    egui::Color32::from_rgb(140, 220, 140)
                } else {
                    egui::Color32::LIGHT_RED
                };
                ui.colored_label(color, msg);
            }
        });
    });
}
