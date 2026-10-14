//! Record the production loopback analyzer once, then replay identical music
//! through every production shader. All output stays in the supplied directory.
use crate::{audio_analysis::Frame, curve, glstage::*, show::ShowMode, theme};
use anyhow::{ensure, Context, Result};
use eframe::{egui, glow::{self, HasContext}};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::{Path, PathBuf}, process::{Command, Stdio},
    sync::{Arc, atomic::AtomicBool}, time::{Duration, Instant}};

const W: u32 = 960;
const H: u32 = 540;
const FRAMES: usize = 480;
const SHOTS: [usize; 4] = [119, 239, 359, 479];

#[derive(Serialize, Deserialize)]
struct Sample {
    #[serde(default)]
    extra_signals: [f32;8],
    tick: u64, rms: f32, peak: f32,
    bass: f32, mid: f32, treble: f32, pulse: f32, phase: f32,
    centroid: f32, bpm: f32, confidence: f32, flux: f32,
    loud: f32, crest: f32, corr: f32, width: f32,
    spec: Vec<f32>, wave: Vec<f32>, peaks: Vec<f32>,
    stereo: Vec<[f32; 2]>, loudness: Vec<f32>,
}
impl Sample {
    fn from_frame(f: &Frame) -> Self {
        let b = &f.bus;
        Self { extra_signals:[b.sub_bass,b.kick,b.band_onsets[0],b.band_onsets[1],b.band_onsets[2],b.balance,b.tonality,0.0], tick: f.ticks, rms: f.rms, peak: f.peak, bass: b.bass,
            mid: b.mid, treble: b.treble, pulse: b.pulse, phase: b.beat_phase,
            centroid: b.centroid, bpm: b.bpm, confidence: b.bpm_conf,
            flux: b.flux, loud: b.loud_db, crest: b.crest_db, corr: b.corr,
            width: b.width, spec: f.spectrum.to_vec(), peaks: f.peaks.to_vec(),
            wave: f.wave.iter().step_by((f.wave.len()/WAVE_W).max(1)).take(WAVE_W).copied().collect(),
            stereo: f.stereo.iter().step_by(4).copied().collect(),
            loudness: f.loudness.iter().step_by(4).copied().collect() }
    }
    fn frame(&self)->Frame {
        let mut f=crate::audio_analysis::Analyzer::new(48000,2).snapshot();
        let b=&mut f.bus;
        b.bass=self.bass;b.mid=self.mid;b.treble=self.treble;b.pulse=self.pulse;
        b.flux=self.flux;b.width=self.width;b.centroid=self.centroid;
        b.loud_db=self.loud;b.crest_db=self.crest;b.corr=self.corr;b.bpm=self.bpm;b.bpm_conf=self.confidence;b.beat_phase=self.phase;
        b.sub_bass=self.extra_signals[0];b.kick=self.extra_signals[1];b.band_onsets.copy_from_slice(&self.extra_signals[2..5]);b.balance=self.extra_signals[5];b.tonality=self.extra_signals[6];
        f.spectrum.copy_from_slice(&self.spec);f.wave=self.wave.clone();f.stereo=self.stereo.clone();f.rms=self.rms;f.peak=self.peak;f.ticks=self.tick;
        f
    }
    fn configured(&self, _mode: GlMode, scene: ShowMode, config: &crate::viz_settings::VizSettings, runtime: &mut crate::viz_motion::Runtime, flash:&mut crate::viz_performance::FlashEnvelope) -> Uniforms {
        crate::viz_performance::uniforms(config,scene,&self.frame(),runtime,flash,1.0/60.0,0)
    }
    fn uniforms(&self, mode: GlMode) -> Uniforms {
        let mut u = Uniforms { mode, bass: self.bass, mid: self.mid, treble: self.treble,
            pulse: self.pulse, beat_phase: self.phase, bright: self.centroid,
            bpm: self.bpm, bpm_conf: self.confidence, flux: self.flux,
            loud: ((self.loud + 60.) / 60.).clamp(0., 1.),
            crest: (self.crest / 24.).clamp(0., 1.), corr: self.corr, width: self.width,
            ..Default::default() };
        u.spec.copy_from_slice(&self.spec);
        u.wave.copy_from_slice(&self.wave);
        u.stereo = std::array::from_fn(|i| self.stereo.get(i*self.stereo.len().max(1)/STEREO_W).copied().unwrap_or([0.;2]));
        u
    }
}

