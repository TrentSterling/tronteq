//! One browser, one set of controls, one ordered effect stack for every scene.
use crate::{
    curve::Layers,
    show::ShowMode,
    theme,
    viz_settings::{Effect, EffectSlot, VizSettings},
};
use eframe::egui;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    #[default]
    Scenes,
    Style,
    Effects,
    Meters,
    Audition,
    Perform,
    Signals,
    Patch,
    Xy,
    Record,
}

#[derive(Default)]
pub struct Browser {
    pub patch:crate::patch_bay::Panel,
    pub recorder:crate::viz_record::Recorder,
    pub performance: crate::viz_performance::Runtime,
    pub motion: crate::viz_motion::Runtime,
    pub motion_epoch: u64,
    pub audition: crate::viz_audition::Audition,
    pub search: String,
    pub category: String,
    pub favorites_only: bool,
    pub page: Page,
    pub look_name: String,
    pub naming_look: bool,
    pub selected_look: Option<String>,
    pub removed_look: Option<crate::viz_looks::Look>,
    pub present_requested: bool,
    pub gallery: crate::scene_gallery::Gallery,
    pub render_stats: Option<crate::glstage::RenderStats>,
    pub playback_expanded: bool,
    pub revealed_scene: Option<ShowMode>,
    pub editor: crate::viz_edit::Editor,
}

pub fn show(
    ui: &mut egui::Ui,
    mode: &mut ShowMode,
    config: &mut VizSettings,
    browser: &mut Browser,
    layers: &mut Layers,
    shuffle: &mut bool,
    available: bool,
) {
    browser
        .editor
        .observe(*mode, config, &browser.selected_look);
    let row_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.horizontal(|ui| {
        let title_width = (ui.available_width() - 94.0).max(80.0);
        ui.add_sized(
            [title_width, 26.0],
            egui::Label::new(egui::RichText::new(mode.label()).size(21.0).strong()).truncate(),
        )
        .on_hover_text(format!("{}: {}", mode.category(), mode.description()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if let Some(edit) = browser.editor.controls(ui) {
                edit.apply(mode, config, &mut browser.selected_look);
            }
        });
    });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut config.stage_view, false, "EQ view");
        ui.selectable_value(&mut config.stage_view, true, "Stage view")
            .on_hover_text("A clear stage for the visuals. EQ continues processing.");
        if ui
            .add_enabled(
                *mode != ShowMode::Off && available,
                egui::Button::new("Present").small(),
            )
            .on_hover_text("Fullscreen artwork (F11). Controls hide when idle; Escape returns here.")
            .clicked()
        {
            browser.present_requested = true;
        }
    });
    if !available {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            "Visual renderer unavailable. Restart TrontEQ to retry.",
        );
    }
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().button_padding.x = 4.0;
        ui.spacing_mut().item_spacing.x = 2.0;
        for (page, label) in [
            (Page::Scenes, "Scenes"),
            (Page::Style, "Style"),
            (Page::Effects, "Effects"),
            (Page::Meters, "Meters"),
            (Page::Perform, "Perform"),
        ] {
            ui.selectable_value(&mut browser.page, page, label);
        }
        ui.menu_button(match browser.page { Page::Audition => "Audition", Page::Signals => "Signals", Page::Patch=>"Patch bay", Page::Xy=>"XY pad",Page::Record=>"Record",_ => "More" },|ui| {
            if ui.selectable_label(browser.page==Page::Audition,"Audition").clicked(){browser.page=Page::Audition;ui.close();}
            if ui.selectable_label(browser.page==Page::Signals,"Signals").clicked(){browser.page=Page::Signals;ui.close();}
            for (page,label) in [(Page::Patch,"Patch bay"),(Page::Xy,"XY pad"),(Page::Record,"Record")]{if ui.selectable_label(browser.page==page,label).clicked(){browser.page=page;ui.close();}}
        });
    });
    ui.spacing_mut().item_spacing.y = row_spacing;
    ui.add_space(2.0);
    if browser.page == Page::Scenes {
        contents(ui, mode, config, browser, layers, shuffle);
    } else {
        // Keep scene identity, view controls, and navigation visible while editing.
        let reserve = if browser.page == Page::Meters {
            28.0
        } else {
            0.0
        };
        egui::ScrollArea::vertical()
            .id_salt(("visual-controls", browser.page as u8))
            .max_height((ui.available_height() - reserve).max(28.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                contents(ui, mode, config, browser, layers, shuffle)
            });
    }
    browser
        .editor
        .observe(*mode, config, &browser.selected_look);
    if !ui.input(|i| i.pointer.primary_down()) {
        browser.editor.finish_gesture();
    }
}

