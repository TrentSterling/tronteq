#include "TrontEqApo.h"
#include "Guids.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <new>
#include <objbase.h>

namespace tronteq {

// Diagnostics. audiodg.exe is a protected Session-0 process, so OutputDebugString
// is awkward to capture; we also append to a file we can read directly. The file
// write needs the install dir to grant the audiodg token write access.
static void ApoLog(const char* msg) {
    OutputDebugStringA("[TrontEqApo] ");
    OutputDebugStringA(msg);
    OutputDebugStringA("\n");

    HANDLE h = CreateFileW(L"C:\\ProgramData\\TrontEq\\apo.log",
                           FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE,
                           nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (h != INVALID_HANDLE_VALUE) {
        char line[360];
        int n = _snprintf_s(line, sizeof(line), _TRUNCATE, "%s\r\n", msg);
        if (n > 0) {
            DWORD written = 0;
            WriteFile(h, line, static_cast<DWORD>(n), &written, nullptr);
        }
        CloseHandle(h);
    }
}

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
    ApoLog("ctor: audiodg created an instance");
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
    HRESULT hr = CBaseAudioProcessingObject::Initialize(cbDataSize, pbyData);
    char buf[96];
    sprintf_s(buf, "Initialize cb=%u baseHr=0x%08X", cbDataSize, static_cast<unsigned>(hr));
    ApoLog(buf);
    return S_OK;
}

HRESULT STDMETHODCALLTYPE TrontEqApo::GetEffectsList(
    LPGUID* ppEffectsIds, UINT* pcEffects, HANDLE /*Event*/)
{
    ApoLog("GetEffectsList");
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

    m_shared.TryOpen(); // OK if absent; reopen attempts happen on each buffer

    char buf[128];
    sprintf_s(buf, "LockForProcess ch=%u fs=%u sharedOpen=%d",
              m_channels, m_framesPerSecond, m_shared.IsOpen() ? 1 : 0);
    ApoLog(buf);
    m_loggedProcess = false;
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
        return;
    }

    // APO_FLAG_INPLACE is only a request — at the endpoint (EFX) stage the engine
    // gives us SEPARATE input/output buffers. Always emit to the output buffer;
    // copy first when they don't alias, otherwise output stays silent.
    if (outBuf != inBuf) {
        std::memcpy(outBuf, inBuf, sampleCount * sizeof(float));
    }

    if (m_cached.bypass != 0) {
        return; // pass-through (output now holds the input)
    }

    RecomputeCoeffsIfDirty(m_cached.bands);
    // Clamp preamp defensively: the shared file is POD a hostile process could
    // fill with inf/NaN -> inf gain -> NaN audio blast.
    float preampDb = std::isfinite(m_cached.preamp_db) ? m_cached.preamp_db : 0.0f;
    preampDb = (std::max)(-24.0f, (std::min)(24.0f, preampDb));
    const float preGain = preampDb != 0.0f ? std::pow(10.0f, preampDb / 20.0f) : 1.0f;
    ProcessBlockFloat32(outBuf, in->u32ValidFrameCount, m_channels, preGain);

    // Dynamics: AGC -> compressor -> limiter (each gated by its own enable).
    m_dynamics.Process(outBuf, in->u32ValidFrameCount, m_channels, m_cached.dynamics);
}

} // namespace tronteq
