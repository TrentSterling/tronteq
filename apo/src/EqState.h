// EqState.h — C mirror of shared/src/lib.rs::EqState.
// UPDATE BOTH if you change either. 192 bytes, 8-byte aligned (incl. preamp + dynamics).
#pragma once

#include <cstdint>
#include <atomic>
#include <type_traits>

namespace tronteq {

constexpr std::size_t kNumBands = 8;
constexpr std::size_t kStateBytes = 192;

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

struct EqState {
    std::atomic<uint64_t> version;  // seqlock: odd=writing, even=committed
    Band bands[kNumBands];          // 128 bytes
    float preamp_db;                // head-of-chain gain (was bass_boost)
    uint8_t bypass;
    uint8_t _pad[3];
    Dynamics dynamics;
};
static_assert(sizeof(EqState) == kStateBytes, "EqState ABI drift — mirror shared/src/lib.rs");
static_assert(alignof(EqState) == 8, "EqState must be 8-byte aligned for atomic version");
static_assert(std::atomic<uint64_t>::is_always_lock_free,
    "atomic<uint64_t> must be lock-free for seqlock to be RT-safe");

} // namespace tronteq
