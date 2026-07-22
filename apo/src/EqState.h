// EqState.h — C mirror of shared/src/lib.rs::EqState.
// UPDATE BOTH if you change either. 224 bytes, 8-byte aligned
// (version + bands + preamp + dynamics + delay_ms + reserved + APO-written
// telemetry).
#pragma once

#include <cstdint>
#include <cstddef>
#include <atomic>
#include <type_traits>

namespace tronteq {

constexpr std::size_t kNumBands = 8;
constexpr std::size_t kStateBytes = 224;      // full file incl. telemetry
constexpr std::size_t kStateCoreBytes = 200;  // state portion (no telemetry)
constexpr float kMaxDelayMs = 2000.0f;        // A/V-sync delay ceiling

enum class BandKind : uint32_t {
    Peak = 0,
    LowShelf = 1,
    HighShelf = 2,
    HighPass = 3,
    LowPass = 4,
    BandPass = 5,
    Notch = 6,
    AllPass = 7,
};

struct Band {
    float freq;   // Hz
    float gain;   // dB
    float q;      // dimensionless
    uint32_t kind;
};
static_assert(sizeof(Band) == 16, "Band ABI drift");

// Dynamics params — mirror of shared/src/lib.rs::Dynamics. *_enabled are u32.
struct Dynamics {
    uint32_t comp_enabled;
    float comp_threshold_db;
    float comp_ratio;
    float comp_attack_ms;
    float comp_release_ms;
    float comp_knee_db;
    float comp_makeup_db;
    uint32_t limiter_enabled;
    float limiter_ceiling_db;
    uint32_t agc_enabled;
    float agc_target_db;
    float agc_max_gain_db;
};
static_assert(sizeof(Dynamics) == 48, "Dynamics ABI drift");

// Meter telemetry — mirror of shared/src/lib.rs::Telemetry. APO writes, GUI reads.
struct Telemetry {
    uint32_t seq;     // increments per processed buffer
    float in_peak;    // pre-chain peak (linear)
    float in_rms;     // pre-chain RMS
    float out_peak;   // post-chain peak
    float out_rms;    // post-chain RMS
    float gr_db;      // dynamics gain reduction, dB >= 0
};
static_assert(sizeof(Telemetry) == 24, "Telemetry ABI drift");

struct EqState {
    std::atomic<uint64_t> version;  // seqlock: odd=writing, even=committed
    Band bands[kNumBands];          // 128 bytes
    float preamp_db;                // head-of-chain gain (was bass_boost)
    uint8_t bypass;
    uint8_t _pad[3];
    Dynamics dynamics;
    float delay_ms;                 // A/V-sync delay, ms (GUI-written, seqlock'd)
    uint8_t _reserved[4];           // 8-byte align + future headroom
    Telemetry telemetry;            // APO-written; not under the seqlock
};
static_assert(sizeof(EqState) == kStateBytes, "EqState ABI drift — mirror shared/src/lib.rs");
static_assert(offsetof(EqState, delay_ms) == 192,
    "delay_ms must sit at offset 192 (mirror shared/src/lib.rs)");
static_assert(offsetof(EqState, _reserved) == 196,
    "_reserved must sit at offset 196 (mirror shared/src/lib.rs)");
static_assert(offsetof(EqState, telemetry) == kStateCoreBytes,
    "telemetry must start at the end of the 200-byte state (incl. delay_ms)");
static_assert(alignof(EqState) == 8, "EqState must be 8-byte aligned for atomic version");
static_assert(std::atomic<uint64_t>::is_always_lock_free,
    "atomic<uint64_t> must be lock-free for seqlock to be RT-safe");

} // namespace tronteq
