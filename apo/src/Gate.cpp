#include "Gate.h"

#include <algorithm>
#include <cmath>
#include <cstring>

namespace tronteq {

static inline double DbToLin(double db) { return std::pow(10.0, db / 20.0); }
static inline double RetainCoef(double ms, double fs) {
    if (ms <= 0.0) return 0.0;
    return std::exp(-1.0 / (ms * 0.001 * fs));
}
// dB per sample to cover `span_db` in `ms` (linear-in-dB ramp, so attack and
// release mean "time to fully open / close").
static inline double RampStep(double span_db, double ms, double fs) {
    const double samples = ms * 0.001 * fs;
    return samples > 1.0 ? span_db / samples : span_db;
}

Gate SanitizeGate(const Gate& in) {
    auto clampf = [](float v, float lo, float hi, float def) -> float {
        if (!std::isfinite(v)) return def;
        return v < lo ? lo : (v > hi ? hi : v);
    };
    Gate g = in;
    g.enabled = in.enabled != 0 ? 1u : 0u;
    g.threshold_db = clampf(g.threshold_db, -90.0f, 0.0f, -50.0f);
    g.hysteresis_db = clampf(g.hysteresis_db, 0.0f, 30.0f, 6.0f);
    g.range_db = clampf(g.range_db, -90.0f, 0.0f, -24.0f);
    g.attack_ms = clampf(g.attack_ms, 0.1f, 500.0f, 2.0f);
    g.hold_ms = clampf(g.hold_ms, 0.0f, 5000.0f, 200.0f);
    g.release_ms = clampf(g.release_ms, 1.0f, 5000.0f, 250.0f);
    g._reserved = 0;
    return g;
}

void GateProcessor::Reset(double sample_rate) {
    m_fs = sample_rate > 0.0 ? sample_rate : 48000.0;
    m_env = 0.0;
    m_gainDb = 0.0;
    m_open = false;
    m_holdLeft = 0;
    m_haveCache = false;
}

void GateProcessor::UpdateCoeffs(const Gate& p) {
    m_openLin = DbToLin(p.threshold_db);
    m_closeLin = DbToLin(static_cast<double>(p.threshold_db) - p.hysteresis_db);
    m_detRetain = RetainCoef(20.0, m_fs); // ~20 ms peak-detector decay
    const double span = -static_cast<double>(p.range_db);
    m_attStep = RampStep(span, p.attack_ms, m_fs);
    m_relStep = RampStep(span, p.release_ms, m_fs);
    m_holdSamples = static_cast<uint32_t>(p.hold_ms * 0.001 * m_fs);
    m_cached = p;
    m_haveCache = true;
}

double GateProcessor::GainDb() const {
    return m_gainDb;
}

void GateProcessor::Process(float* data, uint32_t frames, uint32_t channels, const Gate& p) {
    if (p.enabled == 0) {
        // Disabled: unity, and start closed next time so enabling mid-silence
        // doesn't pass one burst of noise.
        m_gainDb = 0.0;
        m_open = false;
        m_holdLeft = 0;
        return;
    }
    if (!data || frames == 0 || channels == 0) return;
    if (!m_haveCache || std::memcmp(&p, &m_cached, sizeof(Gate)) != 0) {
        UpdateCoeffs(p);
    }

    for (uint32_t f = 0; f < frames; ++f) {
        float* frame = data + static_cast<size_t>(f) * channels;
        double level = 0.0;
        for (uint32_t c = 0; c < channels; ++c)
            level = std::max(level, std::fabs(static_cast<double>(frame[c])));
        m_env = std::max(level, m_env * m_detRetain);

        if (m_env >= m_openLin) {
            m_open = true;
            m_holdLeft = m_holdSamples;
        } else if (m_env < m_closeLin) {
            if (m_holdLeft > 0) {
                --m_holdLeft;
            } else {
                m_open = false;
            }
        } else if (m_open) {
            m_holdLeft = m_holdSamples; // inside the hysteresis band: stay open
        }

        if (m_open) {
            m_gainDb = std::min(0.0, m_gainDb + m_attStep);
        } else {
            m_gainDb = std::max(static_cast<double>(p.range_db), m_gainDb - m_relStep);
        }
        const float g = static_cast<float>(DbToLin(m_gainDb));
        for (uint32_t c = 0; c < channels; ++c) frame[c] *= g;
    }
}

} // namespace tronteq
