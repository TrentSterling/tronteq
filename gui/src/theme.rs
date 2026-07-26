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

impl Palette {
    /// THE READABILITY GUARANTEE. Every palette that reaches the screen passes
    /// through here, whatever built it: a hand-written built-in, a premade, a
    /// derived accent, or Random.
    ///
    /// Why it is needed: `muted` was a blind `mix_colors(text, surface, 0.45)`
    /// with no contrast check, and it paints every small caption in the app
    /// (`widgets.noninteractive.fg_stroke`) - the knob labels, the section
    /// blurbs, the hints. On a light or low-contrast ground that mix washed out
    /// to nearly invisible grey. Now both text roles are walked (hue preserved)
    /// until APCA says they clear their floor against the panel they sit on,
    /// with pure black/white as the unconditional backstop.
    ///
    /// `panel2` is checked too because knob captions sit on the inset surface,
    /// which is a different colour from `panel`.
    pub fn enforce_readability(mut self) -> Self {
        let panel = rgb_of(self.panel);
        let panel2 = rgb_of(self.panel2);

        let mut text = color::readable_against(rgb_of(self.text), panel, color::LC_TEXT_MIN);
        text = color::readable_against(text, panel2, color::LC_TEXT_MIN);
        self.text = c32(text);

        let mut muted = color::readable_against(rgb_of(self.muted), panel, color::LC_MUTED);
        muted = color::readable_against(muted, panel2, color::LC_MUTED);
        self.muted = c32(muted);

        self
    }
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
    /// stored source colors, else fall back to the classic. `dark` is only
    /// consulted for the single-accent ("Custom") path below — multi-color
    /// sources re-derive their own mode from the list via `from_colors`
    /// (unchanged existing behavior).
    pub fn resolve(name: &str, source: &[String], dark: bool) -> Palette {
        match name {
            "Electric Cyan" => return Palette::electric_cyan(),
            "Paper" => return Palette::paper(),
            "Synthwave" => return Palette::synthwave(),
            _ => {}
        }
        // A single stored hex means this was picked via the big inline color
        // picker (`from_accent`) - `from_colors` needs >= 2 colors and would
        // return None, so it gets its own path re-deriving for the
        // persisted mode.
        if source.len() == 1 {
            if let Some(rgb) = color::hex_to_rgb(&source[0]) {
                return Palette::from_accent(rgb, dark);
            }
        }
        if source.len() >= 2 {
            let rgb: Vec<Rgb> = source.iter().filter_map(|h| color::hex_to_rgb(h)).collect();
            if let Some(p) = Palette::from_colors(name, &rgb) {
                return p;
            }
        }
        // Unknown name with no source: try the premade list, else classic.
        Palette::premade(name).unwrap_or_else(Palette::electric_cyan)
    }