#[derive(Serialize, Deserialize)]
struct Recording { source: String, rate: u32, channels: usize, samples: Vec<Sample> }

fn capture() -> Result<Recording> {
    let capture = crate::visualizer::Visualizer::start(Arc::new(AtomicBool::new(false)));
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let s = capture.read();
        if s.status == crate::visualizer::Status::Live && s.frame.rms > 0.0001 { break; }
        ensure!(Instant::now() < deadline, "No active music received: {:?}", s.status);
        std::thread::sleep(Duration::from_millis(50));
    }
    // Fill analyzer history and permit the onset estimator to settle.
    std::thread::sleep(Duration::from_secs(3));
    let source = capture.read().source.context("No loopback source")?;
    println!("GAUNTLET capturing 8 seconds: {}", source.name);
    let start = Instant::now();
    let mut samples = Vec::with_capacity(FRAMES);
    for i in 0..FRAMES {
        let due = start + Duration::from_secs_f64(i as f64 / 60.);
        if let Some(wait) = due.checked_duration_since(Instant::now()) { std::thread::sleep(wait); }
        let s = capture.read();
        ensure!(s.status == crate::visualizer::Status::Live, "Capture interrupted: {:?}", s.status);
        ensure!(s.source.as_ref().is_some_and(|v| v.endpoint_id == source.endpoint_id), "Output changed during capture");
        samples.push(Sample::from_frame(&s.frame));
    }
    let advance = samples.last().unwrap().tick - samples[0].tick;
    ensure!(advance > 420, "Analyzer did not advance enough: {advance} ticks");
    ensure!(samples.iter().filter(|s| s.rms > 0.0001).count() > FRAMES / 2, "Mostly silent recording");
    println!("GAUNTLET LIVE PASS: {} ticks, RMS {:.4}..{:.4}", advance,
        samples.iter().map(|s| s.rms).fold(f32::INFINITY, f32::min),
        samples.iter().map(|s| s.rms).fold(0., f32::max));
    Ok(Recording { source: source.name, rate: source.format.rate, channels: source.format.channels, samples })
}

unsafe fn pixels(gl: &glow::Context) -> image::RgbaImage {
    let mut bytes = vec![0; (W * H * 4) as usize];
    gl.read_pixels(0, 0, W as i32, H as i32, glow::RGBA, glow::UNSIGNED_BYTE,
        glow::PixelPackData::Slice(Some(&mut bytes)));
    let mut img = image::RgbaImage::from_raw(W, H, bytes).unwrap();
    image::imageops::flip_vertical_in_place(&mut img);
    for p in img.pixels_mut() { p.0[3] = 255; }
    img
}

fn difference(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
    a.pixels().zip(b.pixels()).map(|(a,b)| (0..3).map(|i| a[i].abs_diff(b[i]) as f64).sum::<f64>()).sum::<f64>() / (W * H * 3) as f64
}

