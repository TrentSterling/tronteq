#include "SharedState.h"

#include <atomic>
#include <cstring>

namespace tronteq {

static const wchar_t* kStatePath = L"C:\\ProgramData\\TrontEq\\state.bin";
static constexpr float kDefaultFreqs[kNumBands] = {
    31.25f, 62.5f, 125.0f, 250.0f, 500.0f, 1000.0f, 2000.0f, 4000.0f,
};

void FillDefault(EqState& out) {
    out.version.store(0, std::memory_order_relaxed);
    for (std::size_t i = 0; i < kNumBands; ++i) {
        out.bands[i].freq = kDefaultFreqs[i];
        out.bands[i].gain = 0.0f;
        out.bands[i].q = 1.0f;
        out.bands[i].kind = static_cast<uint32_t>(BandKind::Peak);
    }
    out.preamp_db = 0.0f;
    out.bypass = 1; // default: bypass until the GUI/CLI explicitly enables
    out._pad[0] = out._pad[1] = out._pad[2] = 0;
    Dynamics d{};
    d.comp_threshold_db = -18.0f; d.comp_ratio = 2.0f; d.comp_attack_ms = 10.0f;
    d.comp_release_ms = 120.0f; d.comp_knee_db = 6.0f; d.comp_makeup_db = 0.0f;
    d.limiter_ceiling_db = -0.3f; d.agc_target_db = -16.0f; d.agc_max_gain_db = 18.0f;
    // all *_enabled stay 0 (passive fallback)
    out.dynamics = d;
}

SharedStateReader::SharedStateReader() {}
SharedStateReader::~SharedStateReader() { Close(); }

void SharedStateReader::Close() {
    if (m_view) {
        UnmapViewOfFile(m_view);
        m_view = nullptr;
    }
    if (m_mapping) {
        CloseHandle(m_mapping);
        m_mapping = nullptr;
    }
    if (m_file != INVALID_HANDLE_VALUE) {
        CloseHandle(m_file);
        m_file = INVALID_HANDLE_VALUE;
    }
    m_writable = false;
}

void SharedStateReader::TryOpen() {
    if (IsOpen()) return;

    // Prefer read/write so we can publish meter telemetry back to the GUI. Falls
    // back to read-only if the ProgramData ACL doesn't grant audiodg write (the EQ
    // still works; only the in/out meter goes dark).
    bool writable = true;
    HANDLE f = CreateFileW(
        kStatePath,
        GENERIC_READ | GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr);
    if (f == INVALID_HANDLE_VALUE) {
        writable = false;
        f = CreateFileW(
            kStatePath,
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            nullptr,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            nullptr);
    }
    if (f == INVALID_HANDLE_VALUE) return;

    LARGE_INTEGER sz{};
    // Need at least the core state to read; telemetry needs the full size.
    if (!GetFileSizeEx(f, &sz) || sz.QuadPart < static_cast<LONGLONG>(kStateCoreBytes)) {
        CloseHandle(f);
        return;
    }
    const bool telemetryFits = sz.QuadPart >= static_cast<LONGLONG>(kStateBytes);
    const DWORD mapLen = static_cast<DWORD>(telemetryFits ? kStateBytes : kStateCoreBytes);

    HANDLE m = CreateFileMappingW(f, nullptr,
                                  writable ? PAGE_READWRITE : PAGE_READONLY,
                                  0, mapLen, nullptr);
    if (!m) {
        CloseHandle(f);
        return;
    }

    void* view = MapViewOfFile(m, writable ? (FILE_MAP_READ | FILE_MAP_WRITE)
                                           : FILE_MAP_READ,
                               0, 0, mapLen);
    if (!view) {
        CloseHandle(m);
        CloseHandle(f);
        return;
    }

    m_file = f;
    m_mapping = m;
    m_view = reinterpret_cast<EqState*>(view);
    m_writable = writable && telemetryFits;
}

void SharedStateReader::WriteTelemetry(float inPeak, float inRms,
                                       float outPeak, float outRms, float grDb) {
    if (!m_view || !m_writable) return;
    Telemetry& t = m_view->telemetry;
    t.in_peak = inPeak;
    t.in_rms = inRms;
    t.out_peak = outPeak;
    t.out_rms = outRms;
    t.gr_db = grDb;
    // Bump seq last so the GUI seeing a fresh seq also sees fresh values (any
    // tearing within a single buffer is cosmetic on a meter).
    t.seq = t.seq + 1;
}

bool SharedStateReader::Read(EqState& out) const {
    if (!m_view) {
        FillDefault(out);
        return false;
    }

    // Seqlock reader. Bail after a few spins if the writer is thrashing — we'll
    // use the previous buffer's state instead (caller's `out` was already valid).
    for (int spin = 0; spin < 4; ++spin) {
        uint64_t v1 = m_view->version.load(std::memory_order_acquire);
        if ((v1 & 1ull) != 0) continue;

        // Memcpy is fine — everything past `version` is plain-old-data.
        std::memcpy(&out.bands, &m_view->bands, sizeof(out.bands));
        out.preamp_db = m_view->preamp_db;
        out.bypass = m_view->bypass;
        out.dynamics = m_view->dynamics;

        std::atomic_thread_fence(std::memory_order_acquire);
        uint64_t v2 = m_view->version.load(std::memory_order_acquire);
        if (v1 == v2 && v1 != 0) {
            out.version.store(v2, std::memory_order_relaxed);
            return true;
        }
        // else: torn or uninitialized (v==0), retry
    }
    // Couldn't get a consistent read. Don't clobber caller.
    return false;
}

} // namespace tronteq
