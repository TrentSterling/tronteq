//! Draggable 8-band curve on a log-frequency / linear-dB canvas.
//! Uses egui's low-level painter for full control. Styled to the tront.xyz
//! electric-cyan theme: glowing curve, gradient fill, neon nodes.

use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use tronteq_shared::{Band, BandKind, NUM_BANDS};

use crate::dsp_preview;
use crate::theme;

/// Stackable realtime overlays drawn behind the EQ curve. Any combination can be
/// on at once. `peak_hold` is a modifier on `spectrum`.
#[derive(Clone, Copy)]
pub struct Layers {
    pub spectrum: bool,
    pub peak_hold: bool,
    pub analyzer: bool,
    pub waterfall: bool,
    pub waveform: bool,
    pub goniometer: bool,
    pub loudness: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Self {
            spectrum: true,
            peak_hold: true,
            analyzer: false,
            waterfall: false,
            waveform: false,
            goniometer: false,
            loudness: false,
        }
    }
}

/// All the realtime data the overlays need this frame.
pub struct VizData<'a> {
    pub layers: Layers,
    pub spectrum: &'a [f32],
    pub peaks: &'a [f32],
    pub waveform: &'a [f32],
    pub stereo: &'a [[f32; 2]],
    pub loudness: &'a [f32],
    pub spectro_tex: Option<egui::TextureId>,
}

const F_MIN: f32 = 20.0;
const F_MAX: f32 = 20_000.0;
const DB_RANGE: f32 = 24.0; // -DB_RANGE..=+DB_RANGE on Y
const NODE_RADIUS: f32 = 7.5;
const NODE_HIT_RADIUS: f32 = 22.0; // generous grab area (visual node is smaller)

pub struct DrawResponse {
    pub changed: bool,
}