// Draw the exact production overlay functions over a captured stage frame.
// Replacing only the initial background shape avoids advancing the shader clock.
unsafe fn overlay(gl: &Arc<glow::Context>, fbo: glow::Framebuffer, base: &image::RgbaImage,
    samples: &[Sample], rate: u32, bands: &[tronteq_shared::Band; 8]) -> Result<image::RgbaImage> {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut renderer = eframe::egui_glow::Painter::new(gl.clone(), "", None, false).map_err(anyhow::Error::msg)?;
    let background = ctx.load_texture("stage-frame", egui::ColorImage::from_rgba_unmultiplied([W as usize,H as usize],base.as_raw()), egui::TextureOptions::LINEAR);
    let mut spectro = crate::spectrogram::Spectrogram::new();
    for s in samples.iter().rev().take(180).rev() { spectro.push(&s.spec); }
    let s = samples.last().unwrap();
    let mut bands = *bands;
    let mut show = crate::show::ShowState::new(ShowMode::Off);
    for _ in 0..2 {
        let output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(W as f32,H as f32))), ..Default::default() }, |ctx| {
            let waterfall = spectro.texture(ctx, true);
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                let start = ui.painter().add(egui::Shape::Noop);
                let data = curve::VizData { layers: curve::Layers { spectrum:true, peak_hold:true, analyzer:true,
                    waterfall:true, waveform:true, goniometer:true, loudness:true },
                    spectrum:&s.spec, peaks:&s.peaks, waveform:&s.wave, stereo:&s.stereo,
                    loudness:&s.loudness, spectro_tex:waterfall };
                curve::draw(ui,&mut bands,0,rate as f32,true,&data,&mut show,None,Uniforms::default());
                // The next shape is curve::draw's canvas background. Clear it,
                // and place our real shader result at the preceding shape slot.
                ui.painter().set(egui::layers::ShapeIdx(start.0+1),egui::Shape::Noop);
                let mut mesh = egui::Mesh::with_texture(background.id());
                mesh.add_rect_with_uv(rect,egui::Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.,1.)),egui::Color32::WHITE);
                ui.painter().set(start,egui::Shape::mesh(mesh));
            });
        });
        gl.bind_framebuffer(glow::FRAMEBUFFER,Some(fbo));
        renderer.clear([W,H],[0.,0.,0.,1.]);
        renderer.paint_and_update_textures([W,H],1.,&ctx.tessellate(output.shapes,1.),&output.textures_delta);
    }
    let result = pixels(gl);
    renderer.destroy();
    Ok(result)
}