    /// "Your accent color on the standard ground" (colormagic single-accent
    /// path, ported from SpaceView's `theme::from_accent`): unlike
    /// `from_colors` (which needs a full list and infers dark/light from it),
    /// this takes ONE accent + an explicit mode, so the big inline color
    /// picker + gradient-preset-sync can retheme without fighting the
    /// current dark/light choice.
    pub fn from_accent(accent: Rgb, dark: bool) -> Palette {
        // Record intent at the one point a raw pick enters the theme, so the
        // picker and the gradient can read back exactly what was chosen rather
        // than the readability-corrected version of it.
        set_accent_seed(accent);
        // DISCORD GROUND PARITY: the ground takes the accent's hue at low
        // saturation, scaled by the pick's own saturation so a gray/black
        // accent yields a neutral ground instead of being forced colorful.
        let a = color::rgb_to_hsl(accent);
        let hue = a.h;
        let satf = (a.s / 50.0).clamp(0.0, 1.0);
        let (bg, panel, text, muted) = if dark {
            (
                color::hsl_to_rgb(hue, 24.0 * satf, 7.0),
                color::hsl_to_rgb(hue, 22.0 * satf, 11.0),
                color::hsl_to_rgb(hue, 18.0 * satf, 92.0),
                color::hsl_to_rgb(hue, 12.0 * satf, 62.0),
            )
        } else {
            // Light grounds must commit to the hue: at 94-98 lightness no
            // color survives, so pull lightness down + saturation up until
            // the tint actually reads.
            (
                color::hsl_to_rgb(hue, 48.0 * satf, 86.0),
                color::hsl_to_rgb(hue, 42.0 * satf, 92.0),
                color::hsl_to_rgb(hue, 35.0 * satf, 13.0),
                color::hsl_to_rgb(hue, 18.0 * satf, 38.0),
            )
        };
        let panel2 =
            color::mix_colors(panel, if dark { [255, 255, 255] } else { [0, 0, 0] }, 0.07);
        // Canvas sits a step deeper than the chrome on dark themes, a step
        // brighter on light ones - same convention as `from_colors`.
        let canvas_bg = if dark {
            color::mix_colors(bg, [0, 0, 0], 0.45)
        } else {
            color::mix_colors(bg, [255, 255, 255], 0.55)
        };
        let ink = color::contrast_color(canvas_bg);
        let ok = if dark { OK } else { LIGHT_OK };

        Palette {
            name: "Custom".into(),
            source: vec![color::rgb_to_hex(accent)],
            dark,
            bg: c32(bg),
            panel: c32(panel),
            panel2: c32(panel2),
            text: c32(text),
            muted: c32(muted),
            accent: c32(accent),
            accent_dim: c32(color::mix_colors(accent, bg, 0.45)),
            ok,
            canvas_bg: c32(canvas_bg),
            ink: c32(ink),
            viz_accent: c32(accent),
            viz_cap: c32(color::mix_colors(accent, ink, 0.5)),
            glow: c32(color::mix_colors(accent, ink, 0.35)),
            rainbow_s: if dark { 0.85 } else { 0.95 },
            rainbow_v: if dark { 1.0 } else { 0.72 },
        }
    }
}

/// Pick the most saturated color in a list - the one swatch a palette (or a
/// generated harmony/flavor spread) would read as its "accent" at a glance.
/// Used by the gradient preset shelf's "preset sets theme colors" sync.
pub fn most_saturated(colors: &[Rgb]) -> Option<Rgb> {
    colors
        .iter()
        .copied()
        .max_by(|a, b| {
            color::rgb_to_hsl(*a).s.partial_cmp(&color::rgb_to_hsl(*b).s).unwrap_or(std::cmp::Ordering::Equal)
        })
}

static CURRENT: LazyLock<RwLock<Palette>> =
    // Enforced here too so even the pre-startup default obeys the guarantee -
    // `set_palette` is the only other way in.
    LazyLock::new(|| RwLock::new(Palette::electric_cyan().enforce_readability()));

/// Discord-style background wash toggle. Default ON; persisted in
/// settings.json like the palette itself (see `AppSettings::gradient`).
static GRADIENT: LazyLock<RwLock<bool>> = LazyLock::new(|| RwLock::new(true));

/// The accent the USER picked, raw and unprocessed.
///
/// THE RULE: the picker binds to intent, never to the processed result. It used
/// to seed from `theme::cyan()`, which is the ENFORCED ink accessor (already
/// walked for readability against the panel), so every frame the widget was
/// handed back its own corrected output. Dragging toward yellow got walked
/// darker and desaturated and snapped to brown, and pure black was unreachable.
/// Enforcement still happens downstream, where it cannot write back here.
static ACCENT_SEED: LazyLock<RwLock<Rgb>> = LazyLock::new(|| RwLock::new([86, 204, 255]));

