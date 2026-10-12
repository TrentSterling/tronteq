//! Static, production-rendered thumbnails. Browsing does not start extra scene renderers.
use crate::{show::ShowMode, theme, viz_settings::VizSettings};
use eframe::egui;
use serde::Deserialize;

#[derive(Deserialize)]
struct Atlas {
    columns: usize,
    tile_width: usize,
    tile_height: usize,
    scenes: Vec<String>,
}

impl Atlas {
    fn bundled() -> Self {
        serde_json::from_str(include_str!("../assets/scene-atlas.json"))
            .expect("bundled scene atlas metadata")
    }
    fn uv(&self, scene: ShowMode) -> Option<egui::Rect> {
        let index = self.scenes.iter().position(|id| id == scene.as_str())?;
        let rows = self.scenes.len().div_ceil(self.columns) as f32;
        let x = (index % self.columns) as f32;
        let y = (index / self.columns) as f32;
        Some(egui::Rect::from_min_max(
            egui::pos2(x / self.columns as f32, y / rows),
            egui::pos2((x + 1.0) / self.columns as f32, (y + 1.0) / rows),
        ))
    }
}

#[derive(Default)]
pub struct Gallery {
    pub open: bool,
    texture: Option<egui::TextureHandle>,
    atlas: Option<Atlas>,
}

pub fn filtered(
    scene: ShowMode,
    query: &str,
    category: &str,
    favorites_only: bool,
    config: &VizSettings,
) -> bool {
    (category.is_empty() || scene.category() == category)
        && (query.is_empty()
            || format!("{} {}", scene.label(), scene.category())
                .to_lowercase()
                .contains(query))
        && (!favorites_only || config.favorites.iter().any(|id| id == scene.as_str()))
}

pub fn filters(ui: &mut egui::Ui, category: &mut String, favorites_only: &mut bool) {
    egui::ComboBox::from_id_salt("scene-category")
        .selected_text(if category.is_empty() {
            "All scenes"
        } else {
            category.as_str()
        })
        .width(130.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(category, String::new(), "All scenes");
            for name in [
                "Atmosphere",
                "Space",
                "Geometry",
                "Audio",
                "Retro",
                "Motion",
            ] {
                ui.selectable_value(category, name.into(), name);
            }
        });
    ui.toggle_value(favorites_only, "Favorites");
}

