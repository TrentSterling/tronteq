//! Dynamic theme system. v0.2 had one hardcoded electric-cyan dark palette and
//! a light variant; as of v0.5 the whole app (chrome AND canvas) reads from a
//! runtime [`Palette`] — built-ins, 32 premade palettes, and a colormagic
//! randomizer that derives themes which are always readable (WCAG contrast
//! rules from `color.rs`, the TrontColors math vendored via Boxel).
//!
//! Every color in the app flows through the accessor fns below, so swapping
//! the palette restyles everything live. The original electric-cyan values are
//! preserved verbatim as the default built-in.

use std::sync::{LazyLock, RwLock};

use eframe::egui::{self, Color32, Stroke};

use crate::color::{self, Rgb};

// Original tront.xyz electric-cyan values (kept as consts: they seed the two
// classic built-ins and stay available for reference).
pub const BG: Color32 = Color32::from_rgb(8, 13, 20); // deepest (canvas)
pub const PANEL: Color32 = Color32::from_rgb(11, 18, 27); // toolbars/panels
pub const PANEL2: Color32 = Color32::from_rgb(16, 27, 39); // widgets
pub const CYAN: Color32 = Color32::from_rgb(0, 224, 255); // primary accent
pub const CYAN_DIM: Color32 = Color32::from_rgb(0, 150, 184);
pub const TEXT: Color32 = Color32::from_rgb(216, 240, 248);
pub const MUTED: Color32 = Color32::from_rgb(110, 150, 168);
pub const OK: Color32 = Color32::from_rgb(90, 230, 200);

pub const LIGHT_BG: Color32 = Color32::from_rgb(236, 240, 245);
pub const LIGHT_PANEL: Color32 = Color32::from_rgb(224, 230, 237);
pub const LIGHT_PANEL2: Color32 = Color32::from_rgb(205, 214, 224);
pub const LIGHT_TEXT: Color32 = Color32::from_rgb(22, 32, 42);
pub const LIGHT_MUTED: Color32 = Color32::from_rgb(92, 110, 126);
pub const LIGHT_CYAN: Color32 = Color32::from_rgb(0, 128, 160);
pub const LIGHT_OK: Color32 = Color32::from_rgb(20, 150, 118);
pub const LIGHT_CANVAS_BG: Color32 = Color32::from_rgb(233, 238, 243);

/// A fully-resolved theme: chrome + canvas + viz colors. Built-ins are
/// hand-tuned; everything else derives from a color list via colormagic's
/// `generate_auto_theme` + contrast enforcement.
#[derive(Clone)]
pub struct Palette {
    pub name: String,
    /// Source hex list for persistence (empty = built-in, rebuilt by name).
    pub source: Vec<String>,
    pub dark: bool,

    // Chrome
    pub bg: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_dim: Color32,
    pub ok: Color32,

    // Canvas + viz
    pub canvas_bg: Color32,
    pub ink: Color32, // node rings / peak caps / meter ticks
    pub viz_accent: Color32,
    pub viz_cap: Color32,
    pub glow: Color32, // glow-line core family
    pub rainbow_s: f32,
    pub rainbow_v: f32,
}

fn c32(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}
fn rgb_of(c: Color32) -> Rgb {
    [c.r(), c.g(), c.b()]
}

impl Palette {
    /// The classic: tront.xyz electric cyan on near-black. Exact v0.2 values.
    pub fn electric_cyan() -> Palette {
        Palette {
            name: "Electric Cyan".into(),
            source: Vec::new(),
            dark: true,
            bg: BG,
            panel: PANEL,
            panel2: PANEL2,
            text: TEXT,
            muted: MUTED,
            accent: CYAN,
            accent_dim: CYAN_DIM,
            ok: OK,
            canvas_bg: BG,
            ink: Color32::WHITE,
            viz_accent: CYAN,
            viz_cap: Color32::from_rgb(150, 245, 255),
            glow: Color32::from_rgb(150, 240, 255),
            rainbow_s: 0.85,
            rainbow_v: 1.0,
        }
    }