/// The raw, unprocessed accent the user chose. The picker and gradient read this.
pub fn accent_seed() -> Rgb {
    *ACCENT_SEED.read().unwrap()
}
pub fn set_accent_seed(rgb: Rgb) {
    *ACCENT_SEED.write().unwrap() = rgb;
}

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
    *CURRENT.write().unwrap() = p.enforce_readability();
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
/// The accent AS DRAWN ON THE PANEL: section headers ("SIGNAL CHAIN", "A/V
/// SYNC"), knob arcs, and every other accent-coloured mark sitting on chrome.
///
/// Guaranteed legible. The raw `accent` is a fill colour first - it stays
/// verbatim for selection/active backgrounds (see `build_visuals`, which reads
/// `p.accent` directly) - but the same value used as INK on a light or
/// low-contrast panel washes out to nearly nothing, which is what made the
/// amber and teal section headers disappear on pale themes.
///
/// Cheap: `readable_against` returns immediately when the pair already passes,
/// so the walk only runs for palettes that actually need rescuing.
pub fn cyan() -> Color32 {
    let p = CURRENT.read().unwrap();
    c32(color::readable_against(
        rgb_of(p.accent),
        rgb_of(p.panel2),
        color::LC_MUTED,
    ))
}
/// Make ANY colour safe to draw as ink on the current chrome.
///
/// The escape hatch for semantic colours that must keep their meaning: the
/// amber A/V-SYNC accent stays amber and the limiter red stays red, but each is
/// walked (hue preserved) until APCA says it clears the caption floor against
/// the panel. Hardcoded literals like those are precisely what survived every
/// theme change and then vanished on a pale ground.
pub fn readable_ink(c: Color32) -> Color32 {
    let panel2 = rgb_of(CURRENT.read().unwrap().panel2);
    c32(color::readable_against([c.r(), c.g(), c.b()], panel2, color::LC_MUTED))
}

