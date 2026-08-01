//! The Theme window: gradient v2 controls, ported from SpaceView's
//! `Window::new("Theme")` (colormagic accent picker, dark/light + premades +
//! randomize, the Discord-style multi-stop wash, and the reset "wayback
//! machine"). Opened from the toolbar's accent swatch, next to the Light/Dark
//! toggle (see `main.rs`).
//!
//! Follows the established "copy locals, mutate inside the closure, write
//! back after it closes" pattern (`inspector.rs`) so nothing from `App` is
//! captured inside the nested egui closure — `ctx` is a cheap `Copy` handle,
//! and the palette/gradient-cfg/frost knobs all live in `theme.rs`'s own
//! statics, so the only two bits of `App` state this window needs
//! (`show_theme_window`, `gradient_preset_sync`) are plain bools.

use eframe::egui;

use crate::color;
use crate::{theme, App};

/// Read-only view of the pegs the ramp actually resolved to.
///
/// In Harmony and Presets mode the stops are derived, so there is no picker to
/// show — which previously meant the row was an empty 26px spacer and you had no
/// way to know what colours the gradient was built from.
fn read_only_pegs(ui: &mut egui::Ui) {
    let pegs = theme::gradient_pegs();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let border = ui.visuals().widgets.noninteractive.bg_stroke.color;
        for p in &pegs {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(34.0, 20.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, border);
            ui.painter()
                .rect_filled(rect.shrink(1.0), 3.0, egui::Color32::from_rgb(p[0], p[1], p[2]));
            resp.on_hover_text(color::rgb_to_hex(*p));
        }
    });
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let mut show_win = app.show_theme_window;
    if !show_win {
        return;
    }
    // Modal law: Esc closes, same as the window's own X.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        show_win = false;
    }
    let mut sync = app.gradient_preset_sync;

    if show_win {
        let mut open = true;
        // ONE height budget, shared. The window frame gets `win_h`; the scroll
        // viewport gets that minus the title bar and frame margins, so the
        // SCROLL AREA is what clamps the content. Sizing the two independently is
        // exactly what left the bottom half clipped by the window instead of
        // scrolled by the area — the window won the fight and swallowed Frost,
        // the mode chips and every peg picker.
        // Constrain ONLY the scroll viewport, never the window frame. Clamping
        // the Window's max_height just truncates the frame and clips whatever is
        // inside it — the ScrollArea then believes it fits and never scrolls, so
        // the bottom of the panel silently vanishes. Leaving the window free to
        // auto-size around a deliberately conservative viewport is what makes the
        // scrollbar appear and Frost / mode chips / peg pickers reachable.
        // Now that the panel starts at a known inset from the top, it can use
        // nearly the whole app height: subtract the inset, the title bar and the
        // frame margins rather than guessing with a fraction.
        let scroll_h = (ctx.content_rect().height() - 112.0).max(200.0);
        egui::Window::new("Theme")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(280.0)
            // TrontEQ's window is 460px tall by default — barely half of
            // SpaceView's, where this UI was designed. Without a height cap and
            // a scrollbar the panel simply ran off the bottom of the app, which
            // silently hid Frost, the Harmony/Presets/Custom chips, the peg
            // count and every peg picker: they rendered, just below the screen.
            // Same fix the About window already uses in this repo.
            // Deterministic placement near the top-left of the content area.
            // Centering via `default_pos(content_rect().center())` was evaluated
            // on the FIRST frame, before the viewport size settles, so the panel
            // landed low-right and its bottom rows fell off the app edge. A fixed
            // inset always starts fully inside, and the window stays draggable
            // (an `anchor()` would pin it immovably - the Boxel lesson).
            .default_pos(ctx.content_rect().min + egui::vec2(28.0, 28.0))
            .pivot(egui::Align2::LEFT_TOP)
            // egui remembers a window's position across sessions, so a panel that
            // was dragged (or auto-placed) near the bottom stays there even after
            // it grows — and its lower rows get clipped by the app edge, which
            // looks exactly like "the pegs are missing" all over again. Pin it
            // inside the app rect so the whole panel is always reachable.
            .constrain_to(ctx.content_rect())
            .show(ctx, |ui| {
                // MAKE THE SCROLLBAR VISIBLE. This panel was scrollable all
                // along; egui draws the handle with `widgets.inactive.bg_fill`,
                // which in this theme lands within a few RGB units of the panel
                // fill. Measured on a capture: the bar column was byte-identical
                // to the background, so the window read as simply truncated and
                // Frost / the mode chips / the peg pickers looked missing rather
                // than one scroll away.
                {
                    let accent = theme::cyan();
                    let v = ui.visuals_mut();
                    v.widgets.inactive.bg_fill = accent.gamma_multiply(0.55);
                    v.widgets.hovered.bg_fill = accent;
                    v.widgets.active.bg_fill = accent;
                }
                // `floating` is egui's DEFAULT scroll style: the bar is an overlay
                // that fades in on hover, which is why ScrollBarVisibility::
                // AlwaysVisible appeared to do nothing and a pixel scan of the
                // window's right edge found only panel fill. Turning it off gives
                // a solid, always-present bar that also reserves its own width.
                ui.spacing_mut().scroll.floating = false;
                ui.spacing_mut().scroll.bar_width = 10.0;

                // COMPACT METRICS for this panel only. TrontEQ's house spacing is
                // deliberately generous — right for the main HUD chrome, far too
                // tall for a dense settings panel. At full spacing the control set
                // needed ~750px inside a window that gets ~430, which is why the
                // bottom half (Frost, mode chips, peg pickers) lived below the
                // fold. Tightening rows here is what actually makes it all fit.
                {
                    let sp = ui.spacing_mut();
                    sp.item_spacing.y = 3.0;
                    sp.interact_size.y = 19.0;
                    sp.button_padding = egui::vec2(6.0, 2.0);
                }

                // Window::max_height + vscroll(true) did NOT clamp this panel —
                // the big inline picker forces a tall minimum and the bottom half
                // still ran off the app window. An explicit ScrollArea with a
                // computed height is deterministic, and it is what actually makes
                // Frost / the mode chips / the peg pickers reachable in a 460px
                // tall app.
                egui::ScrollArea::vertical()
                    .max_height(scroll_h)
                    // Take the full viewport instead of shrinking to content, so
                    // the height above is a real constraint.
                    .auto_shrink([false, false])
                    // ALWAYS show the bar: the full control set is taller than
                    // this app's window no matter how compact the picker gets, so
                    // the user has to be able to SEE that there is more below.
                    // A hover-only scrollbar is why it read as "just missing".
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                    )
                    .show(ui, |ui| {
                // ---- THE picker, big and up front: one swatch exists (the
                // toolbar block); in here the full picker IS the interface.
                // The picker lives in a COLLAPSED header by default. It is ~280px
                // tall — more than half of everything this window gets inside
                // TrontEQ's 460px-tall app — and while it was expanded it pushed
                // Frost, the mode chips and every peg picker off the bottom. The
                // toolbar swatch is already the shortcut for "I want to pick a
                // color"; opening this panel is usually about the gradient. One
                // click expands it, and the header carries the live accent so you
                // can still see what the theme is.
                let acc = theme::cyan();
                egui::CollapsingHeader::new("Accent color")
                    .default_open(false)
                    .show(ui, |ui| {
                        // Bind to the RAW pick, not `theme::cyan()` (the enforced
                        // ink accessor). Seeding from the corrected value handed the
                        // widget its own output every frame, so yellow was walked
                        // toward readable-on-panel and snapped back to brown, and
                        // black could not be selected at all.
                        let seed = theme::accent_seed();
                        let mut accent_color =
                            egui::Color32::from_rgb(seed[0], seed[1], seed[2]);
                        ui.spacing_mut().slider_width = 104.0;
                        if egui::color_picker::color_picker_color32(
                            ui,
                            &mut accent_color,
                            egui::color_picker::Alpha::Opaque,
                        ) {
                            let rgb = [accent_color.r(), accent_color.g(), accent_color.b()];
                            let dark = theme::dark_mode();
                            theme::set_palette(ctx, theme::Palette::from_accent(rgb, dark));
                        }
                    });
                ui.separator();

                // ---- Dark|Light chips + premades combo + Random, one row --
                ui.horizontal(|ui| {
                    let dark = theme::dark_mode();
                    if ui.selectable_label(dark, "Dark").clicked() && !dark {
                        theme::set_mode(ctx, true);
                    }
                    if ui.selectable_label(!dark, "Light").clicked() && dark {
                        theme::set_mode(ctx, false);
                    }
                    ui.separator();
                    let cur_name = theme::current().name;
                    egui::ComboBox::from_id_salt("theme_premade_selector")
                        .selected_text(if cur_name.is_empty() { "Theme" } else { cur_name.as_str() })
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for name in ["Electric Cyan", "Paper", "Synthwave"] {
                                if ui.selectable_label(cur_name == name, name).clicked() {
                                    let p = match name {
                                        "Electric Cyan" => theme::Palette::electric_cyan(),
                                        "Paper" => theme::Palette::paper(),
                                        _ => theme::Palette::synthwave(),
                                    };
                                    theme::set_palette(ctx, p);
                                }
                            }
                            for pp in color::PREMADE_PALETTES {
                                if ui.selectable_label(cur_name == pp.name, pp.name).clicked() {
                                    if let Some(pal) = theme::Palette::premade(pp.name) {
                                        theme::set_palette(ctx, pal);
                                    }
                                }
                            }
                        });
                    if ui.button("Random").on_hover_text("Roll a theme").clicked() {
                        theme::set_palette(ctx, theme::Palette::randomize());
                    }
                });
                ui.separator();

                // ---- Gradient -----------------------------------------------
                let mut gradient = theme::gradient_enabled();
                if ui
                    .checkbox(&mut gradient, "Gradient")
                    .on_hover_text("Discord-style background wash behind the chrome")
                    .changed()
                {
                    theme::set_gradient(ctx, gradient);
                }
                if !gradient {
                    return;
                }
                let mut cfg = theme::gradient_cfg();
                let mut dirty = false;

                // Live preview: TOP band = the raw wash, BOTTOM band = the
                // wash as perceived through the current frost — preview ==
                // background by construction (the smart-slot invariant).
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 26.0),
                    egui::Sense::hover(),
                );
                const SLICES: usize = 48;
                let mid_y = rect.top() + rect.height() * 0.5;
                for i in 0..SLICES {
                    let t0 = i as f32 / SLICES as f32;
                    let t1 = (i + 1) as f32 / SLICES as f32;
                    let tm = (t0 + t1) * 0.5;
                    let x0 = rect.left() + rect.width() * t0;
                    let x1 = rect.left() + rect.width() * t1;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, mid_y)),
                        0.0,
                        theme::ramp_sample(tm),
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(egui::pos2(x0, mid_y), egui::pos2(x1, rect.bottom())),
                        0.0,
                        theme::ramp_sample_frosted(tm),
                    );
                }
                ui.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.0, theme::muted()),
                    egui::StrokeKind::Middle,
                );
                ui.add_space(6.0);

                // Labels INLINE with their sliders: a label-above-slider stack
                // costs ~22px per control, and three of them was the difference
                // between the peg pickers fitting and not.
                ui.horizontal(|ui| {
                    ui.label("Direction");
                    dirty |= ui
                        .add(egui::Slider::new(&mut cfg.angle_deg, 0.0..=360.0).suffix(" deg"))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("Intensity");
                    dirty |= ui
                        .add(
                            egui::Slider::new(&mut cfg.intensity, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                        )
                        .changed();
                });
                // FROST: panel opacity over the wash. 0% = panels vanish,
                // background IS the preview ramp (WYSIWYG); high = solid chrome.
                let dark = theme::dark_mode();
                let mut frost = theme::frost(dark);
                ui.horizontal(|ui| {
                    ui.label("Frost");
                    if ui
                        .add(
                            egui::Slider::new(&mut frost, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                        )
                        .changed()
                    {
                        theme::set_frost(dark, frost);
                        // Frost only exists inside build_visuals' panel alpha, so
                        // it has to be re-applied to be seen at all.
                        theme::refresh_visuals(ctx);
                        dirty = true;
                    }
                });
                ui.separator();

                // Source mode chips: harmony / preset shelf / custom.
                ui.horizontal(|ui| {
                    if ui.selectable_label(cfg.preset == -1, "Harmony").clicked() {
                        cfg.preset = -1;
                        dirty = true;
                    }
                    if ui.selectable_label(cfg.preset >= 0, "Presets").clicked() {
                        if cfg.preset < 0 {
                            cfg.preset = 0;
                        }
                        dirty = true;
                    }
                    if ui.selectable_label(cfg.preset == -2, "Custom").clicked() {
                        cfg.preset = -2;
                        dirty = true;
                    }
                });

                if cfg.preset == -1 {
                    ui.horizontal(|ui| {
                        ui.label("Pegs");
                        for n in 1..=4u8 {
                            if ui.selectable_label(cfg.pegs == n, format!("{n}")).clicked() {
                                cfg.pegs = n;
                                dirty = true;
                            }
                        }
                    });
                    let rule_name =
                        color::HARMONY_RULES[(cfg.harmony as usize) % color::HARMONY_RULES.len()];
                    egui::ComboBox::from_id_salt("gradient_harmony")
                        .selected_text(rule_name)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for (i, rule) in color::HARMONY_RULES.iter().enumerate() {
                                if ui.selectable_label(cfg.harmony == i as u8, *rule).clicked() {
                                    cfg.harmony = i as u8;
                                    dirty = true;
                                }
                            }
                        });
                } else if cfg.preset >= 0 {
                    let preset_before = cfg.preset;
                    let n_presets = theme::GRADIENT_PRESETS.len() as i16;
                    ui.horizontal(|ui| {
                        // Prev/next flank the dropdown so you can ride
                        // through the shelf without opening it. ASCII
                        // arrows: Rajdhani has no glyph for U+2190/U+2192.
                        if ui.button("<").clicked() {
                            cfg.preset = (cfg.preset - 1).rem_euclid(n_presets);
                            dirty = true;
                        }
                        let preset_label = theme::GRADIENT_PRESETS
                            .get(cfg.preset as usize)
                            .map(|(n, _)| *n)
                            .unwrap_or("Preset");
                        egui::ComboBox::from_id_salt("gradient_preset")
                            .selected_text(preset_label)
                            .width(160.0)
                            .show_ui(ui, |ui| {
                                for (i, (name, _)) in theme::GRADIENT_PRESETS.iter().enumerate() {
                                    if ui.selectable_label(cfg.preset == i as i16, *name).clicked() {
                                        cfg.preset = i as i16;
                                        dirty = true;
                                    }
                                }
                            });
                        if ui.button(">").clicked() {
                            cfg.preset = (cfg.preset + 1).rem_euclid(n_presets);
                            dirty = true;
                        }
                    });
                    ui.checkbox(&mut sync, "Preset sets theme colors").on_hover_text(
                        "On: picking a preset rethemes the whole app (coherent). Off: preset only drives the wash over your accent's ground.",
                    );
                    // A preset SETS the primary color too when sync is on
                    // (accent = the preset's most-saturated stop) - otherwise
                    // a bold preset can fight an unrelated accent ground.
                    if sync && dirty && cfg.preset != preset_before {
                        if let Some((pname, stops)) = theme::GRADIENT_PRESETS.get(cfg.preset as usize) {
                            let rgbs: Vec<color::Rgb> =
                                stops.iter().filter_map(|h| color::hex_to_rgb(h)).collect();
                            if let Some(acc) = theme::most_saturated(&rgbs) {
                                let mut p = theme::Palette::from_accent(acc, theme::dark_mode());
                                p.name = (*pname).to_string();
                                theme::set_palette(ctx, p);
                            }
                        }
                    }
                    // Harmony and Presets derive their pegs, so there is nothing
                    // to edit — but you should still be able to SEE what stops
                    // the ramp landed on. Read-only swatches, same height as the
                    // spacer they replace so the window still doesn't jump.
                    read_only_pegs(ui);
                } else {
                    // Custom: your colors, your rules.
                    ui.horizontal(|ui| {
                        ui.label("Pegs");
                        for n in 1..=4u8 {
                            if ui.selectable_label(cfg.pegs == n, format!("{n}")).clicked() {
                                cfg.pegs = n;
                                dirty = true;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        // A color_edit_button draws its swatch at `interact_size`,
                        // whose default is a THIN SLIVER (same trap TrontSnap hit
                        // — it sizes the swatch 96x30 explicitly for this reason).
                        // The compact metrics this panel sets for its slider rows
                        // shrank it further, which is why this row rendered as an
                        // empty gap: the buttons were there, just ~0px of paint.
                        ui.spacing_mut().interact_size = egui::vec2(42.0, 24.0);
                        // SLOT 0 IS THE ACCENT (linked): editing it rethemes
                        // the app; it always participates in the ramp.
                        // Slots 1..N are free pegs.
                        // READ THE SEED, NOT THE INK. `theme::cyan()` is the
                        // ENFORCED accessor (already walked for readability
                        // against the panel), which is precisely what the rule
                        // on ACCENT_SEED forbids binding a picker to. The
                        // renderer pushes `accent_seed()` as peg 0, so painting
                        // this swatch from `cyan()` made the swatch and the
                        // actual ramp disagree by construction.
                        let acc = theme::accent_seed();
                        let mut rgb = acc;
                        if ui
                            .color_edit_button_srgb(&mut rgb)
                            .on_hover_text("Slot 1 = your theme accent (linked)")
                            .changed()
                        {
                            let dark = theme::dark_mode();
                            theme::set_palette(ctx, theme::Palette::from_accent(rgb, dark));
                            dirty = true;
                        }
                        // srgb (no alpha) pickers: same picker, smaller and
                        // simpler - and physically incapable of showing a
                        // dead Blending/Additive row.
                        for i in 1..(cfg.pegs.clamp(1, 4) as usize) {
                            if ui.color_edit_button_srgb(&mut cfg.custom[i]).changed() {
                                dirty = true;
                            }
                        }
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Magic")
                        .on_hover_text("Roll a colormagic flavor palette into custom pegs")
                        .clicked()
                    {
                        let mut rng = color::Rng::from_clock();
                        let kind = color::PaletteKind::ALL[rng.range(0, 5) as usize];
                        let cols = color::generate_random_palette(kind, 4, &mut rng);
                        for (i, h) in cols.iter().take(4).enumerate() {
                            cfg.custom[i] = color::hsl_to_rgb(h.h, h.s, h.l);
                        }
                        cfg.preset = -2;
                        cfg.pegs = if rng.range(0, 1) == 0 { 3 } else { 4 };
                        cfg.angle_deg = rng.range(0, 359) as f32;
                        dirty = true;
                    }
                    if ui
                        .button("Reset")
                        .on_hover_text(
                            "The wayback machine: restores the known-good look (intensity 45%, frost 85%/59%, harmony pegs)",
                        )
                        .clicked()
                    {
                        cfg = theme::GradientCfg::default();
                        // Reset MUST restore frost too, or it's a trap: the
                        // ramp resets but frost stays wherever it was, so the
                        // sweet spot the defaults describe is unreachable.
                        theme::set_frost(true, 0.85);
                        theme::set_frost(false, 0.59);
                        theme::refresh_visuals(ctx);
                        dirty = true;
                    }
                });

                if dirty {
                    theme::set_gradient_cfg(cfg);
                }
                });
            });
        if !open {
            show_win = false;
        }
    }

    app.show_theme_window = show_win;
    app.gradient_preset_sync = sync;
}
