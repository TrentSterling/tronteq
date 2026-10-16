// Gate.h — noise gate / downward expander for the mic stack. Real-time safe (no
// allocation, no locks). Stereo-linked like Dynamics: one gain for all
// channels. Params come from the Gate block appended to mic.bin.
#pragma once

#include <cstdint>
#include "EqState.h"

namespace tronteq {

class GateProcessor {
public:
    // Reset detector/gain and cache the sample rate. Call from LockForProcess.
    void Reset(double sample_rate);

    // Apply the gate in place over an interleaved float buffer. No-op (and the
    // gain snaps back to unity) when disabled.
    void Process(float* data, uint32_t frames, uint32_t channels, const Gate& p);

    // Current gain in dB (0 = open). For telemetry/tests.
    double GainDb() const;
    bool IsOpen() const { return m_open; }

private:
    void UpdateCoeffs(const Gate& p);

    double m_fs = 48000.0;
    double m_env = 0.0;    // peak detector (linear)
    double m_gainDb = 0.0; // applied gain, ramped linearly in dB
    bool m_open = false;
    uint32_t m_holdLeft = 0;

    Gate m_cached{};
    bool m_haveCache = false;
    double m_openLin = 0.0;
    double m_closeLin = 0.0;
    double m_detRetain = 0.0;
    double m_attStep = 0.0;  // dB per sample while opening
    double m_relStep = 0.0;  // dB per sample while closing
    uint32_t m_holdSamples = 0;
};

// Clamp POD params from the shared file into sane ranges (NaN -> default).
Gate SanitizeGate(const Gate& g);

} // namespace tronteq
