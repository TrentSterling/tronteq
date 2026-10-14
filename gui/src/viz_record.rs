//! Bounded resolved-input takes and an isolated hidden GPU export worker.
use crate::glstage::{GlStage, Uniforms};
use anyhow::{ensure, Context, Result};
use eframe::{
    egui,
    glow::{self, HasContext},
};
use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

const MAX_FRAMES: usize = 3600;
const MAX_SECONDS: f32 = 60.0;
pub use crate::glstage::ReplayFrame as Frame;
#[derive(Clone, Serialize, Deserialize)]
pub struct Take {
    pub schema: u32,
    pub frames: Vec<Frame>,
}
impl Default for Take {
    fn default() -> Self {
        Self {
            schema: 1,
            frames: vec![],
        }
    }
}
impl Take {
    pub fn seconds(&self) -> f32 {
        self.frames.iter().fold(0.0, |total, f| total + f.dt)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "Unsupported take version");
        ensure!(
            !self.frames.is_empty() && self.frames.len() <= MAX_FRAMES,
            "Take must contain 1 to {MAX_FRAMES} frames"
        );
        ensure!(
            self.seconds() <= MAX_SECONDS + 0.11,
            "Take is longer than 60 seconds"
        );
        for f in &self.frames {
            if let Some((_, mix)) = f.b {
                ensure!(
                    mix.is_finite() && (0.0..=1.0).contains(&mix),
                    "Invalid deck mix"
                );
            }
            ensure!(
                f.dt.is_finite() && f.dt > 0.0 && f.dt <= 0.1,
                "Invalid frame duration"
            );
            for u in std::iter::once(&f.a).chain(f.b.as_ref().map(|(u, _)| u)) {
                ensure!(
                    u.fx_count <= 10 && u.blend <= 4,
                    "Invalid effect count or blend"
                );
                ensure!(u.interaction_mode <= 3 && u.interaction_direction <= 2
                    && u.interaction.iter().all(|v| v.is_finite())
                    && (0.0..=1.0).contains(&u.interaction[0])
                    && (0.25..=4.0).contains(&u.interaction[1])
                    && (-std::f32::consts::PI..=std::f32::consts::PI).contains(&u.interaction[2])
                    && (0.0..=1.0).contains(&u.interaction[3]), "Invalid deck distortion");
                ensure!(
                    u.spec.iter().chain(u.wave.iter()).all(|v| v.is_finite()),
                    "Invalid signal value"
                );
                ensure!(
                    u.global_pulse
                        .iter()
                        .chain(u.transform.iter())
                        .chain(u.controls.iter())
                        .chain(u.details.iter())
                        .chain(u.fx_mix.iter())
                        .chain(u.accent.iter())
                        .chain(u.stereo.iter().flatten())
                        .all(|v| v.is_finite() && v.abs() <= 64.0),
                    "Invalid visual value"
                );
                ensure!(
                    [
                        u.bass,
                        u.mid,
                        u.treble,
                        u.pulse,
                        u.beat_phase,
                        u.bright,
                        u.bpm,
                        u.bpm_conf,
                        u.flux,
                        u.loud,
                        u.crest,
                        u.corr,
                        u.width,
                        u.speed,
                        u.brightness,
                        u.hue,
                        u.saturation,
                        u.render_scale,
                        u.transition_seconds,
                        u.flash
                    ]
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1000.0),
                    "Invalid scalar"
                );
                ensure!(
                    (0.0..=2.0).contains(&u.speed)
                        && (0.5..=1.0).contains(&u.render_scale)
                        && (0.0..=10.0).contains(&u.transition_seconds),
                    "Invalid timing or scale"
                );
                ensure!(u.fx_order.iter().all(|b| *b <= 512), "Invalid effect");
                ensure!(
                    u.spec.iter().chain(u.wave.iter()).all(|v| v.abs() <= 8.0),
                    "Signal out of range"
                );
            }
        }
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        serde_json::to_writer(&mut f, self)?;
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        ensure!(
            std::fs::metadata(path)?.len() <= 128 * 1024 * 1024,
            "Take exceeds 128 MB"
        );
        let t: Self = serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
        t.validate()?;
        Ok(t)
    }
}
pub struct Recorder {
    pub take: Take,
    pub recording: bool,
    pub playing: bool,
    pub looping: bool,
    pub message: String,
    pub folder: String,
    pub load_path: String,
    index: usize,
    clock: f32,
    due: f32,
    epoch: u64,
    last: Option<Frame>,
    elapsed: f32,
    export: Option<(Child, PathBuf)>,
}
impl Default for Recorder {
    fn default() -> Self {
        Self {
            take: Take::default(),
            recording: false,
            playing: false,
            looping: false,
            message: String::new(),
            folder: std::env::var("USERPROFILE")
                .map_or_else(|_| ".".into(), |p| format!("{p}/Videos/TrontEQ")),
            load_path: String::new(),
            index: 0,
            clock: 0.0,
            due: 0.0,
            epoch: 1_000_000,
            last: None,
            elapsed: 0.0,
            export: None,
        }
    }
}
impl Recorder {
    pub fn start(&mut self) {
        self.take.frames.clear();
        self.recording = true;
        self.playing = false;
        self.elapsed = 0.0;
        self.epoch = self.epoch.wrapping_add(10_000);
        self.message = "Recording artwork and controls. Limit: 60 seconds / 3600 frames.".into();
    }
    pub fn play(&mut self) {
        if !self.take.frames.is_empty() {
            self.recording = false;
            self.playing = true;
            self.index = 0;
            self.clock = 0.0;
            self.due = 0.0;
            self.last = None;
            self.epoch = self.epoch.wrapping_add(10_000);
        }
    }
    pub fn record(
        &mut self,
        dt: f32,
        a: &mut Uniforms,
        b: &mut Option<(Uniforms, f32)>,
        swap: bool,
    ) {
        if !self.recording {
            return;
        }
        if self.take.frames.len() >= MAX_FRAMES || self.elapsed >= MAX_SECONDS {
            self.recording = false;
            self.message = "Take complete. Replay, save or export it below.".into();
            return;
        }
        let dt = dt.clamp(0.001, 0.1);
        a.epoch = a.epoch.wrapping_add(self.epoch);
        a.step_seconds = Some(dt);
        if let Some((b, _)) = b {
            b.epoch = b.epoch.wrapping_add(self.epoch);
            b.step_seconds = Some(dt);
        }
        self.take.frames.push(Frame {
            dt,
            a: *a,
            b: *b,
            swap: swap && !self.take.frames.is_empty(),
        });
        self.elapsed += dt;
    }
    pub fn playback(&mut self, dt: f32) -> Option<Vec<Frame>> {
        if !self.playing {
            return None;
        }
        self.clock += dt.clamp(0.0, 0.1);
        let mut out = vec![];
        while self.index < self.take.frames.len() && self.due < self.clock {
            let mut f = self.take.frames[self.index].clone();
            f.a.epoch = f.a.epoch.wrapping_add(self.epoch);
            f.a.playback = true;
            f.a.draw_editor = false;
            f.a.draw_overlays = false;
            if let Some((u, _)) = &mut f.b {
                u.epoch = u.epoch.wrapping_add(self.epoch);
            }
            self.due += f.dt;
            self.index += 1;
            self.last = Some(f.clone());
            out.push(f);
        }
        if self.index == self.take.frames.len() && self.clock >= self.due {
            if self.looping {
                self.index = 0;
                self.clock -= self.due;
                self.due = 0.0;
                self.epoch = self.epoch.wrapping_add(10_000);
            } else {
                self.playing = false;
                self.message = "Replay complete. Live visualization resumes.".into();
            }
        }
        Some(out)
    }
    pub fn last_uniforms(&self) -> Option<Uniforms> {
        self.last.as_ref().map(|f| f.a)
    }
    fn unique(&self, suffix: &str) -> Result<PathBuf> {
        let folder = PathBuf::from(&self.folder);
        std::fs::create_dir_all(&folder)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis();
        Ok(folder.join(format!("TrontEQ-{stamp}{suffix}")))
    }
    pub fn panel(&mut self, ui: &mut egui::Ui) {
        if let Some((child, path)) = &mut self.export {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.message = if status.success() {
                        format!("Exported {}", path.display())
                    } else {
                        format!(
                            "Export failed. See {}",
                            path.with_extension("log").display()
                        )
                    };
                    self.export = None;
                }
                Err(e) => {
                    self.message = e.to_string();
                    self.export = None;
                }
                _ => {}
            }
        }
        ui.strong("Performance recorder");
        ui.small("Records the artwork, resolved signals, pads, XY moves and deck transitions. Replay reproduces the take while your music continues live.");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.recording, egui::Button::new("Record new take"))
                .clicked()
            {
                self.start();
            }
            if ui
                .add_enabled(self.recording || self.playing, egui::Button::new("Stop"))
                .clicked()
            {
                self.recording = false;
                self.playing = false;
            }
            if ui
                .add_enabled(
                    !self.recording && !self.take.frames.is_empty(),
                    egui::Button::new("Replay"),
                )
                .clicked()
            {
                self.play();
            }
        });
        ui.checkbox(&mut self.looping, "Loop replay");
        ui.label(format!(
            "{:0.1} s / 60 s  |  {} frames",
            self.take.seconds(),
            self.take.frames.len()
        ));
        if self.recording {
            ui.colored_label(egui::Color32::LIGHT_RED, "Recording");
        }
        if self.playing {
            ui.add(
                egui::ProgressBar::new(self.index as f32 / self.take.frames.len().max(1) as f32)
                    .text("Playing take"),
            );
        }
        ui.separator();
        ui.label("Save / export folder");
        ui.add(egui::TextEdit::singleline(&mut self.folder).desired_width(ui.available_width()));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.recording && !self.take.frames.is_empty(),
                    egui::Button::new("Save take"),
                )
                .clicked()
            {
                let result = self.unique(".trontperf.json").and_then(|p| {
                    self.take.save(&p)?;
                    Ok(p)
                });
                self.message = match result {
                    Ok(p) => {
                        self.load_path = p.to_string_lossy().into();
                        format!("Saved {}", p.display())
                    }
                    Err(e) => e.to_string(),
                };
            }
            if ui
                .add_enabled(
                    !self.recording && !self.take.frames.is_empty() && self.export.is_none(),
                    egui::Button::new("Export MP4"),
                )
                .clicked()
            {
                let result = (|| -> Result<(Child, PathBuf)> {
                    let input = self.unique(".trontperf.json")?;
                    self.take.save(&input)?;
                    let output = input.with_extension("mp4");
                    let log = std::fs::File::create(output.with_extension("log"))?;
                    let child = Command::new(std::env::current_exe()?)
                        .arg("--render-take")
                        .arg(&input)
                        .arg(&output)
                        .creation_flags(0x08000000)
                        .stdout(Stdio::null())
                        .stderr(log)
                        .spawn()?;
                    Ok((child, output))
                })();
                match result {
                    Ok(job) => {
                        self.export = Some(job);
                        self.message = "Exporting 960 x 540 at 30 fps in the background...".into();
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
        });
        ui.small("MP4 is artwork only, without UI or recorded sound. FFmpeg must be on PATH. Export runs independently of the live window.");
        ui.separator();
        ui.label("Take file to load");
        ui.add(
            egui::TextEdit::singleline(&mut self.load_path)
                .desired_width(ui.available_width())
                .hint_text(".trontperf.json"),
        );
        if ui
            .add_enabled(!self.recording, egui::Button::new("Load take"))
            .clicked()
        {
            match Take::load(Path::new(&self.load_path)) {
                Ok(t) => {
                    self.take = t;
                    self.playing = false;
                    self.message = "Take loaded".into();
                }
                Err(e) => self.message = e.to_string(),
            }
        }
        if self.export.is_some() {
            ui.spinner();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(200));
        }
        ui.label(&self.message);
    }
}

