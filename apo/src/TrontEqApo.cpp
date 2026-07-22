#include "TrontEqApo.h"
#include "Guids.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <new>
#include <objbase.h>

namespace tronteq {

// {9D8C1A32-4F6E-4D21-9A77-21550C338801} — see Guids.h. Concrete storage so it
// can be returned from GetEffectsList.
static const GUID kTrontEqEffectId =
    { 0x9d8c1a32, 0x4f6e, 0x4d21, { 0x9a, 0x77, 0x21, 0x55, 0x0c, 0x33, 0x88, 0x01 } };

// Registration properties — built once via CRegAPOProperties helper, which
// handles the variable-length iidAPOInterfaceList tail.
static const CRegAPOProperties<1> s_regProps(
    __uuidof(TrontEqApo),
    L"TrontEQ Stream Effect APO",
    L"TrontEQ",
    1, 0,
    __uuidof(IAudioProcessingObject),
    static_cast<APO_FLAG>(APO_FLAG_INPLACE |
                          APO_FLAG_SAMPLESPERFRAME_MUST_MATCH |
                          APO_FLAG_FRAMESPERSECOND_MUST_MATCH |
                          APO_FLAG_BITSPERSAMPLE_MUST_MATCH));

// ---- ctor/dtor --------------------------------------------------------------

TrontEqApo::TrontEqApo()
    : CBaseAudioProcessingObject(s_regProps)
{
    for (std::size_t i = 0; i < kNumBands; ++i) {
        m_coeffs[i] = Identity();
        m_coeffBands[i] = Band{0.0f, 0.0f, 1.0f, 0};
        m_cached.bands[i] = Band{0.0f, 0.0f, 1.0f, 0};
    }
}

TrontEqApo::~TrontEqApo() {
    m_shared.Close();
}

// ---- System-effects plumbing ------------------------------------------------

HRESULT STDMETHODCALLTYPE TrontEqApo::Initialize(UINT32 cbDataSize, BYTE* pbyData) {
    // Let the base do its generic setup, but never reject: the engine hands us
    // an APOInitSystemEffects/2/3 blob we don't need (our params come from the
    // shared state file), and rejecting it would stop audiodg loading us.
    // Run the base setup for its side effects; never reject (S_OK regardless).
    (void)CBaseAudioProcessingObject::Initialize(cbDataSize, pbyData);
    return S_OK;
}

HRESULT STDMETHODCALLTYPE TrontEqApo::GetEffectsList(
    LPGUID* ppEffectsIds, UINT* pcEffects, HANDLE /*Event*/)
{
    if (!ppEffectsIds || !pcEffects) return E_POINTER;

    // We don't signal effect-list changes (single static effect), so the Event
    // handle is intentionally ignored. Report exactly one effect: TrontEQ.
    LPGUID ids = static_cast<LPGUID>(CoTaskMemAlloc(sizeof(GUID)));
    if (!ids) {
        *ppEffectsIds = nullptr;
        *pcEffects = 0;
        return E_OUTOFMEMORY;
    }
    *ids = kTrontEqEffectId;
    *ppEffectsIds = ids;
    *pcEffects = 1;
    return S_OK;
}

// ---- Lock/Unlock ------------------------------------------------------------

HRESULT STDMETHODCALLTYPE TrontEqApo::LockForProcess(
    UINT32 u32NumInputConnections,
    APO_CONNECTION_DESCRIPTOR** ppInputConnections,
    UINT32 u32NumOutputConnections,
    APO_CONNECTION_DESCRIPTOR** ppOutputConnections)
{
    HRESULT hr = CBaseAudioProcessingObject::LockForProcess(
        u32NumInputConnections, ppInputConnections,
        u32NumOutputConnections, ppOutputConnections);
    if (FAILED(hr)) return hr;

    // Denormal flushing on the RT thread (APOProcess runs on the caller of LockForProcess... actually audio mixer thread — safer: flip here AND early in APOProcess).
    EnableDenormalFlushing();

    UINT32 samplesPerFrame = GetSamplesPerFrame();
    m_channels = (std::min)(samplesPerFrame, kMaxChannels);
    m_framesPerSecond = static_cast<UINT32>(GetFramesPerSecond());

    for (UINT32 c = 0; c < kMaxChannels; ++c) {
        for (std::size_t b = 0; b < kNumBands; ++b) {
            m_state[c][b].z1 = 0.0;
            m_state[c][b].z2 = 0.0;
        }
    }
    m_coeffsReady = false;
    m_dynamics.Reset(static_cast<double>(m_framesPerSecond > 0 ? m_framesPerSecond : 48000));

    // A/V-sync delay ring: sized for the worst case (kMaxDelayMs) at the real
    // sample rate. Heap allocation is safe here (not the RT thread).
    {
        const double fs = m_framesPerSecond > 0 ? static_cast<double>(m_framesPerSecond) : 48000.0;
        m_delayMaxFrames = static_cast<uint32_t>(std::ceil(kMaxDelayMs / 1000.0 * fs)) + 1;
        m_delayBuf.assign(static_cast<size_t>(m_delayMaxFrames) * m_channels, 0.0f);
        m_delayWrite = 0;
        m_delayFramesPrev = 0;
    }

    m_shared.TryOpen(); // open the shared-state mapping now; no per-buffer reopen

    return S_OK;
}

HRESULT STDMETHODCALLTYPE TrontEqApo::UnlockForProcess() {
    // Do NOT Close() the shared mapping here: a late APOProcess may still be reading
    // the view on the RT thread, and unmapping under it = AV inside audiodg (system
    // audio crash). TryOpen is idempotent; the view is released only in the dtor.
    return CBaseAudioProcessingObject::UnlockForProcess();
}

// ---- Coefficient recompute --------------------------------------------------

void TrontEqApo::RecomputeCoeffsIfDirty(const Band* bands) {
    bool dirty = !m_coeffsReady;
    if (!dirty) {
        for (std::size_t i = 0; i < kNumBands; ++i) {
            const Band& a = bands[i];
            const Band& b = m_coeffBands[i];
            if (a.freq != b.freq || a.gain != b.gain || a.q != b.q || a.kind != b.kind) {
                dirty = true;
                break;
            }
        }
    }
    if (!dirty) return;

    const double fs = static_cast<double>(m_framesPerSecond > 0 ? m_framesPerSecond : 48000);
    for (std::size_t i = 0; i < kNumBands; ++i) {
        m_coeffs[i] = ComputeBiquad(bands[i], fs);
        m_coeffBands[i] = bands[i];
    }
    m_coeffsReady = true;
}

// ---- Actual DSP -------------------------------------------------------------

void TrontEqApo::ProcessBlockFloat32(float* data, UINT32 frames, UINT32 channels, float preGain) {
    if (!data || frames == 0 || channels == 0) return;

    for (UINT32 f = 0; f < frames; ++f) {
        for (UINT32 c = 0; c < channels; ++c) {
            float x = data[f * channels + c] * preGain; // preamp at head of chain
            if (!std::isfinite(x)) x = 0.0f; // keep NaN/Inf out of the biquad state
            for (std::size_t b = 0; b < kNumBands; ++b) {
                x = ProcessSample(m_coeffs[b], m_state[c][b], x);
            }
            if (!std::isfinite(x)) x = 0.0f; // a poisoned biquad must not reach the DAC
            data[f * channels + c] = x;
        }
    }
}

// ---- APOProcess (RT thread) -------------------------------------------------

void STDMETHODCALLTYPE TrontEqApo::APOProcess(
    UINT32 u32NumInputConnections,
    APO_CONNECTION_PROPERTY** ppInputConnections,
    UINT32 u32NumOutputConnections,
    APO_CONNECTION_PROPERTY** ppOutputConnections)
{
    UNREFERENCED_PARAMETER(u32NumInputConnections);
    UNREFERENCED_PARAMETER(u32NumOutputConnections);

    // MXCSR is per-thread; set denormal flush on the actual RT mixer thread
    // (LockForProcess may run on a different thread, so its call isn't enough).
    EnableDenormalFlushing();

    if (!ppInputConnections || !ppOutputConnections ||
        !ppInputConnections[0] || !ppOutputConnections[0]) {
        return;
    }

    APO_CONNECTION_PROPERTY* in = ppInputConnections[0];
    APO_CONNECTION_PROPERTY* out = ppOutputConnections[0];

    // Refresh cached params from the shared mmap (opened in LockForProcess). NEVER
    // open files or log here — any file I/O / syscall on the RT mixer thread
    // glitches all system audio.
    if (m_shared.IsOpen()) {
        EqState tmp;
        if (m_shared.Read(tmp)) {
            for (std::size_t i = 0; i < kNumBands; ++i) {
                m_cached.bands[i] = tmp.bands[i];
            }
            m_cached.preamp_db = tmp.preamp_db;
            m_cached.bypass = tmp.bypass;
            m_cached.dynamics = tmp.dynamics;
            m_cached.delay_ms = tmp.delay_ms;
        }
    }

    out->u32ValidFrameCount = in->u32ValidFrameCount;
    out->u32BufferFlags = in->u32BufferFlags;

    float* inBuf  = reinterpret_cast<float*>(in->pBuffer);
    float* outBuf = reinterpret_cast<float*>(out->pBuffer);
    if (!inBuf || !outBuf) return;
    const std::size_t sampleCount =
        static_cast<std::size_t>(in->u32ValidFrameCount) * m_channels;

    if (in->u32BufferFlags == BUFFER_SILENT) {
        // Separate in/out buffers at the EFX stage: zero the output so a consumer
        // that ignores the silent flag can't play stale scratch.
        if (outBuf != inBuf) {
            std::memset(outBuf, 0, sampleCount * sizeof(float));
        }
        m_shared.WriteTelemetry(0.0f, 0.0f, 0.0f, 0.0f, 0.0f); // meters fall to silence
        return;
    }

    // Meter the input (pre-chain) for the GUI's in/out VU. Cheap: one pass over a
    // few hundred samples, no syscalls (RT-safe).
    float inPeak = 0.0f;
    double inSq = 0.0;
    for (std::size_t i = 0; i < sampleCount; ++i) {
        float v = inBuf[i];
        if (!std::isfinite(v)) v = 0.0f;
        const float a = std::fabs(v);
        if (a > inPeak) inPeak = a;
        inSq += static_cast<double>(v) * v;
    }
    const float inRms = sampleCount ? static_cast<float>(std::sqrt(inSq / sampleCount)) : 0.0f;

    // APO_FLAG_INPLACE is only a request — at the endpoint (EFX) stage the engine
    // gives us SEPARATE input/output buffers. Always emit to the output buffer;
    // copy first when they don't alias, otherwise output stays silent.
    if (outBuf != inBuf) {
        std::memcpy(outBuf, inBuf, sampleCount * sizeof(float));
    }

    if (m_cached.bypass != 0) {
        // Pass-through: output == input, so the in/out meter shows no change.
        m_shared.WriteTelemetry(inPeak, inRms, inPeak, inRms, 0.0f);
        return;
    }

    RecomputeCoeffsIfDirty(m_cached.bands);
    // Clamp preamp defensively: the shared file is POD a hostile process could
    // fill with inf/NaN -> inf gain -> NaN audio blast.
    float preampDb = std::isfinite(m_cached.preamp_db) ? m_cached.preamp_db : 0.0f;
    preampDb = (std::max)(-24.0f, (std::min)(24.0f, preampDb));
    const float preGain = preampDb != 0.0f ? std::pow(10.0f, preampDb / 20.0f) : 1.0f;
    ProcessBlockFloat32(outBuf, in->u32ValidFrameCount, m_channels, preGain);

    // Post-EQ RMS is the baseline for the gain-reduction meter: GR is how much the
    // dynamics stage (next) pulls the level down from here.
    double postEqSq = 0.0;
    for (std::size_t i = 0; i < sampleCount; ++i) {
        postEqSq += static_cast<double>(outBuf[i]) * outBuf[i];
    }
    const float postEqRms = sampleCount ? static_cast<float>(std::sqrt(postEqSq / sampleCount)) : 0.0f;

    // Dynamics: AGC -> compressor -> limiter. Clamp params first — they're POD from
    // the shared file and a finite-but-huge makeup/gain would otherwise make +Inf
    // gain (the EQ-stage sanitize runs before this stage, not after).
    Dynamics dyn = m_cached.dynamics;
    auto clampf = [](float v, float lo, float hi, float def) -> float {
        if (!std::isfinite(v)) return def;
        return v < lo ? lo : (v > hi ? hi : v);
    };
    dyn.comp_threshold_db = clampf(dyn.comp_threshold_db, -80.0f, 0.0f, -18.0f);
    dyn.comp_ratio = clampf(dyn.comp_ratio, 1.0f, 100.0f, 2.0f);
    dyn.comp_attack_ms = clampf(dyn.comp_attack_ms, 0.1f, 500.0f, 10.0f);
    dyn.comp_release_ms = clampf(dyn.comp_release_ms, 1.0f, 5000.0f, 120.0f);
    dyn.comp_knee_db = clampf(dyn.comp_knee_db, 0.0f, 24.0f, 6.0f);
    dyn.comp_makeup_db = clampf(dyn.comp_makeup_db, 0.0f, 48.0f, 0.0f);
    dyn.limiter_ceiling_db = clampf(dyn.limiter_ceiling_db, -24.0f, 0.0f, -0.3f);
    dyn.agc_target_db = clampf(dyn.agc_target_db, -60.0f, 0.0f, -16.0f);
    dyn.agc_max_gain_db = clampf(dyn.agc_max_gain_db, 0.0f, 48.0f, 18.0f);
    m_dynamics.Process(outBuf, in->u32ValidFrameCount, m_channels, dyn);

    // Final safety net: nothing reaches the DAC non-finite or past a hard ceiling,
    // regardless of params or a poisoned biquad state. Meter the output here too.
    float outPeak = 0.0f;
    double outSq = 0.0;
    for (std::size_t i = 0; i < sampleCount; ++i) {
        float v = outBuf[i];
        if (!std::isfinite(v)) {
            v = 0.0f;
        } else if (v > 4.0f) {
            v = 4.0f;
        } else if (v < -4.0f) {
            v = -4.0f;
        }
        outBuf[i] = v;
        const float a = std::fabs(v);
        if (a > outPeak) outPeak = a;
        outSq += static_cast<double>(v) * v;
    }
    const float outRms = sampleCount ? static_cast<float>(std::sqrt(outSq / sampleCount)) : 0.0f;

    // Gain reduction = how much the dynamics stage pulled the level below post-EQ.
    // Only count reduction (makeup/AGC boost is not "reduction").
    float grDb = 0.0f;
    if (postEqRms > 1e-7f && outRms > 1e-7f && postEqRms > outRms) {
        grDb = 20.0f * std::log10(postEqRms / outRms);
        if (grDb < 0.0f) grDb = 0.0f;
    }

    m_shared.WriteTelemetry(inPeak, inRms, outPeak, outRms, grDb);

    // ---- A/V-sync delay (final step) ----------------------------------------
    // Ring buffer over the already-processed output. RT-safe: no allocation, no
    // syscalls, no logging. delay_ms == 0 -> skipped entirely (zero-latency path
    // stays byte-for-byte identical).
    const double fs = m_framesPerSecond > 0 ? static_cast<double>(m_framesPerSecond) : 48000.0;
    float dms = std::isfinite(m_cached.delay_ms) ? m_cached.delay_ms : 0.0f;
    dms = (std::max)(0.0f, (std::min)(kMaxDelayMs, dms));
    uint32_t delayFrames = static_cast<uint32_t>(std::lround(dms / 1000.0f * static_cast<float>(fs)));
    if (m_delayMaxFrames > 0 && delayFrames >= m_delayMaxFrames) {
        delayFrames = m_delayMaxFrames - 1;
    }

    if (delayFrames > 0 && !m_delayBuf.empty()) {
        if (m_delayFramesPrev == 0) {
            std::fill(m_delayBuf.begin(), m_delayBuf.end(), 0.0f); // clean 0->N enable
        }
        const uint32_t cap = m_delayMaxFrames;
        const uint32_t ch = m_channels;
        for (UINT32 f = 0; f < in->u32ValidFrameCount; ++f) {
            // write current output frame, then read a delayed one back into it
            float* slotW = &m_delayBuf[static_cast<size_t>(m_delayWrite) * ch];
            float* frame = &outBuf[static_cast<size_t>(f) * ch];
            std::memcpy(slotW, frame, ch * sizeof(float));
            uint32_t rp = (m_delayWrite + cap - delayFrames) % cap;
            std::memcpy(frame, &m_delayBuf[static_cast<size_t>(rp) * ch], ch * sizeof(float));
            m_delayWrite = (m_delayWrite + 1) % cap;
        }
    }
    m_delayFramesPrev = delayFrames;
}

} // namespace tronteq
