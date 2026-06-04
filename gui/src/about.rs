//! The About window. Extracted from `main.rs`; takes only the fields it needs
//! (the open flag + the lazily-loaded face texture) rather than all of `App`.

use eframe::egui;

use crate::theme;

pub fn show(
    ctx: &egui::Context,
    show_about: &mut bool,
    icon: &mut Option<egui::TextureHandle>,
) {
    let mut about_open = *show_about;
    if !about_open {
        return;
    }

    // Lazy-load the eyepatch face/icon texture on first open.
    if icon.is_none() {
        if let Ok(img) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [img.width as usize, img.height as usize],
                &img.rgba,
            );
            *icon = Some(ctx.load_texture("tronteq-about", color, egui::TextureOptions::LINEAR));
        }
    }
    let icon_tex = icon.clone();
    let mut close_about = false;

    egui::Window::new("About TrontEQ")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut about_open)
        .show(ctx, |ui| {
            ui.set_max_width(380.0);
            ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                if let Some(tex) = &icon_tex {
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(
                        tex.id(),
                        egui::vec2(76.0, 76.0),
                    )));
                    ui.add_space(8.0);
                }
                ui.label(
                    egui::RichText::new("TrontEQ")
                        .family(egui::FontFamily::Name("display".into()))
                        .color(theme::cyan())
                        .strong()
                        .size(30.0),
                );
                ui.label(
                    egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).color(theme::muted()),
                );
                ui.add_space(8.0);
                ui.label("Zero-latency Windows system EQ + dynamics");
                ui.label(
                    egui::RichText::new(
                        "Runs as a real Audio Processing Object inside the Windows audio \
                         engine, so it adds no latency to your sound.",
                    )
                    .color(theme::muted())
                    .small(),
                );
                ui.add_space(10.0);
            });

            ui.separator();
            ui.add_space(6.0);
            ui.label(egui::RichText::new("SIGNAL CHAIN").color(theme::cyan()).strong());
            ui.label(
                egui::RichText::new("Preamp  >  8-band EQ  >  Compressor  >  Limiter  >  Auto-loudness")
                    .color(theme::muted())
                    .small(),
            );
            ui.add_space(10.0);

            ui.label(egui::RichText::new("CONTROLS").color(theme::cyan()).strong());
            ui.add_space(2.0);
            egui::Grid::new("about_controls")
                .num_columns(2)
                .spacing([18.0, 5.0])
                .show(ui, |ui| {
                    let mut row = |a: &str, b: &str| {
                        ui.label(egui::RichText::new(a).color(theme::text()));
                        ui.label(egui::RichText::new(b).color(theme::muted()));
                        ui.end_row();
                    };
                    row("Drag node", "Gain");
                    row("Shift-drag / scroll", "Q / bandwidth");
                    row("Ctrl + drag", "Frequency");
                    row("Double-click node", "Flatten band");
                    row("Right-click node", "Cycle filter type");
                    row("Drag / scroll knob", "Adjust value");
                    row("Viz toggles", "Stack any overlays");
                });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    ui.hyperlink_to("tront.xyz", "https://tront.xyz");
                    ui.label(egui::RichText::new("·").color(theme::muted()));
                    ui.hyperlink_to("GitHub", "https://github.com/TrentSterling");
                    ui.label(egui::RichText::new("·").color(theme::muted()));
                    ui.hyperlink_to("Discord", "https://tront.xyz/discord/");
                });
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("by Trent Sterling  ·  free + open source")
                        .color(theme::muted())
                        .small(),
                );
                ui.label(egui::RichText::new("Built with Rust + egui").color(theme::muted()).small());
                ui.add_space(8.0);
                if ui.button("Close").clicked() {
                    close_about = true;
                }
                ui.add_space(2.0);
            });
        });

    if close_about {
        about_open = false;
    }
    *show_about = about_open;
}
