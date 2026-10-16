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

// Mic stack: the capture-side class in the same DLL. Reads mic.bin.
// {68FF0293-9E2F-414B-834B-BE2ED82F607E}
DEFINE_GUID(CLSID_TrontEqMicApo,
    0x68ff0293, 0x9e2f, 0x414b, 0x83, 0x4b, 0xbe, 0x2e, 0xd8, 0x2f, 0x60, 0x7e);
// Mic effect id for GetEffectsList (kTrontEqMicEffectId in TrontEqApo.cpp).
// {69C648B4-33A0-42C4-BCE9-C8902F10ADC3}