pub unsafe fn run(gl: Arc<glow::Context>) -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let arg = |key: &str| args.iter().position(|a| a == key).and_then(|i| args.get(i+1));
    let dir = PathBuf::from(arg("--out").context("--out requires a new output directory")?);
    ensure!(!dir.exists(), "Output already exists; choose a new directory");
    std::fs::create_dir_all(&dir)?;
    let recording: Recording = if let Some(path) = arg("--replay") {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else { capture()? };
    ensure!(recording.samples.len() == FRAMES, "Expected 480 samples");
    ensure!(recording.samples.iter().all(|s| s.spec.len() == SPEC_W && s.wave.len() == WAVE_W), "Invalid recording dimensions");
    std::fs::write(dir.join("analysis.json"),serde_json::to_vec(&recording)?)?;
    // Read sound controls only; opening StateHandle could migrate the live file.
    let state = std::fs::read("C:/ProgramData/TrontEq/state.bin")?;
    ensure!(state.len() >= 136, "Sound snapshot is too short");
    let mut bands = std::array::from_fn(|i| {
        let o = 8+i*16;
        let f = |j| f32::from_le_bytes(state[o+j..o+j+4].try_into().unwrap());
        tronteq_shared::Band { freq:f(0), gain:f(4), q:f(8), kind:u32::from_le_bytes(state[o+12..o+16].try_into().unwrap()) }
    });
    if let Some(path) = arg("--replay") {
        let original = Path::new(path).with_file_name("overlay-bands.json");
        bands = serde_json::from_slice(&std::fs::read(original)?)?;
    }
    std::fs::write(dir.join("overlay-bands.json"),serde_json::to_vec_pretty(&bands)?)?;
    let tex = gl.create_texture().map_err(anyhow::Error::msg)?;
    gl.bind_texture(glow::TEXTURE_2D,Some(tex));
    gl.tex_image_2d(glow::TEXTURE_2D,0,glow::RGBA8 as i32,W as i32,H as i32,0,glow::RGBA,glow::UNSIGNED_BYTE,glow::PixelUnpackData::Slice(None));
    let fbo = gl.create_framebuffer().map_err(anyhow::Error::msg)?;
    gl.bind_framebuffer(glow::FRAMEBUFFER,Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER,glow::COLOR_ATTACHMENT0,glow::TEXTURE_2D,Some(tex),0);
    ensure!(gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE,"Incomplete framebuffer");
    let mut results = Vec::new();
    let adaptive = args.iter().any(|s| s == "--adaptive");
    let duets = args.iter().any(|s| s == "--duets");
    let cases: Vec<_> = if duets {
        [(ShowMode::GlTerrain,"Flame flow"),(ShowMode::GlKaleido,"Spectral glass"),(ShowMode::GlCopper,"Prism light")]
            .into_iter().enumerate().map(|(i,(scene,name))|(scene,format!("duet-{i}"),name.into(),None)).collect()
    } else if args.iter().any(|s| s == "--looks") {
        crate::viz_looks::factory().into_iter().filter(|look|arg("--look").is_none_or(|name|*name==look.name)).map(|look| {
            let scene=ShowMode::from_str(&look.scene);
            let id=format!("look-{}",look.name.to_lowercase().replace(' ',"-"));
            (scene,id,look.name.clone(),Some(look))
        }).collect()
    } else {
        ShowMode::ALL.into_iter().skip(1).filter(|s| arg("--scene").is_none_or(|id| *id == s.as_str()))
            .map(|s| (s,s.as_str().into(),s.label().into(),None)).collect()
    };
    let case_count=cases.len();
    for (scene,id,name,look) in cases {
        let mode = scene.gl_mode().unwrap();
        let mut stage = GlStage::new(&gl).map_err(anyhow::Error::msg)?;
        let mut config=crate::viz_settings::VizSettings::default();
        if let Some(look)=look {
            look.apply(&mut scene.clone(),&mut config);
            std::fs::write(dir.join(format!("{id}-look.json")),serde_json::to_vec_pretty(&look)?)?;
        }
        if adaptive {config.signal_response=crate::viz_signals::Settings::preset(0.025,0.18);}
        if let Some(index)=id.strip_prefix("duet-").and_then(|s|s.parse::<usize>().ok()) {crate::performance_panel::interaction_recipe(index,&mut scene.clone(),&mut config);}
        let deck=config.performance.deck_b.as_ref().map(|look| {let mut other=crate::viz_settings::VizSettings::default();let mut m=ShowMode::Off;look.apply(&mut m,&mut other);other.transition_seconds=0.0;(m,other)});
        let mut runtime_b=crate::viz_motion::Runtime::default();let mut flash_b=crate::viz_performance::FlashEnvelope::default();
        let mut signal_trace=Vec::new();
        let mut runtime=crate::viz_motion::Runtime::default();
        let mut flash=crate::viz_performance::FlashEnvelope::default();
        let mut encoder = Command::new("ffmpeg").args(["-v","error","-nostdin","-f","rawvideo","-pix_fmt","rgba","-s","960x540","-r","30","-i","pipe:0","-an","-vf","pad=ceil(iw/2)*2:ceil(ih/2)*2","-c:v","libx264","-preset","ultrafast","-crf","22","-pix_fmt","yuv420p","-movflags","+faststart"])
            .arg(dir.join(format!("{id}.mp4"))).stdin(Stdio::piped()).spawn().context("ffmpeg is required")?;
        let mut sink = encoder.stdin.take().unwrap();
        let mut shots = Vec::new();
        let mut timings = Vec::new();
        let mut motion = Vec::new();
        let mut previous: Option<image::RgbaImage> = None;
        for (i,s) in recording.samples.iter().enumerate() {
            let u = s.configured(mode, scene, &config, &mut runtime,&mut flash);
            if let Some((m,c))=&deck {stage.set_deck_b(Some((s.configured(m.gl_mode().unwrap(),*m,c,&mut runtime_b,&mut flash_b),config.performance.crossfade)));}
            if adaptive || duets {signal_trace.push(serde_json::json!({"frame":i,"input":runtime.conditioning.raw,"output":runtime.signals,"distortion":u.interaction[0]}));}
            gl.bind_framebuffer(glow::FRAMEBUFFER,Some(fbo));
            gl.scissor(0,0,W as i32,H as i32);
            stage.paint_elapsed(&gl,&u,[0,0,W as i32,H as i32],1./60.);
            if i > 60 { if let Some(ms) = stage.stats().gpu_ms { timings.push(ms); } }
            if i % 2 == 1 {
                let img = pixels(&gl);
                if let Some(prev) = previous.as_ref() { motion.push(difference(prev,&img)); }
                sink.write_all(img.as_raw())?;
                previous = Some(img);
            }
            if let Some(n) = SHOTS.iter().position(|f| *f == i) {
                let clean = pixels(&gl);
                clean.save(dir.join(format!("{id}-clean-{n}.png")))?;
                let mut fx = u;
                fx.fx_count = 5;
                fx.fx_order[..5].copy_from_slice(&[FX_MIRROR,FX_SCANLINES,FX_GRAIN,FX_EDGEGLOW,FX_ZOOMBLUR]);
                fx.fx_mix[..5].fill(0.7);
                stage.paint_elapsed(&gl,&fx,[0,0,W as i32,H as i32],0.);
                pixels(&gl).save(dir.join(format!("{id}-stack-{n}.png")))?;
                if n == 2 {
                    overlay(&gl,fbo,&clean,&recording.samples[..=i],recording.rate,&bands)?.save(dir.join(format!("{id}-eq.png")))?;
                }
                shots.push(clean);
            }
            ensure!(gl.get_error() == glow::NO_ERROR,"GL error in {id}, frame {i}");
        }
        drop(sink);
        ensure!(encoder.wait()?.success(),"Encoder failed for {id}");
        // Matched clocks with silent inputs separates time animation from audio response.
        let mut silent = GlStage::new(&gl).map_err(anyhow::Error::msg)?;
        let mut silent_runtime=crate::viz_motion::Runtime::default();
        let mut silent_flash=crate::viz_performance::FlashEnvelope::default();
        let mut silent_b_runtime=crate::viz_motion::Runtime::default();let mut silent_b_flash=crate::viz_performance::FlashEnvelope::default();
        let silent_sample=Sample::from_frame(&crate::audio_analysis::Analyzer::new(48000,2).snapshot());
        for _ in 0..FRAMES {
            gl.bind_framebuffer(glow::FRAMEBUFFER,Some(fbo));
            gl.scissor(0,0,W as i32,H as i32);
            if let Some((m,c))=&deck {silent.set_deck_b(Some((silent_sample.configured(m.gl_mode().unwrap(),*m,c,&mut silent_b_runtime,&mut silent_b_flash),config.performance.crossfade)));}
            silent.paint_elapsed(&gl,&silent_sample.configured(mode,scene,&config,&mut silent_runtime,&mut silent_flash),[0,0,W as i32,H as i32],1./60.);
        }
        let silence = pixels(&gl);
        silence.save(dir.join(format!("{id}-silent.png")))?;
        if !signal_trace.is_empty(){std::fs::write(dir.join(format!("{id}-signals.json")),serde_json::to_vec(&signal_trace)?)?;}
        let last = shots.last().unwrap();
        let audio_delta = difference(last,&silence);
        let black = last.pixels().filter(|p| p[0].max(p[1]).max(p[2]) < 10).count() as f64 / (W*H) as f64;
        let white = last.pixels().filter(|p| p[0].min(p[1]).min(p[2]) > 245).count() as f64 / (W*H) as f64;
        let mut strip = image::RgbaImage::new(W*2,H*2);
        for (i,shot) in shots.iter().enumerate() { image::imageops::replace(&mut strip,shot, (i%2) as i64*W as i64,(i/2) as i64*H as i64); }
        strip.save(dir.join(format!("{id}-sequence.png")))?;
        timings.sort_by(f32::total_cmp);
        let gpu = timings.get(timings.len()/2).copied().unwrap_or(0.);
        let temporal = motion.iter().sum::<f64>()/motion.len().max(1) as f64;
        ensure!(black < 0.9999,"Empty scene {id}");
        results.push(serde_json::json!({"id":id,"name":name,"gpu_median_ms":gpu,
            "audio_difference_0_255":audio_delta,"frame_difference_0_255":temporal,
            "dark_fraction":black,"white_fraction":white}));
        println!("GAUNTLET {}/{} {}: GPU {:.3}ms, audio delta {:.2}, motion {:.2}, dark {:.0}%",results.len(),case_count,name,gpu,audio_delta,temporal,black*100.);
        // Drop is insufficient: GlStage owns raw GL handles.
        stage.destroy(&gl);
        silent.destroy(&gl);
    }
    ensure!(!results.is_empty(),"No matching scenes");
    std::fs::write(dir.join("metrics.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "gpu":gl.get_parameter_string(glow::RENDERER),"source":recording.source,"rate":recording.rate,
        "channels":recording.channels,"samples":FRAMES,"adaptive":adaptive,"duets":duets,"dimensions":[W,H],"scenes":results,
        "stack":"Mirror, CRT scanlines, Film grain, Edge glow, Zoom blur; 70% each",
        "notes":"Captured live analyzer snapshots, replayed at 60 Hz; 30 fps silent videos. Metrics flag review candidates, not aesthetic pass/fail. EQ view uses all seven production overlays and the sound curve read at run start."}))?)?;
    write_index(&dir,&results)?;
    if args.iter().any(|s|s=="--performance") {performance_review(&gl,fbo,&recording,&dir)?;}
    gl.delete_framebuffer(fbo);
    gl.delete_texture(tex);
    println!("GAUNTLET COMPLETE {}/{} scenes: {}",results.len(),case_count,dir.join("index.html").display());
    Ok(())
}