    /// The classic light mode ("Paper"). Exact v0.3 values.
    pub fn paper() -> Palette {
        Palette {
            name: "Paper".into(),
            source: Vec::new(),
            dark: false,
            bg: LIGHT_BG,
            panel: LIGHT_PANEL,
            panel2: LIGHT_PANEL2,
            text: LIGHT_TEXT,
            muted: LIGHT_MUTED,
            accent: LIGHT_CYAN,
            accent_dim: LIGHT_CYAN,
            ok: LIGHT_OK,
            canvas_bg: LIGHT_CANVAS_BG,
            ink: Color32::from_rgb(22, 32, 42),
            viz_accent: Color32::from_rgb(0, 122, 156),
            viz_cap: Color32::from_rgb(0, 88, 116),
            glow: Color32::from_rgb(0, 96, 128),
            rainbow_s: 0.95,
            rainbow_v: 0.72,
        }
    }

    /// Hand-tuned synthwave: hot magenta on deep purple-black.
    pub fn synthwave() -> Palette {
        Palette {
            name: "Synthwave".into(),
            source: Vec::new(),
            dark: true,
            bg: Color32::from_rgb(16, 6, 26),
            panel: Color32::from_rgb(22, 10, 36),
            panel2: Color32::from_rgb(34, 16, 52),
            text: Color32::from_rgb(244, 226, 255),
            muted: Color32::from_rgb(150, 118, 178),
            accent: Color32::from_rgb(255, 62, 200),
            accent_dim: Color32::from_rgb(170, 40, 134),
            ok: Color32::from_rgb(62, 242, 200),
            canvas_bg: Color32::from_rgb(10, 3, 18),
            ink: Color32::from_rgb(255, 240, 255),
            viz_accent: Color32::from_rgb(255, 62, 200),
            viz_cap: Color32::from_rgb(255, 170, 235),
            glow: Color32::from_rgb(255, 150, 230),
            rainbow_s: 0.85,
            rainbow_v: 1.0,
        }
    }

    /// Derive a full theme from a color list via colormagic: AutoTheme picks
    /// bg/surface/primary, then contrast rules guarantee readable text and a
    /// viz accent that pops on the canvas — randomize all day, stays pretty.
    pub fn from_colors(name: &str, colors: &[Rgb]) -> Option<Palette> {
        let t = color::generate_auto_theme(colors)?;
        let dark = t.is_dark;
        let bg = t.bg;
        let surface = t.surface;

        // Canvas sits a step deeper than the chrome on dark themes (scope
        // look), a step brighter on light ones (paper look).
        let canvas = if dark {
            color::mix_colors(bg, [0, 0, 0], 0.45)
        } else {
            color::mix_colors(bg, [255, 255, 255], 0.55)
        };
        let ink = color::contrast_color(canvas);

        // Primary accent must read against BOTH the panel and the canvas;
        // walk lightness until it does (bounded).
        let mut prim = t.primary;
        let mut guard = 0;
        while (color::contrast_ratio(prim, canvas) < 2.6
            || color::contrast_ratio(prim, surface) < 2.2)
            && guard < 14
        {
            let h = color::rgb_to_hsl(prim);
            let l = if dark { (h.l + 6.0).min(92.0) } else { (h.l - 6.0).max(8.0) };
            prim = color::hsl_to_rgb(h.h, h.s.max(45.0), l);
            guard += 1;
        }

        // Text: WCAG 4.5 on the panel or it gets replaced outright.
        let mut text = t.text;
        if color::contrast_ratio(text, surface) < 4.5 {
            text = color::contrast_color(surface);
        }
        let muted = color::mix_colors(text, surface, 0.45);
        let panel2 =
            color::mix_colors(surface, if dark { [255, 255, 255] } else { [0, 0, 0] }, 0.07);

        Some(Palette {
            name: name.into(),
            source: colors.iter().map(|&c| color::rgb_to_hex(c)).collect(),
            dark,
            bg: c32(bg),
            panel: c32(surface),
            panel2: c32(panel2),
            text: c32(text),
            muted: c32(muted),
            accent: c32(prim),
            accent_dim: c32(color::mix_colors(prim, bg, 0.45)),
            ok: c32(t.success),
            canvas_bg: c32(canvas),
            ink: c32(ink),
            viz_accent: c32(prim),
            viz_cap: c32(color::mix_colors(prim, ink, 0.5)),
            glow: c32(color::mix_colors(prim, ink, 0.35)),
            rainbow_s: if dark { 0.85 } else { 0.95 },
            rainbow_v: if dark { 1.0 } else { 0.72 },
        })
    }

