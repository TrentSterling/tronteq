//! Headless UI snapshot harness ("uishot"). Renders the TrontEQ canvas, SHOW
//! modes, About window, and theme sheet to PNGs — no window, no admin, no
//! eyeballing at different zooms. Run: `cargo run -p tronteq-uitest`.
//! Output: `harness_shots/*.png` (repo root), reviewed by human or AI vision.
//!
//! The gui crate's display modules are mirrored in via #[path] so their
//! `crate::` paths resolve identically; the app shell (state writer, WASAPI,
//! tray, devices, elevation) never loads here. Renders via egui_kittest's wgpu
//! backend — the in-app screenshot route is a dead end on glow (Boxel lesson:
//! ViewportCommand::Screenshot is silently dropped on GL).

#[path = "../../gui/src/color.rs"]
mod color;
#[path = "../../gui/src/theme.rs"]
mod theme;
#[path = "../../gui/src/dsp_preview.rs"]
mod dsp_preview;
#[path = "../../gui/src/glstage.rs"]
mod glstage;
#[path = "../../gui/src/show.rs"]
mod show;
#[path = "../../gui/src/curve.rs"]
mod curve;
#[path = "../../gui/src/about.rs"]
mod about;

use eframe::egui;
use egui_kittest::Harness;
use tronteq_shared::{Band, BandKind, DEFAULT_FREQS, NUM_BANDS};

const OUT_DIR: &str = "harness_shots";

/// Deterministic synthetic viz data: a musical-looking spectrum (bass hump,
/// mid scoop, presence sparkle), a chord-ish waveform, a rotating lissajous
/// stereo field, and a breathing loudness history.
struct FakeViz {
    spectrum: Vec<f32>,
    peaks: Vec<f32>,
    waveform: Vec<f32>,
    stereo: Vec<[f32; 2]>,
    loudness: Vec<f32>,
}

impl FakeViz {
    fn new() -> Self {
        let n = 64;
        let spectrum: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / n as f32;
                let bass = (1.0 - t * 2.2).max(0.0) * 0.9;
                let mids = ((t - 0.45) * 8.0).sin().max(0.0) * 0.45;
                let sparkle = if i % 7 == 0 { 0.35 } else { 0.0 };
                (bass + mids + sparkle + 0.06).min(1.0)
            })
            .collect();
        let peaks: Vec<f32> = spectrum.iter().map(|v| (v * 1.15).min(1.0)).collect();
        let waveform: Vec<f32> = (0..1024)
            .map(|i| {
                let t = i as f32 / 1024.0 * std::f32::consts::TAU;
                ((t * 3.0).sin() * 0.5 + (t * 7.0).sin() * 0.25 + (t * 13.0).sin() * 0.1) * 0.9
            })
            .collect();
        let stereo: Vec<[f32; 2]> = (0..512)
            .map(|i| {
                let t = i as f32 / 512.0 * std::f32::consts::TAU;
                [(t * 2.0).sin() * 0.6, (t * 3.0).cos() * 0.5]
            })
            .collect();
        let loudness: Vec<f32> = (0..480)
            .map(|i| 0.25 + 0.2 * (i as f32 * 0.05).sin().abs())
            .collect();
        FakeViz { spectrum, peaks, waveform, stereo, loudness }
    }
}

fn demo_bands() -> [Band; NUM_BANDS] {
    // A hard-V so the composite curve has drama.
    let gains = [7.0, 4.0, 0.0, -4.0, -5.0, -1.0, 3.0, 6.0];
    let mut b = [Band::flat(0.0); NUM_BANDS];
    for (i, f) in DEFAULT_FREQS.iter().enumerate() {
        b[i] = Band { freq: *f, gain: gains[i], q: 1.0, kind: BandKind::Peak as u32 };
    }
    b
}