fn write_index(dir: &Path, rows: &[serde_json::Value]) -> Result<()> {
    let mut html = String::from("<!doctype html><meta charset='utf-8'><title>TrontEQ live music gauntlet</title><style>body{background:#0a1018;color:#dceaf3;font:16px system-ui;margin:32px auto;max-width:1450px}button,select{font:inherit;padding:8px;background:#182735;color:inherit;border:1px solid #506374}article{border-top:1px solid #344452;padding:20px 0}video{width:64%;vertical-align:top}img{width:34%;cursor:zoom-in}p{color:#a0b4c6}h1{margin-bottom:8px}</style><h1>TrontEQ live music gauntlet</h1><p>Same eight seconds of live music analysis in every production scene. Clean motion, EQ overlays, and a five-effect stack. Videos contain no recorded audio. Click stills to inspect full size.</p><p><button onclick=\"document.querySelectorAll('video').forEach(v=>v.pause())\">Pause all</button> <select id='view' onchange='changeView()'><option value='eq'>All EQ overlays</option><option value='stack-2'>Five-effect stack</option><option value='sequence'>2 / 4 / 6 / 8 second sequence</option></select></p>");
    for row in rows {
        let id = row["id"].as_str().unwrap();
        html.push_str(&format!("<article><h2>{}</h2><p>GPU {:.3} ms at 960×540 · audio difference {:.2}/255 · frame difference {:.2}/255 · dark {:.0}%</p><video controls loop muted preload='none' poster='{id}-clean-2.png' src='{id}.mp4'></video> <a data-id='{id}' href='{id}-eq.png' target='_blank'><img src='{id}-eq.png' loading='lazy'></a></article>",row["name"].as_str().unwrap(),row["gpu_median_ms"].as_f64().unwrap(),row["audio_difference_0_255"].as_f64().unwrap(),row["frame_difference_0_255"].as_f64().unwrap(),row["dark_fraction"].as_f64().unwrap()*100.));
    }
    html.push_str("<script>function changeView(){for(const a of document.querySelectorAll('a[data-id]')){a.href=a.dataset.id+'-'+document.getElementById('view').value+'.png';a.firstChild.src=a.href}}const obs=new IntersectionObserver(es=>{for(const e of es)if(!e.isIntersecting)e.target.pause()});document.querySelectorAll('video').forEach(v=>{obs.observe(v);v.addEventListener('play',()=>document.querySelectorAll('video').forEach(o=>{if(o!==v)o.pause()}))});</script>");
    std::fs::write(dir.join("index.html"),html)?;
    Ok(())
}

