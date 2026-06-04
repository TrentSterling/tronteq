// SharedState.h — file-backed memory-mapped IPC to the Rust GUI.
// Bridges Session 0 (audiodg) <-> user session (GUI).
#pragma once

#include <Windows.h>
#include "EqState.h"

namespace tronteq {

class SharedStateReader {
public:
    SharedStateReader();
    ~SharedStateReader();

    SharedStateReader(const SharedStateReader&) = delete;
    SharedStateReader& operator=(const SharedStateReader&) = delete;

    // Try to open C:\ProgramData\TrontEq\state.bin. Succeeds silently if the file
    // is absent; the APO will fall back to a flat-bypass state until the GUI
    // (or CLI) creates the file.
    void TryOpen();
    void Close();

    // Seqlock read into `out`. On torn read (or no file), fills with defaults.
    // Return true if state came from the file, false if defaults were used.
    bool Read(EqState& out) const;

    bool IsOpen() const { return m_view != nullptr; }

private:
    HANDLE m_file = INVALID_HANDLE_VALUE;
    HANDLE m_mapping = nullptr;
    EqState* m_view = nullptr;
};

// Populate `out` with flat bands at the POC default frequencies + bypass on.
void FillDefault(EqState& out);

} // namespace tronteq
