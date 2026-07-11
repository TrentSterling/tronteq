//! SHOW — full-canvas eye-candy modes (winamp / WMP energy), deliberately
//! separate from the ANALYZE data layers. One mode at a time, drawn as the
//! deepest canvas layer; the analyzers and the EQ curve stack on top.
//!
//! Perf rules (learned from the S59 geometry bomb): everything batches into a
//! handful of meshes / connected polylines per frame, pools are fixed-capacity
//! (zero steady-state allocation), and animation advances on the same 16ms
//! wall-clock gate as the analyzer histories (burst-proof on focus regain).

use std::collections::VecDeque;
use std::f32::consts::TAU;
use std::time::Instant;

use eframe::egui::{self, Color32, Pos2, Rect, Stroke};

use crate::curve::VizData;
use crate::theme;

const STEP: std::time::Duration = std::time::Duration::from_millis(16);
const DT: f32 = 1.0 / 60.0;
const TRAIL_FRAMES: usize = 14;
const TRAIL_POINTS: usize = 320;
const MAX_PARTICLES: usize = 512;
const BARS: usize = 48;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShowMode {
    Off,
    BarsXl,
    ScopeTrails,
    Tunnel,
    Particles,
    GlWarp,
    GlFlame,
    GlSmoke,
}

impl ShowMode {
    pub const ALL: [ShowMode; 8] = [
        ShowMode::Off,
        ShowMode::BarsXl,
        ShowMode::ScopeTrails,
        ShowMode::Tunnel,
        ShowMode::Particles,
        ShowMode::GlWarp,
        ShowMode::GlFlame,
        ShowMode::GlSmoke,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ShowMode::Off => "Off",
            ShowMode::BarsXl => "Bars XL",
            ShowMode::ScopeTrails => "Scope trails",
            ShowMode::Tunnel => "Tunnel",
            ShowMode::Particles => "Particles",
            ShowMode::GlWarp => "Warp (GL)",
            ShowMode::GlFlame => "Flame (GL)",
            ShowMode::GlSmoke => "Smoke (GL)",
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ShowMode::Off => "off",
            ShowMode::BarsXl => "bars_xl",
            ShowMode::ScopeTrails => "scope_trails",
            ShowMode::Tunnel => "tunnel",
            ShowMode::Particles => "particles",
            ShowMode::GlWarp => "gl_warp",
            ShowMode::GlFlame => "gl_flame",
            ShowMode::GlSmoke => "gl_smoke",
        }
    }
    pub fn from_str(s: &str) -> ShowMode {
        match s {
            "bars_xl" => ShowMode::BarsXl,
            "scope_trails" => ShowMode::ScopeTrails,
            "tunnel" => ShowMode::Tunnel,
            "particles" => ShowMode::Particles,
            "gl_warp" => ShowMode::GlWarp,
            "gl_flame" => ShowMode::GlFlame,
            "gl_smoke" => ShowMode::GlSmoke,
            _ => ShowMode::Off,
        }
    }
    /// GL-stage modes are rendered by glstage, not the painter.
    pub fn gl_mode(self) -> Option<crate::glstage::GlMode> {
        match self {
            ShowMode::GlWarp => Some(crate::glstage::GlMode::Warp),
            ShowMode::GlFlame => Some(crate::glstage::GlMode::Flame),
            ShowMode::GlSmoke => Some(crate::glstage::GlMode::Smoke),
            _ => None,
        }
    }
}

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max_life: f32,
    hue: f32,
    size: f32,
}

pub struct ShowState {
    pub mode: ShowMode,
    last_step: Instant,
    rng: u32,

    // Beat feel: slow bass envelope + a fast normalized pulse on kicks.
    env: f32,
    pulse: f32,
    burst_cooldown: f32,

    // Scope trails: ring of decimated waveform frames (phosphor ghosts).
    trails: VecDeque<Vec<f32>>,

    // Tunnel: rotation phase + expanding echo-ring ages (0..1).
    phase: f32,
    rings: Vec<f32>,

    // Particles: fixed pool, dead slots reused (life <= 0 = dead).
    parts: Vec<Particle>,
}

