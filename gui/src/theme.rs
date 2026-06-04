//! tront.xyz "electric cyan" theme — near-black surfaces, a faint cyan grid, and
//! neon-cyan accents with glow. Palette lifted from `assets/css/tront-themes.css`.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, Stroke};

pub const BG: Color32 = Color32::from_rgb(8, 13, 20); // deepest (canvas)
pub const PANEL: Color32 = Color32::from_rgb(11, 18, 27); // toolbars/panels
pub const PANEL2: Color32 = Color32::from_rgb(16, 27, 39); // widgets
pub const CYAN: Color32 = Color32::from_rgb(0, 224, 255); // primary accent
pub const CYAN_SOFT: Color32 = Color32::from_rgb(120, 235, 255);
pub const CYAN_DIM: Color32 = Color32::from_rgb(0, 150, 184);
pub const TEXT: Color32 = Color32::from_rgb(216, 240, 248);
pub const MUTED: Color32 = Color32::from_rgb(110, 150, 168);
pub const OK: Color32 = Color32::from_rgb(90, 230, 200);

// Light-mode chrome palette. The EQ canvas (curve / spectrum / waterfall) and the
// output meter stay dark in BOTH modes (a "scope" display), so only the chrome
// (toolbar, side panel, About, buttons, text) swaps here.
pub const LIGHT_BG: Color32 = Color32::from_rgb(236, 240, 245);
pub const LIGHT_PANEL: Color32 = Color32::from_rgb(224, 230, 237);
pub const LIGHT_PANEL2: Color32 = Color32::from_rgb(205, 214, 224);
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(22, 32, 42);
pub const LIGHT_MUTED: Color32 = Color32::from_rgb(92, 110, 126);
pub const LIGHT_CYAN: Color32 = Color32::from_rgb(0, 128, 160); // deeper accent for contrast on white
pub const LIGHT_OK: Color32 = Color32::from_rgb(20, 150, 118);

static DARK: AtomicBool = AtomicBool::new(true);

pub fn dark_mode() -> bool {
    DARK.load(Ordering::Relaxed)
}

// Runtime chrome accessors (dark default / light variant). Use these for any
// chrome color; canvas code keeps using the fixed dark consts above.
pub fn bg() -> Color32 {
    if dark_mode() { BG } else { LIGHT_BG }
}
pub fn panel() -> Color32 {
    if dark_mode() { PANEL } else { LIGHT_PANEL }
}
pub fn panel2() -> Color32 {
    if dark_mode() { PANEL2 } else { LIGHT_PANEL2 }
}
pub fn text() -> Color32 {
    if dark_mode() { TEXT } else { LIGHT_TEXT }
}
pub fn muted() -> Color32 {
    if dark_mode() { MUTED } else { LIGHT_MUTED }
}
pub fn cyan() -> Color32 {
    if dark_mode() { CYAN } else { LIGHT_CYAN }
}
pub fn ok() -> Color32 {
    if dark_mode() { OK } else { LIGHT_OK }
}

fn border(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(0, 224, 255, a)
}

/// HSV -> Color32. h, s, v in 0..=1. Used for the rainbow-skittles theme.
pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let h = (h.fract() + 1.0).fract() * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

pub fn apply(ctx: &egui::Context) {
    // Embed Rajdhani (techy squarish HUD face) as the default UI font so the app
    // reads like product UI, not a debug panel. SemiBold powers headings/wordmark.
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "rajdhani".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/Rajdhani-Medium.ttf"
        ))),
    );
    fonts.font_data.insert(
        "rajdhani-sb".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/Rajdhani-SemiBold.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "rajdhani".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "rajdhani".to_owned());
    fonts.families.insert(
        egui::FontFamily::Name("display".into()),
        vec!["rajdhani-sb".to_owned(), "rajdhani".to_owned()],
    );
    ctx.set_fonts(fonts);

    set_mode(ctx, dark_mode());

    let mut style = (*ctx.style()).clone();
    // Rajdhani is condensed, so nudge sizes up for legibility.
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::new(20.0, FontFamily::Name("display".into()))),
        (TextStyle::Body, FontId::new(15.5, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(15.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace)),
        (TextStyle::Small, FontId::new(12.5, FontFamily::Proportional)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(9.0, 7.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    // One uniform interactive height for every button/checkbox/combo; x=0 so
    // buttons size to content width (no chunky min-width on tiny ↺ buttons).
    style.spacing.interact_size = egui::vec2(0.0, 26.0);
    // Labels aren't selectable -> normal arrow cursor over text (not the I-beam).
    style.interaction.selectable_labels = false;
    style.visuals.clip_rect_margin = 0.0;
    ctx.set_style(style);
}

/// Switch dark/light at runtime and re-apply chrome visuals (fonts + spacing are
/// mode-independent and stay as set by `apply`).
pub fn set_mode(ctx: &egui::Context, dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
    ctx.set_visuals(build_visuals());
}

/// egui Visuals for the current mode. Chrome only; the EQ canvas paints itself
/// with the fixed dark consts regardless of mode.
fn build_visuals() -> egui::Visuals {
    let dark = dark_mode();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let fg_strong = if dark { Color32::WHITE } else { LIGHT_TEXT };
    let hover = if dark {
        Color32::from_rgb(18, 42, 56)
    } else {
        Color32::from_rgb(214, 226, 236)
    };
    let accent_dim = if dark { CYAN_DIM } else { LIGHT_CYAN };

    v.panel_fill = panel();
    v.window_fill = panel();
    v.window_stroke = Stroke::new(1.0, border(40));
    v.extreme_bg_color = bg();
    v.faint_bg_color = panel2();
    v.override_text_color = Some(text());
    v.hyperlink_color = cyan();
    v.selection.bg_fill = cyan().gamma_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0, cyan());

    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, muted());
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border(28));

    v.widgets.inactive.bg_fill = panel2();
    v.widgets.inactive.weak_bg_fill = panel2();
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text());
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, border(60));

    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, fg_strong);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, accent_dim);

    v.widgets.active.bg_fill = cyan().gamma_multiply(0.22);
    v.widgets.active.weak_bg_fill = cyan().gamma_multiply(0.22);
    v.widgets.active.fg_stroke = Stroke::new(1.0, fg_strong);
    v.widgets.active.bg_stroke = Stroke::new(1.5, cyan());

    v.clip_rect_margin = 0.0;
    v
}
