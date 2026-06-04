// Dll.cpp — COM plumbing for TrontEqApo.dll.
// Uses ATL's CAtlDllModuleT to auto-wire the four exports + class factory.

#include <Windows.h>
#include <atlbase.h>
#include <atlcom.h>
#include <atlctl.h>
#include <new>

#include "Guids.h"
#include "TrontEqApo.h"

using tronteq::TrontEqApo;

class CTrontEqModule : public CAtlDllModuleT<CTrontEqModule> {
public:
    DECLARE_LIBID(LIBID_ATLLib) // unused, but CAtlDllModuleT wants one
};

CTrontEqModule _AtlModule;
static HMODULE g_module = nullptr;

// ---- Module lifecycle -------------------------------------------------------

BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpv) {
    if (fdwReason == DLL_PROCESS_ATTACH) {
        g_module = hinstDLL;
    }
    return _AtlModule.DllMain(fdwReason, lpv);
}

// ---- Exports (names match combaseapi.h / olectl.h forward declarations) -----

STDAPI DllGetClassObject(REFCLSID rclsid, REFIID riid, LPVOID* ppv) {
    return _AtlModule.DllGetClassObject(rclsid, riid, ppv);
}

STDAPI DllCanUnloadNow() {
    return _AtlModule.DllCanUnloadNow();
}

// ---- Self-registration ------------------------------------------------------
//
// ATL's DllRegisterServer uses .rgs script files; we don't want that dependency,
// so we write the minimal CLSID subkey by hand. InprocServer32 -> DLL path.

static const wchar_t* kClsidText = L"{CA64E60A-A3C4-43B8-970F-0360055172F2}";
static const wchar_t* kFriendly  = L"TrontEQ Stream Effect APO";

static HRESULT SetKeyValue(HKEY root, const wchar_t* subkey,
                           const wchar_t* valueName, const wchar_t* data) {
    HKEY k = nullptr;
    LONG r = RegCreateKeyExW(root, subkey, 0, nullptr, REG_OPTION_NON_VOLATILE,
                             KEY_WRITE, nullptr, &k, nullptr);
    if (r != ERROR_SUCCESS) return HRESULT_FROM_WIN32(r);
    DWORD bytes = static_cast<DWORD>((wcslen(data) + 1) * sizeof(wchar_t));
    r = RegSetValueExW(k, valueName, 0, REG_SZ,
                       reinterpret_cast<const BYTE*>(data), bytes);
    RegCloseKey(k);
    return r == ERROR_SUCCESS ? S_OK : HRESULT_FROM_WIN32(r);
}

STDAPI DllRegisterServer() {
    wchar_t modulePath[MAX_PATH];
    if (!GetModuleFileNameW(g_module, modulePath, MAX_PATH)) {
        return HRESULT_FROM_WIN32(GetLastError());
    }

    wchar_t clsidKey[256];
    swprintf_s(clsidKey, L"CLSID\\%s", kClsidText);
    wchar_t inprocKey[256];
    swprintf_s(inprocKey, L"CLSID\\%s\\InprocServer32", kClsidText);

    HRESULT hr;
    hr = SetKeyValue(HKEY_CLASSES_ROOT, clsidKey, nullptr, kFriendly);
    if (FAILED(hr)) return hr;
    hr = SetKeyValue(HKEY_CLASSES_ROOT, inprocKey, nullptr, modulePath);
    if (FAILED(hr)) return hr;
    hr = SetKeyValue(HKEY_CLASSES_ROOT, inprocKey, L"ThreadingModel", L"Both");
    return hr;
}

STDAPI DllUnregisterServer() {
    wchar_t clsidKey[256];
    swprintf_s(clsidKey, L"CLSID\\%s", kClsidText);
    LSTATUS r = RegDeleteTreeW(HKEY_CLASSES_ROOT, clsidKey);
    if (r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND) return S_OK;
    return HRESULT_FROM_WIN32(r);
}
