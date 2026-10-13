//! Isolated audit probes. No device changes and no production state-file access.
#![allow(dead_code)]

#[path = "../../../gui/src/color.rs"]
mod color;
#[path = "../../../gui/src/curve.rs"]
mod curve;
#[path = "../../../gui/src/dsp_preview.rs"]
mod dsp_preview;
#[path = "../../../gui/src/glstage.rs"]
mod glstage;
mod preview;
mod thumbnails;
mod gauntlet;
#[path = "../../../gui/src/audio_analysis.rs"]
mod audio_analysis;
#[path = "../../../gui/src/audio_format.rs"]
mod audio_format;
#[path = "../../../gui/src/visualizer.rs"]
mod visualizer;
#[path = "../../../gui/src/vizbus.rs"]
mod vizbus;
#[path = "../../../gui/src/spectrogram.rs"]
mod spectrogram;
#[path = "../../../gui/src/show.rs"]
mod show;
#[path = "../../../gui/src/theme.rs"]
mod theme;
#[path = "../../../gui/src/window_chrome.rs"]
mod window_chrome;
#[path = "../../../gui/src/viz_panel.rs"]
mod viz_panel;
#[path = "../../../gui/src/viz_looks.rs"]
mod viz_looks;
#[path = "../../../gui/src/presentation.rs"]
mod presentation;
#[path = "../../../gui/src/scene_gallery.rs"]
mod scene_gallery;
#[path = "../../../gui/src/render_timer.rs"]
mod render_timer;
#[path = "../../../gui/src/sound_edit.rs"]
mod sound_edit;
#[path = "../../../gui/src/eq_editor.rs"]
mod eq_editor;
#[path = "../../../gui/src/state_writer.rs"]
mod state_writer;
#[path = "../../../gui/src/state_recovery.rs"]
mod state_recovery;
#[path = "../../../gui/src/profile_picker.rs"]
mod profile_picker;
#[path = "../../../gui/src/viz_settings.rs"]
mod viz_settings;
#[path = "../../../gui/src/viz_motion.rs"]
mod viz_motion;
#[path = "../../../gui/src/viz_signals.rs"]
mod viz_signals;
#[path = "../../../gui/src/viz_performance.rs"]
mod viz_performance;
#[path = "../../../gui/src/viz_xy.rs"]
mod viz_xy;
#[path = "../../../gui/src/patch_bay.rs"]
mod patch_bay;
#[path = "../../../gui/src/viz_compositor.rs"]
mod viz_compositor;
#[path = "../../../gui/src/viz_record.rs"]
mod viz_record;
#[path = "../../../gui/src/performance_panel.rs"]
mod performance_panel;
#[path = "../../../gui/src/viz_audition.rs"]
mod viz_audition;
#[path = "../../../gui/src/viz_edit.rs"]
mod viz_edit;