fn contents(
    ui: &mut egui::Ui,
    mode: &mut ShowMode,
    config: &mut VizSettings,
    browser: &mut Browser,
    layers: &mut Layers,
    shuffle: &mut bool,
) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
    if browser.page == Page::Patch {
        browser.patch.show(ui,*mode,config,&browser.motion);
    } else if browser.page == Page::Xy {
        crate::viz_xy::panel(ui,*mode,config,&mut browser.motion.xy);
    } else if browser.page == Page::Record {
        browser.recorder.panel(ui);
    } else if browser.page == Page::Perform {
        ui.horizontal_wrapped(|ui|{for (page,label) in [(Page::Patch,"Patch bay"),(Page::Xy,"XY pad"),(Page::Record,"Record")]{if ui.small_button(label).clicked(){browser.page=page;}}});
        crate::performance_panel::show(ui,mode,config,&mut browser.performance,&mut browser.motion,shuffle);
    } else if browser.page == Page::Signals {
        crate::performance_panel::signals(ui,config,&mut browser.motion);
    } else if browser.page == Page::Audition {
        crate::viz_audition::panel(ui, mode, config, &mut browser.audition, &mut browser.selected_look, shuffle);
    } else if browser.page == Page::Scenes {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 24.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.selectable_value(mode, ShowMode::Off, "Off");
                if ui.small_button("Gallery").clicked() {
                    browser.gallery.open = true;
                }
                if ui.small_button("Next").clicked() {
                    let pool = config.scene_pool();
                    let idx = pool
                        .iter()
                        .position(|m| m == mode)
                        .map_or(0, |i| (i + 1) % pool.len());
                    *mode = pool[idx];
                }
                ui.add_sized(
                    [ui.available_width().max(60.0), 24.0],
                    egui::TextEdit::singleline(&mut browser.search).hint_text("Search scenes..."),
                );
            },
        );
        ui.horizontal(|ui| {
            crate::scene_gallery::filters(ui, &mut browser.category, &mut browser.favorites_only);
        });
        let query = browser.search.trim().to_lowercase();
        let library_height = (ui.clip_rect().bottom() - ui.cursor().top() - 45.0).max(28.0);
        let reveal = browser.revealed_scene != Some(*mode);
        egui::ScrollArea::vertical()
            .id_salt("scene-library")
            .max_height(library_height)
            .min_scrolled_height(0.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let scenes: Vec<_> = ShowMode::ALL
                    .into_iter()
                    .skip(1)
                    .filter(|m| {
                        crate::scene_gallery::filtered(
                            *m,
                            &query,
                            &browser.category,
                            browser.favorites_only,
                            config,
                        )
                    })
                    .collect();
                if scenes.is_empty() {
                    ui.label(egui::RichText::new("No matching scenes.").color(theme::muted()));
                    if ui.small_button("Clear filters").clicked() {
                        browser.search.clear();
                        browser.category.clear();
                        browser.favorites_only = false;
                    }
                } else {
                    browser
                        .gallery
                        .grid(ui, &scenes, mode, config, 120.0, reveal);
                }
            });
        browser.revealed_scene = Some(*mode);
        ui.horizontal(|ui| {
            ui.checkbox(shuffle, "Shuffle");
            ui.add(
                egui::DragValue::new(&mut config.shuffle_seconds)
                    .range(5.0..=120.0)
                    .suffix(" s")
                    .speed(1.0),
            );
            ui.add_enabled(
                !config.favorites.is_empty(),
                egui::Checkbox::new(&mut config.shuffle_favorites, "Favorites only"),
            );
        });
    } else if browser.page == Page::Style {
        looks(ui, mode, config, browser);
        ui.separator();
        ui.add(egui::Slider::new(&mut config.motion.intensity, 0.0..=1.0).text("Intensity").custom_formatter(|v, _| format!("{:.0}%", v*100.0)));
        ui.small("Use React to... under a slider to map audio or Intensity.");
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("FEEL").strong());
            if ui.small_button("Reset feel").clicked() {
                config.speed = 1.0;
                config.energy = 1.0;
                config.brightness = 1.0;
            }
        });
        ui.add(
            egui::Slider::new(&mut config.speed, 0.0..=2.0)
                .text("Speed")
                .suffix("x"),
        );
        crate::viz_motion::binding(ui, "speed", config.speed, &mut config.motion, &browser.motion);
        ui.add(
            egui::Slider::new(&mut config.energy, 0.0..=2.0)
                .text("Audio response")
                .suffix("x"),
        );
        crate::viz_motion::binding(ui, "energy", config.energy, &mut config.motion, &browser.motion);
        ui.add(egui::Slider::new(&mut config.brightness, 0.1..=1.5).text("Brightness"));
        crate::viz_motion::binding(ui, "brightness", config.brightness, &mut config.motion, &browser.motion);
        ui.horizontal(|ui| {
            ui.label("Color");
            ui.selectable_value(&mut config.rainbow, true, "Spectrum");
            ui.selectable_value(&mut config.rainbow, false, "Theme");
        });
        egui::CollapsingHeader::new("Palette").show(ui, |ui| {
            let c = theme::viz_accent();
            let mut accent = config.accent.unwrap_or([c.r() as f32/255.0,c.g() as f32/255.0,c.b() as f32/255.0]);
            ui.horizontal(|ui| {
                ui.label("Art color");
                if ui.color_edit_button_rgb(&mut accent).changed() { config.accent = Some(accent); }
                if ui.small_button("Use theme").clicked() { config.accent = None; }
            });
            ui.add(egui::Slider::new(&mut config.hue, 0.0..=1.0).text("Color shift"));
            crate::viz_motion::binding(ui, "hue", config.hue, &mut config.motion, &browser.motion);
            ui.add(egui::Slider::new(&mut config.saturation, 0.0..=2.0).text("Saturation"));
            crate::viz_motion::binding(ui, "saturation", config.saturation, &mut config.motion, &browser.motion);
        });
        if *mode == ShowMode::GlFlame || *mode == ShowMode::StereoScope {
            let variants: &[&str] = if *mode == ShowMode::GlFlame { &["Original", "Molten metal", "Electric ink", "Solar surface", "Bioluminescence"] } else { &["Meter", "Stereo sculpture"] };
            let mut variant = config.scene_variants.get(mode.as_str()).copied().unwrap_or(0).min(variants.len() as u32 - 1);
            egui::ComboBox::from_id_salt("scene-variation").selected_text(variants[variant as usize]).show_ui(ui, |ui| {
                for (i, label) in variants.iter().enumerate() { ui.selectable_value(&mut variant, i as u32, *label); }
            });
            if variant != 0 { config.scene_variants.insert(mode.as_str().into(), variant); } else { config.scene_variants.remove(mode.as_str()); }
        }
        if let Some(labels) = scene_control_labels(*mode) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{} controls", mode.label())).strong());
                if ui.small_button("Reset scene").clicked() {
                    config.scene_controls.remove(mode.as_str());
                }
            });
            let mut values = config.scene_controls.get(mode.as_str()).copied().unwrap_or([1.0; 4]);
            let mut changed = false;
            for (i, (value, label)) in values.iter_mut().zip(labels).enumerate() {
                changed |= ui.add(egui::Slider::new(value, 0.0..=2.0).text(label)).changed();
                crate::viz_motion::binding(ui, &crate::viz_motion::scene_key(mode.as_str(), i), *value, &mut config.motion, &browser.motion);
            }
            if changed { config.scene_controls.insert(mode.as_str().into(), values); }
        }
        if *mode == ShowMode::GlFlame || *mode == ShowMode::StereoScope {
            egui::CollapsingHeader::new("Sculpt the scene").show(ui, |ui| {
                let labels = if *mode == ShowMode::GlFlame { ["Curl scale", "Filament width", "Color spread", "Source height"] } else { ["Ribbon depth", "Orbit", "Ribbon glow", "Twist"] };
                let mut values = config.scene_details.get(mode.as_str()).copied().unwrap_or([1.0; 4]);
                let mut changed = false;
                for (i, (value, label)) in values.iter_mut().zip(labels).enumerate() {
                    changed |= ui.add(egui::Slider::new(value, 0.0..=2.0).text(label)).changed();
                    crate::viz_motion::binding(ui, &crate::viz_motion::detail_key(mode.as_str(), i), *value, &mut config.motion, &browser.motion);
                }
                if changed { config.scene_details.insert(mode.as_str().into(), values); }
            });
        }
        egui::CollapsingHeader::new("Audio drivers").show(ui, |ui| {
            if ui.small_button("Reset drivers").clicked() { config.signal_gains = [1.0; 6]; }
            for (gain, label) in config.signal_gains.iter_mut().zip([
                "Bass", "Mids", "Treble", "Beat punch", "Spectral change", "Stereo width",
            ]) {
                ui.add(egui::Slider::new(gain, 0.0..=2.0).text(label).suffix("x"));
            }
            ui.label(egui::RichText::new("Shape the scene and its effects independently of your sound.").small().color(theme::muted()));
        });
        let playback = egui::CollapsingHeader::new("Playback & quality")
            .open(Some(browser.playback_expanded)).show(ui, |ui| {
            ui.add(egui::Slider::new(&mut config.transition_seconds, 0.0..=2.0)
                .text("Scene fade").suffix(" s"))
                .on_hover_text("Crossfade between scenes. Zero switches instantly. Audio is unchanged.");
            ui.horizontal(|ui| {
                ui.label("Resolution");
                for (quality, label) in [
                    (crate::viz_settings::Quality::Full, "100%"),
                    (crate::viz_settings::Quality::Balanced, "75%"),
                    (crate::viz_settings::Quality::Low, "50%"),
                ] { ui.selectable_value(&mut config.quality, quality, label); }
            });
            ui.label(egui::RichText::new("Scales the artwork. Controls and meters stay sharp.").small().color(theme::muted()));
            if *mode != ShowMode::Off {
                if let Some(stats) = browser.render_stats {
                    let timing = stats.gpu_ms.map_or_else(|| "Measuring GPU...".into(), |ms| format!("GPU {ms:.2} ms"));
                    ui.label(egui::RichText::new(format!("{timing}  |  {} x {}", stats.width, stats.height)).small().color(theme::muted()))
                        .on_hover_text("GPU time for the visualizer and effects only; averaged over recent rendered frames.");
                }
            }
        });
        if playback.header_response.clicked() {
            browser.playback_expanded = !browser.playback_expanded;
        }
    } else if browser.page == Page::Effects {
        crate::performance_panel::racks(ui,config,&mut browser.performance);
        crate::viz_compositor::panel(ui,&mut config.pulse_control);
        ui.horizontal(|ui| {
            ui.checkbox(&mut config.effects_enabled, "Enable effects")
                .on_hover_text("Compare this scene with and without the full effect stack. Keeps every effect and mix setting.");
            ui.menu_button("+ Add", |ui| {
                for effect in Effect::ALL {
                    if ui
                        .add_enabled(
                            !config.effects.iter().any(|s| s.effect == effect),
                            egui::Button::new(effect.label()),
                        ).on_hover_text(effect.description())
                        .clicked()
                    {
                        config.effects.push(EffectSlot {
                            effect,
                            enabled: true,
                            mix: 0.7,
                        });
                        ui.close();
                    }
                }
            });
        });
        if config.effects.is_empty() {
            ui.label(
                egui::RichText::new("Add a finishing effect to any scene.")
                    .small()
                    .color(theme::muted()),
            );
        } else {
            ui.label(
                egui::RichText::new(if config.effects_enabled {
                    "Applied top to bottom"
                } else {
                    "Effects are off. Your stack is preserved."
                })
                .small()
                .color(theme::muted()),
            );
        }
        let mut move_up = None;
        let mut move_down = None;
        let mut remove = None;
        let count = config.effects.len();
        for (i, slot) in config.effects.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut slot.enabled, slot.effect.label())
                        .on_hover_text(slot.effect.description());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let menu = ui.menu_button("...", |ui| {
                            if ui
                                .add_enabled(i > 0, egui::Button::new("Move up"))
                                .clicked()
                            {
                                move_up = Some(i);
                                ui.close();
                            }
                            if ui
                                .add_enabled(i + 1 < count, egui::Button::new("Move down"))
                                .clicked()
                            {
                                move_down = Some(i);
                                ui.close();
                            }
                            if ui.button("Remove effect").clicked() {
                                remove = Some(i);
                                ui.close();
                            }
                        });
                        menu.response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                format!("Actions for {}", slot.effect.label()),
                            )
                        });
                        menu.response
                            .on_hover_text(format!("Reorder or remove {}", slot.effect.label()));
                    });
                });
                ui.add_enabled(
                    config.effects_enabled && slot.enabled,
                    egui::Slider::new(&mut slot.mix, 0.0..=1.0)
                        .text("Mix")
                        .custom_formatter(|value, _| format!("{:.0}%", value * 100.0))
                        .custom_parser(|text| {
                            text.trim()
                                .trim_end_matches('%')
                                .trim()
                                .parse::<f64>()
                                .ok()
                                .map(|value| value / 100.0)
                        }),
                );
                crate::viz_motion::binding(ui, &crate::viz_motion::effect_key(slot.effect.bit()), slot.mix, &mut config.motion, &browser.motion);
            });
        }
        if let Some(i) = move_up {
            config.effects.swap(i, i - 1);
        }
        if let Some(i) = move_down {
            config.effects.swap(i, i + 1);
        }
        if let Some(i) = remove {
            config.effects.remove(i);
        }
        crate::performance_panel::flash(ui,config,&browser.motion);
    } else {
        ui.label(egui::RichText::new("Analyzer overlays").strong());
        if config.stage_view {
            ui.checkbox(&mut config.overlays_in_stage, "Show overlays in Stage view");
        }
        ui.add_enabled_ui(!config.stage_view || config.overlays_in_stage, |ui| {
            ui.checkbox(&mut layers.spectrum, "Spectrum bars");
            ui.checkbox(&mut layers.peak_hold, "Peak hold");
            ui.checkbox(&mut layers.analyzer, "Analyzer line");
            ui.checkbox(&mut layers.waterfall, "Waterfall");
            ui.checkbox(&mut layers.waveform, "Waveform");
            ui.checkbox(&mut layers.goniometer, "Stereo scope");
            ui.checkbox(&mut layers.loudness, "Level history");
        });
    }
}