pub fn draw(
    ui: &mut egui::Ui,
    bands: &mut [Band; NUM_BANDS],
    sample_rate: f32,
    rainbow: bool,
    viz: &VizData,
) -> DrawResponse {
    let avail = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(avail, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    let mut changed = false;

    // Background + faint neon frame.
    painter.rect_filled(rect, 6.0, theme::BG);
    draw_grid(&painter, rect);

    // Stackable realtime overlays (any combination), back to front.
    let l = viz.layers;
    if l.waterfall {
        if let Some(tex) = viz.spectro_tex {
            draw_spectro(&painter, rect, tex);
        }
    }
    if l.spectrum {
        let peaks = if l.peak_hold { Some(viz.peaks) } else { None };
        draw_spectrum(&painter, rect, viz.spectrum, peaks, rainbow);
    }
    if l.analyzer {
        draw_analyzer(&painter, rect, viz.spectrum, rainbow);
    }
    if l.waveform {
        draw_waveform(&painter, rect, viz.waveform, rainbow);
    }
    if l.loudness {
        draw_loudness(&painter, rect, viz.loudness, rainbow);
    }
    if l.goniometer {
        draw_goniometer(&painter, rect, viz.stereo);
    }

    // Composite response curve (computed by evaluating all 8 biquads).
    draw_composite(&painter, rect, bands, sample_rate, rainbow);

    // Draggable nodes.
    let pointer = response.interact_pointer_pos();
    let shift = ui.input(|i| i.modifiers.shift);
    let ctrl = ui.input(|i| i.modifiers.ctrl);

    let drag_started = response.drag_started();
    let dragging = response.dragged();
    let hover_pos = response.hover_pos();

    let id = response.id;
    let mut drag_idx: Option<usize> = ui.ctx().memory(|m| m.data.get_temp(id)).unwrap_or(None);

    if drag_started {
        if let Some(pos) = pointer {
            drag_idx = nearest_node(bands, rect, pos);
            ui.ctx().memory_mut(|m| m.data.insert_temp(id, drag_idx));
        }
    }
    if !dragging {
        drag_idx = None;
        ui.ctx().memory_mut(|m| m.data.insert_temp(id, None::<usize>));
    }

    if let (Some(idx), Some(pos), true) = (drag_idx, pointer, dragging) {
        let b = &mut bands[idx];
        if ctrl {
            // Ctrl+drag slides the band along frequency (gain held).
            let f = pixel_to_freq(pos.x, rect).clamp(F_MIN, F_MAX);
            if f != b.freq {
                b.freq = f;
                changed = true;
            }
        } else if shift {
            // Q via vertical drag: up=narrower, down=wider.
            let dy = response.drag_delta().y;
            let q = (b.q + -dy * 0.02).clamp(0.1, 10.0);
            if q != b.q {
                b.q = q;
                changed = true;
            }
        } else {
            // Standard: gain via Y position.
            let db = pixel_to_db(pos.y, rect).clamp(-DB_RANGE, DB_RANGE);
            if db != b.gain {
                b.gain = db;
                changed = true;
            }
        }
    }

    // Draw nodes on top of curve.
    let mut cycle_kind_idx: Option<usize> = None;
    for i in 0..NUM_BANDS {
        let (freq, gain, q, kind) = {
            let b = &bands[i];
            (b.freq, b.gain, b.q, b.kind)
        };
        let p = pos_from(freq, gain, rect);
        let color = if rainbow {
            theme::hsv(i as f32 / NUM_BANDS as f32, 0.85, 1.0)
        } else {
            band_color_from(kind)
        };
        let hovered = hover_pos.map_or(false, |hp| (hp - p).length() < NODE_HIT_RADIUS);

        // Scroll over a node tweaks its Q (more discoverable than shift+drag).
        if hovered {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                bands[i].q = (bands[i].q + scroll.signum() * 0.3).clamp(0.1, 10.0);
                changed = true;
            }
        }

        // Neon glow halo, brighter on hover.
        let halo = if hovered { 80 } else { 44 };
        painter.circle_filled(p, NODE_RADIUS + 7.0, rgba(color, halo / 2));
        painter.circle_filled(p, NODE_RADIUS + 3.0, rgba(color, halo));
        painter.circle_filled(p, NODE_RADIUS, color);
        painter.circle_stroke(p, NODE_RADIUS, Stroke::new(1.5, Color32::WHITE));
        painter.circle_filled(p, 2.0, Color32::WHITE);

        if hovered {
            let txt = format!(
                "{}\n{:.0} Hz\n{:+.1} dB\nQ {:.2}",
                kind_short(kind),
                freq,
                gain,
                q
            );
            let pos = Pos2::new(p.x + 13.0, p.y - 30.0);
            painter.rect_filled(
                Rect::from_min_size(pos, Vec2::new(94.0, 64.0)),
                4.0,
                Color32::from_rgba_unmultiplied(6, 16, 24, 230),
            );
            painter.text(
                Pos2::new(pos.x + 7.0, pos.y + 5.0),
                Align2::LEFT_TOP,
                txt,
                FontId::proportional(11.0),
                theme::TEXT,
            );
        }

        let node_rect = Rect::from_center_size(p, Vec2::splat(NODE_HIT_RADIUS * 2.0));
        let node_resp = ui.interact(node_rect, egui::Id::new(("tronteq-node", i)), Sense::click());
        if node_resp.secondary_clicked() {
            cycle_kind_idx = Some(i);
        }
        if node_resp.double_clicked() {
            // Easy reset: flatten this band in place (gain 0, Q 1).
            bands[i].gain = 0.0;
            bands[i].q = 1.0;
            changed = true;
        }
    }
    if let Some(i) = cycle_kind_idx {
        bands[i].kind = BandKind::from_u32(bands[i].kind).next() as u32;
        changed = true;
    }

    DrawResponse { changed }
}