use eframe::glow::{self, HasContext};
use glstage::*;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn log_line(message: &str) { println!("{message}"); }
fn tid() -> u32 { unsafe { windows::Win32::System::Threading::GetCurrentThreadId() } }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--snapshot-child") {
        let path = PathBuf::from(args.get(2).expect("explicit disposable path required"));
        let state = tronteq_shared::StateHandle::open_at(&path)?;
        match state.snapshot() {
            Ok(s) => println!("snapshot version={}", s.version),
            Err(tronteq_shared::StateError::Busy { version }) => println!("interrupted version {version}: bounded error"),
            Err(e) => return Err(e.into()),
        }
        return Ok(());
    }
    // This unrelated diagnostic is opt-in; the default run only checks visuals.
    if args.iter().any(|a| a == "--seqlock") {
        // Fixture lives beside this executable in the repo's target directory.
        let fixture = std::env::current_exe()?
            .parent()
            .unwrap()
            .join(format!("audit-state-{}.bin", std::process::id()));
        // Own a new fixture; never overwrite a previous run's diagnostic data.
        std::fs::OpenOptions::new().write(true).create_new(true).open(&fixture)?;
        let state = tronteq_shared::StateHandle::open_at(&fixture)?;
        let prior = state.snapshot()?;
        drop(state);
        let mut data = std::fs::read(&fixture)?;
        data[..8].copy_from_slice(&2u64.to_le_bytes());
        std::fs::write(&fixture, &data)?;
        let run_child = || -> Result<bool, Box<dyn std::error::Error>> {
            let mut child = Command::new(std::env::current_exe()?)
                .arg("--snapshot-child")
                .arg(&fixture)
                .stdout(Stdio::null())
                .spawn()?;
            let start = Instant::now();
            loop {
                if let Some(status) = child.try_wait()? {
                    return Ok(status.success());
                }
                if start.elapsed() >= Duration::from_secs(2) {
                    child.kill()?;
                    child.wait()?;
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        assert!(run_child()?, "even-version baseline must complete");
        data[..8].copy_from_slice(&3u64.to_le_bytes());
        std::fs::write(&fixture, &data)?;
        let odd_completed = run_child()?;
        assert!(odd_completed, "an abandoned odd snapshot must return a bounded error");
        assert_eq!(std::fs::read(&fixture)?, data, "a read must never repair or reset the interrupted data");
        let state = tronteq_shared::StateHandle::open_at(&fixture)?;
        state.recover(3, &prior)?;
        let recovered = state.snapshot()?;
        assert_eq!(recovered.bands, prior.bands);
        assert_eq!(recovered.preamp_db, prior.preamp_db);
        assert_eq!(recovered.bypass, prior.bypass);
        assert_eq!(recovered.delay_ms, prior.delay_ms);
        drop(state);
        std::fs::remove_file(&fixture)?;
        println!("SEQLOCK PASS: even snapshot; bounded odd snapshot without mutation; explicit recovery preserved controls");
        return Ok(());
    }

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_visible(false)
            .with_inner_size([64.0, 64.0]),
        ..Default::default()
    };
    let preview_only = args.iter().any(|a| a == "--preview-only");
    let thumbnails_only = args.iter().any(|a| a == "--thumbnails-only");
    let live_gauntlet = args.iter().any(|a| a == "--live-gauntlet");
    eframe::run_native(
        "TrontEQ isolated audit",
        options,
        Box::new(move |cc| {
            let result = if live_gauntlet {
                unsafe { gauntlet::run(cc.gl.as_ref().expect("GL context").clone()) }
                    .map_err(|e| format!("{e:#}"))
            } else if thumbnails_only {
                unsafe { thumbnails::render(cc.gl.as_ref().expect("GL context")) }
            } else {
                (if preview_only { Ok(()) } else { unsafe { probe_gl(cc.gl.as_ref().expect("GL context")) } })
                    .and_then(|_| unsafe { preview::render(cc.gl.as_ref().unwrap().clone()) })
            };
            match result {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("GL probe failed: {e}");
                    std::process::exit(1);
                }
            }
        }),
    )?;
    Ok(())
}

