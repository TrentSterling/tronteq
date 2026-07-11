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
    dirty: bool,        // a new row landed (or color mode changed) since last upload
    last_rainbow: bool, // detect a rainbow toggle so we recolor on the next upload
    last_dark: bool,    // detect a theme flip so the ramp recolors too
}

impl Spectrogram {
    pub fn new() -> Self {
        Self {
            rows: VecDeque::with_capacity(ROWS),
            tex: None,
            dirty: false,
            last_rainbow: true,
            last_dark: true,
        }
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
        self.dirty = true;
    }

    /// Return the texture id for `painter.image`, rebuilding + re-uploading only
    /// when a new row landed or the color mode flipped. Skipping redundant uploads
    /// is what keeps GPU work down between history steps (and while unfocused).
    pub fn texture(&mut self, ctx: &egui::Context, rainbow: bool) -> Option<TextureId> {
        if rainbow != self.last_rainbow {
            self.last_rainbow = rainbow;
            self.dirty = true;
        }
        let dark = theme::dark_mode();
        if dark != self.last_dark {
            self.last_dark = dark;
            self.dirty = true;
        }
        if !self.dirty {
            if let Some(t) = &self.tex {
                return Some(t.id());
            }
        }
        let w = self.rows.front()?.len();
        let h = ROWS;
        let mut bytes = vec![0u8; w * h * 4];
        for (r, row) in self.rows.iter().enumerate() {
            // Clamp to `w` (the newest row's width / texture stride). Older rows that
            // differ in length (a future bar-count change, or a transient short/empty
            // spectrum) would otherwise let `c` run past the stride and write out of
            // `bytes` -> an instant abort on the GUI thread.
            for (c, &v) in row.iter().take(w).enumerate() {
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
        self.dirty = false;
        Some(tex.id())
    }
}

/// Map a bar's intensity (0..1) to a color. Rainbow tints by frequency column;
/// otherwise a cyan ramp (electric on dark, deep teal on light). Alpha scales
/// with intensity so quiet bins fade to the canvas. Loud bins push toward white
/// on dark and toward black on light — "hot" reads correctly on both papers.
fn color_for(col: usize, w: usize, v: f32, rainbow: bool) -> Color32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.004 {
        return Color32::TRANSPARENT;
    }
    let dark = theme::dark_mode();
    let (mut r, mut g, mut b) = if rainbow {
        let c = theme::viz_hsv(col as f32 / w as f32);
        (c.r(), c.g(), c.b())
    } else {
        let c = theme::viz_accent();
        (c.r(), c.g(), c.b())
    };
    if dark {
        let hot = ((v - 0.7).max(0.0) / 0.3 * 200.0) as u8;
        r = r.saturating_add(hot);
        g = g.saturating_add(hot / 2);
        b = b.saturating_add(hot / 4);
    } else {
        let hot = ((v - 0.7).max(0.0) / 0.3 * 90.0) as u8;
        r = r.saturating_sub(hot);
        g = g.saturating_sub(hot / 2);
        b = b.saturating_sub(hot / 4);
    }
    let a = (v * 235.0) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}
