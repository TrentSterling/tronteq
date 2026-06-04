#include "Dynamics.h"

#include <algorithm>
#include <cmath>
#include <cstring>

namespace tronteq {

static inline double DbToLin(double db) { return std::pow(10.0, db / 20.0); }

// One-pole *retain* coef for a time constant in ms (env = c*env + (1-c)*x).
static inline double RetainCoef(double ms, double fs) {
    if (ms <= 0.0) return 0.0;
    return std::exp(-1.0 / (ms * 0.001 * fs));
}
// Per-sample *lerp* coef toward a target (x += (target-x)*c) for time const ms.
static inline double LerpCoef(double ms, double fs) {
    if (ms <= 0.0) return 1.0;
    return 1.0 - std::exp(-1.0 / (ms * 0.001 * fs));
}

void DynamicsProcessor::Reset(double sample_rate) {
    m_fs = sample_rate > 0.0 ? sample_rate : 48000.0;
    m_agcMeanSq = 0.0;
    m_agcGainDb = 0.0;
    m_compEnv = 0.0;
    m_limGain = 1.0;
    m_haveCache = false;
}

void DynamicsProcessor::UpdateCoeffs(const Dynamics& p) {
    m_attCoef = RetainCoef(p.comp_attack_ms, m_fs);
    m_relCoef = RetainCoef(p.comp_release_ms, m_fs);
    m_agcRmsCoef = RetainCoef(300.0, m_fs);   // ~300 ms loudness window
    m_agcSlewCoef = LerpCoef(1500.0, m_fs);   // ~1.5 s gain ride (set-and-forget)
    m_limRelCoef = LerpCoef(50.0, m_fs);      // ~50 ms limiter release
    m_cached = p;
    m_haveCache = true;
}

void DynamicsProcessor::Process(float* data, uint32_t frames, uint32_t channels,
                                const Dynamics& p) {
    const bool agc = p.agc_enabled != 0;
    const bool comp = p.comp_enabled != 0;
    const bool lim = p.limiter_enabled != 0;
    if (!agc && !comp && !lim) return;
    if (!data || frames == 0 || channels == 0) return;

    if (!m_haveCache || std::memcmp(&p, &m_cached, sizeof(Dynamics)) != 0) {
        UpdateCoeffs(p);
    }

    const double ceilingLin = DbToLin(p.limiter_ceiling_db);
    const double ratio = p.comp_ratio > 1.0 ? p.comp_ratio : 1.0;
    const double knee = p.comp_knee_db > 0.0 ? p.comp_knee_db : 0.0001;
    const double slope = 1.0 - 1.0 / ratio;

    for (uint32_t f = 0; f < frames; ++f) {
        float* frame = data + static_cast<size_t>(f) * channels;

        // --- AGC: ride a slow gain toward target loudness ---
        if (agc) {
            double ms = 0.0;
            for (uint32_t c = 0; c < channels; ++c) ms += static_cast<double>(frame[c]) * frame[c];
            ms /= channels;
            m_agcMeanSq = m_agcRmsCoef * m_agcMeanSq + (1.0 - m_agcRmsCoef) * ms;
            if (m_agcMeanSq > 1e-8) { // above noise floor — don't amplify silence
                const double rmsDb = 10.0 * std::log10(m_agcMeanSq);
                double desired = static_cast<double>(p.agc_target_db) - rmsDb;
                desired = std::clamp(desired, -24.0, static_cast<double>(p.agc_max_gain_db));
                m_agcGainDb += (desired - m_agcGainDb) * m_agcSlewCoef;
            }
            const float g = static_cast<float>(DbToLin(m_agcGainDb));
            for (uint32_t c = 0; c < channels; ++c) frame[c] *= g;
        }

        // --- Compressor: stereo-linked, soft knee, makeup ---
        if (comp) {
            double level = 0.0;
            for (uint32_t c = 0; c < channels; ++c)
                level = std::max(level, std::fabs(static_cast<double>(frame[c])));
            const double coef = (level > m_compEnv) ? m_attCoef : m_relCoef;
            m_compEnv = coef * m_compEnv + (1.0 - coef) * level;

            double gdb = static_cast<double>(p.comp_makeup_db);
            if (m_compEnv > 1e-9) {
                const double envDb = 20.0 * std::log10(m_compEnv);
                const double over = envDb - static_cast<double>(p.comp_threshold_db);
                double reduction;
                if (over <= -knee * 0.5) {
                    reduction = 0.0;
                } else if (over >= knee * 0.5) {
                    reduction = slope * over;
                } else {
                    const double t = over + knee * 0.5; // soft-knee quadratic
                    reduction = slope * t * t / (2.0 * knee);
                }
                gdb -= reduction;
            }
            const float g = static_cast<float>(DbToLin(gdb));
            for (uint32_t c = 0; c < channels; ++c) frame[c] *= g;
        }

        // --- Limiter: brickwall ceiling, instant attack / smooth release ---
        if (lim) {
            double peak = 0.0;
            for (uint32_t c = 0; c < channels; ++c)
                peak = std::max(peak, std::fabs(static_cast<double>(frame[c])));
            const double target = (peak > ceilingLin && peak > 0.0) ? ceilingLin / peak : 1.0;
            if (target < m_limGain) {
                m_limGain = target; // instant attack — clamp this frame
            } else {
                m_limGain += (1.0 - m_limGain) * m_limRelCoef; // release back up
            }
            const float g = static_cast<float>(m_limGain);
            for (uint32_t c = 0; c < channels; ++c) frame[c] *= g;
        }
    }
}

} // namespace tronteq