impl Gallery {
    /// Shared thumbnail browser for the inspector and expanded gallery.
    pub fn grid(
        &mut self,
        ui: &mut egui::Ui,
        scenes: &[ShowMode],
        mode: &mut ShowMode,
        config: &mut VizSettings,
        minimum_width: f32,
        reveal_active: bool,
    ) -> bool {
        let atlas = self.atlas.get_or_insert_with(Atlas::bundled);
        let texture = self.texture.get_or_insert_with(|| {
            let image = image::load_from_memory(include_bytes!("../assets/scene-atlas.png"))
                .expect("bundled scene atlas PNG")
                .to_rgb8();
            ui.ctx().load_texture(
                "scene-atlas",
                egui::ColorImage::from_rgb(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            )
        });
        let gap = 8.0;
        let available = ui.available_width();
        let compact = ui.clip_rect().height() < 110.0;
        let columns = if compact {
            1
        } else {
            ((available + gap) / (minimum_width + gap)).floor().max(1.0) as usize
        };
        let width = (available - gap * (columns - 1) as f32) / columns as f32;
        let height = if compact {
            ui.clip_rect().height().clamp(28.0, 32.0)
        } else {
            width * atlas.tile_height as f32 / atlas.tile_width as f32 + 28.0
        };
        let mut selected = false;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            for row in scenes.chunks(columns) {
                ui.horizontal_top(|ui| {
                    for &scene in row {
                        ui.push_id(scene.as_str(), |ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(width, height),
                                egui::Sense::hover(),
                            );
                            let response =
                                ui.interact(rect, ui.id().with("select"), egui::Sense::click());
                            let active = *mode == scene;
                            if active && reveal_active {
                                ui.scroll_to_rect(rect, Some(egui::Align::Center));
                            }
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Button,
                                    true,
                                    active,
                                    scene.label(),
                                )
                            });
                            let preview = if compact {
                                egui::Rect::from_min_size(
                                    rect.min + egui::vec2(2.0, 2.0),
                                    egui::vec2(
                                        (height - 4.0) * atlas.tile_width as f32
                                            / atlas.tile_height as f32,
                                        height - 4.0,
                                    ),
                                )
                            } else {
                                egui::Rect::from_min_max(
                                    rect.min,
                                    rect.right_bottom() - egui::vec2(0.0, 28.0),
                                )
                            };
                            ui.painter().rect_filled(
                                rect,
                                2.0,
                                if active {
                                    ui.visuals().selection.bg_fill
                                } else {
                                    ui.visuals().widgets.inactive.bg_fill
                                },
                            );
                            ui.painter().image(
                                texture.id(),
                                preview,
                                atlas.uv(scene).expect("scene has a bundled thumbnail"),
                                egui::Color32::WHITE,
                            );
                            let mut label = egui::text::LayoutJob::simple_singleline(
                                scene.label().into(),
                                egui::FontId::proportional(14.0),
                                theme::text(),
                            );
                            let text_x = if compact {
                                preview.right() + 8.0
                            } else {
                                rect.left() + 7.0
                            };
                            let text_y = if compact {
                                rect.center().y
                            } else {
                                rect.bottom() - 14.0
                            };
                            label.wrap.max_width = (rect.right() - text_x - 29.0).max(1.0);
                            label.wrap.max_rows = 1;
                            label.wrap.break_anywhere = true;
                            let galley = ui.painter().layout_job(label);
                            ui.painter().galley(
                                egui::pos2(text_x, text_y - galley.size().y * 0.5),
                                galley,
                                theme::text(),
                            );
                            if active || response.hovered() || response.has_focus() {
                                ui.painter().rect_stroke(
                                    rect,
                                    2.0,
                                    egui::Stroke::new(
                                        if active { 2.0 } else { 1.0 },
                                        theme::cyan(),
                                    ),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if response.on_hover_text(scene.description()).clicked() {
                                *mode = scene;
                                selected = true;
                            }
                            let favorite = config.favorites.iter().any(|id| id == scene.as_str());
                            let star_rect = egui::Rect::from_min_size(
                                rect.right_bottom() - egui::vec2(28.0, 28.0),
                                egui::vec2(28.0, 28.0),
                            );
                            let star = ui.interact(
                                star_rect,
                                ui.id().with("favorite"),
                                egui::Sense::click(),
                            );
                            star.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    favorite,
                                    format!("Favorite {}", scene.label()),
                                )
                            });
                            let ink = if favorite || star.hovered() || star.has_focus() {
                                theme::cyan()
                            } else {
                                theme::muted()
                            };
                            let center = star_rect.center();
                            let points = (0..10)
                                .map(|i| {
                                    let angle = i as f32 * std::f32::consts::PI / 5.0
                                        - std::f32::consts::FRAC_PI_2;
                                    center
                                        + egui::vec2(angle.cos(), angle.sin())
                                            * if i % 2 == 0 { 6.5 } else { 3.0 }
                                })
                                .collect();
                            if favorite {
                                ui.painter().circle_filled(center, 2.7, ink);
                            }
                            ui.painter().add(egui::Shape::closed_line(
                                points,
                                egui::Stroke::new(1.2, ink),
                            ));
                            if star.has_focus() {
                                ui.painter().rect_stroke(
                                    star_rect.shrink(2.0),
                                    2.0,
                                    egui::Stroke::new(1.0, ink),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if star
                                .on_hover_text(if favorite {
                                    "Remove favorite"
                                } else {
                                    "Add favorite"
                                })
                                .clicked()
                            {
                                if favorite {
                                    config.favorites.retain(|id| id != scene.as_str());
                                } else {
                                    config.favorites.push(scene.as_str().into());
                                }
                            }
                        });
                    }
                });
            }
        });
        selected
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        mode: &mut ShowMode,
        config: &mut VizSettings,
        search: &mut String,
        category: &mut String,
        favorites_only: &mut bool,
    ) {
        if !self.open {
            return;
        }
        let width = (ctx.content_rect().width() - 48.0).clamp(220.0, 900.0);
        let height = (ctx.content_rect().height() - 210.0).clamp(60.0, 600.0);
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("scene-gallery")).show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.heading("Scene gallery");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close |= ui.button("Done").clicked();
                });
            });
            ui.label(
                egui::RichText::new("Choose a scene to see it react to your audio.")
                    .color(theme::muted()),
            );
            ui.add_sized(
                [width, 26.0],
                egui::TextEdit::singleline(search).hint_text("Search scenes..."),
            );
            ui.horizontal_wrapped(|ui| filters(ui, category, favorites_only));
            ui.separator();
            let query = search.trim().to_lowercase();
            let scenes: Vec<_> = ShowMode::ALL
                .into_iter()
                .skip(1)
                .filter(|m| filtered(*m, &query, category, *favorites_only, config))
                .collect();
            if scenes.is_empty() {
                ui.label("No scenes match these filters.");
                if ui.button("Clear filters").clicked() {
                    search.clear();
                    category.clear();
                    *favorites_only = false;
                }
            }
            egui::ScrollArea::vertical()
                .id_salt("gallery-scroll")
                .max_height(height)
                .min_scrolled_height(0.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    close |= self.grid(ui, &scenes, mode, config, 177.0, false);
                });
            ui.label(
                egui::RichText::new(format!(
                    "{} scenes  |  Previews use sample audio",
                    scenes.len()
                ))
                .small()
                .color(theme::muted()),
            );
        });
        if close || response.should_close() {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};
    #[test]
    fn bundled_atlas_covers_every_scene_once() {
        let atlas = Atlas::bundled();
        let image = image::load_from_memory(include_bytes!("../assets/scene-atlas.png")).unwrap();
        assert_eq!(image.width() as usize, atlas.columns * atlas.tile_width);
        assert_eq!(
            image.height() as usize,
            atlas.scenes.len().div_ceil(atlas.columns) * atlas.tile_height
        );
        assert_eq!(atlas.scenes.len(), ShowMode::ALL.len() - 1);
        for scene in ShowMode::ALL.into_iter().skip(1) {
            assert_eq!(
                atlas
                    .scenes
                    .iter()
                    .filter(|id| *id == scene.as_str())
                    .count(),
                1
            );
            let uv = atlas.uv(scene).unwrap();
            assert!(uv.min.x >= 0.0 && uv.min.y >= 0.0 && uv.max.x <= 1.0 && uv.max.y <= 1.0);
        }
    }
    #[test]
    fn gallery_filters_favorites_and_selects_without_changing_style() {
        let state = (
            Gallery {
                open: true,
                ..Default::default()
            },
            ShowMode::GlFlame,
            VizSettings {
                speed: 0.4,
                ..Default::default()
            },
            "ocean".to_string(),
            String::new(),
            false,
        );
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .build_state(
                |ctx, s| {
                    s.0.show(ctx, &mut s.1, &mut s.2, &mut s.3, &mut s.4, &mut s.5);
                },
                state,
            );
        h.get_by_label("Favorite Moonlit ocean").click();
        h.run();
        assert_eq!(h.state().2.favorites, ["gl_ocean"]);
        assert!(h.state().0.open);
        assert!(
            h.state().1 == ShowMode::GlFlame,
            "Favoriting must not select the tile"
        );
        h.get_by_label("Favorites").click();
        h.run();
        h.get_by_label("Moonlit ocean").click();
        h.run();
        assert!(!h.state().0.open);
        assert!(h.state().1 == ShowMode::GlOcean);
        assert_eq!(h.state().2.speed, 0.4);
    }
    #[test]
    fn empty_gallery_can_clear_filters_and_escape() {
        let state = (
            Gallery {
                open: true,
                ..Default::default()
            },
            ShowMode::GlFlame,
            VizSettings::default(),
            "no-such-scene".to_string(),
            "Audio".to_string(),
            true,
        );
        let mut h = Harness::builder()
            .with_size(egui::vec2(600.0, 460.0))
            .build_state(
                |ctx, s| {
                    s.0.show(ctx, &mut s.1, &mut s.2, &mut s.3, &mut s.4, &mut s.5);
                },
                state,
            );
        h.get_by_label("Clear filters").click();
        h.run();
        assert!(h.state().3.is_empty() && h.state().4.is_empty() && !h.state().5);
        let rect = h.get_by_label("Bars XL").rect();
        assert!(rect.left() >= 0.0 && rect.right() <= 600.0);
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(!h.state().0.open);
        assert!(h.state().1 == ShowMode::GlFlame);
    }
}