    /// Look up a premade palette by name and derive a theme from it.
    pub fn premade(name: &str) -> Option<Palette> {
        let p = color::PREMADE_PALETTES.iter().find(|p| p.name == name)?;
        let rgb: Vec<Rgb> = p.colors.iter().filter_map(|h| color::hex_to_rgb(h)).collect();
        Palette::from_colors(name, &rgb)
    }

    /// Roll a new theme: random flavor palette, random harmony spread, or a
    /// random premade — all funneled through the same contrast-safe deriver.
    pub fn randomize() -> Palette {
        let mut rng = color::Rng::from_clock();
        let pick = rng.range(0, 2);
        let derived = match pick {
            0 => {
                let kind = color::PaletteKind::ALL[rng.range(0, 5) as usize];
                let cols = color::generate_random_palette(kind, 5, &mut rng);
                let rgb: Vec<Rgb> =
                    cols.iter().map(|h| color::hsl_to_rgb(h.h, h.s, h.l)).collect();
                Palette::from_colors(&format!("Random {}", kind.label()), &rgb)
            }
            1 => {
                let base = color::Hsl::new(
                    rng.range(0, 359) as f32,
                    rng.range(55, 95) as f32,
                    rng.range(28, 62) as f32,
                );
                let rule = color::HARMONY_RULES[rng.range(0, 6) as usize];
                let cols = color::generate_harmony(base, rule);
                let rgb: Vec<Rgb> =
                    cols.iter().map(|h| color::hsl_to_rgb(h.h, h.s, h.l)).collect();
                Palette::from_colors(&format!("Random {rule}"), &rgb)
            }
            _ => {
                let n = color::PREMADE_PALETTES.len() as i32;
                let p = &color::PREMADE_PALETTES[rng.range(0, n - 1) as usize];
                let rgb: Vec<Rgb> =
                    p.colors.iter().filter_map(|h| color::hex_to_rgb(h)).collect();
                Palette::from_colors(p.name, &rgb)
            }
        };
        derived.unwrap_or_else(Palette::electric_cyan)
    }

    /// Resolve a persisted theme: built-in by name, else rebuild from the
    /// stored source colors, else fall back to the classic.
    pub fn resolve(name: &str, source: &[String]) -> Palette {
        match name {
            "Electric Cyan" => return Palette::electric_cyan(),
            "Paper" => return Palette::paper(),
            "Synthwave" => return Palette::synthwave(),
            _ => {}
        }
        if !source.is_empty() {
            let rgb: Vec<Rgb> = source.iter().filter_map(|h| color::hex_to_rgb(h)).collect();
            if let Some(p) = Palette::from_colors(name, &rgb) {
                return p;
            }
        }
        // Unknown name with no source: try the premade list, else classic.
        Palette::premade(name).unwrap_or_else(Palette::electric_cyan)
    }
}

static CURRENT: LazyLock<RwLock<Palette>> =
    LazyLock::new(|| RwLock::new(Palette::electric_cyan()));

/// Discord-style background wash toggle. Default ON; persisted in
/// settings.json like the palette itself (see `AppSettings::gradient`).
static GRADIENT: LazyLock<RwLock<bool>> = LazyLock::new(|| RwLock::new(true));

pub fn gradient_enabled() -> bool {
    *GRADIENT.read().unwrap()
}

/// Flip the gradient toggle and re-apply egui visuals (panel translucency
/// follows it immediately).
pub fn set_gradient(ctx: &egui::Context, on: bool) {
    *GRADIENT.write().unwrap() = on;
    ctx.set_visuals(build_visuals());
}

/// Swap the live palette and re-apply egui visuals.
pub fn set_palette(ctx: &egui::Context, p: Palette) {
    *CURRENT.write().unwrap() = p;
    ctx.set_visuals(build_visuals());
}

/// Current palette snapshot (name + persistence fields; cheap enough outside
/// per-frame hot paths).
pub fn current() -> Palette {
    CURRENT.read().unwrap().clone()
}

pub fn dark_mode() -> bool {
    CURRENT.read().unwrap().dark
}

/// Legacy toggle: swaps between the two classic built-ins. (Arbitrary themes
/// are picked in SETUP; this stays as the quick toolbar flip.)
pub fn set_mode(ctx: &egui::Context, dark: bool) {
    set_palette(ctx, if dark { Palette::electric_cyan() } else { Palette::paper() });
}

// ---- Chrome accessors --------------------------------------------------------
// bg/panel are unused right now (build_visuals reads the palette directly) but
// they complete the accessor API; keep them callable.

