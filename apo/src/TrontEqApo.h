// TrontEqApo.h — the APO class. Subclasses CBaseAudioProcessingObject from the
// Windows SDK plus CComObjectRootEx/CComCoClass from ATL (same pattern as the
// SwapAPO sample) to resolve the three IUnknowns baked into
// CBaseAudioProcessingObject's multi-inheritance.
#pragma once

#include <Windows.h>
#include <atlbase.h>
#include <atlcom.h>
#include <audioenginebaseapo.h>
#include <baseaudioprocessingobject.h>

#include <vector>

#include "EqState.h"
#include "Biquad.h"
#include "SharedState.h"
#include "Dynamics.h"
#include "Guids.h"

namespace tronteq {

class __declspec(uuid("CA64E60A-A3C4-43B8-970F-0360055172F2"))
    ATL_NO_VTABLE TrontEqApo
    : public CComObjectRootEx<CComMultiThreadModel>
    , public CComCoClass<TrontEqApo, &CLSID_TrontEqApo>
    , public CBaseAudioProcessingObject
    , public IAudioSystemEffects2  // marks us as a system-effect APO so audiodg loads us
{
public:
    TrontEqApo();
    virtual ~TrontEqApo();

    DECLARE_NO_REGISTRY()
    DECLARE_PROTECT_FINAL_CONSTRUCT()

    BEGIN_COM_MAP(TrontEqApo)
        COM_INTERFACE_ENTRY(IAudioProcessingObject)
        COM_INTERFACE_ENTRY(IAudioProcessingObjectRT)
        COM_INTERFACE_ENTRY(IAudioProcessingObjectConfiguration)
        COM_INTERFACE_ENTRY(IAudioSystemEffects)
        COM_INTERFACE_ENTRY(IAudioSystemEffects2)
    END_COM_MAP()

    // IAudioProcessingObject — override the base so we never reject the engine's
    // APOInitSystemEffects/2/3 init blob (we get our params from the shared file).
    STDMETHOD(Initialize)(UINT32 cbDataSize, BYTE* pbyData) override;

    // IAudioSystemEffects2 — report the single "TrontEQ" effect we apply.
    STDMETHOD(GetEffectsList)(LPGUID* ppEffectsIds, UINT* pcEffects, HANDLE Event) override;

    // CBaseAudioProcessingObject virtuals.
    STDMETHOD(LockForProcess)(UINT32 u32NumInputConnections,
                              APO_CONNECTION_DESCRIPTOR** ppInputConnections,
                              UINT32 u32NumOutputConnections,
                              APO_CONNECTION_DESCRIPTOR** ppOutputConnections) override;
    STDMETHOD(UnlockForProcess)() override;

    STDMETHOD_(void, APOProcess)(UINT32 u32NumInputConnections,
                                 APO_CONNECTION_PROPERTY** ppInputConnections,
                                 UINT32 u32NumOutputConnections,
                                 APO_CONNECTION_PROPERTY** ppOutputConnections) override;

private:
    void RecomputeCoeffsIfDirty(const Band* bands);
    void ProcessBlockFloat32(float* data, UINT32 frames, UINT32 channels, float preGain);

    SharedStateReader m_shared;

    struct LocalState {
        Band bands[kNumBands]{};
        uint8_t bypass = 1;
        float preamp_db = 0.0f;
        Dynamics dynamics{};
        float delay_ms = 0.0f;
    };
    LocalState m_cached;

    // A/V-sync delay ring buffer (interleaved frames * channels). Allocated in
    // LockForProcess (not RT-safe there is fine); applied per-buffer in
    // APOProcess (must stay allocation-free there).
    std::vector<float> m_delayBuf;
    uint32_t m_delayMaxFrames = 0;
    uint32_t m_delayWrite = 0;
    uint32_t m_delayFramesPrev = 0;

    // Covers 7.1.4 (12) and 9.1.6 (16). >16ch still clamps (22.2 is essentially
    // nonexistent on consumer Windows); raising further only grows m_state.
    static constexpr UINT32 kMaxChannels = 16;

    BiquadState m_state[kMaxChannels][kNumBands]{};
    BiquadCoeffs m_coeffs[kNumBands]{};

    UINT32 m_channels = 0;
    UINT32 m_framesPerSecond = 0;

    Band m_coeffBands[kNumBands]{};
    bool m_coeffsReady = false;

    DynamicsProcessor m_dynamics; // AGC + compressor + limiter, after the EQ
};

// For use with ATL's OBJECT_ENTRY_AUTO (optional) and CComObject<> factory.
OBJECT_ENTRY_AUTO(__uuidof(TrontEqApo), TrontEqApo)

} // namespace tronteq