fn rgba(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Draw a connected polyline as ONE `Shape::line` (or a few hue chunks for the
/// rainbow sweep) instead of N separate `line_segment` calls. N disjoint
/// feathered segments was the geometry bomb that blew the GPU index buffer
/// (~67k indices → staging-buffer alloc failure → panic=abort) on 2026-06-06: a
/// connected line shares vertices at its joints and is a single draw, so this
/// also lowers the steady wgpu buffer high-water-mark. For rainbow we split into
/// a small fixed number of connected hue chunks (overlapping by a point so they
/// join) rather than one segment per hue.
fn glow_line(painter: &egui::Painter, pts: &[Pos2], width: f32, alpha: u8, rainbow: bool) {
    if pts.len() < 2 {
        return;
    }
    if !rainbow {
        let col = Color32::from_rgba_unmultiplied(150, 240, 255, alpha);
        painter.add(egui::Shape::line(pts.to_vec(), Stroke::new(width, col)));
        return;
    }
    const CHUNKS: usize = 24;
    let n = pts.len();
    let step = (n / CHUNKS).max(1);
    let mut i = 0;
    while i < n - 1 {
        let end = (i + step).min(n - 1);
        let hue = theme::hsv((i as f32 + 0.5) / n as f32, 0.85, 1.0);
        painter.add(egui::Shape::line(
            pts[i..=end].to_vec(),
            Stroke::new(width, rgba(hue, alpha)),
        ));
        i = end;
    }
}

fn band_color_from(kind: u32) -> Color32 {
    match BandKind::from_u32(kind) {
        BandKind::Peak => Color32::from_rgb(80, 220, 255),
        BandKind::LowShelf => Color32::from_rgb(255, 180, 120),
        BandKind::HighShelf => Color32::from_rgb(200, 170, 255),
        BandKind::HighPass => Color32::from_rgb(255, 120, 120),
        BandKind::LowPass => Color32::from_rgb(120, 255, 160),
        BandKind::BandPass => Color32::from_rgb(255, 230, 110),
        BandKind::Notch => Color32::from_rgb(255, 120, 200),
        BandKind::AllPass => Color32::from_rgb(170, 170, 190),
    }
}

fn pos_from(freq: f32, gain: f32, rect: Rect) -> Pos2 {
    Pos2::new(freq_to_pixel(freq, rect), db_to_pixel(gain, rect))
}
fn kind_short(k: u32) -> &'static str {
    BandKind::from_u32(k).short_label()
}

fn freq_to_pixel(f: f32, rect: Rect) -> f32 {
    let lf = f.clamp(F_MIN, F_MAX).log10();
    let lmin = F_MIN.log10();
    let lmax = F_MAX.log10();
    let t = (lf - lmin) / (lmax - lmin);
    rect.left() + t * rect.width()
}

fn pixel_to_freq(x: f32, rect: Rect) -> f32 {
    let t = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let lmin = F_MIN.log10();
    let lmax = F_MAX.log10();
    10f32.powf(lmin + t * (lmax - lmin))
}

fn db_to_pixel(db: f32, rect: Rect) -> f32 {
    let t = (db + DB_RANGE) / (2.0 * DB_RANGE); // 0 = bottom, 1 = top
    rect.bottom() - t * rect.height()
}

fn pixel_to_db(y: f32, rect: Rect) -> f32 {
    let t = 1.0 - ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);
    (t * 2.0 - 1.0) * DB_RANGE
}

fn band_point(b: &Band, rect: Rect) -> Pos2 {
    Pos2::new(freq_to_pixel(b.freq, rect), db_to_pixel(b.gain, rect))
}

fn nearest_node(bands: &[Band; NUM_BANDS], rect: Rect, pos: Pos2) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, b) in bands.iter().enumerate() {
        let p = band_point(b, rect);
        let d = (p - pos).length();
        if d < NODE_HIT_RADIUS {
            if best.map_or(true, |(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
    }
    best.map(|(i, _)| i)
}

fn draw_grid(painter: &egui::Painter, rect: Rect) {
    let grid = Color32::from_rgba_unmultiplied(0, 224, 255, 15);
    let zero = Color32::from_rgba_unmultiplied(0, 224, 255, 55);
    // Vertical: octaves.
    let mut f = 20.0;
    while f <= 20_000.0 {
        let x = freq_to_pixel(f, rect);
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, grid),
        );
        if (f - 20.0).abs() < 1e-3 || f == 100.0 || f == 1000.0 || f == 10000.0 {
            let label = if f < 1000.0 {
                format!("{:.0}", f)
            } else {
                format!("{:.0}k", f / 1000.0)
            };
            painter.text(
                Pos2::new(x + 4.0, rect.bottom() - 16.0),
                Align2::LEFT_TOP,
                label,
                FontId::proportional(10.0),
                theme::MUTED,
            );
        }
        f *= 2.0;
    }
    // Horizontal: 6 dB steps.
    let mut db = -DB_RANGE;
    while db <= DB_RANGE + 0.01 {
        let y = db_to_pixel(db, rect);
        let is_zero = db == 0.0;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(if is_zero { 1.3 } else { 1.0 }, if is_zero { zero } else { grid }),
        );
        if db as i32 % 6 == 0 {
            painter.text(
                Pos2::new(rect.left() + 5.0, y - 10.0),
                Align2::LEFT_TOP,
                format!("{:+.0} dB", db),
                FontId::proportional(10.0),
                theme::MUTED,
            );
        }
        db += 6.0;
    }
}