pub unsafe fn export(gl: &glow::Context, take: &Take, path: &Path) -> Result<usize> {
    unsafe {
        take.validate()?;
        ensure!(!path.exists(), "Export already exists");
        let (w, h) = (960, 540);
        let tex = gl.create_texture().map_err(anyhow::Error::msg)?;
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
        let fbo = gl.create_framebuffer().map_err(anyhow::Error::msg)?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(tex),
            0,
        );
        ensure!(
            gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE,
            "Export framebuffer is incomplete"
        );
        let mut stage = GlStage::new(gl).map_err(anyhow::Error::msg)?;
        let mut child = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-s",
                "960x540",
                "-r",
                "30",
                "-i",
                "pipe:0",
                "-an",
                "-vf",
                "vflip",
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "20",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(path)
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("FFmpeg could not start; add ffmpeg to PATH")?;
        let result = (|| -> Result<usize> {
            let mut sink = child.stdin.take().unwrap();
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            let mut elapsed = 0.0f64;
            let mut frames = 0usize;
            for f in &take.frames {
                if f.swap {
                    stage.swap_decks();
                }
                stage.set_deck_b(f.b);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.scissor(0, 0, w, h);
                stage.paint_elapsed(gl, &f.a, [0, 0, w, h], f.dt);
                ensure!(gl.get_error() == glow::NO_ERROR, "GPU export error");
                elapsed += f.dt as f64;
                if frames as f64 / 30.0 + 0.00001 < elapsed {
                    gl.read_pixels(
                        0,
                        0,
                        w,
                        h,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelPackData::Slice(Some(&mut pixels)),
                    );
                    while (frames as f64) / 30.0 + 0.00001 < elapsed {
                        sink.write_all(&pixels)?;
                        frames += 1;
                    }
                }
            }
            drop(sink);
            ensure!(child.wait()?.success(), "FFmpeg failed");
            Ok(frames)
        })();
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        stage.destroy(gl);
        gl.delete_framebuffer(fbo);
        gl.delete_texture(tex);
        result
    }
}

