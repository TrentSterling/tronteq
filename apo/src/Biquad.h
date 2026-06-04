// Biquad.h — RBJ cookbook biquads in Direct Form II Transposed.
// Coefficients are recomputed only when band params change.
#pragma once

#include <cstdint>
#include "EqState.h"

namespace tronteq {

// Direct Form II Transposed biquad state (per channel).
struct BiquadState {
    double z1, z2;
};

struct BiquadCoeffs {
    double b0, b1, b2;
    double a1, a2; // a0 normalized out
};

// Compute coefficients from a Band for a given sample rate.
// kind dispatches RBJ cookbook formulas. If gain==0 dB and kind==Peak, returns identity.
BiquadCoeffs ComputeBiquad(const Band& band, double sample_rate);

// Process one sample through a single biquad (DF-II-T).
inline float ProcessSample(const BiquadCoeffs& c, BiquadState& s, float x_in) {
    double x = static_cast<double>(x_in);
    double y = c.b0 * x + s.z1;
    s.z1 = c.b1 * x - c.a1 * y + s.z2;
    s.z2 = c.b2 * x - c.a2 * y;
    return static_cast<float>(y);
}

// Identity coefficients (pass-through).
inline BiquadCoeffs Identity() {
    return BiquadCoeffs{1.0, 0.0, 0.0, 0.0, 0.0};
}

// Enable FTZ + DAZ (denormal flushing) on the calling thread.
// Must be called from the audio thread once before processing begins.
void EnableDenormalFlushing();

} // namespace tronteq