pub fn scene_control_labels(mode: ShowMode) -> Option<[&'static str; 4]> {
    match mode {
        ShowMode::GlKaleido => Some(["Mirror sectors", "Pattern scale", "Line glow", "Persistence"]),
        ShowMode::GlCopper => Some(["Ribbon count", "Thickness", "Wave bend", "Persistence"]),
        ShowMode::GlLaser => Some(["Fan spread", "Beam width", "Haze", "Persistence"]),
        ShowMode::Tunnel => Some(["Ring density", "Spectrum shape", "Twist", "Trail length"]),
        ShowMode::GlTerrain => Some(["Mountain height", "Grid density", "Horizon glow", "Camera height"]),
        ShowMode::GlMatrix => Some(["Glyph size", "Rain density", "Fall speed", "Trail length"]),
        ShowMode::GlFlame => Some(["Turbulence", "Lift", "Embers", "Heat sustain"]),
        ShowMode::StereoScope => Some(["Scope gain", "Point size", "Grid brightness", "Persistence"]),
        ShowMode::SpectralCity => Some(["Tower height", "Block width", "Window glow", "Camera height"]),
        ShowMode::Cymatics => Some(["Harmonic complexity", "Line sharpness", "Grain", "Drift"]),
        ShowMode::StereoOrbit => Some(["Orbit radius", "Wave gain", "Orbit tilt", "Persistence"]),
        ShowMode::Spectrogram => Some(["Contrast", "Intensity", "Frequency color", "Grid brightness"]),
        _ => None,
    }
}