#[allow(dead_code)]
pub fn bg() -> Color32 {
    CURRENT.read().unwrap().bg
}
#[allow(dead_code)]
pub fn panel() -> Color32 {
    CURRENT.read().unwrap().panel
}
pub fn panel2() -> Color32 {
    CURRENT.read().unwrap().panel2
}
pub fn text() -> Color32 {
    CURRENT.read().unwrap().text
}
pub fn muted() -> Color32 {
    CURRENT.read().unwrap().muted
}
pub fn cyan() -> Color32 {
    CURRENT.read().unwrap().accent
}
pub fn ok() -> Color32 {
    CURRENT.read().unwrap().ok
}

fn border(a: u8) -> Color32 {
    let c = CURRENT.read().unwrap().accent;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

// ---- Canvas accessors ----------------------------------------------------------

pub fn canvas_bg() -> Color32 {
    CURRENT.read().unwrap().canvas_bg
}
pub fn canvas_grid() -> Color32 {
    let p = CURRENT.read().unwrap();
    let c = if p.dark { p.accent } else { p.ink };
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), if p.dark { 15 } else { 26 })
}
pub fn canvas_zero() -> Color32 {
    let p = CURRENT.read().unwrap();
    let c = if p.dark { p.accent } else { p.ink };
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), if p.dark { 55 } else { 85 })
}
pub fn canvas_label() -> Color32 {
    CURRENT.read().unwrap().muted
}
/// "White" accents (node rings, peak caps, meter ticks): ink on the canvas.
pub fn ink(a: u8) -> Color32 {
    let c = CURRENT.read().unwrap().ink;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
pub fn glow_core(a: u8) -> Color32 {
    let c = CURRENT.read().unwrap().glow;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
pub fn tooltip_bg() -> Color32 {
    let p = CURRENT.read().unwrap();
    if p.dark {
        let b = rgb_of(p.bg);
        let m = color::mix_colors(b, [0, 0, 0], 0.35);
        Color32::from_rgba_unmultiplied(m[0], m[1], m[2], 230)
    } else {
        Color32::from_rgba_unmultiplied(250, 252, 255, 240)
    }
}
pub fn tooltip_text() -> Color32 {
    CURRENT.read().unwrap().text
}
pub fn inset_bg() -> Color32 {
    let p = CURRENT.read().unwrap();
    if p.dark {
        let b = rgb_of(p.canvas_bg);
        let m = color::mix_colors(b, [0, 0, 0], 0.4);
        Color32::from_rgba_unmultiplied(m[0], m[1], m[2], 150)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 170)
    }
}
pub fn viz_accent() -> Color32 {
    CURRENT.read().unwrap().viz_accent
}
pub fn viz_cap() -> Color32 {
    CURRENT.read().unwrap().viz_cap
}
pub fn meter_track() -> Color32 {
    let p = CURRENT.read().unwrap();
    if p.dark { p.canvas_bg } else { p.panel2 }
}
/// Rainbow sweep tuned per palette (neon on dark, ink-dense on light).
pub fn viz_hsv(h: f32) -> Color32 {
    let p = CURRENT.read().unwrap();
    hsv(h, p.rainbow_s, p.rainbow_v)
}
pub fn viz_hsv_cap(h: f32) -> Color32 {
    if dark_mode() { hsv(h, 0.5, 1.0) } else { hsv(h, 0.9, 0.55) }
}

/// HSV -> Color32. h, s, v in 0..=1. The rainbow workhorse.
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

    ctx.set_visuals(build_visuals());

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

/// egui Visuals derived entirely from the current palette.
fn build_visuals() -> egui::Visuals {
    let p = CURRENT.read().unwrap().clone();
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let fg_strong = if p.dark { Color32::WHITE } else { p.text };
    let hover = c32(color::mix_colors(rgb_of(p.panel2), rgb_of(p.accent), 0.14));
    let accent_dim = p.accent_dim;

    // Gradient ON: panel/window fills go slightly translucent so the
    // background wash reads through the chrome. OFF: fully solid, the
    // pre-gradient look. The EQ canvas + OUT meter paint their own opaque
    // rects over this (canvas_bg()/meter_track()) so they stay solid dark
    // in both cases (house rule) regardless of this alpha.
    let panel_alpha: u8 = if gradient_enabled() { 216 } else { 255 };
    let panel_fill =
        Color32::from_rgba_unmultiplied(p.panel.r(), p.panel.g(), p.panel.b(), panel_alpha);
    v.panel_fill = panel_fill;
    v.window_fill = panel_fill;
    v.window_stroke = Stroke::new(1.0, border(40));
    v.extreme_bg_color = p.bg;
    v.faint_bg_color = p.panel2;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0, p.accent);

    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.muted);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border(28));

    v.widgets.inactive.bg_fill = p.panel2;
    v.widgets.inactive.weak_bg_fill = p.panel2;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, border(60));

    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, fg_strong);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, accent_dim);

    v.widgets.active.bg_fill = p.accent.gamma_multiply(0.22);
    v.widgets.active.weak_bg_fill = p.accent.gamma_multiply(0.22);
    v.widgets.active.fg_stroke = Stroke::new(1.0, fg_strong);
    v.widgets.active.bg_stroke = Stroke::new(1.5, p.accent);

    v.clip_rect_margin = 0.0;
    v
}

