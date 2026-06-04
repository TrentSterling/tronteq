//! Biquad magnitude evaluation for the drawn response curve.
//! Mirrors the RBJ coefficient math in `apo/src/Biquad.cpp` — display only.

use std::f32::consts::PI;
use tronteq_shared::{Band, BandKind, NUM_BANDS};

#[derive(Copy, Clone, Default)]
pub struct Coeffs {
    pub b0: f32, pub b1: f32, pub b2: f32,
    pub a1: f32, pub a2: f32,
}

pub fn identity() -> Coeffs {
    Coeffs { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
}

pub fn compute(band: &Band, sample_rate: f32) -> Coeffs {
    if band.gain == 0.0 && band.kind == BandKind::Peak as u32 {
        return identity();
    }
    if sample_rate <= 0.0 || band.freq <= 0.0 || band.freq >= sample_rate * 0.5 {
        return identity();
    }
    let a = 10f32.powf(band.gain / 40.0);
    let w0 = 2.0 * PI * band.freq / sample_rate;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let q = band.q.max(0.05);
    let alpha = sin_w0 / (2.0 * q);

    let (b0, b1, b2, a0, a1, a2) = match BandKind::from_u32(band.kind) {
        BandKind::Peak => (
            1.0 + alpha * a,
            -2.0 * cos_w0,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos_w0,
            1.0 - alpha / a,
        ),
        BandKind::LowShelf => {
            let sqrt_a = a.sqrt();
            let beta = 2.0 * sqrt_a * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos_w0 + beta),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                a * ((a + 1.0) - (a - 1.0) * cos_w0 - beta),
                (a + 1.0) + (a - 1.0) * cos_w0 + beta,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                (a + 1.0) + (a - 1.0) * cos_w0 - beta,
            )
        }
        BandKind::HighShelf => {
            let sqrt_a = a.sqrt();
            let beta = 2.0 * sqrt_a * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos_w0 + beta),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                a * ((a + 1.0) + (a - 1.0) * cos_w0 - beta),
                (a + 1.0) - (a - 1.0) * cos_w0 + beta,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                (a + 1.0) - (a - 1.0) * cos_w0 - beta,
            )
        }
        BandKind::HighPass => (
            (1.0 + cos_w0) / 2.0,
            -(1.0 + cos_w0),
            (1.0 + cos_w0) / 2.0,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
        BandKind::LowPass => (
            (1.0 - cos_w0) / 2.0,
            1.0 - cos_w0,
            (1.0 - cos_w0) / 2.0,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
        BandKind::BandPass => (
            alpha,
            0.0,
            -alpha,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
        BandKind::Notch => (
            1.0,
            -2.0 * cos_w0,
            1.0,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
        BandKind::AllPass => (
            1.0 - alpha,
            -2.0 * cos_w0,
            1.0 + alpha,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
    };
    if a0 == 0.0 { return identity(); }
    Coeffs {
        b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
        a1: a1 / a0, a2: a2 / a0,
    }
}

/// Magnitude response of a single biquad at normalized freq (freq/fs), in dB.
fn biquad_mag_db(c: &Coeffs, f_norm: f32) -> f32 {
    let w = 2.0 * PI * f_norm;
    // H(e^jw) = (b0 + b1 e^-jw + b2 e^-2jw) / (1 + a1 e^-jw + a2 e^-2jw)
    let cos_w = w.cos();
    let cos_2w = (2.0 * w).cos();
    let sin_w = w.sin();
    let sin_2w = (2.0 * w).sin();
    let num_re = c.b0 + c.b1 * cos_w + c.b2 * cos_2w;
    let num_im = -c.b1 * sin_w - c.b2 * sin_2w;
    let den_re = 1.0 + c.a1 * cos_w + c.a2 * cos_2w;
    let den_im = -c.a1 * sin_w - c.a2 * sin_2w;
    let num_sq = num_re * num_re + num_im * num_im;
    let den_sq = den_re * den_re + den_im * den_im;
    let mag_sq = num_sq / den_sq.max(1e-30);
    10.0 * mag_sq.max(1e-30).log10()
}

/// Composite response of all 8 bands at a given real frequency (Hz).
pub fn composite_db(bands: &[Band; NUM_BANDS], freq_hz: f32, sample_rate: f32) -> f32 {
    if freq_hz <= 0.0 || freq_hz >= sample_rate * 0.5 { return 0.0; }
    let f_norm = freq_hz / sample_rate;
    let mut total_db = 0.0;
    for b in bands {
        let c = compute(b, sample_rate);
        total_db += biquad_mag_db(&c, f_norm);
    }
    total_db
}