pub fn gallery(
    ctx: &egui::Context,
    mode: &mut ShowMode,
    visual: &mut VizSettings,
    browser: &mut Browser,
) {
    if !browser.gallery.open {
        return;
    }
    browser
        .editor
        .observe(*mode, visual, &browser.selected_look);
    browser.gallery.show(
        ctx,
        mode,
        visual,
        &mut browser.search,
        &mut browser.category,
        &mut browser.favorites_only,
    );
    browser
        .editor
        .observe(*mode, visual, &browser.selected_look);
    if !ctx.input(|i| i.pointer.primary_down()) {
        browser.editor.finish_gesture();
    }
}

fn looks(ui: &mut egui::Ui, mode: &mut ShowMode, visual: &mut VizSettings, browser: &mut Browser) {
    use crate::viz_looks::{self, Look};
    let factory = viz_looks::factory();
    let active = browser
        .selected_look
        .as_ref()
        .and_then(|name| {
            visual
                .saved_looks
                .iter()
                .chain(factory.iter())
                .find(|l| &l.name == name)
        })
        .or_else(|| {
            visual
                .saved_looks
                .iter()
                .chain(factory.iter())
                .find(|l| l.matches(*mode, visual))
        })
        .cloned();
    let label = active
        .as_ref()
        .map(|l| {
            if l.matches(*mode, visual) {
                l.name.clone()
            } else {
                format!("{} *", l.name)
            }
        })
        .unwrap_or_else(|| "Choose look".into());
    let mut picked = None;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 90.0).max(100.0);
        ui.allocate_ui_with_layout(
            egui::vec2(width, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_max_width(width);
                egui::ComboBox::from_id_salt("visual-look")
                    .selected_text(label)
                    .truncate()
                    .width(width)
                    .show_ui(ui, |ui| {
                        if !visual.saved_looks.is_empty() {
                            ui.label(
                                egui::RichText::new("Your looks")
                                    .small()
                                    .color(theme::muted()),
                            );
                            for look in &visual.saved_looks {
                                if ui
                                    .add_enabled(
                                        look.available(),
                                        egui::Button::new(&look.name).selected(
                                            active.as_ref().is_some_and(|a| a.name == look.name),
                                        ),
                                    )
                                    .clicked()
                                {
                                    picked = Some(look.clone());
                                }
                            }
                            ui.separator();
                        }
                        ui.label(
                            egui::RichText::new("Built-in looks")
                                .small()
                                .color(theme::muted()),
                        );
                        for look in &factory {
                            if ui
                                .selectable_label(
                                    active.as_ref().is_some_and(|a| a.name == look.name),
                                    &look.name,
                                )
                                .clicked()
                            {
                                picked = Some(look.clone());
                            }
                        }
                    })
                    .response
                    .on_hover_text("Recall a scene, its style, and its effects together.");
            },
        );
        if ui.button("Save as").clicked() {
            browser.naming_look = !browser.naming_look;
        }
    });
    if browser.naming_look {
        ui.label("Name this visual look");
        ui.add(
            egui::TextEdit::singleline(&mut browser.look_name)
                .hint_text("Look name")
                .char_limit(48)
                .desired_width(ui.available_width() - 8.0),
        );
        let validation = viz_looks::validate_name(&browser.look_name, &visual.saved_looks);
        if !browser.look_name.trim().is_empty() {
            if let Err(message) = validation {
                ui.colored_label(theme::muted(), message);
            }
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(validation.is_ok(), egui::Button::new("Save look"))
                .clicked()
            {
                let look = Look::capture(&browser.look_name, *mode, visual);
                browser.selected_look = Some(look.name.clone());
                visual.saved_looks.push(look);
                browser.look_name.clear();
                browser.naming_look = false;
            }
            if ui.button("Cancel").clicked() {
                browser.naming_look = false;
            }
        });
    }
    if let Some(look) = picked {
        if look.apply(mode, visual) {
            browser.selected_look = Some(look.name);
        }
    }
    if let Some(active) = active {
        if let Some(index) = visual
            .saved_looks
            .iter()
            .position(|l| l.name == active.name)
        {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !active.matches(*mode, visual),
                        egui::Button::new("Update look").small(),
                    )
                    .clicked()
                {
                    visual.saved_looks[index] = Look::capture(&active.name, *mode, visual);
                }
                if ui.small_button("Remove look").clicked() {
                    browser.removed_look = Some(visual.saved_looks.remove(index));
                    browser.selected_look = None;
                }
            });
        }
    }
    if let Some(removed) = &browser.removed_look {
        if ui
            .add_enabled(
                viz_looks::validate_name(&removed.name, &visual.saved_looks).is_ok(),
                egui::Button::new("Undo remove").small(),
            )
            .clicked()
        {
            visual
                .saved_looks
                .push(browser.removed_look.take().unwrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};
    type State = (ShowMode, VizSettings, Browser, Layers, bool);
    fn harness(state: State) -> Harness<'static, State> {
        Harness::builder()
            .with_size(egui::vec2(310.0, 460.0))
            .build_ui_state(
                |ui, s| {
                    show(ui, &mut s.0, &mut s.1, &mut s.2, &mut s.3, &mut s.4, true);
                    gallery(ui.ctx(), &mut s.0, &mut s.1, &mut s.2);
                },
                state,
            )
    }
    #[test]
    fn adaptive_presets_and_distortion_recipe_are_usable_and_undoable() {
        let mut h=harness((ShowMode::GlWarp,VizSettings::default(),Browser{page:Page::Signals,..Default::default()},Layers::default(),false));
        h.get_by_label("Percussive").click();h.run();
        assert!(h.state().1.signal_response.enabled);
        assert_eq!(h.state().1.signal_response.channels[1].attack,0.0);
        assert!(h.get_by_label("Adaptive response").rect().right()<=310.0);
        h.get_by_label("Undo").click();h.run();
        assert!(!h.state().1.signal_response.enabled);
        h.state_mut().2.page=Page::Perform;h.run();
        h.get_by_label("Cross-deck distortion").scroll_to_me();h.run();
        h.get_by_label("Cross-deck distortion").click();h.run();
        h.get_by_label("Flame flow").scroll_to_me();h.run();
        h.get_by_label("Flame flow").click();h.run();
        assert!(h.state().0==ShowMode::GlTerrain);
        assert!(h.state().1.performance.enabled);
        assert_eq!(h.state().1.performance.interaction.mode,crate::viz_compositor::InteractionMode::Flow);
        h.get_by_label("Undo").click();h.run();
        assert!(h.state().0==ShowMode::GlWarp);
        assert!(!h.state().1.performance.enabled);
        assert_eq!(h.state().1.performance.interaction.mode,crate::viz_compositor::InteractionMode::Off);
    }
    #[test]
    fn resolution_control_preserves_scene_and_style() {
        let state = (
            ShowMode::GlFlame,
            VizSettings::default(),
            Browser {
                page: Page::Style,
                ..Default::default()
            },
            Layers::default(),
            false,
        );
        let mut h = Harness::builder()
            .with_size(egui::vec2(310.0, 680.0))
            .build_ui_state(
                |ui, s| {
                    show(ui, &mut s.0, &mut s.1, &mut s.2, &mut s.3, &mut s.4, true);
                },
                state,
            );
        h.get_by_label("Playback & quality").click();
        h.run();
        let control = h.get_by_label("50%");
        assert!(control.rect().right() <= 310.0);
        control.click();
        h.run();
        assert_eq!(h.state().1.quality.scale(), 0.5);
        assert!(h.state().0 == ShowMode::GlFlame);
        assert_eq!(h.state().1.speed, 1.0);
        assert!(h.state().3.spectrum);
    }
    #[test]
    fn gallery_button_opens_and_escape_returns_to_the_panel() {
        let mut h = harness((
            ShowMode::GlWarp,
            VizSettings::default(),
            Browser::default(),
            Layers::default(),
            false,
        ));
        h.get_by_label("Gallery").click();
        h.run();
        assert!(h.state().2.gallery.open);
        assert!(h.get_by_label("Done").rect().right() <= 310.0);
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(!h.state().2.gallery.open);
        assert!(h.state().0 == ShowMode::GlWarp);
    }
    #[test]
    fn long_look_names_leave_save_control_visible() {
        let mut visual = VizSettings::default();
        visual.saved_looks.push(crate::viz_looks::Look::capture(
            &"W".repeat(48),
            ShowMode::GlWarp,
            &visual,
        ));
        let h = harness((
            ShowMode::GlWarp,
            visual,
            Browser {
                page: Page::Style,
                ..Default::default()
            },
            Layers::default(),
            false,
        ));
        assert!(h.get_by_label("Save as").rect().right() <= 310.0);
    }
    #[test]
    fn compact_scene_controls_stay_inside_the_panel() {
        let h = harness((
            ShowMode::GlWarp,
            VizSettings::default(),
            Browser::default(),
            Layers::default(),
            false,
        ));
        for label in [
            "Off", "Next", "Bars XL", "Scenes", "Style", "Effects", "Meters", "Perform", "More",
        ] {
            let rect = h.get_by_label(label).rect();
            assert!(
                rect.left() >= 0.0 && rect.right() <= 310.0,
                "{label} escapes panel: {rect:?}"
            );
        }
    }
    #[test]
    fn more_menu_reaches_signals_and_performance_controls_fit() {
        let mut h=harness((ShowMode::GlFlame,VizSettings::default(),Browser::default(),Layers::default(),false));
        h.get_by_label("More").click();h.run();h.get_by_label("Signals").click();h.run();assert!(h.query_by_label("Signal lab").is_some());
        h.get_by_label("Perform").click();h.run();
        for label in ["1 Punch","2 Scatter","3 Freeze","4 Sweep","5 Color burst","Enable deck B","Fade to A","Fade to B"] {
            let r=h.get_by_label(label).rect();assert!(r.left()>=0.0&&r.right()<=310.0,"{label} escapes panel: {r:?}");
        }
    }
    #[test]
    fn performance_links_connect_a_route_seed_xy_and_start_recording() {
        let mut h=harness((ShowMode::GlFlame,VizSettings::default(),Browser{page:Page::Perform,..Default::default()},Layers::default(),false));
        h.get_by_label("Patch bay").click();h.run();h.get_by_label("Connect signal").click();h.run();
        assert!(h.state().1.motion.routes.contains_key("zoom"));
        h.get_by_label("Gate").scroll_to_me();h.run();h.get_by_label("Gate").click();h.run();assert_eq!(h.state().1.motion.routes["zoom"].curve,[0.0,0.0,0.0,0.9,1.0]);
        h.get_by_label("Perform").click();h.run();h.get_by_label("XY pad").click();h.run();
        h.get_by_label("Seed four corners from current look").click();h.run();assert!(h.state().1.xy.enabled);assert!(h.state().1.xy.corners.iter().all(Option::is_some));
        h.get_by_label("Audio pilot").click();h.run();assert!(h.state().1.motion.routes.contains_key("xy_x"));
        h.get_by_label("Manual XY").click();h.run();assert!(!h.state().1.motion.routes.contains_key("xy_x"));
        h.get_by_label("Perform").click();h.run();h.get_by_label("Record").click();h.run();
        h.get_by_label("Record new take").click();h.run();assert!(h.state().2.recorder.recording);
        h.get_by_label("Stop").click();h.run();assert!(!h.state().2.recorder.recording);
    }
    #[test]
    fn audition_capture_and_ratings_fit_the_compact_panel() {
        let mut h = harness((ShowMode::GlFlame, VizSettings::default(), Browser {
            page: Page::Audition, ..Default::default()
        }, Layers::default(), true));
        for label in ["Audition", "Record 8-second phrase", "Keeper", "Needs work", "Boring"] {
            assert!(h.get_by_label(label).rect().right() <= 310.0, "{label} escapes");
        }
        h.get_by_label("Keeper").click(); h.run();
        assert_eq!(h.state().1.review_ratings.len(), 1);
        h.get_by_label("Record 8-second phrase").click(); h.run();
        assert!(!h.state().4);
        h.get_by_label("Cancel recording").click(); h.run();
        assert!(h.query_by_label("Record 8-second phrase").is_some());
    }
    #[test]
    fn stage_toggle_preserves_analyzer_preferences() {
        let mut h = harness((
            ShowMode::GlWarp,
            VizSettings::default(),
            Browser::default(),
            Layers::default(),
            false,
        ));
        h.get_by_label("Stage view").click();
        h.run();
        assert!(h.state().1.stage_view);
        assert!(h.state().3.spectrum);
        assert!(!h.state().1.overlays_in_stage);
        h.get_by_label("EQ view").click();
        h.run();
        assert!(!h.state().1.stage_view);
        assert!(h.state().3.spectrum);
    }
    #[test]
    fn unified_library_selects_a_legacy_scene_from_search() {
        let browser = Browser {
            search: "particles".into(),
            ..Default::default()
        };
        let mut h = harness((
            ShowMode::Off,
            VizSettings::default(),
            browser,
            Layers::default(),
            false,
        ));
        h.get_by_label("Particles").click();
        h.run();
        assert!(h.state().0 == ShowMode::Particles);
        h.get_by_label("Favorite Particles").click();
        h.run();
        assert_eq!(h.state().1.favorites, ["particles"]);
    }
    #[test]
    fn shuffle_remains_visible_in_a_short_inspector() {
        let state = (
            ShowMode::GlWarp,
            VizSettings::default(),
            Browser::default(),
            Layers::default(),
            false,
        );
        let h = Harness::builder()
            .with_size(egui::vec2(310.0, 280.0))
            .build_ui_state(
                |ui, s| {
                    show(ui, &mut s.0, &mut s.1, &mut s.2, &mut s.3, &mut s.4, true);
                },
                state,
            );
        assert!(h.get_by_label("Shuffle").rect().bottom() <= 280.0);
    }
    #[test]
    fn save_recall_update_and_undo_remove_a_visual_look() {
        let browser = Browser {
            page: Page::Style,
            ..Default::default()
        };
        let mut h = harness((
            ShowMode::GlFlame,
            VizSettings::default(),
            browser,
            Layers::default(),
            false,
        ));
        h.get_by_value("Choose look").click();
        h.run();
        h.get_by_label("Phosphor").click();
        h.run();
        assert!(h.state().0 == ShowMode::ScopeTrails);
        assert!(!h.state().1.rainbow);
        assert_eq!(h.state().1.effects.len(), 2);
        h.get_by_label("Save as").click();
        h.run();
        h.get_by_role(egui::accesskit::Role::TextInput).click();
        h.run();
        h.get_by_role(egui::accesskit::Role::TextInput)
            .type_text("My phosphor");
        h.run();
        h.get_by_label("Save look").click();
        h.run();
        assert_eq!(h.state().1.saved_looks[0].name, "My phosphor");
        h.state_mut().1.speed = 0.5;
        h.run();
        h.get_by_label("Update look").click();
        h.run();
        assert_eq!(h.state().1.saved_looks[0].speed, 0.5);
        h.get_by_label("Remove look").click();
        h.run();
        assert!(h.state().1.saved_looks.is_empty());
        h.get_by_label("Undo remove").click();
        h.run();
        assert_eq!(h.state().1.saved_looks.len(), 1);
        h.state_mut().0 = ShowMode::GlFlame;
        h.state_mut().1.effects.clear();
        h.run();
        h.get_by_value("Choose look").click();
        h.run();
        h.get_by_label("My phosphor").click();
        h.run();
        assert!(h.state().0 == ShowMode::ScopeTrails);
        assert_eq!(h.state().1.effects.len(), 2);
        assert_eq!(h.state().1.speed, 0.5);
    }
    #[test]
    fn add_effect_menu_builds_the_render_stack() {
        let mut h = harness((
            ShowMode::GlWarp,
            VizSettings::default(),
            Browser::default(),
            Layers::default(),
            false,
        ));
        h.get_by_label("Effects").click();
        h.run();
        h.get_by_label("+ Add").click();
        h.run();
        h.get_by_label("CRT scanlines").click();
        h.run();
        assert_eq!(h.state().1.effects.len(), 1);
        assert_eq!(h.state().1.effect_uniforms().0[0], 1 << 5);
        h.get_by_label("CRT scanlines").click();
        h.run();
        assert_eq!(h.state().1.effect_uniforms().2, 0);
    }
    #[test]
    fn effect_stack_can_be_compared_reordered_removed_and_undone() {
        let effects = vec![
            EffectSlot {
                effect: Effect::Aberration,
                enabled: true,
                mix: 0.35,
            },
            EffectSlot {
                effect: Effect::Grain,
                enabled: false,
                mix: 0.2,
            },
        ];
        let visual = VizSettings {
            effects: effects.clone(),
            ..Default::default()
        };
        let mut h = harness((
            ShowMode::GlWarp,
            visual,
            Browser {
                page: Page::Effects,
                ..Default::default()
            },
            Layers::default(),
            false,
        ));
        h.get_by_label("Enable effects").click();
        h.run();
        assert_eq!(h.state().1.effect_uniforms().2, 0);
        assert!(h.state().1.effects == effects);
        h.get_by_label("Undo").click();
        h.run();
        assert!(h.state().1.effects_enabled && h.state().1.effects == effects);
        h.get_by_label("Actions for Color split").click();
        h.run();
        h.get_by_label("Move down").click();
        h.run();
        assert!(h.state().1.effects[0].effect == Effect::Grain);
        h.get_by_label("Undo").click();
        h.run();
        assert!(h.state().1.effects == effects);
        h.get_by_label("Redo").click();
        h.run();
        assert!(h.state().1.effects[0].effect == Effect::Grain);
        h.get_by_label("Actions for Film grain").click();
        h.run();
        h.get_by_label("Remove effect").click();
        h.run();
        assert_eq!(h.state().1.effects.len(), 1);
        h.get_by_label("Undo").click();
        h.run();
        assert!(h.state().1.effects[0].effect == Effect::Grain);
        assert!(!h.state().1.effects[0].enabled);
        assert_eq!(h.state().1.effects[0].mix, 0.2);
    }
    #[test]
    fn choosing_a_scene_is_undoable_and_favoriting_is_not_an_art_edit() {
        let mut h = harness((
            ShowMode::GlWarp,
            VizSettings::default(),
            Browser {
                search: "particles".into(),
                ..Default::default()
            },
            Layers::default(),
            false,
        ));
        h.get_by_label("Particles").click();
        h.run();
        assert!(h.state().0 == ShowMode::Particles);
        h.get_by_label("Favorite Particles").click();
        h.run();
        h.get_by_label("Undo").click();
        h.run();
        assert!(h.state().0 == ShowMode::GlWarp);
        assert_eq!(h.state().1.favorites, ["particles"]);
        h.get_by_label("Redo").click();
        h.run();
        assert!(h.state().0 == ShowMode::Particles);
    }
}
