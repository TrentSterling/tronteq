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
}

#[derive(Default)]
pub struct Browser {
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
            .small_button("Full")
            .on_hover_text("Toggle fullscreen (F11)")
            .clicked()
        {
            let full = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(!full));
        }
        if ui
            .add_enabled(
                *mode != ShowMode::Off && available,
                egui::Button::new("Present").small(),
            )
            .on_hover_text("Fullscreen artwork with controls hidden. Escape returns here.")
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
    ui.horizontal(|ui| {
        for (page, label) in [
            (Page::Scenes, "Scenes"),
            (Page::Style, "Style"),
            (Page::Effects, "Effects"),
            (Page::Meters, "Meters"),
        ] {
            ui.selectable_value(&mut browser.page, page, label);
        }
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
    if browser.page == Page::Scenes {
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
        ui.add(
            egui::Slider::new(&mut config.energy, 0.0..=2.0)
                .text("Audio response")
                .suffix("x"),
        );
        ui.add(egui::Slider::new(&mut config.brightness, 0.1..=1.5).text("Brightness"));
        ui.horizontal(|ui| {
            ui.label("Color");
            ui.selectable_value(&mut config.rainbow, true, "Spectrum");
            ui.selectable_value(&mut config.rainbow, false, "Theme");
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
            "Off", "Next", "Bars XL", "Scenes", "Style", "Effects", "Meters",
        ] {
            let rect = h.get_by_label(label).rect();
            assert!(
                rect.left() >= 0.0 && rect.right() <= 310.0,
                "{label} escapes panel: {rect:?}"
            );
        }
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