unsafe fn probe_gl(gl: &glow::Context) -> Result<(), String> {
    println!("GPU {}", gl.get_parameter_string(glow::RENDERER));
    let (w, h) = (512, 288);
    let tex = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        w,
        h,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
    );
    let fbo = gl.create_framebuffer()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        Some(tex),
        0,
    );
    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
        return Err("incomplete output framebuffer".into());
    }
    let render = |fx| -> Result<Vec<u8>, String> {
        // Fresh production stage makes first-frame time, history and feedback identical.
        let mut stage = GlStage::new(gl)?;
        let mut uniforms = Uniforms {
            mode: GlMode::Plasma,
            fx,
            bass: 0.7,
            mid: 0.5,
            treble: 0.4,
            pulse: 0.8,
            loud: 0.7,
            beat_phase: 0.03,
            bright: 0.5,
            ..Default::default()
        };
        for (i, s) in uniforms.spec.iter_mut().enumerate() {
            *s = (i as f32 * 0.3).sin() * 0.3 + 0.5;
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.scissor(0, 0, w, h);
        stage.paint(gl, &uniforms, [0, 0, w, h]);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        gl.read_pixels(
            0,
            0,
            w,
            h,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
        let error = gl.get_error();
        if error != glow::NO_ERROR {
            return Err(format!("GL error {error}"));
        }
        if !pixels
            .chunks_exact(4)
            .any(|p| p[..3].iter().any(|v| *v != 0))
        {
            return Err("empty render".into());
        }
        Ok(pixels)
    };
    let baseline = render(0)?;
    println!(
        "All {} production scene shaders compiled and linked",
        MODE_COUNT
    );
    for (name, early, late) in [
        ("Zoom blur + Aberration", FX_ZOOMBLUR, FX_ABERRATION),
        ("Aberration + Pixelate", FX_ABERRATION, FX_PIXELATE),
        ("Halftone + Scanlines", FX_HALFTONE, FX_SCANLINES),
    ] {
        let early_pixels = render(early)?;
        let late_pixels = render(late)?;
        let stacked_pixels = render(early | late)?;
        let changed = |a: &[u8], b: &[u8]| {
            a.chunks_exact(4)
                .zip(b.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count()
        };
        let early_delta = changed(&early_pixels, &baseline);
        let late_delta = changed(&late_pixels, &baseline);
        let stack_delta = changed(&stacked_pixels, &late_pixels);
        assert!(
            early_delta > 0 && late_delta > 0,
            "each effect must alter baseline"
        );
        assert!(stack_delta > 0, "stacking must retain the earlier effect");
        println!("FX {name}: early vs base {early_delta} pixels; late vs base {late_delta}; stacked vs late {stack_delta}");
    }
    let read = || {
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        gl.read_pixels(
            0,
            0,
            w,
            h,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
        pixels
    };
    let uniform = Uniforms {
        bass: 0.7,
        mid: 0.5,
        treble: 0.4,
        pulse: 0.8,
        loud: 0.7,
        bright: 0.5,
        spec: [0.6; SPEC_W],
        wave: std::array::from_fn(|i| (i as f32 * 0.1).sin() * 0.4),
        stereo: std::array::from_fn(|i| [(i as f32 * 0.13).sin() * 0.2, (i as f32 * 0.13 + 0.7).sin() * 0.2]),
        width: 0.6,
        ..Default::default()
    };
    let mut all_scenes = GlStage::new(gl)?;
    for scene in show::ShowMode::ALL.into_iter().skip(1) {
        let u = Uniforms {
            mode: scene.gl_mode().unwrap(),
            ..uniform
        };
        for _ in 0..12 {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.scissor(0, 0, w, h);
            all_scenes.paint_elapsed(gl, &u, [0, 0, w, h], 1.0 / 60.0);
        }
        assert!(
            read()
                .chunks_exact(4)
                .any(|p| p[..3].iter().any(|v| *v > 0)),
            "empty scene {}",
            scene.label()
        );
        assert_eq!(
            gl.get_error(),
            glow::NO_ERROR,
            "GL error in {}",
            scene.label()
        );
    }
    println!("{MODE_COUNT}/{MODE_COUNT} scenes rendered nonempty frames without GL errors");
    let timeline = |fps: usize, speed: f32, count: usize, u: Uniforms| -> Result<Vec<u8>, String> {
        let mut stage = GlStage::new(gl)?;
        let u = Uniforms { speed, ..u };
        for frame in 0..=count {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.scissor(0, 0, w, h);
            stage.paint_elapsed(
                gl,
                &u,
                [0, 0, w, h],
                if frame == 0 { 0.0 } else { 1.0 / fps as f32 },
            );
        }
        assert_eq!(gl.get_error(), glow::NO_ERROR);
        let result = read();
        stage.destroy(gl);
        Ok(result)
    };
    for mode in [GlMode::Warp, GlMode::Trails, GlMode::Terrain] {
        let u = Uniforms { mode, ..uniform };
        let at60 = timeline(60, 1.0, 60, u)?;
        assert!(
            at60 == timeline(20, 1.0, 20, u)?,
            "20/60 fps simulation differs"
        );
        assert!(
            at60 == timeline(144, 1.0, 144, u)?,
            "144/60 fps simulation differs"
        );
        assert!(
            at60 == timeline(60, 2.0, 30, u)?,
            "2x speed does not scale feedback consistently"
        );
        assert!(
            timeline(60, 0.0, 60, u)? == timeline(60, 1.0, 0, u)?,
            "zero speed must hold the scene"
        );
    }
    println!(
        "Warp / Scope trails / Terrain: identical at 20, 60, 144 fps; speed 0x and 2x verified"
    );
    for mode in [GlMode::Flame, GlMode::Tunnel, GlMode::Terrain, GlMode::Matrix, GlMode::StereoScope, GlMode::Spectrogram, GlMode::SpectralCity, GlMode::Cymatics, GlMode::StereoOrbit, GlMode::Kaleido, GlMode::Copper, GlMode::Laser] {
        for axis in 0..4 {
            let mut lo = Uniforms { mode, ..uniform };
            lo.controls[axis] = 0.0;
            let mut hi = lo;
            hi.controls[axis] = 2.0;
            let control_frame = |mut input: Uniforms| -> Result<Vec<u8>, String> {
                if mode != GlMode::StereoScope || axis != 3 { return timeline(60, 1.0, 120, input); }
                // Persistence is observable when a trace moves or disappears,
                // not while repainting the same stationary signal indefinitely.
                let mut stage = GlStage::new(gl)?;
                for frame in 0..80 {
                    if frame == 60 { input.stereo.fill([0.0; 2]); }
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                    gl.scissor(0, 0, w, h);
                    stage.paint_elapsed(gl, &input, [0,0,w,h], 1.0/60.0);
                }
                let pixels = read();
                stage.destroy(gl);
                Ok(pixels)
            };
            assert!(control_frame(lo)? != control_frame(hi)?,
                "scene {} control {} has no rendered effect", mode as usize, axis);
        }
    }
    println!("48/48 scene controls change rendered pixels at their range endpoints");
    for (mode, variant) in [(GlMode::Flame, 0), (GlMode::StereoScope, 1)] {
        for axis in 0..4 {
            let mut lo = Uniforms { mode, variant, ..uniform };
            lo.details[axis] = 0.0;
            let mut hi = lo;
            hi.details[axis] = 2.0;
            assert!(timeline(60,1.0,120,lo)? != timeline(60,1.0,120,hi)?,
                "detail control {axis} has no rendered effect");
        }
    }
    let original = timeline(60,1.0,120,Uniforms { mode:GlMode::Flame,..uniform })?;
    let mut variants = Vec::new();
    for variant in 1..=4 {
        let pixels=timeline(60,1.0,120,Uniforms { mode:GlMode::Flame,variant,..uniform })?;
        assert!(pixels != original && !variants.contains(&pixels), "Flame variations must be distinct");
        variants.push(pixels);
    }
    let base=timeline(60,1.0,20,uniform)?;
    assert!(base != timeline(60,1.0,20,Uniforms { hue:0.4,..uniform })?);
    assert!(base != timeline(60,1.0,20,Uniforms { saturation:0.0,..uniform })?);
    println!("8/8 detail controls, four distinct Flame variants, hue and saturation verified");
    let orientation = |opposite: bool| -> Result<(f64,f64),String> {
        let mut u = uniform;
        u.mode = GlMode::StereoScope;
        u.rainbow = false;
        u.controls[2] = 0.0; // Measure only the signal, without the scope grid.
        u.stereo = std::array::from_fn(|i| {
            let s = (i as f32 * 0.08).sin() * 0.12;
            [s, if opposite { -s } else { s }]
        });
        let image = timeline(60, 1.0, 1, u)?;
        let (mut xx,mut yy) = (0.0,0.0);
        for (i,p) in image.chunks_exact(4).enumerate() {
            let weight = (p[0] as f64 + p[1] as f64 + p[2] as f64) / 765.0;
            let x = (i % w as usize) as f64 - (w-1) as f64 * 0.5;
            let y = (i / w as usize) as f64 - (h-1) as f64 * 0.5;
            xx += x*x*weight; yy += y*y*weight;
        }
        Ok((xx,yy))
    };
    let mono = orientation(false)?;
    let side = orientation(true)?;
    assert!(mono.1 > mono.0 * 4.0 && side.0 > side.1 * 4.0,
        "Stereo scope must display mono vertically and opposite phase horizontally");
    println!("Stereo scope: mono vertical / opposite phase horizontal verified from rendered pixels");
    let mut ordered = Uniforms {
        mode: GlMode::Plasma,
        fx_count: 2,
        ..uniform
    };
    ordered.fx_order[..2].copy_from_slice(&[FX_HALFTONE, FX_SCANLINES]);
    let forward = timeline(60, 1.0, 0, ordered)?;
    ordered.fx_order.swap(0, 1);
    assert!(
        forward != timeline(60, 1.0, 0, ordered)?,
        "effect order must affect the result"
    );
    ordered.fx_mix = [0.0; 10];
    assert!(
        timeline(60, 1.0, 0, ordered)?
            == timeline(
                60,
                1.0,
                0,
                Uniforms {
                    fx_count: 0,
                    ..ordered
                }
            )?,
        "0% mix must preserve the source"
    );
    println!("Effect reorder and zero-mix checks passed");
    let paint = |stage: &mut GlStage, u: Uniforms, elapsed: f32| {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.scissor(0, 0, w, h);
        stage.paint_elapsed(gl, &u, [0, 0, w, h], elapsed);
        read()
    };
    let a=Uniforms{mode:GlMode::Plasma,speed:0.0,transition_seconds:0.0,..uniform};
    let mut b=Uniforms{mode:GlMode::Voronoi,speed:0.0,transition_seconds:0.0,fx_count:1,..uniform};
    b.fx_order[0]=crate::viz_settings::Effect::Scanlines.bit();b.fx_mix[0]=0.7;
    let mut dual=GlStage::new(gl)?;
    dual.set_deck_b(Some((b,0.0)));let a_frame=paint(&mut dual,a,0.0);
    dual.set_deck_b(Some((b,1.0)));let b_frame=paint(&mut dual,a,0.0);
    assert_ne!(a_frame,b_frame,"decks must retain independent scenes and effects");
    dual.set_deck_b(Some((b,0.5)));let mixed=paint(&mut dual,a,0.0);
    for ((a,b),m) in a_frame.iter().zip(&b_frame).zip(&mixed) {assert!(((*a as f32+*b as f32)*0.5-*m as f32).abs()<=1.1,"dual deck midpoint");}
    dual.swap_decks();dual.set_deck_b(None);
    assert_eq!(b_frame,paint(&mut dual,b,0.0),"take B must preserve its image and feedback");
    let frozen=paint(&mut dual,Uniforms{freeze:true,hue:0.7,bass:1.0,..b},0.05);
    assert_eq!(b_frame,frozen,"Freeze holds the composed frame");
    let moved=paint(&mut dual,Uniforms{transform:[1.5,0.3,0.1,0.0],..b},0.0);
    assert_ne!(b_frame,moved,"frame movement must affect the image");
    let plain=paint(&mut dual,b,0.0);
    let mut flashed=b;flashed.fx_count=2;flashed.fx_order[1]=crate::viz_settings::Effect::Strobe.bit();flashed.fx_mix[1]=1.0;flashed.flash=0.0;
    assert_eq!(plain,paint(&mut dual,flashed,0.0),"zero flash must preserve the source");
    flashed.flash=1.0;assert_ne!(plain,paint(&mut dual,flashed,0.0),"explicit flash envelope must drive the effect");
    for (name,pixels) in [("deck-a",a_frame),("deck-b",b_frame),("deck-mix",mixed),("deck-transform",moved)] {
        let mut image=image::RgbaImage::from_raw(w as u32,h as u32,pixels).unwrap();image::imageops::flip_vertical_in_place(&mut image);image.save(format!("target/performance-{name}.png")).map_err(|e|e.to_string())?;
    }
    assert_eq!(gl.get_error(),glow::NO_ERROR);dual.destroy(gl);
    println!("Two live decks / independent effects / 50% mix / take B continuity / freeze / frame movement / flash envelope passed");
    b.spec=[0.15;SPEC_W];
    for blend in 0..5 {
        let mut stage=GlStage::new(gl)?;let u=Uniforms{blend,..a};
        stage.set_deck_b(None);let clean_a=paint(&mut stage,u,0.0);
        stage.set_deck_b(Some((b,0.0)));assert_eq!(clean_a,paint(&mut stage,u,0.0),"blend {blend} A endpoint");
        stage.set_deck_b(Some((b,1.0)));let clean_b=paint(&mut stage,u,0.0);
        let mut reference=GlStage::new(gl)?;assert_eq!(clean_b,paint(&mut reference,b,0.0),"blend {blend} B endpoint");reference.destroy(gl);
        stage.set_deck_b(Some((b,0.37)));let middle=paint(&mut stage,u,0.0);assert_ne!(middle,clean_a);assert_ne!(middle,clean_b);
        stage.swap_decks();stage.set_deck_b(Some((a,0.63)));let swapped=paint(&mut stage,Uniforms{blend,blend_reverse:true,..b},0.0);
        assert!(middle.iter().zip(swapped).all(|(a,b)|(*a as i32-b as i32).abs()<=1),"blend {blend} swap continuity");stage.destroy(gl);
    }
    // Spatial deck interaction must remain reversible and keep pure crossfader endpoints.
    for kind in 1..=3 {
        for direction in 0..=2 {
            let mut stage=GlStage::new(gl)?;
            let u=Uniforms{interaction_mode:kind,interaction_direction:direction,interaction:[0.7,1.2,0.4,0.3],..a};
            stage.set_deck_b(None);let clean_a=paint(&mut stage,u,0.0);
            stage.set_deck_b(Some((b,0.0)));assert_eq!(clean_a,paint(&mut stage,u,0.0),"distortion A endpoint");
            let mut reference=GlStage::new(gl)?;let clean_b=paint(&mut reference,b,0.0);reference.destroy(gl);
            stage.set_deck_b(Some((b,1.0)));assert_eq!(clean_b,paint(&mut stage,u,0.0),"distortion B endpoint");
            stage.set_deck_b(Some((b,0.37)));let plain=paint(&mut stage,Uniforms{interaction_mode:0,..u},0.0);
            let mut zero=u;zero.interaction[0]=0.0;assert_eq!(plain,paint(&mut stage,zero,0.0),"zero amount preserves blend");
            for blend in 0..5 {
                let u=Uniforms{blend,..u};let mixed=paint(&mut stage,u,0.0);
                assert_ne!(plain,mixed,"distortion has no effect");
                let swapped_direction=match direction {0=>1,1=>0,_=>2};
                stage.swap_decks();stage.set_deck_b(Some((a,0.63)));
                let swapped=paint(&mut stage,Uniforms{interaction_direction:swapped_direction,blend_reverse:true,..Uniforms{mode:b.mode,fx_count:b.fx_count,fx_order:b.fx_order,fx_mix:b.fx_mix,spec:b.spec,..u}},0.0);
                assert!(mixed.iter().zip(&swapped).all(|(a,b)|(*a as i32-*b as i32).abs()<=1),"distortion {kind}/{direction} blend {blend} swap continuity");
                stage.swap_decks();stage.set_deck_b(Some((b,0.37)));
                if direction==0 && blend==0 {let mut img=image::RgbaImage::from_raw(w as u32,h as u32,mixed).unwrap();image::imageops::flip_vertical_in_place(&mut img);img.save(format!("target/interaction-{kind}.png")).map_err(|e|e.to_string())?;}
            }
            stage.destroy(gl);
        }
    }
    println!("Cross-deck distortion: 3 modes x 3 directions x 5 blends, swaps, pure endpoints and zero amount passed");
    let mut stage=GlStage::new(gl)?;
    let u=Uniforms{global_pulse:[1.0,0.0,0.4,0.2],..a};
    let low=paint(&mut stage,u,1.0/60.0);assert!(low.chunks_exact(4).all(|p|p[..3].iter().all(|v|*v<=103)),"global brightness ceiling");
    let raised=paint(&mut stage,Uniforms{brightness:1.5,..u},1.0/60.0);
    let peak_delta=low.iter().zip(&raised).map(|(a,b)|(*a as i32-*b as i32).abs()).max().unwrap();assert!(peak_delta<12,"soft attack prevents a one-frame jump");
    stage.destroy(gl);println!("All five blend endpoints, swaps, pulse ceiling and soft attack passed");
    // Record the same resolved values the application sends, including a deck swap.
    let mut recorder=crate::viz_record::Recorder::default();recorder.start();let mut original=GlStage::new(gl)?;let mut expected=vec![];
    for i in 0..120 {
        let swapped=i==70;if swapped{original.swap_decks();}
        let mut u=Uniforms{mode:if i<70{GlMode::Warp}else{GlMode::Flame},epoch:8,blend:2,interaction_mode:2,interaction_direction:if i<70{0}else{1},interaction:[0.6,1.3,0.4,0.3],global_pulse:[0.6,0.7,0.8,0.2],pulse:(i as f32*0.1).sin().abs(),..uniform};
        let mut other=Some((Uniforms{mode:if i<70{GlMode::Flame}else{GlMode::Warp},..u},if i<70{i as f32/100.0}else{0.3}));
        recorder.record(1.0/60.0,&mut u,&mut other,swapped);original.set_deck_b(other);expected=paint(&mut original,u,1.0/60.0);
    }
    original.destroy(gl);recorder.recording=false;recorder.take.validate().map_err(|e|e.to_string())?;
    for fps in [20,60,144]{let mut replay=GlStage::new(gl)?;recorder.play();let mut frames=0;let mut last=vec![];
        for _ in 0..fps*3 {if let Some(batch)=recorder.playback(1.0/fps as f32){frames+=batch.len();let u=recorder.last_uniforms().unwrap();replay.set_replay(Some(batch));gl.bind_framebuffer(glow::FRAMEBUFFER,Some(fbo));gl.scissor(0,0,w,h);replay.paint(gl,&u,[0,0,w,h]);last=read();}}
        assert_eq!(frames,120);assert_eq!(last,expected,"record/replay at {fps} fps");replay.destroy(gl);
    }
    let folder=std::path::Path::new("target/performance-018");std::fs::create_dir_all(folder).map_err(|e|e.to_string())?;
    let take_path=folder.join("verified.trontperf.json");if !take_path.exists(){recorder.take.save(&take_path).map_err(|e|e.to_string())?;}
    assert_eq!(gl.get_error(),glow::NO_ERROR);println!("Resolved take: pixel-identical replay at 20/60/144 fps including deck swap and final pulse history");
    let mut transition = GlStage::new(gl)?;
    let from = paint(&mut transition, Uniforms { mode: GlMode::Plasma, ..uniform }, 0.0);
    let to = Uniforms { mode: GlMode::Voronoi, speed: 0.0, transition_seconds: 1.0, ..uniform };
    assert!(from == paint(&mut transition, to, 0.0), "scene switch must start at the previous image");
    let target = paint(&mut GlStage::new(gl)?, Uniforms { transition_seconds: 0.0, ..to }, 0.0);
    assert!(from != target);
    let middle = paint(&mut transition, to, 0.5);
    for (name, pixels) in [("before", &from), ("middle", &middle), ("after", &target)] {
        let mut image = image::RgbaImage::from_raw(w as u32, h as u32, pixels.clone()).unwrap();
        image::imageops::flip_vertical_in_place(&mut image);
        image.save(format!("target/transition-{name}.png")).map_err(|e| e.to_string())?;
    }
    for ((a, b), mixed) in from.iter().zip(&target).zip(&middle) {
        assert!(((*a as f32 + *b as f32) * 0.5 - *mixed as f32).abs() <= 1.1, "mid-fade must blend both scenes");
    }
    let interrupted = Uniforms { mode: GlMode::Metaballs, ..to };
    assert!(middle == paint(&mut transition, interrupted, 0.0), "rapid selection must retain the currently visible mixture");
    let end = paint(&mut transition, interrupted, 1.0);
    let target = paint(&mut GlStage::new(gl)?, Uniforms { transition_seconds: 0.0, ..interrupted }, 0.0);
    assert!(end == target, "fade must finish even when scene speed is zero");
    let cut = Uniforms { mode: GlMode::Plasma, transition_seconds: 0.0, ..to };
    assert!(paint(&mut transition, cut, 0.0) == paint(&mut GlStage::new(gl)?, cut, 0.0), "zero fade must cut immediately");
    transition.suspend();
    assert!(paint(&mut transition, to, 0.0) == paint(&mut GlStage::new(gl)?, to, 0.0), "Off must not revive an old frame");
    println!("Crossfade start / midpoint / interruption / completion / instant cut / Off passed");
    let at_fps = |fps: usize, base: Uniforms| -> Result<Vec<u8>, String> {
        let mut stage = GlStage::new(gl)?;
        paint(&mut stage, Uniforms { mode: GlMode::Plasma, ..base }, 0.0);
        let next = Uniforms { mode: GlMode::Warp, transition_seconds: 1.0, ..base };
        paint(&mut stage, next, 0.0);
        let mut frame = Vec::new();
        for _ in 0..fps/2 { frame = paint(&mut stage, next, 1.0/fps as f32); }
        Ok(frame)
    };
    let fade60 = at_fps(60, uniform)?;
    for fps in [20, 144] {
        let frame = at_fps(fps, uniform)?;
        assert!(fade60.iter().zip(frame).all(|(a, b)| a.abs_diff(b) <= 1), "crossfade timing differs with frame rate");
    }
    println!("Crossfade matches at 20 / 60 / 144 fps within one quantization level");

    // Quality must retain egui's native viewport and clip, including during allocation.
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.disable(glow::SCISSOR_TEST);
    gl.clear_color(0.13, 0.25, 0.6, 1.0);
    gl.clear(glow::COLOR_BUFFER_BIT);
    let background = read();
    gl.scissor(66, 45, 321, 166);
    gl.enable(glow::SCISSOR_TEST);
    transition.paint_elapsed(gl, &Uniforms { render_scale: 0.5, ..to }, [57, 31, 350, 192], 0.0);
    let scaled = read();
    assert_eq!((transition.stats().width, transition.stats().height), (175, 96));
    let mut inside_changed = 0;
    for y in 0..h { for x in 0..w {
        let i = ((y * w + x) * 4) as usize;
        if x < 66 || x >= 387 || y < 45 || y >= 211 {
            assert_eq!(&scaled[i..i+4], &background[i..i+4], "quality rendering escaped the canvas clip");
        } else if scaled[i..i+4] != background[i..i+4] { inside_changed += 1; }
    }}
    assert!(inside_changed > 1000);
    assert_eq!(gl.get_error(), glow::NO_ERROR);
    println!("50% render dimensions, viewport offset, resize and scissor isolation passed");

    // Disposable native-size target for measured GPU cost; finish is confined to this probe.
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1920, 1080, 0,
        glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(None));
    for scale in [1.0, 0.75, 0.5] {
        let mut measured = GlStage::new(gl)?;
        let mut u = Uniforms { mode: GlMode::Nebula, render_scale: scale, fx_count: 2, ..uniform };
        u.fx_order[..2].copy_from_slice(&[FX_ABERRATION, FX_EDGEGLOW]);
        for _ in 0..90 {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.scissor(0, 0, 1920, 1080);
            measured.paint_elapsed(gl, &u, [0, 0, 1920, 1080], 1.0/60.0);
            gl.finish();
        }
        let stats = measured.stats();
        let ms = stats.gpu_ms.ok_or("no GPU timing result")?;
        assert!(ms > 0.0 && ms.is_finite());
        assert_eq!(gl.get_error(), glow::NO_ERROR);
        println!("GPU Nebula + Color split + Edge glow: {}% = {}x{}, {ms:.3} ms", (scale*100.) as i32, stats.width, stats.height);
    }
    gl.delete_framebuffer(fbo);
    gl.delete_texture(tex);
    println!("RENDER CHECKS PASSED");
    Ok(())
}
