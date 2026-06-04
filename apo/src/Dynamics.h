// Dynamics.h — AGC + compressor + limiter, applied after the EQ. Real-time safe
// (no allocation, no locks). Stereo-linked (one gain applied to all channels) so
// imaging stays stable. Params come from the shared EqState::Dynamics block.
#pragma once

#include <cstdint>
#include "EqState.h"

namespace tronteq {

class DynamicsProcessor {
public:
    // Reset envelopes/gains and cache the sample rate. Call from LockForProcess.
    void Reset(double sample_rate);

    // Apply AGC → compressor → limiter in place over an interleaved float buffer.
    // No-op when all three stages are disabled.
    void Process(float* data, uint32_t frames, uint32_t channels, const Dynamics& p);

private:
    void UpdateCoeffs(const Dynamics& p);

    double m_fs = 48000.0;

    // AGC
    double m_agcMeanSq = 0.0; // running mean-square (RMS^2)
    double m_agcGainDb = 0.0; // smoothed gain in dB

    // Compressor
    double m_compEnv = 0.0; // smoothed detector level (linear)

    // Limiter
    double m_limGain = 1.0;

    // Cached params + derived coefficients (recomputed only when params change).
    Dynamics m_cached{};
    bool m_haveCache = false;
    double m_attCoef = 0.0;     // compressor attack one-pole retain coef
    double m_relCoef = 0.0;     // compressor release one-pole retain coef
    double m_agcRmsCoef = 0.0;  // AGC RMS-window retain coef
    double m_agcSlewCoef = 0.0; // AGC gain slew (per-sample lerp toward target)
    double m_limRelCoef = 0.0;  // limiter release (per-sample lerp toward 1.0)
};

} // namespace tronteq