/// Render one canvas scene to a PNG. `steps` pre-runs the harness so the SHOW
/// simulations (trails / particles / tunnel rings) have state to draw.
fn shot_canvas(name: &str, palette: theme::Palette, mode: show::ShowMode, rainbow: bool, steps: u32) {
    let viz_data = FakeViz::new();
    let mut bands = demo_bands();
    let mut show_state = show::ShowState::new(mode);
    let mut applied = false;

    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(1100.0, 620.0))
        .build_ui(move |ui| {
            if !applied {
                // Full theme::apply: registers Rajdhani + the "display" font
                // family (About panics without it) and the base style.
                theme::apply(ui.ctx());
                theme::set_palette(ui.ctx(), palette.clone());
                applied = true;
            }
            let viz = curve::VizData {
                layers: curve::Layers::default(),
                spectrum: &viz_data.spectrum,
                peaks: &viz_data.peaks,
                waveform: &viz_data.waveform,
                stereo: &viz_data.stereo,
                loudness: &viz_data.loudness,
                spectro_tex: None,
            };
            // No GL stage in the kittest harness (wgpu renderer): painter path only.
            curve::draw(
                ui,
                &mut bands,
                48_000.0,
                rainbow,
                &viz,
                &mut show_state,
                None,
                glstage::Uniforms::default(),
            );
        });

    // Let the wall-clock-gated simulations advance (16ms per step).
    for _ in 0..steps {
        harness.run();
        std::thread::sleep(std::time::Duration::from_millis(17));
    }
    harness.run();
    save(&mut harness, name);
}

/// Render the About window at a given zoom.
fn shot_about(name: &str, ppp: f32) {
    let mut icon: Option<egui::TextureHandle> = None;
    let mut open = true;
    let mut frame_n = 0u32;
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(900.0, 640.0))
        .with_pixels_per_point(ppp)
        .build_ui(move |ui| {
            if frame_n == 0 {
                // set_fonts is deferred to the NEXT frame; drawing the
                // "display"-family wordmark this frame would panic. Warm up.
                theme::apply(ui.ctx());
                theme::set_palette(ui.ctx(), theme::Palette::electric_cyan());
            } else {
                about::show(ui.ctx(), &mut open, &mut icon);
            }
            frame_n += 1;
        });
    harness.run();
    harness.run();
    harness.run();
    save(&mut harness, name);
}

fn save(harness: &mut Harness<'_>, name: &str) {
    match harness.render() {
        Ok(img) => {
            let path = format!("{OUT_DIR}/{name}.png");
            if let Err(e) = img.save(&path) {
                eprintln!("[uishot] save {name}: {e}");
            } else {
                println!("[uishot] wrote {path}");
            }
        }
        Err(e) => eprintln!("[uishot] render {name} FAILED: {e:?}"),
    }
}

fn main() {
    std::fs::create_dir_all(OUT_DIR).expect("create harness_shots dir");

    // Canvas: both classics, rainbow on, no SHOW.
    shot_canvas("canvas_dark", theme::Palette::electric_cyan(), show::ShowMode::Off, true, 2);
    shot_canvas("canvas_light", theme::Palette::paper(), show::ShowMode::Off, true, 2);

    // Every SHOW mode on the classic dark theme.
    shot_canvas("show_bars_xl", theme::Palette::electric_cyan(), show::ShowMode::BarsXl, true, 6);
    shot_canvas("show_scope_trails", theme::Palette::electric_cyan(), show::ShowMode::ScopeTrails, true, 40);
    shot_canvas("show_tunnel", theme::Palette::electric_cyan(), show::ShowMode::Tunnel, true, 40);
    shot_canvas("show_particles", theme::Palette::electric_cyan(), show::ShowMode::Particles, true, 60);

    // Theme sheet: built-ins + featured premades + three random rolls, same scene.
    let mut sheet: Vec<theme::Palette> = vec![
        theme::Palette::synthwave(),
    ];
    for name in ["Dracula", "Tokyo Night", "Gruvbox", "Hades Fire", "Deep Ocean", "Arctic Aurora"] {
        if let Some(p) = theme::Palette::premade(name) {
            sheet.push(p);
        }
    }
    for i in 0..3 {
        let mut p = theme::Palette::randomize();
        p.name = format!("random_{i}_{}", p.name);
        sheet.push(p);
    }
    for p in sheet {
        let slug: String = p
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        shot_canvas(&format!("theme_{slug}"), p, show::ShowMode::Off, false, 2);
    }

    // About window at 100% and 200% (the reported overflow case).
    shot_about("about_100", 1.0);
    shot_about("about_200", 2.0);

    println!("[uishot] done");
}
