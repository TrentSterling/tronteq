//! The About window. Extracted from `main.rs`; takes only the fields it needs
//! (the open flag + the lazily-loaded face texture) rather than all of `App`.

use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

use eframe::egui;

use crate::theme;

/// `(exe path, file modified time)` of the running binary.
///
/// The stale-build lesson from the theme audit: an old install can pass for
/// the current one for days when nothing in the UI says which executable is
/// running. Read from the exe itself rather than baked in by `build.rs`, whose
/// timestamp would go stale across incremental builds. Cached because About
/// repaints every frame while open.
fn binary_identity() -> &'static (String, String) {
    static ID: OnceLock<(String, String)> = OnceLock::new();
    ID.get_or_init(|| {
        let exe = std::env::current_exe().ok();
        let path = exe
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".into());
        let modified = exe
            .and_then(|p| std::fs::metadata(p).ok())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| format_utc(d.as_secs()))
            .unwrap_or_else(|| "unknown".into());
        (path, modified)
    })
}

/// `YYYY-MM-DD HH:MM UTC` from Unix seconds (Hinnant's civil-from-days; no
/// date crate in this app).
fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3_600, rem % 3_600 / 60)
}

/// Lazy-load the app icon (bars-over-face) as a texture. Shared by the About
/// window and the toolbar wordmark.
pub fn icon_texture(
    ctx: &egui::Context,
    icon: &mut Option<egui::TextureHandle>,
) -> Option<egui::TextureHandle> {
    if icon.is_none() {
        if let Ok(img) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [img.width as usize, img.height as usize],
                &img.rgba,
            );
            *icon = Some(ctx.load_texture("tronteq-about", color, egui::TextureOptions::LINEAR));
        }
    }
    icon.clone()
}

pub fn show(
    ctx: &egui::Context,
    show_about: &mut bool,
    icon: &mut Option<egui::TextureHandle>,
) {
    let mut about_open = *show_about;
    if !about_open {
        return;
    }

    let icon_tex = icon_texture(ctx, icon);
    let mut close_about = false;

    egui::Window::new("About TrontEQ")
        .collapsible(false)
        .resizable(false)
        // Centered on open but NOT `.anchor()`ed — anchored windows are pinned
        // and can't be dragged out of the way.
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        // Never taller than the viewport (at 200% zoom the content used to
        // overflow both screen edges with no way to scroll); scroll instead.
        .max_height(ctx.content_rect().height() * 0.85)
        .vscroll(true)
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
                let (exe_path, exe_modified) = binary_identity();
                ui.label(
                    egui::RichText::new(format!("Binary modified {exe_modified}"))
                        .color(theme::muted())
                        .small(),
                );
                ui.label(egui::RichText::new(exe_path).color(theme::muted()).small());
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
                ui.label(
                    egui::RichText::new("ideas + inspiration: aFoolsDuty")
                        .color(theme::muted())
                        .small(),
                );
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

#[cfg(test)]
mod tests {
    use super::format_utc;

    #[test]
    fn utc_formatter_matches_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00 UTC"); // leap day
        assert_eq!(format_utc(1_000_000_000), "2001-09-09 01:46 UTC");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(format_utc(2_147_483_647), "2038-01-19 03:14 UTC");
    }
}