impl ShowState {
    pub fn new(mode: ShowMode) -> Self {
        ShowState {
            mode,
            last_step: Instant::now(),
            rng: 0x1234_5678,
            env: 0.0,
            pulse: 0.0,
            burst_cooldown: 0.0,
            trails: VecDeque::with_capacity(TRAIL_FRAMES),
            phase: 0.0,
            rings: Vec::with_capacity(6),
            parts: Vec::with_capacity(MAX_PARTICLES),
        }
    }

    /// Deterministic tiny LCG (no rand dep). Returns 0..1.
    fn rand(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 16) as f32 / 65_535.0
    }

    /// Draw the active mode as the deepest canvas layer. Advances the animation
    /// on a 16ms wall-clock gate internally; safe to call every frame.
    pub fn draw(&mut self, painter: &egui::Painter, rect: Rect, viz: &VizData, rainbow: bool) {
        if self.mode == ShowMode::Off || self.mode.gl_mode().is_some() {
            return;
        }
        let now = Instant::now();
        if now.duration_since(self.last_step) >= STEP {
            self.last_step = now;
            self.step(viz, rect);
        }
        match self.mode {
            ShowMode::Off | ShowMode::GlWarp | ShowMode::GlFlame | ShowMode::GlSmoke => {}
            ShowMode::BarsXl => self.draw_bars_xl(painter, rect, viz.spectrum, rainbow),
            ShowMode::ScopeTrails => self.draw_scope_trails(painter, rect, rainbow),
            ShowMode::Tunnel => self.draw_tunnel(painter, rect, viz.spectrum, rainbow),
            ShowMode::Particles => self.draw_particles(painter, rect),
        }
    }

    /// One 60Hz animation step: beat detection + per-mode simulation.
    fn step(&mut self, viz: &VizData, rect: Rect) {
        // Bass = mean of the low quarter of the log-spaced bins (~20-110 Hz).
        let n = viz.spectrum.len().max(1);
        let low = (n / 4).max(1);
        let bass: f32 = viz.spectrum.iter().take(low).sum::<f32>() / low as f32;
        self.env = self.env * 0.94 + bass * 0.06;
        let raw = ((bass - self.env * 1.25) * 4.0).clamp(0.0, 1.0);
        self.pulse = (self.pulse * 0.86).max(raw);
        self.burst_cooldown = (self.burst_cooldown - DT).max(0.0);
        let kicked = self.pulse > 0.55 && self.burst_cooldown <= 0.0;
        if kicked {
            self.burst_cooldown = 0.22;
        }

        match self.mode {
            ShowMode::ScopeTrails => {
                // Push a decimated copy of the newest waveform frame.
                let w = viz.waveform;
                if w.len() >= 2 {
                    let step = (w.len() / TRAIL_POINTS).max(1);
                    let frame: Vec<f32> = w.iter().step_by(step).copied().collect();
                    self.trails.push_back(frame);
                    while self.trails.len() > TRAIL_FRAMES {
                        self.trails.pop_front();
                    }
                }
            }
            ShowMode::Tunnel => {
                self.phase = (self.phase + DT * (0.15 + bass * 0.9)) % TAU;
                for a in self.rings.iter_mut() {
                    *a += DT * 0.45;
                }
                self.rings.retain(|a| *a < 1.0);
                if kicked && self.rings.len() < 6 {
                    self.rings.push(0.0);
                }
            }
            ShowMode::Particles => {
                // Spectral fountain: hot bins launch particles from the floor.
                let stride = (n / BARS).max(1);
                let mut i = 0;
                while i < n {
                    let v = viz.spectrum[i].clamp(0.0, 1.0);
                    if v > 0.18 && self.rand() < v * 0.5 {
                        let x = rect.left() + (i as f32 + 0.5) / n as f32 * rect.width();
                        let vy = -(140.0 + v * 620.0 + self.pulse * 180.0);
                        let vx = (self.rand() - 0.5) * 70.0;
                        let life = 1.0 + self.rand() * 1.2;
                        let hue = i as f32 / n as f32;
                        let size = 2.0 + v * 3.0;
                        self.spawn(x, rect.bottom() - 2.0, vx, vy, life, hue, size);
                    }
                    i += stride;
                }
                // Radial burst from center on a kick.
                if kicked {
                    let c = rect.center();
                    for _ in 0..36 {
                        let ang = self.rand() * TAU;
                        let speed = 190.0 + self.rand() * 280.0;
                        let hue = self.rand();
                        let life = 0.7 + self.rand() * 0.8;
                        self.spawn(c.x, c.y, ang.cos() * speed, ang.sin() * speed, life, hue, 2.5);
                    }
                }
                // Integrate.
                for p in self.parts.iter_mut() {
                    if p.life <= 0.0 {
                        continue;
                    }
                    p.vy += 380.0 * DT; // gravity
                    p.x += p.vx * DT;
                    p.y += p.vy * DT;
                    p.life -= DT;
                }
            }
            _ => {}
        }
    }

    fn spawn(&mut self, x: f32, y: f32, vx: f32, vy: f32, life: f32, hue: f32, size: f32) {
        let p = Particle { x, y, vx, vy, life, max_life: life, hue, size };
        if let Some(slot) = self.parts.iter_mut().find(|p| p.life <= 0.0) {
            *slot = p;
        } else if self.parts.len() < MAX_PARTICLES {
            self.parts.push(p);
        }
    }

    /// Fat mirrored bars from the vertical center — WMP9 energy. Bright caps,
    /// gradient body, dimmer reflection, whole thing bounces on bass.
    fn draw_bars_xl(&self, painter: &egui::Painter, rect: Rect, spectrum: &[f32], rainbow: bool) {
        if spectrum.len() < 2 {
            return;
        }
        let n = spectrum.len();
        let group = (n / BARS).max(1);
        let bars = n / group;
        let mid = rect.center().y;
        let scale = 1.0 + self.pulse * 0.18;
        let up_max = rect.height() * 0.44 * scale;
        let slot = rect.width() / bars as f32;
        let gap = (slot * 0.22).clamp(1.0, 6.0);
        let mut mesh = egui::Mesh::default();

        for b in 0..bars {
            let v = spectrum[b * group..(b * group + group).min(n)]
                .iter()
                .fold(0.0f32, |m, &x| m.max(x))
                .clamp(0.0, 1.0);
            if v <= 0.01 {
                continue;
            }
            let base = if rainbow {
                theme::viz_hsv(b as f32 / bars as f32)
            } else {
                theme::viz_accent()
            };
            let x0 = rect.left() + b as f32 * slot + gap * 0.5;
            let x1 = rect.left() + (b + 1) as f32 * slot - gap * 0.5;
            let h = v * up_max;

            // Body: bright at the center line, fading toward the tip.
            quad(&mut mesh, x0, x1, mid - h, mid, rgba(base, 60), rgba(base, 165));
            // Cap: hot 3px line at the tip.
            let cap = if rainbow {
                theme::viz_hsv_cap(b as f32 / bars as f32)
            } else {
                theme::viz_cap()
            };
            quad(&mut mesh, x0, x1, mid - h - 3.0, mid - h, rgba(cap, 235), rgba(cap, 235));
            // Reflection: dimmer, shorter, below the line.
            let rh = h * 0.55;
            quad(&mut mesh, x0, x1, mid, mid + rh, rgba(base, 70), rgba(base, 8));
        }
        painter.add(egui::Shape::mesh(mesh));
    }

    /// Phosphor oscilloscope: ghost frames trail behind the live waveform,
    /// fading out and drifting hue.
    fn draw_scope_trails(&self, painter: &egui::Painter, rect: Rect, rainbow: bool) {
        let count = self.trails.len();
        if count == 0 {
            return;
        }
        let mid = rect.center().y;
        let amp = rect.height() * 0.36 * (1.0 + self.pulse * 0.12);
        for (i, frame) in self.trails.iter().enumerate() {
            if frame.len() < 2 {
                continue;
            }
            let t = (i + 1) as f32 / count as f32; // 0=oldest .. 1=newest
            let a = (10.0 + 195.0 * t * t) as u8;
            let col = if rainbow {
                rgba(theme::viz_hsv(0.52 + 0.28 * (1.0 - t)), a)
            } else {
                theme::glow_core(a)
            };
            let m = frame.len();
            let pts: Vec<Pos2> = frame
                .iter()
                .enumerate()
                .map(|(j, s)| {
                    Pos2::new(
                        rect.left() + j as f32 / (m - 1) as f32 * rect.width(),
                        mid - s.clamp(-1.0, 1.0) * amp,
                    )
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(0.9 + 1.5 * t, col)));
        }
    }

    /// Radial spectrum ring: log-freq wraps the circle, magnitude points
    /// outward, rotation rides the bass, echo rings expand on kicks.
    fn draw_tunnel(&self, painter: &egui::Painter, rect: Rect, spectrum: &[f32], rainbow: bool) {
        if spectrum.len() < 2 {
            return;
        }
        let n = spectrum.len();
        let c = rect.center();
        let m = rect.width().min(rect.height());
        let r_in = m * 0.12 * (1.0 + self.pulse * 0.35);
        let r_max = m * 0.46;
        let half_w = TAU / n as f32 * 0.42;
        let mut mesh = egui::Mesh::default();

        for (i, &v) in spectrum.iter().enumerate() {
            let vv = v.clamp(0.0, 1.0);
            if vv <= 0.01 {
                continue;
            }
            let base = if rainbow {
                theme::viz_hsv(i as f32 / n as f32)
            } else {
                theme::viz_accent()
            };
            let ang = self.phase + i as f32 / n as f32 * TAU;
            let r_out = r_in + vv * (r_max - r_in);
            let (a0, a1) = (ang - half_w, ang + half_w);
            let p = |r: f32, a: f32| Pos2::new(c.x + a.cos() * r, c.y + a.sin() * r);
            let idx = mesh.vertices.len() as u32;
            mesh.colored_vertex(p(r_in, a0), rgba(base, 190));
            mesh.colored_vertex(p(r_in, a1), rgba(base, 190));
            mesh.colored_vertex(p(r_out, a1), rgba(base, 22));
            mesh.colored_vertex(p(r_out, a0), rgba(base, 22));
            mesh.add_triangle(idx, idx + 1, idx + 2);
            mesh.add_triangle(idx, idx + 2, idx + 3);
        }
        painter.add(egui::Shape::mesh(mesh));

        // Expanding echo rings (one per detected kick).
        for &age in &self.rings {
            let r = r_in + age * (r_max * 1.05 - r_in);
            let a = ((1.0 - age) * 70.0) as u8;
            let pts: Vec<Pos2> = (0..=64)
                .map(|k| {
                    let ang = k as f32 / 64.0 * TAU;
                    Pos2::new(c.x + ang.cos() * r, c.y + ang.sin() * r)
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.5, theme::glow_core(a))));
        }
    }

    /// Spectral fountain + kick bursts, one mesh for the whole pool.
    fn draw_particles(&self, painter: &egui::Painter, rect: Rect) {
        let mut mesh = egui::Mesh::default();
        for p in &self.parts {
            if p.life <= 0.0 || !rect.contains(Pos2::new(p.x, p.y)) {
                continue;
            }
            let frac = (p.life / p.max_life).clamp(0.0, 1.0);
            let col = rgba(theme::viz_hsv(p.hue), (230.0 * frac) as u8);
            let r = p.size * (0.6 + 0.4 * frac);
            let idx = mesh.vertices.len() as u32;
            mesh.colored_vertex(Pos2::new(p.x - r, p.y - r), col);
            mesh.colored_vertex(Pos2::new(p.x + r, p.y - r), col);
            mesh.colored_vertex(Pos2::new(p.x + r, p.y + r), col);
            mesh.colored_vertex(Pos2::new(p.x - r, p.y + r), col);
            mesh.add_triangle(idx, idx + 1, idx + 2);
            mesh.add_triangle(idx, idx + 2, idx + 3);
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}

fn rgba(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Axis-aligned gradient quad: `top_col` at y0 fading to `bot_col` at y1.
fn quad(mesh: &mut egui::Mesh, x0: f32, x1: f32, y0: f32, y1: f32, top_col: Color32, bot_col: Color32) {
    let idx = mesh.vertices.len() as u32;
    mesh.colored_vertex(Pos2::new(x0, y0), top_col);
    mesh.colored_vertex(Pos2::new(x1, y0), top_col);
    mesh.colored_vertex(Pos2::new(x1, y1), bot_col);
    mesh.colored_vertex(Pos2::new(x0, y1), bot_col);
    mesh.add_triangle(idx, idx + 1, idx + 2);
    mesh.add_triangle(idx, idx + 2, idx + 3);
}
