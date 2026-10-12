//! Render the real panel and GL canvas to disposable PNGs without opening the app.
use crate::{curve, glstage, show, theme, viz_panel, viz_settings};
use eframe::{
    egui,
    glow::{self, HasContext},
};
use std::sync::{Arc, Mutex};

pub unsafe fn render(gl: Arc<glow::Context>) -> Result<(), String> {
    let mut scenarios = vec![
        ("stage", 1360, 960, false, true, viz_panel::Page::Scenes),
        ("compact", 1000, 460, false, false, viz_panel::Page::Scenes),
        ("paper", 1360, 960, true, false, viz_panel::Page::Scenes),
        (
            "compact-effects",
            1000,
            460,
            false,
            true,
            viz_panel::Page::Effects,
        ),
        (
            "compact-style",
            1000,
            460,
            false,
            true,
            viz_panel::Page::Style,
        ),
    ];
    scenarios.push((
        "short-scenes",
        1000,
        365,
        false,
        false,
        viz_panel::Page::Scenes,
    ));
    for name in [
        "Neon drift",
        "Solar tide",
        "Phosphor",
        "Night drive",
        "Still water",
    ] {
        scenarios.push((name, 1280, 720, false, true, viz_panel::Page::Style));
    }
    scenarios.push((
        "presentation",
        1280,
        720,
        false,
        true,
        viz_panel::Page::Scenes,
    ));
    scenarios.push(("gallery", 1360, 960, false, true, viz_panel::Page::Scenes));
    scenarios.push(("gallery-compact", 760, 460, false, true, viz_panel::Page::Scenes));
    scenarios.push(("gallery-paper", 1100, 800, true, true, viz_panel::Page::Scenes));
    scenarios.push(("playback", 1360, 960, false, true, viz_panel::Page::Style));
    scenarios.push(("quality-half", 1100, 800, true, false, viz_panel::Page::Style));
    scenarios.push(("eq-editor", 1360, 960, false, false, viz_panel::Page::Scenes));
    scenarios.push(("eq-compact", 1000, 600, false, false, viz_panel::Page::Scenes));
    scenarios.push(("eq-paper", 1100, 800, true, false, viz_panel::Page::Scenes));
    scenarios.push(("recovery", 1100, 800, false, true, viz_panel::Page::Scenes));
    scenarios.push(("recovery-compact", 760, 460, false, true, viz_panel::Page::Scenes));
    scenarios.push(("recovery-empty", 760, 460, true, true, viz_panel::Page::Scenes));
    scenarios.push(("effects-off", 1000, 460, false, true, viz_panel::Page::Effects));
    scenarios.push(("effects-paper", 1100, 700, true, true, viz_panel::Page::Effects));
    for (name, w, h, light, stage_view, page) in scenarios {
        let presenting = name == "presentation";
        let editing = name.starts_with("eq-");
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        theme::set_palette(
            &ctx,
            if light {
                theme::Palette::paper()
            } else {
                theme::Palette::electric_cyan()
            },
        );
        let mut renderer = eframe::egui_glow::Painter::new(gl.clone(), "", None, false)
            .map_err(|e| e.to_string())?;
        let mut browser = viz_panel::Browser::default();
        browser.page = page;
        browser.gallery.open = name.starts_with("gallery");
        browser.playback_expanded = name == "playback" || name == "quality-half";
        let mut visual = viz_settings::VizSettings::from_legacy((1 << 2) | (1 << 6), true);
        visual.stage_view = stage_view;
        if name == "quality-half" { visual.quality = viz_settings::Quality::Low; }
        visual.favorites = vec!["gl_warp".into(), "particles".into(), "gl_ocean".into()];
        let mut mode = show::ShowMode::GlWarp;
        if let Some(look) = crate::viz_looks::factory()
            .into_iter()
            .find(|l| l.name == name)
        {
            look.apply(&mut mode, &mut visual);
        }
        if name == "effects-paper" {
            visual.effects = viz_settings::Effect::ALL.into_iter().map(|effect| viz_settings::EffectSlot {
                effect, enabled: true, mix: 0.35,
            }).collect();
        }
        if name == "effects-off" {
            browser.editor.observe(mode, &visual, &None);
            visual.effects_enabled = false;
            browser.editor.observe(mode, &visual, &None);
            browser.editor.finish_gesture();
        }
        let mut presentation = crate::presentation::Presentation::default();
        if presenting {
            presentation.enter(false, -10.0);
        }
        let mut scene = show::ShowState::new(mode);
        let mut shuffle = false;
        let mut layers = curve::Layers::default();
        let mut bands = std::array::from_fn(|i| tronteq_shared::Band {
            freq: tronteq_shared::DEFAULT_FREQS[i],
            gain: [4., 3., 0., -3., -3., 0., 4., 2.][i],
            q: 1.0,
            kind: 0,
        });
        let mut selected = 3;
        let mut preamp = 0.0;
        let mut sound_editor = crate::sound_edit::Editor::new(crate::sound_edit::Sound {
            bands, preamp, dynamics: tronteq_shared::Dynamics::default_passive(), profile: None,
        });
        let stage = Arc::new(Mutex::new(glstage::GlStage::new(&gl)?));
        let spec: Vec<_> = (0..64)
            .map(|i| (i as f32 * 0.13).sin().abs() * 0.55 + 0.08)
            .collect();
        let wave: Vec<_> = (0..256).map(|i| (i as f32 * 0.08).sin() * 0.3).collect();
        let tex = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        let fbo = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(tex),
            0,
        );
        for frame in 0..20 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(w as f32, h as f32),
                )),
                time: Some(frame as f64 / 60.),
                ..Default::default()
            };
            let output = ctx.run(raw, |ctx| {
                browser.render_stats = Some(stage.lock().unwrap().stats());
                theme::paint_gradient(ctx);
                if !presenting {
                    egui::TopBottomPanel::top("top").show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("TrontEQ")
                                    .size(23.)
                                    .color(theme::cyan())
                                    .strong(),
                            );
                            ui.separator();
                            crate::profile_picker::show(ui, &[
                                crate::profile_picker::Choice { name: "V-shape", active: true, modified: false },
                                crate::profile_picker::Choice { name: "Flat", active: false, modified: false },
                            ], 180.0);
                            ui.separator();
                            ui.selectable_label(stage_view, "Stage")
                                .on_hover_text("Clear visualizer canvas");
                        });
                    });
                    egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
                        ui.label("IN > OUT                                         60 fps");
                    });
                    egui::SidePanel::right("inspector")
                        .exact_width(310.)
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                for name in ["CHAIN", "VIZ", "DATA", "SETUP"] {
                                    let _ = ui.selectable_label(name == if editing { "CHAIN" } else { "VIZ" }, name);
                                }
                            });
                            ui.separator();
                            if editing {
                                sound_editor.controls(ui);
                                ui.horizontal(|ui| { for tab in ["EQ", "Dynamics", "A/V sync"] { let _ = ui.selectable_label(tab == "EQ", tab); } });
                                ui.separator();
                            }
                            if editing {
                                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                    crate::eq_editor::show(ui, &mut bands, &mut selected, &mut preamp);
                                });
                            } else {
                                viz_panel::show(ui, &mut mode, &mut visual, &mut browser, &mut layers, &mut shuffle, true);
                            }
                        });
                }
                let viz = curve::VizData {
                    layers,
                    spectrum: &spec,
                    peaks: &spec,
                    waveform: &wave,
                    stereo: &[],
                    loudness: &[],
                    spectro_tex: None,
                };
                let (fx_order, fx_mix, fx_count) = visual.effect_uniforms();
                let mut uniforms = glstage::Uniforms {
                    fx_order,
                    fx_mix,
                    fx_count,
                    draw_editor: !stage_view && !presenting,
                    draw_overlays: !stage_view && !presenting,
                    rainbow: visual.rainbow,
                    speed: visual.speed,
                    brightness: visual.brightness,
                    render_scale: visual.quality.scale(),
                    transition_seconds: visual.transition_seconds,
                    bass: 0.55,
                    mid: 0.4,
                    treble: 0.3,
                    pulse: 0.5,
                    ..Default::default()
                };
                uniforms.spec.copy_from_slice(&spec);
                uniforms.wave.copy_from_slice(&wave);
                for v in &mut uniforms.spec {
                    *v = (*v * visual.energy).min(1.0);
                }
                for v in &mut uniforms.wave {
                    *v = (*v * visual.energy).clamp(-1.0, 1.0);
                }
                uniforms.bass *= visual.energy;
                uniforms.mid *= visual.energy;
                uniforms.treble *= visual.energy;
                uniforms.pulse *= visual.energy;
                scene.mode = mode;
                let panel = egui::CentralPanel::default();
                let panel = if presenting {
                    panel.frame(egui::Frame::NONE)
                } else {
                    panel
                };
                panel.show(ctx, |ui| {
                    curve::draw(
                        ui,
                        &mut bands,
                        selected,
                        48000.,
                        true,
                        &viz,
                        &mut scene,
                        Some(&stage),
                        uniforms,
                    );
                });
                if presenting { presentation.controls(ctx, mode.label()); }
                viz_panel::gallery(ctx, &mut mode, &mut visual, &mut browser);
                if name.starts_with("recovery") {
                    let problem = crate::state_writer::Problem { version: Some(3), message: "The sound state has an unfinished write (version 3).".into() };
                    let saved = if name == "recovery-empty" { None } else {
                        Some(tronteq_shared::Snapshot { preamp_db: -4.5, delay_ms: 317.5, ..tronteq_shared::Snapshot::flat(true) })
                    };
                    crate::state_recovery::show(ctx, &problem, saved, if saved.is_some() { "Last confirmed sound from this session" } else { "No valid recovery backup is available." });
                }
            });
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            renderer.clear([w as u32, h as u32], [0., 0., 0., 1.]);
            let primitives = ctx.tessellate(output.shapes, 1.0);
            renderer.paint_and_update_textures(
                [w as u32, h as u32],
                1.0,
                &primitives,
                &output.textures_delta,
            );
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        gl.read_pixels(
            0,
            0,
            w,
            h,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
        let mut img = image::RgbaImage::from_raw(w as u32, h as u32, pixels).unwrap();
        image::imageops::flip_vertical_in_place(&mut img);
        for p in img.pixels_mut() {
            p.0[3] = 255;
        }
        let path = format!("target/viz-review-{name}.png");
        img.save(&path).map_err(|e| e.to_string())?;
        println!("PREVIEW {path}");
        renderer.destroy();
        gl.delete_framebuffer(fbo);
        gl.delete_texture(tex);
    }
    Ok(())
}
