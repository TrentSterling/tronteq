#include "Biquad.h"

#include <cmath>
#include <xmmintrin.h>
#include <pmmintrin.h>

namespace tronteq {

static constexpr double kPi = 3.14159265358979323846;

BiquadCoeffs ComputeBiquad(const Band& band, double sample_rate) {
    if (band.gain == 0.0f && static_cast<BandKind>(band.kind) == BandKind::Peak) {
        return Identity();
    }
    if (sample_rate <= 0.0 || band.freq <= 0.0f || band.freq >= sample_rate * 0.5) {
        return Identity();
    }

    const double A = std::pow(10.0, static_cast<double>(band.gain) / 40.0);
    const double w0 = 2.0 * kPi * static_cast<double>(band.freq) / sample_rate;
    const double cos_w0 = std::cos(w0);
    const double sin_w0 = std::sin(w0);
    const double Q = static_cast<double>(band.q) > 0.05 ? static_cast<double>(band.q) : 0.05;
    const double alpha = sin_w0 / (2.0 * Q);

    double b0, b1, b2, a0, a1, a2;

    switch (static_cast<BandKind>(band.kind)) {
    case BandKind::Peak: {
        // Peaking EQ. RBJ cookbook.
        b0 = 1.0 + alpha * A;
        b1 = -2.0 * cos_w0;
        b2 = 1.0 - alpha * A;
        a0 = 1.0 + alpha / A;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha / A;
        break;
    }
    case BandKind::LowShelf: {
        const double sqrtA = std::sqrt(A);
        const double beta = 2.0 * sqrtA * alpha;
        b0 =      A * ((A + 1.0) - (A - 1.0) * cos_w0 + beta);
        b1 =  2.0 * A * ((A - 1.0) - (A + 1.0) * cos_w0);
        b2 =      A * ((A + 1.0) - (A - 1.0) * cos_w0 - beta);
        a0 =           (A + 1.0) + (A - 1.0) * cos_w0 + beta;
        a1 =   -2.0 * ((A - 1.0) + (A + 1.0) * cos_w0);
        a2 =           (A + 1.0) + (A - 1.0) * cos_w0 - beta;
        break;
    }
    case BandKind::HighShelf: {
        const double sqrtA = std::sqrt(A);
        const double beta = 2.0 * sqrtA * alpha;
        b0 =      A * ((A + 1.0) + (A - 1.0) * cos_w0 + beta);
        b1 = -2.0 * A * ((A - 1.0) + (A + 1.0) * cos_w0);
        b2 =      A * ((A + 1.0) + (A - 1.0) * cos_w0 - beta);
        a0 =           (A + 1.0) - (A - 1.0) * cos_w0 + beta;
        a1 =    2.0 * ((A - 1.0) - (A + 1.0) * cos_w0);
        a2 =           (A + 1.0) - (A - 1.0) * cos_w0 - beta;
        break;
    }
    case BandKind::HighPass: {
        b0 = (1.0 + cos_w0) / 2.0;
        b1 = -(1.0 + cos_w0);
        b2 = (1.0 + cos_w0) / 2.0;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    }
    case BandKind::LowPass: {
        b0 = (1.0 - cos_w0) / 2.0;
        b1 = 1.0 - cos_w0;
        b2 = (1.0 - cos_w0) / 2.0;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    }
    case BandKind::BandPass: {
        // Constant 0 dB peak gain.
        b0 = alpha;
        b1 = 0.0;
        b2 = -alpha;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    }
    case BandKind::Notch: {
        b0 = 1.0;
        b1 = -2.0 * cos_w0;
        b2 = 1.0;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    }
    case BandKind::AllPass: {
        b0 = 1.0 - alpha;
        b1 = -2.0 * cos_w0;
        b2 = 1.0 + alpha;
        a0 = 1.0 + alpha;
        a1 = -2.0 * cos_w0;
        a2 = 1.0 - alpha;
        break;
    }
    default:
        return Identity();
    }

    if (a0 == 0.0) return Identity();
    BiquadCoeffs out;
    out.b0 = b0 / a0;
    out.b1 = b1 / a0;
    out.b2 = b2 / a0;
    out.a1 = a1 / a0;
    out.a2 = a2 / a0;
    return out;
}

void EnableDenormalFlushing() {
    // Flush-to-Zero + Denormals-are-Zero. Required on audiodg thread for RT safety.
    _MM_SET_FLUSH_ZERO_MODE(_MM_FLUSH_ZERO_ON);
    _MM_SET_DENORMALS_ZERO_MODE(_MM_DENORMALS_ZERO_ON);
}

} // namespace tronteq