// ---- background gradient ---------------------------------------------------

/// Discord-style dynamic background wash, derived from the live palette.
/// TrontStack canonical recipe (same shape across every app, see
/// SpaceView's `theme::gradient_colors` — this is a straight port):
///   top-left = bg -> toward accent (strongest)
///   top-right = bg -> toward accent (half as strong)
///   bottom-left = bg -> toward a deeper/darker accent (hue +40deg, value halved)
///   bottom-right = bg darkened
/// Light mode blends the same way but pulls back toward white so it stays
/// airy instead of muddy.
///
/// BLEND scales the baseline 14/7/8/6% mix up so the wash still reads once
/// composited under the ~85%-opaque panel fill (`build_visuals`'s
/// `panel_alpha`) — same tuning SpaceView landed on.
const GRADIENT_BLEND: f32 = 8.0;

pub fn gradient_colors(p: &Palette) -> [Color32; 4] {
    let bg = rgb_of(p.bg);
    let accent = rgb_of(p.accent);
    let a = color::rgb_to_hsl(accent);

    if p.dark {
        let deep = color::hsl_to_rgb((a.h + 40.0).rem_euclid(360.0), a.s, (a.l * 0.5).max(6.0));
        [
            c32(color::mix_colors(bg, accent, (0.14 * GRADIENT_BLEND).min(0.85))), // top-left
            c32(color::mix_colors(bg, accent, (0.07 * GRADIENT_BLEND).min(0.85))), // top-right
            c32(color::mix_colors(bg, deep, (0.08 * GRADIENT_BLEND).min(0.85))),   // bottom-left
            c32(color::mix_colors(bg, [0, 0, 0], (0.06 * GRADIENT_BLEND).min(0.85))), // bottom-right
        ]
    } else {
        let white = [255u8, 255, 255];
        let deep = color::hsl_to_rgb(
            (a.h + 40.0).rem_euclid(360.0),
            (a.s * 0.7).max(20.0),
            (a.l * 1.25).min(85.0),
        );
        // Two-stage: blend toward accent/deep first (BLEND-scaled, same as
        // dark mode), then pull most of the way back toward white so it
        // stays airy instead of muddy.
        let tl = color::mix_colors(color::mix_colors(bg, accent, (0.10 * GRADIENT_BLEND).min(0.7)), white, 0.45);
        let tr = color::mix_colors(color::mix_colors(bg, accent, (0.05 * GRADIENT_BLEND).min(0.7)), white, 0.55);
        let bl = color::mix_colors(color::mix_colors(bg, deep, (0.06 * GRADIENT_BLEND).min(0.7)), white, 0.40);
        let br = color::mix_colors(bg, white, (0.11 * GRADIENT_BLEND).min(0.7));
        [c32(tl), c32(tr), c32(bl), c32(br)]
    }
}

/// Paint the gradient as one 4-vertex mesh into the background layer, before
/// any panel draws. Cost is a single quad — negligible. No-op when the
/// toggle is off.
pub fn paint_gradient(ctx: &egui::Context) {
    if !gradient_enabled() {
        return;
    }
    let rect = ctx.screen_rect();
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let p = current();
    let [tl, tr, bl, br] = gradient_colors(&p);

    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), tl);
    mesh.colored_vertex(rect.right_top(), tr);
    mesh.colored_vertex(rect.left_bottom(), bl);
    mesh.colored_vertex(rect.right_bottom(), br);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);

    ctx.layer_painter(egui::LayerId::background()).add(egui::Shape::mesh(mesh));
}
