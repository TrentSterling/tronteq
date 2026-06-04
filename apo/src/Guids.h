// TrontEQ APO GUIDs. Generated once; never regenerate.
// Mirror in cli/src/com_reg.rs.
#pragma once

#include <guiddef.h>

// {CA64E60A-A3C4-43B8-970F-0360055172F2}
DEFINE_GUID(CLSID_TrontEqApo,
    0xca64e60a, 0xa3c4, 0x43b8, 0x97, 0x0f, 0x03, 0x60, 0x05, 0x51, 0x72, 0xf2);

// Effect id reported by IAudioSystemEffects2::GetEffectsList (NOT the CLSID — an
// arbitrary stable id naming the "TrontEQ" effect). Defined with storage in
// TrontEqApo.cpp (kTrontEqEffectId), since it is read at runtime.
// {9D8C1A32-4F6E-4D21-9A77-21550C338801}
