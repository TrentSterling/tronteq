//! Scrolling FFT-history waterfall. Each GUI frame a new spectrum column is
//! pushed; the texture maps frequency -> X (log-spaced, matching the EQ curve),
//! time -> Y (newest at top, scrolling down), intensity -> color. Built into a
//! texture so it draws as a single backdrop image behind the curve.

use std::collections::VecDeque;

use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureId, TextureOptions};

use crate::theme;

const ROWS: usize = 180; // time history depth

pub struct Spectrogram {
    rows: VecDeque<Vec<f32>>,
    tex: Option<TextureHandle>,
}

impl Spectrogram {
    pub fn new() -> Self {
        Self { rows: VecDeque::with_capacity(ROWS), tex: None }
    }

    /// Push the latest spectrum bars as the newest (top) row.
    pub fn push(&mut self, bars: &[f32]) {
        if bars.len() < 2 {
            return;
        }
        self.rows.push_front(bars.to_vec());
        while self.rows.len() > ROWS {
            self.rows.pop_back();
        }
    }

    /// Rebuild + upload the texture, returning its id for `painter.image`.
    pub fn texture(&mut self, ctx: &egui::Context, rainbow: bool) -> Option<TextureId> {
        let w = self.rows.front()?.len();
        let h = ROWS;
        let mut bytes = vec![0u8; w * h * 4];
        for (r, row) in self.rows.iter().enumerate() {
            for (c, &v) in row.iter().enumerate() {
                let col = color_for(c, w, v, rainbow);
                let o = (r * w + c) * 4;
                bytes[o] = col.r();
                bytes[o + 1] = col.g();
                bytes[o + 2] = col.b();
                bytes[o + 3] = col.a();
            }
        }
        let img = ColorImage::from_rgba_unmultiplied([w, h], &bytes);
        let tex = self
            .tex
            .get_or_insert_with(|| ctx.load_texture("tronteq-spectro", img.clone(), TextureOptions::LINEAR));
        tex.set(img, TextureOptions::LINEAR);
        Some(tex.id())
    }
}

/// Map a bar's intensity (0..1) to a color. Rainbow tints by frequency column;
/// otherwise an electric-cyan ramp. Alpha scales with intensity so quiet bins
/// fade out and the curve stays readable; loud bins push toward white.
fn color_for(col: usize, w: usize, v: f32, rainbow: bool) -> Color32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.004 {
        return Color32::TRANSPARENT;
    }
    let (mut r, mut g, mut b) = if rainbow {
        let c = theme::hsv(col as f32 / w as f32, 0.85, 1.0);
        (c.r(), c.g(), c.b())
    } else {
        (0u8, 224, 255)
    };
    // hot core: lift toward white for the loudest bins
    let hot = ((v - 0.7).max(0.0) / 0.3 * 200.0) as u8;
    r = r.saturating_add(hot);
    g = g.saturating_add(hot / 2);
    b = b.saturating_add(hot / 4);
    let a = (v * 235.0) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}
