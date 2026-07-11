//! Scoped frame profiler: a fixed, enum-indexed set of stopwatches that breaks
//! the per-frame CPU budget into named scopes. Pattern ported from Boxel's
//! profiler.rs (same author): main-thread only, ZERO per-frame heap (fixed
//! array on App), EMA for steady-state + raw last-frame so spikes survive the
//! smoothing. Toggle the overlay with F10.
//!
//! We run the glow/GL backend, which has no GPU timestamps — every number here
//! is CPU-side; the GPU fill cost is `frame_ms - update` in your head.

use std::time::Duration;

/// One timed region. `Count` is the sentinel array length — keep it last.
#[repr(usize)]
#[derive(Clone, Copy)]
pub enum Scope {
    /// Whole update() CPU.
    Update,
    /// Toolbar + bottom bar + inspector panels.
    Panels,
    /// History advance (peaks / loudness / spectrogram push).
    Histories,
    /// Central canvas: curve + analyzers + SHOW mode draw.
    Canvas,
    Count,
}

const N: usize = Scope::Count as usize;
const NAMES: [&str; N] = ["update", "panels", "histories", "canvas"];

#[derive(Clone, Copy, Default)]
struct Accum {
    /// Nanoseconds summed within the current frame.
    frame_ns: u64,
    /// EMA-smoothed milliseconds across frames (the steady-state read).
    ms: f32,
    /// Raw last-frame milliseconds, so spikes survive the EMA.
    last_ms: f32,
}

pub struct Profiler {
    acc: [Accum; N],
    /// Overlay visibility (F10). Default off; costs one branch per add when off.
    pub overlay: bool,
}

impl Default for Profiler {
    fn default() -> Self {
        Profiler { acc: [Accum::default(); N], overlay: false }
    }
}

impl Profiler {
    /// Add an elapsed duration to a scope (accumulates within the frame).
    pub fn add(&mut self, scope: Scope, elapsed: Duration) {
        self.acc[scope as usize].frame_ns += elapsed.as_nanos() as u64;
    }

    /// (EMA ms, last-frame ms) for a scope — the DATA tab readout.
    pub fn ms(&self, scope: Scope) -> (f32, f32) {
        let a = self.acc[scope as usize];
        (a.ms, a.last_ms)
    }

    /// Fold this frame's accumulations into the EMAs. Call once per frame,
    /// after every scope has closed.
    pub fn commit_frame(&mut self) {
        for a in self.acc.iter_mut() {
            let ms = a.frame_ns as f32 / 1.0e6;
            a.frame_ns = 0;
            a.last_ms = ms;
            a.ms = if a.ms <= 0.0 { ms } else { a.ms * 0.92 + ms * 0.08 };
        }
    }

    /// Draw the overlay (top-left of the canvas area). `frame_ms` is eframe's
    /// whole-frame CPU EMA so the GPU-ish remainder is visible as the gap.
    pub fn draw_overlay(&self, ctx: &eframe::egui::Context, frame_ms: f32) {
        use eframe::egui::{self, Align2, Color32, FontId};
        if !self.overlay {
            return;
        }
        egui::Area::new(egui::Id::new("tronteq-profiler"))
            .anchor(Align2::LEFT_TOP, [8.0, 76.0])
            .interactable(false)
            .show(ctx, |ui| {
                let p = ui.painter();
                let line_h = 15.0;
                let w = 195.0;
                let h = line_h * (N as f32 + 2.0) + 10.0;
                let rect = egui::Rect::from_min_size(ui.next_widget_position(), egui::vec2(w, h));
                p.rect_filled(rect, 4.0, Color32::from_rgba_unmultiplied(0, 0, 0, 170));
                let font = FontId::monospace(12.0);
                let mut y = rect.top() + 5.0;
                p.text(
                    egui::pos2(rect.left() + 7.0, y),
                    Align2::LEFT_TOP,
                    format!("frame {frame_ms:5.2} ms (F10)"),
                    font.clone(),
                    Color32::WHITE,
                );
                y += line_h;
                for (i, a) in self.acc.iter().enumerate() {
                    p.text(
                        egui::pos2(rect.left() + 7.0, y),
                        Align2::LEFT_TOP,
                        format!("{:<10} {:5.2} | {:5.2}", NAMES[i], a.ms, a.last_ms),
                        font.clone(),
                        Color32::from_rgb(150, 240, 255),
                    );
                    y += line_h;
                }
                p.text(
                    egui::pos2(rect.left() + 7.0, y),
                    Align2::LEFT_TOP,
                    "ema | last  (CPU only)",
                    FontId::monospace(10.0),
                    Color32::from_rgb(140, 150, 160),
                );
                ui.allocate_rect(rect, egui::Sense::hover());
            });
    }
}