unsafe fn performance_review(gl:&Arc<glow::Context>,fbo:glow::Framebuffer,recording:&Recording,dir:&Path)->Result<()> {
    use crate::{viz_performance as perf,viz_settings::VizSettings,viz_motion::Runtime};
    let mut c=VizSettings::default();c.transition_seconds=0.0;c.performance.follow_bpm=false;
    let looks=crate::viz_looks::factory();
    c.performance.setlist=["Molten metal","Pulse passage","Stereo sculpture"].into_iter().map(|name|perf::Cue{look:looks.iter().find(|l|l.name==name).unwrap().clone(),hold_beats:3.0,fade_beats:2.0}).collect();
    let mut mode=ShowMode::Off;let mut rt=perf::Runtime::default();rt.start(&mut mode,&mut c);
    let mut motion=Runtime::default();let mut stage=GlStage::new(gl).map_err(anyhow::Error::msg)?;
    let mut encoder=Command::new("ffmpeg").args(["-v","error","-nostdin","-f","rawvideo","-pix_fmt","rgba","-s","960x540","-r","30","-i","pipe:0","-an","-c:v","libx264","-preset","ultrafast","-crf","22","-pix_fmt","yuv420p","-movflags","+faststart"]).arg(dir.join("performance.mp4")).stdin(Stdio::piped()).spawn()?;
    let mut sink=encoder.stdin.take().unwrap();let mut trace=Vec::new();let mut commits=0;
    for (i,s) in recording.samples.iter().enumerate() {
        let frame=s.frame();let mix=rt.tick(&mut mode,&mut c,&frame.bus,1.0/60.0);
        if std::mem::take(&mut rt.swap_decks) {stage.swap_decks();std::mem::swap(&mut motion,&mut rt.motion_b);std::mem::swap(&mut rt.flash_a,&mut rt.flash_b);commits+=1;}
        let mut u=perf::uniforms(&c,mode,&frame,&mut motion,&mut rt.flash_a,1.0/60.0,0);
        rt.held=std::array::from_fn(|p|{let start=35+p*75;i>=start&&i<start+32});rt.tick_pads(&c,&mut motion,1.0/60.0);rt.apply_pads(&mut u);
        let b=if c.performance.enabled {c.performance.deck_b.as_ref().map(|look|{let mut other=VizSettings::default();let mut m=ShowMode::Off;look.apply(&mut m,&mut other);other.transition_seconds=0.0;let mut u=perf::uniforms(&other,m,&frame,&mut rt.motion_b,&mut rt.flash_b,1.0/60.0,0);rt.apply_pads(&mut u);(u,mix)})}else{None};
        stage.set_deck_b(b);gl.bind_framebuffer(glow::FRAMEBUFFER,Some(fbo));gl.scissor(0,0,W as i32,H as i32);stage.paint_elapsed(gl,&u,[0,0,W as i32,H as i32],1.0/60.0);
        ensure!(gl.get_error()==glow::NO_ERROR,"Performance GL error at {i}");
        if i%2==1 {sink.write_all(pixels(gl).as_raw())?;}
        if let Some(n)=SHOTS.iter().position(|f|*f==i) {pixels(gl).save(dir.join(format!("performance-{n}.png")))?;}
        trace.push(serde_json::json!({"frame":i,"scene":mode.as_str(),"mix":mix,"pads":rt.pads,"gpu_ms":stage.stats().gpu_ms}));
    }
    drop(sink);ensure!(encoder.wait()?.success(),"Performance encoder failed");ensure!(commits>=2,"Setlist did not advance twice");
    std::fs::write(dir.join("performance-trace.json"),serde_json::to_vec_pretty(&trace)?)?;
    let mut html=std::fs::OpenOptions::new().append(true).open(dir.join("index.html"))?;
    html.write_all(b"<article><h2>Performance: two decks, five pads and a three-cue setlist</h2><p>The same captured phrase through the production performance runtime. Manual 120 BPM, 3-beat holds and 2-beat fades. Each pad is held and released in sequence.</p><video controls loop muted src='performance.mp4' poster='performance-2.png'></video><a href='performance-trace.json'>Frame trace</a></article>")?;
    stage.destroy(gl);println!("PERFORMANCE REPLAY PASSED: 480 frames, {commits} continuous deck commits, five momentary pads");Ok(())
}