pub fn ok() -> Color32 {
    let p = CURRENT.read().unwrap();
    c32(color::readable_against(
        rgb_of(p.ok),
        rgb_of(p.panel2),
        color::LC_MUTED,
    ))
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
    //
    // v2: opacity is the FROST knob (per-mode, user-adjustable) instead of
    // the old hardcoded 216 constant. Dark's 0.85 default -> 255*0.85 ~= 216,
    // so the out-of-the-box look is unchanged; light gets its own 0.59
    // default (asymmetric on purpose: white bleaches color, dark preserves
    // it - see `frost()`).
    let f = frost(p.dark);
    let panel_alpha: u8 = if gradient_enabled() {
        if p.dark { (255.0 * f) as u8 } else { (200.0 * f) as u8 }
    } else {
        255
    };
    let panel_fill =
        Color32::from_rgba_unmultiplied(p.panel.r(), p.panel.g(), p.panel.b(), panel_alpha);
    v.panel_fill = panel_fill;
    // Floating windows (About, the Theme window) carry paragraphs of text and
    // stack OVER an already-translucent panel, so they get a near-solid fill
    // instead of inheriting the panel's alpha (SpaceView precedent: "almost
    // too clear" otherwise).
    // Fully opaque, not 246: `window_fill` also paints menus and popups, and a
    // dropdown you have to read should never sit on a moving gradient at all.
    let window_alpha: u8 = 255;
    v.window_fill =
        Color32::from_rgba_unmultiplied(p.panel.r(), p.panel.g(), p.panel.b(), window_alpha);
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

// ---- background gradient v2 (Discord parity) ------------------------------
//
// v1 (removed) mixed every corner toward bg as one 4-vertex quad, so the
// extents never showed real color. v2 is a true multi-stop ramp, ported
// verbatim from SpaceView's `theme.rs`:
//   - 1..=4 PEGS derived from the accent via colormagic harmony rules (or a
//     curated preset shelf, or user-picked custom colors), so the stops can
//     never clash (Discord's trick, our engine).
//   - DIRECTION: any angle, like Discord's Gradient Direction dial.
//   - INTENSITY: 0..1 like Discord's Color Intensity. At 1.0 the background
//     IS the pure peg ramp (panels float on top); at low values it fades to bg.
//   - END-HOLD easing: the ramp saturates to pure first/last peg over the
//     outer ~12% so the extremes read as their color instead of a blend.

/// Direction + intensity + peg-source knobs for the v2 wash. Lives in a
/// static (`GRAD_CFG` below) like the palette itself; `AppSettings` mirrors
/// the fields for persistence (see `settings.rs` + the load/save round-trip
/// in `main.rs`).
#[derive(Clone, Copy, PartialEq)]
pub struct GradientCfg {
    /// Degrees; 0 = left->right, 90 = top->bottom, 135 = TL->BR diagonal.
    pub angle_deg: f32,
    /// 0..1. Discord "Color Intensity". 1.0 = pure peg colors as the ground.
    pub intensity: f32,
    /// 1..=4 color stops (harmony mode; a lone peg gets an auto partner, see
    /// `mono_partner`).
    pub pegs: u8,
    /// Index into `color::HARMONY_RULES` used to derive the pegs from the accent.
    pub harmony: u8,
    /// >= 0: index into `GRADIENT_PRESETS` (curated named ramps, accent ignored).
    /// -1: harmony mode (pegs derived from the live accent).
    /// -2: custom mode (the `custom` pegs below, user-picked, used verbatim).
    pub preset: i16,
    /// Manual pegs for custom mode (first `pegs` entries used; slot 0 is
    /// always the live accent - see `gradient_pegs`).
    pub custom: [Rgb; 4],
}

impl Default for GradientCfg {
    fn default() -> Self {
        GradientCfg {
            angle_deg: 135.0,
            intensity: 0.45,
            pegs: 3,
            harmony: 0,
            preset: -1,
            custom: [[86, 204, 255], [153, 14, 165], [253, 79, 80], [37, 223, 196]],
        }
    }
}

/// Curated, named multi-stop ramps — the "Gradients of the Galaxy / chrome
/// sunset" shelf. Hand-picked hex stops (2-3 per ramp), used verbatim as pegs
/// in dark mode and lifted toward white in light mode. Copied verbatim from
/// SpaceView's `theme.rs` (same shelf across TrontStack apps).
pub const GRADIENT_PRESETS: &[(&str, &[&str])] = &[
    ("Galaxy Punch", &["#FD4F50", "#990EA5"]),
    ("Nebula Rush", &["#E71B7B", "#8324FB"]),
    ("Ultraviolet", &["#B501AA", "#FD37C8"]),
    ("Solar Flare", &["#FC4D1D", "#F1358A"]),
    ("Chrome Sunset", &["#C0C6CC", "#FFB88C", "#DE4313"]),
    ("Vaporwave", &["#FF6FD8", "#3813C2"]),
    ("Synthwave Drive", &["#DC28B2", "#2A41D2"]),
    ("Deep Space", &["#4D153C", "#B30F40"]),
    ("Golden Hour", &["#FEF528", "#B93B41"]),
    ("Blue Hour", &["#2AA9E9", "#005AFF"]),
    ("Tide Pool", &["#0DBEBA", "#00FFFB"]),
    ("Aurora Sky", &["#00C9FF", "#92FE9D"]),
    ("Toxic Slime", &["#25DFC4", "#E4E518"]),
    ("Matrix Rain", &["#00F032", "#00A0EA"]),
    ("Cherry Cola", &["#EB3349", "#F45C43"]),
    ("Berry Smoothie", &["#FF1B6B", "#45CAFF"]),
    ("Miami Nights", &["#FF0080", "#7928CA", "#4A00E0"]),
    ("Ember Fade", &["#F83600", "#F9D423"]),
    ("Concrete", &["#3A3D42", "#95989E"]),
    ("Princess", &["#FF9A9E", "#FAD0C4", "#A18CD1"]),
    ("Ocean Floor", &["#0F2027", "#2C5364", "#00B4DB"]),
    ("Firewatch", &["#CB2D3E", "#EF473A", "#2C3E50"]),
    ("Mint Chip", &["#00B09B", "#96C93D"]),
    ("Bubblegum", &["#FC5C7D", "#6A82FB"]),
    ("Night Drive", &["#0F0C29", "#302B63", "#24243E"]),
    ("Sunburn", &["#FF512F", "#F09819"]),
    ("Glacier", &["#83A4D4", "#B6FBFF"]),
];

static GRAD_CFG: LazyLock<RwLock<GradientCfg>> = LazyLock::new(|| RwLock::new(GradientCfg::default()));

/// FROST: panel opacity over the wash, per mode (asymmetric because white
/// bleaches color and dark preserves it). 0.0 = panels vanish, the background
/// IS the raw ramp (preview == BG, WYSIWYG); 1.0 = solid panels, wash hidden.
static FROST_DARK: LazyLock<RwLock<f32>> = LazyLock::new(|| RwLock::new(0.85));
static FROST_LIGHT: LazyLock<RwLock<f32>> = LazyLock::new(|| RwLock::new(0.59));

pub fn frost(dark: bool) -> f32 {
    if dark { *FROST_DARK.read().unwrap() } else { *FROST_LIGHT.read().unwrap() }
}
pub fn set_frost(dark: bool, v: f32) {
    let v = v.clamp(0.0, 1.0);
    if dark {
        *FROST_DARK.write().unwrap() = v;
    } else {
        *FROST_LIGHT.write().unwrap() = v;
    }
}

/// Re-apply egui Visuals from the CURRENT palette + frost + gradient state.
///
/// Needed because frost lives in `build_visuals`'s panel alpha, and unlike
/// `set_palette`/`set_mode`/`set_gradient` — which each re-apply — storing a new
/// frost value on its own changed nothing on screen until something else
/// happened to rebuild the style. That was the whole "frost didn't work" bug:
/// the value was stored and read correctly, it just never reached egui.
pub fn refresh_visuals(ctx: &egui::Context) {
    ctx.set_visuals(build_visuals());
}

pub fn gradient_cfg() -> GradientCfg {
    *GRAD_CFG.read().unwrap()
}
pub fn set_gradient_cfg(cfg: GradientCfg) {
    *GRAD_CFG.write().unwrap() = cfg;
}

/// The peg colors: accent -> harmony spread, adapted to the mode so dark
/// themes get deep rich stops and light themes get pastel ones. WCAG isn't a
/// factor here (no text sits on the raw ramp; panels carry the text).
pub fn gradient_pegs() -> Vec<Rgb> {
    let p = current();
    let cfg = gradient_cfg();

    // Custom mode: SLOT 0 IS THE ACCENT (linked, always participates - the
    // smart-slot rule: the primary color can never be missing from the ramp).
    // Slots 1..N are the user's exact colors, no adult supervision.
    if cfg.preset == -2 {
        let n = cfg.pegs.clamp(1, 4) as usize;
        let mut pegs: Vec<Rgb> = Vec::with_capacity(n.max(2));
        // Slot 0 is the RAW pick, not `p.accent` (enforced-for-ink and therefore
        // shifted): a pure yellow pick must reach the wash as pure yellow.
        pegs.push(accent_seed());
        if n > 1 {
            pegs.extend_from_slice(&cfg.custom[1..n]);
        }
        return mono_partner(pegs, p.dark);
    }

    // Curated preset: designed stops used verbatim (dark), lifted toward
    // white in light mode so the page stays airy under dark text.
    if cfg.preset >= 0 {
        if let Some((_, hexes)) = GRADIENT_PRESETS.get(cfg.preset as usize) {
            return hexes
                .iter()
                .filter_map(|h| color::hex_to_rgb(h))
                .map(|rgb| if p.dark { rgb } else { color::mix_colors(rgb, [255, 255, 255], 0.40) })
                .collect();
        }
    }

    // Harmony mode: pegs derived from the live accent, clash-proof by rule.
    let rule = color::HARMONY_RULES[(cfg.harmony as usize) % color::HARMONY_RULES.len()];
    // Spread from the RAW pick too, so the harmony is a family around the colour
    // that was actually chosen rather than its ink-corrected cousin.
    let base = color::rgb_to_hsl(accent_seed());
    let spread = color::generate_harmony(base, rule);
    let derived: Vec<Rgb> = spread
        .into_iter()
        .take((cfg.pegs.clamp(1, 4)) as usize)
        .map(|h| {
            // Mode-adapt lightness: deep + rich on dark, pastel on light.
            // Saturation is only capped, never forced up - a gray/black
            // accent legitimately yields a monochrome ramp (go nuts).
            let l = if p.dark { h.l.clamp(20.0, 42.0) } else { h.l.clamp(55.0, 78.0) };
            let s = if p.dark { h.s.min(90.0) } else { h.s.min(75.0) };
            color::hsl_to_rgb(h.h, s, l)
        })
        .collect();
    mono_partner(derived, p.dark)
}

/// ONE-COLOR THEME MODE: a single peg gets an auto-derived deep (dark mode)
/// or airy (light mode) partner so the ramp is a monochrome sweep instead of
/// a flat fill - smart slots means 1 peg is a real, good-looking choice.
fn mono_partner(pegs: Vec<Rgb>, _dark: bool) -> Vec<Rgb> {
    // ONE PEG MEANS ONE COLOUR. This used to derive a second stop so a single peg
    // still swept, but that silently turns "1 peg" into two, which shows up as an
    // unexpected extra swatch in the editor. A gradient with one peg IS a solid
    // colour choice: `ramp` returns that colour at every t, so intensity alone
    // decides how strongly it covers the ground.
    pegs
}

/// Sample the peg ramp at t in [0,1], with end-hold easing so the outer ~12%
/// on each side sits at the pure first/last peg.
fn ramp(pegs: &[Rgb], t: f32) -> Rgb {
    let t = ((t - 0.5) * 1.28 + 0.5).clamp(0.0, 1.0);
    let n = pegs.len();
    if n == 1 {
        return pegs[0];
    }
    let scaled = t * (n - 1) as f32;
    let i = (scaled.floor() as usize).min(n - 2);
    let frac = scaled - i as f32;
    color::mix_colors(pegs[i], pegs[i + 1], frac)
}

/// Paint the gradient as a fine vertex-colored grid into the background layer.
/// A grid (not one quad) because the ramp is multi-stop and runs at an
/// arbitrary angle; 16x16 vertices is still a trivially cheap single mesh.
/// No-op when the toggle is off.
pub fn paint_gradient(ctx: &egui::Context) {
    if !gradient_enabled() {
        return;
    }
    let rect = ctx.screen_rect();
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let cfg = gradient_cfg();
    let pegs = gradient_pegs();
    let bg = rgb_of(current().bg);

    let a = cfg.angle_deg.to_radians();
    let (dx, dy) = (a.cos(), a.sin());
    let c = rect.center();
    // Projection half-extent of the rect onto the gradient axis.
    let half = (rect.width() * 0.5 * dx.abs()) + (rect.height() * 0.5 * dy.abs());
    let half = half.max(1.0);

    const N: usize = 16;
    let mut mesh = egui::Mesh::default();
    for gy in 0..=N {
        for gx in 0..=N {
            let pt = egui::pos2(
                rect.left() + rect.width() * gx as f32 / N as f32,
                rect.top() + rect.height() * gy as f32 / N as f32,
            );
            let t = (((pt.x - c.x) * dx + (pt.y - c.y) * dy) / half) * 0.5 + 0.5;
            let col = color::mix_colors(bg, ramp(&pegs, t), cfg.intensity.clamp(0.0, 1.0));
            mesh.colored_vertex(pt, c32(col));
        }
    }
    let w = (N + 1) as u32;
    for gy in 0..N as u32 {
        for gx in 0..N as u32 {
            let i = gy * w + gx;
            mesh.add_triangle(i, i + 1, i + w);
            mesh.add_triangle(i + 1, i + w + 1, i + w);
        }
    }

    ctx.layer_painter(egui::LayerId::background()).add(egui::Shape::mesh(mesh));
}

/// Sample the final composited ramp (pegs + end-hold easing + intensity mix
/// toward bg) at t in [0,1] — powers the Theme window's live preview bar.
pub fn ramp_sample(t: f32) -> Color32 {
    let p = current();
    let cfg = gradient_cfg();
    let pegs = gradient_pegs();
    c32(color::mix_colors(rgb_of(p.bg), ramp(&pegs, t), cfg.intensity.clamp(0.0, 1.0)))
}

/// The wash as actually PERCEIVED through the current frost (panel
/// compositing included) — so the preview's bottom band can show reality,
/// not just the raw ramp.
pub fn ramp_sample_frosted(t: f32) -> Color32 {
    let wash = ramp_sample(t);
    let p = current();
    let f = frost(p.dark);
    let alpha = if p.dark { f } else { f * (200.0 / 255.0) };
    c32(color::mix_colors(rgb_of(wash), rgb_of(p.panel), alpha))
}