/// Draw the spectrogram history texture stretched across the canvas (freq -> X,
/// time -> Y). Quiet bins are transparent so the curve + grid stay readable.
fn draw_spectro(painter: &egui::Painter, rect: Rect, tex: egui::TextureId) {
    let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
    painter.image(tex, rect, uv, Color32::WHITE);
}

/// Realtime FFT spectrum as glowing log-spaced bars rising from the bottom.
/// Bars are already log-spaced (20 Hz..20 kHz), so bar index maps linearly to
/// the curve's log-frequency X axis. Each bar is a vertical gradient (hot top,
/// faint base) with a bright cap line.
fn draw_spectrum(
    painter: &egui::Painter,
    rect: Rect,
    bars: &[f32],
    peaks: Option<&[f32]>,
    rainbow: bool,
) {
    if bars.len() < 2 {
        return;
    }
    let n = bars.len();
    let bottom = rect.bottom();
    let max_h = rect.height() * 0.92;
    let slot = rect.width() / n as f32;
    let gap = (slot * 0.16).clamp(0.5, 3.0);
    let with_a = |c: Color32, a: u8| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);

    let mut mesh = egui::Mesh::default();
    for (b, &v) in bars.iter().enumerate() {
        let vv = v.clamp(0.0, 1.0);
        if vv <= 0.004 {
            continue;
        }
        let x0 = rect.left() + b as f32 * slot + gap * 0.5;
        let x1 = rect.left() + (b + 1) as f32 * slot - gap * 0.5;
        let top = bottom - vv * max_h;
        let base = if rainbow {
            theme::hsv(b as f32 / n as f32, 0.85, 1.0)
        } else {
            Color32::from_rgb(0, 224, 255)
        };
        let i = mesh.vertices.len() as u32;
        mesh.colored_vertex(Pos2::new(x0, top), with_a(base, 150));
        mesh.colored_vertex(Pos2::new(x1, top), with_a(base, 150));
        mesh.colored_vertex(Pos2::new(x1, bottom), with_a(base, 16));
        mesh.colored_vertex(Pos2::new(x0, bottom), with_a(base, 16));
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    painter.add(egui::Shape::mesh(mesh));

    // Bright caps on top of each bar.
    for (b, &v) in bars.iter().enumerate() {
        let vv = v.clamp(0.0, 1.0);
        if vv <= 0.004 {
            continue;
        }
        let x0 = rect.left() + b as f32 * slot + gap * 0.5;
        let x1 = rect.left() + (b + 1) as f32 * slot - gap * 0.5;
        let top = bottom - vv * max_h;
        let cap = if rainbow {
            theme::hsv(b as f32 / n as f32, 0.5, 1.0)
        } else {
            Color32::from_rgb(150, 245, 255)
        };
        painter.line_segment(
            [Pos2::new(x0, top), Pos2::new(x1, top)],
            Stroke::new(2.0, with_a(cap, 220)),
        );
    }

    // Peak-hold markers (slow-falling caps above the bars; decay is GUI-side).
    if let Some(peaks) = peaks {
        for (b, &pv) in peaks.iter().enumerate() {
            let vv = pv.clamp(0.0, 1.0);
            if vv <= 0.004 {
                continue;
            }
            let x0 = rect.left() + b as f32 * slot + gap * 0.5;
            let x1 = rect.left() + (b + 1) as f32 * slot - gap * 0.5;
            let y = bottom - vv * max_h;
            painter.line_segment(
                [Pos2::new(x0, y), Pos2::new(x1, y)],
                Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 210)),
            );
        }
    }
}