pub fn run_export_cli() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let input = args.get(2).context("Missing take path")?;
    let output = PathBuf::from(args.get(3).context("Missing MP4 path")?);
    let take = Take::load(Path::new(input))?;
    let result = Arc::new(Mutex::new(None));
    let shared = result.clone();
    struct Worker {
        gl: Arc<glow::Context>,
        take: Option<Take>,
        output: PathBuf,
        result: Arc<Mutex<Option<Result<usize, String>>>>,
    }
    impl eframe::App for Worker {
        fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
            if let Some(t) = self.take.take() {
                let r = unsafe { export(&self.gl, &t, &self.output) }.map_err(|e| format!("{e:#}"));
                *self.result.lock().unwrap() = Some(r);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    eframe::run_native(
        "TrontEQ performance export",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            viewport: egui::ViewportBuilder::default()
                .with_visible(false)
                .with_active(false)
                .with_inner_size([64.0, 64.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(Worker {
                gl: cc.gl.clone().expect("OpenGL export"),
                take: Some(take),
                output,
                result: shared,
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let finished = result.lock().unwrap().take();
    match finished {
        Some(Ok(_)) => Ok(()),
        Some(Err(e)) => Err(anyhow::anyhow!(e)),
        None => Err(anyhow::anyhow!("Export window closed before rendering")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn take_roundtrip_and_validation() {
        let mut r = Recorder::default();
        r.start();
        for i in 0..120 {
            let mut u = Uniforms::default();
            u.pulse = i as f32 / 120.0;
            u.interaction_mode=2;u.interaction_direction=1;u.interaction=[0.6,1.3,0.4,0.2];
            let mut b = Some((u, 0.5));
            r.record(1.0 / 60.0, &mut u, &mut b, i == 60);
        }
        r.take.validate().unwrap();
        let json = serde_json::to_vec(&r.take).unwrap();
        let mut copy: Take = serde_json::from_slice(&json).unwrap();
        assert_eq!(copy.frames.len(), 120);
        assert!(copy.frames[60].swap);
        assert_eq!(copy.frames[90].a.pulse, 0.75);
        assert_eq!(copy.frames[90].a.interaction,[0.6,1.3,0.4,0.2]);
        copy.frames[0].a.interaction[1]=0.0;
        assert!(copy.validate().is_err());
        copy.frames[0].a.interaction[1]=1.3;
        copy.frames[0].a.speed = f32::NAN;
        assert!(copy.validate().is_err());
        copy.frames[0].a.speed = 1.0;
        copy.frames[0].b.as_mut().unwrap().1 = 2.0;
        assert!(copy.validate().is_err());
    }
    #[test]
    fn replay_delivers_every_step_at_different_refresh_rates() {
        for fps in [20, 30, 60, 144] {
            let mut r = Recorder::default();
            r.start();
            for i in 0..120 {
                let mut u = Uniforms::default();
                u.pulse = i as f32;
                r.record(1.0 / 60.0, &mut u, &mut None, false);
            }
            r.play();
            let mut steps = vec![];
            for _ in 0..fps * 3 {
                if let Some(frames) = r.playback(1.0 / fps as f32) {
                    steps.extend(frames.into_iter().map(|f| f.a.pulse as usize));
                }
            }
            assert_eq!(steps, (0..120).collect::<Vec<_>>(), "{fps} fps");
            assert!(!r.playing);
        }
    }
    #[test]
    fn recording_stops_at_frame_budget() {
        let mut r = Recorder::default();
        r.start();
        for _ in 0..MAX_FRAMES + 5 {
            r.record(0.001, &mut Uniforms::default(), &mut None, false);
        }
        assert_eq!(r.take.frames.len(), MAX_FRAMES);
        assert!(!r.recording);
    }
    #[test]
    fn repeated_playback_uses_distinct_epoch() {
        let mut r = Recorder::default();
        r.start();
        r.record(0.01, &mut Uniforms::default(), &mut None, false);
        r.play();
        let first = r.playback(0.02).unwrap()[0].a.epoch;
        r.play();
        let second = r.playback(0.02).unwrap()[0].a.epoch;
        assert_ne!(first, second);
    }
}