fn draw_waveform(painter: &egui::Painter, rect: Rect, wave: &[f32], rainbow: bool) {
    if wave.len() < 2 {
        return;
    }
    let mid = rect.center().y;
    let amp = rect.height() * 0.42; // full-scale -> 42% of canvas height
    let n = wave.len();
    let mut pts = Vec::with_capacity(n);
    for (i, s) in wave.iter().enumerate() {
        let x = rect.left() + (i as f32 / (n - 1) as f32) * rect.width();
        let y = mid - s.clamp(-1.0, 1.0) * amp;
        pts.push(Pos2::new(x, y));
    }
    let col = if rainbow {
        Color32::from_rgba_unmultiplied(255, 255, 255, 38)
    } else {
        Color32::from_rgba_unmultiplied(0, 224, 255, 46)
    };
    // One connected path, not ~2000 separate feathered segments.
    painter.add(egui::Shape::line(pts, Stroke::new(1.0, col)));
}

/// Smooth filled analyzer curve through the spectrum bar tops (FabFilter-style),
/// an alternative look to the bars.
fn draw_analyzer(painter: &egui::Painter, rect: Rect, bars: &[f32], rainbow: bool) {
    if bars.len() < 2 {
        return;
    }
    let n = bars.len();
    let bottom = rect.bottom();
    let max_h = rect.height() * 0.92;
    let px = |b: usize| rect.left() + (b as f32 + 0.5) / n as f32 * rect.width();
    let py = |v: f32| bottom - v.clamp(0.0, 1.0) * max_h;
    let hue = |b: usize| theme::hsv(b as f32 / n as f32, 0.85, 1.0);

    let mut mesh = egui::Mesh::default();
    for (b, &v) in bars.iter().enumerate() {
        let base = if rainbow { hue(b) } else { Color32::from_rgb(0, 224, 255) };
        mesh.colored_vertex(Pos2::new(px(b), py(v)), rgba(base, 70));
        mesh.colored_vertex(Pos2::new(px(b), bottom), rgba(base, 0));
    }
    for i in 0..n - 1 {
        let a = (2 * i) as u32;
        let b = (2 * i + 1) as u32;
        let c = (2 * (i + 1)) as u32;
        let d = (2 * (i + 1) + 1) as u32;
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    painter.add(egui::Shape::mesh(mesh));

    let pts: Vec<Pos2> = (0..n).map(|b| Pos2::new(px(b), py(bars[b]))).collect();
    for (w, a) in [(6.0_f32, 26_u8), (3.0, 80), (1.8, 255)] {
        glow_line(painter, &pts, w, a, rainbow);
    }
}

/// Stereo goniometer (Lissajous of L vs R) drawn as a small corner inset. Mid/side
/// rotation puts mono on the vertical axis, hard-panned content on the diagonals.
fn draw_goniometer(painter: &egui::Painter, rect: Rect, stereo: &[[f32; 2]]) {
    if stereo.len() < 2 {
        return;
    }
    let size = (rect.height() * 0.34).clamp(90.0, 190.0);
    let margin = 16.0;
    let center = Pos2::new(rect.left() + margin + size / 2.0, rect.top() + margin + size / 2.0);
    let half = size / 2.0;
    painter.rect_filled(
        Rect::from_center_size(center, Vec2::splat(size)),
        6.0,
        Color32::from_rgba_unmultiplied(4, 10, 16, 150),
    );
    let guide = Color32::from_rgba_unmultiplied(0, 224, 255, 45);
    painter.line_segment(
        [Pos2::new(center.x, center.y - half), Pos2::new(center.x, center.y + half)],
        Stroke::new(1.0, guide),
    );
    painter.line_segment(
        [Pos2::new(center.x - half, center.y), Pos2::new(center.x + half, center.y)],
        Stroke::new(1.0, guide),
    );

    // Plot every point as a tiny quad in ONE mesh rather than ~1000 separate
    // `circle_filled` fans (each its own feathered draw) — same dots, a fraction
    // of the geometry.
    let n = stereo.len();
    let r = 1.3;
    let mut dots = egui::Mesh::default();
    for (i, &[lf, rf]) in stereo.iter().enumerate() {
        let mid = (lf + rf) * 0.7071;
        let side = (lf - rf) * 0.7071;
        let x = center.x + side.clamp(-1.0, 1.0) * half;
        let y = center.y - mid.clamp(-1.0, 1.0) * half;
        let a = (40 + i * 180 / n).min(220) as u8;
        let col = Color32::from_rgba_unmultiplied(120, 240, 255, a);
        let v = dots.vertices.len() as u32;
        dots.colored_vertex(Pos2::new(x - r, y - r), col);
        dots.colored_vertex(Pos2::new(x + r, y - r), col);
        dots.colored_vertex(Pos2::new(x + r, y + r), col);
        dots.colored_vertex(Pos2::new(x - r, y + r), col);
        dots.add_triangle(v, v + 1, v + 2);
        dots.add_triangle(v, v + 2, v + 3);
    }
    painter.add(egui::Shape::mesh(dots));
    painter.text(
        Pos2::new(center.x, center.y - half - 3.0),
        Align2::CENTER_BOTTOM,
        "STEREO",
        FontId::proportional(9.0),
        theme::MUTED,
    );
}

/// Scrolling output-loudness history along the bottom of the canvas.
fn draw_loudness(painter: &egui::Painter, rect: Rect, hist: &[f32], rainbow: bool) {
    if hist.len() < 2 {
        return;
    }
    let n = hist.len();
    let h = rect.height() * 0.18;
    let bottom = rect.bottom();
    let to_db = |v: f32| 20.0 * v.max(1e-5).log10();
    let norm = |v: f32| ((to_db(v) + 54.0) / 54.0).clamp(0.0, 1.0);
    let px = |i: usize| rect.left() + i as f32 / (n - 1) as f32 * rect.width();
    let py = |v: f32| bottom - norm(v) * h;

    let mut mesh = egui::Mesh::default();
    for (i, &v) in hist.iter().enumerate() {
        let base = if rainbow {
            theme::hsv(0.55 - norm(v) * 0.45, 0.85, 1.0)
        } else {
            Color32::from_rgb(0, 224, 255)
        };
        mesh.colored_vertex(Pos2::new(px(i), py(v)), rgba(base, 95));
        mesh.colored_vertex(Pos2::new(px(i), bottom), rgba(base, 0));
    }
    for i in 0..n - 1 {
        let a = (2 * i) as u32;
        let b = (2 * i + 1) as u32;
        let c = (2 * (i + 1)) as u32;
        let d = (2 * (i + 1) + 1) as u32;
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    painter.add(egui::Shape::mesh(mesh));

    let pts: Vec<Pos2> = (0..n).map(|i| Pos2::new(px(i), py(hist[i]))).collect();
    painter.add(egui::Shape::line(
        pts,
        Stroke::new(1.5, Color32::from_rgba_unmultiplied(150, 240, 255, 200)),
    ));
}

fn draw_composite(
    painter: &egui::Painter,
    rect: Rect,
    bands: &[Band; NUM_BANDS],
    sample_rate: f32,
    rainbow: bool,
) {
    const STEPS: usize = 240;
    let baseline = db_to_pixel(0.0, rect);
    let mut pts = Vec::with_capacity(STEPS);
    for i in 0..STEPS {
        let t = i as f32 / (STEPS - 1) as f32;
        let x = rect.left() + t * rect.width();
        let f = pixel_to_freq(x, rect);
        let db = dsp_preview::composite_db(bands, f, sample_rate).clamp(-DB_RANGE * 2.0, DB_RANGE * 2.0);
        pts.push(Pos2::new(x, db_to_pixel(db, rect)));
    }

    // Per-step color: cyan, or a rainbow sweep across X.
    let hue = |i: usize| theme::hsv(i as f32 / STEPS as f32, 0.85, 1.0);
    let with_a = |c: Color32, a: u8| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);

    // Gradient fill from the curve down to the 0 dB baseline.
    let mut mesh = egui::Mesh::default();
    for (i, p) in pts.iter().enumerate() {
        let base = if rainbow { hue(i) } else { Color32::from_rgb(0, 224, 255) };
        mesh.colored_vertex(*p, with_a(base, 58));
        mesh.colored_vertex(Pos2::new(p.x, baseline), with_a(base, 0));
    }
    for i in 0..pts.len() - 1 {
        let a = (2 * i) as u32;
        let b = (2 * i + 1) as u32;
        let c = (2 * (i + 1)) as u32;
        let d = (2 * (i + 1) + 1) as u32;
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    painter.add(egui::Shape::mesh(mesh));

    // Glowing line: wide faint halo to a bright core, as connected paths.
    for (w, a) in [(7.5_f32, 24_u8), (4.0, 64), (2.2, 255)] {
        glow_line(painter, &pts, w, a, rainbow);
    }
}
