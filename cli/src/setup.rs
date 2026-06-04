//! The full system-EQ-APO install recipe, ported from the PowerShell we proved
//! out by hand. Three privileged pieces beyond the basic COM registration:
//!
//!  1. `register_apo` — register the APO in the engine's APO store
//!     (`HKLM\SOFTWARE\Classes\AudioEngine\AudioProcessingObjects\{clsid}`).
//!     Without this the engine never instantiates the CLSID as an APO.
//!  2. `set_endpoint_efx` — point an endpoint's EFX (endpoint-effect) slot at our
//!     CLSID. That lives in the TrustedInstaller-owned `...\FxProperties` key; we
//!     enable SeBackup/SeRestore and open with `REG_OPTION_BACKUP_RESTORE`, which
//!     grants access regardless of the ACL *without* changing key ownership.
//!  3. `grant_programdata_acl` — the APO runs in the restricted `audiodg` token,
//!     so the settings dir must grant `ALL APPLICATION PACKAGES` read.

use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Threading::OpenProcessToken;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_QUERY_VALUE, KEY_SET_VALUE, REG_CREATE_KEY_DISPOSITION, REG_DWORD, REG_OPTION_BACKUP_RESTORE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};

use crate::com_reg::CLSID_STR;

// PKEY_FX_EndpointEffectClsid = {d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6 (EFX slot).
const EFX_VALUE_NAME: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6";
// The system-effects APO interface IID Windows expects in APOInterface0.
const APO_INTERFACE_IID: &str = "{FD7F2B29-24D0-4B5C-B177-592C39F9CA10}";
const APO_STORE: &str = r"SOFTWARE\Classes\AudioEngine\AudioProcessingObjects";

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn wide_bytes(s: &str) -> Vec<u8> {
    let w = wide(s);
    let mut b = Vec::with_capacity(w.len() * 2);
    for u in w {
        b.extend_from_slice(&u.to_le_bytes());
    }
    b
}

/// Derive the registry endpoint key name from an MMDevice id. The id looks like
/// `{0.0.0.00000000}.{76af72a1-...}`; the registry key is the trailing GUID.
pub fn endpoint_reg_guid(device_id: &str) -> Result<String> {
    let guid = device_id
        .rsplit_once('.')
        .map(|(_, g)| g)
        .filter(|g| g.starts_with('{') && g.ends_with('}'))
        .with_context(|| format!("unexpected endpoint id format: {device_id}"))?;
    Ok(guid.to_string())
}

// ---- APO registration (AudioProcessingObjects store) ------------------------

fn open_hklm(subkey: &str, options: u32, sam: u32) -> Result<HKEY> {
    let mut key = HKEY::default();
    let mut disp = REG_CREATE_KEY_DISPOSITION::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide(subkey).as_ptr()),
            0,
            PCWSTR::null(),
            windows::Win32::System::Registry::REG_OPEN_CREATE_OPTIONS(options),
            windows::Win32::System::Registry::REG_SAM_FLAGS(sam),
            None,
            &mut key,
            Some(&mut disp),
        )
    };
    status.ok().with_context(|| format!("RegCreateKeyExW {subkey}"))?;
    Ok(key)
}

fn set_sz(key: HKEY, name: &str, value: &str) -> Result<()> {
    let r = unsafe {
        RegSetValueExW(key, PCWSTR(wide(name).as_ptr()), 0, REG_SZ, Some(&wide_bytes(value)))
    };
    r.ok().with_context(|| format!("set REG_SZ {name}"))
}

fn set_dword(key: HKEY, name: &str, value: u32) -> Result<()> {
    let r = unsafe {
        RegSetValueExW(key, PCWSTR(wide(name).as_ptr()), 0, REG_DWORD, Some(&value.to_le_bytes()))
    };
    r.ok().with_context(|| format!("set REG_DWORD {name}"))
}

/// Register the APO in the engine's AudioProcessingObjects store. Mirrors the
/// metadata Windows' own APOs carry (verified against WMALFXGFXDSP).
pub fn register_apo() -> Result<()> {
    let subkey = format!("{APO_STORE}\\{CLSID_STR}");
    let key = open_hklm(&subkey, REG_OPTION_NON_VOLATILE.0, KEY_SET_VALUE.0)?;
    let res = (|| {
        set_sz(key, "FriendlyName", "TrontEQ APO")?;
        set_sz(key, "Copyright", "Trent Sterling")?;
        set_dword(key, "MajorVersion", 1)?;
        set_dword(key, "MinorVersion", 0)?;
        // APO_FLAG_INPLACE|SAMPLESPERFRAME|FRAMESPERSECOND|BITSPERSAMPLE_MUST_MATCH
        set_dword(key, "Flags", 15)?;
        set_dword(key, "MinInputConnections", 1)?;
        set_dword(key, "MaxInputConnections", 1)?;
        set_dword(key, "MinOutputConnections", 1)?;
        set_dword(key, "MaxOutputConnections", 1)?;
        set_dword(key, "MaxInstances", 0xFFFF_FFFF)?;
        set_dword(key, "NumAPOInterfaces", 1)?;
        set_sz(key, "APOInterface0", APO_INTERFACE_IID)?;
        Ok(())
    })();
    unsafe { let _ = RegCloseKey(key); }
    res
}

pub fn unregister_apo() -> Result<()> {
    let subkey = format!("{APO_STORE}\\{CLSID_STR}");
    let r = unsafe {
        windows::Win32::System::Registry::RegDeleteTreeW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide(&subkey).as_ptr()),
        )
    };
    // 2 == ERROR_FILE_NOT_FOUND (already gone) is fine.
    if r.0 != 0 && r.0 != 2 {
        bail!("RegDeleteTreeW {subkey}: {:?}", r);
    }
    Ok(())
}

// ---- Privileged FxProperties (EFX slot) write -------------------------------

fn enable_privilege(name: &str) -> Result<()> {
    unsafe {
        let proc = HANDLE(-1isize as *mut core::ffi::c_void); // GetCurrentProcess pseudo-handle
        let mut token = HANDLE::default();
        OpenProcessToken(proc, TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token)
            .context("OpenProcessToken")?;
        let mut luid = LUID::default();
        LookupPrivilegeValueW(PCWSTR::null(), PCWSTR(wide(name).as_ptr()), &mut luid)
            .with_context(|| format!("LookupPrivilegeValue {name}"))?;
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        let res = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None)
            .with_context(|| format!("AdjustTokenPrivileges {name}"));
        let _ = CloseHandle(token);
        res
    }
}

fn open_fxproperties(endpoint_reg_guid: &str) -> Result<HKEY> {
    enable_privilege("SeBackupPrivilege")?;
    enable_privilege("SeRestorePrivilege")?;
    let subkey = format!(
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\{endpoint_reg_guid}\FxProperties"
    );
    // REG_OPTION_BACKUP_RESTORE + SeBackup/SeRestore opens regardless of the
    // TrustedInstaller ACL, without changing the key's owner.
    open_hklm(
        &subkey,
        REG_OPTION_BACKUP_RESTORE.0,
        (KEY_SET_VALUE | KEY_QUERY_VALUE).0,
    )
}

/// Point the endpoint's EFX slot at our APO so Windows runs it on that output.
pub fn set_endpoint_efx(endpoint_reg_guid: &str) -> Result<()> {
    let key = open_fxproperties(endpoint_reg_guid)?;
    let res = set_sz(key, EFX_VALUE_NAME, CLSID_STR);
    unsafe { let _ = RegCloseKey(key); }
    res.context("write EFX FxProperties value")
}

/// Remove our APO from the endpoint's EFX slot (restore default behavior).
pub fn clear_endpoint_efx(endpoint_reg_guid: &str) -> Result<()> {
    let key = open_fxproperties(endpoint_reg_guid)?;
    let r = unsafe { RegDeleteValueW(key, PCWSTR(wide(EFX_VALUE_NAME).as_ptr())) };
    unsafe { let _ = RegCloseKey(key); }
    // ERROR_FILE_NOT_FOUND (2) means it was already absent.
    if r.0 != 0 && r.0 != 2 {
        bail!("RegDeleteValueW EFX: {:?}", r);
    }
    Ok(())
}

// ---- ProgramData ACL + audio service ----------------------------------------

/// Grant the restricted audiodg token read access to the settings dir.
pub fn grant_programdata_acl(dir: &str) -> Result<()> {
    // ALL APPLICATION PACKAGES, ALL RESTRICTED APPLICATION PACKAGES, LOCAL SERVICE.
    let status = Command::new("icacls")
        .args([
            dir,
            "/grant",
            "*S-1-15-2-1:(OI)(CI)(RX)",
            "/grant",
            "*S-1-15-2-2:(OI)(CI)(RX)",
            "/grant",
            "*S-1-5-19:(OI)(CI)(RX)",
            "/T",
            "/C",
        ])
        .status()
        .context("run icacls")?;
    if !status.success() {
        bail!("icacls failed: {:?}", status);
    }
    Ok(())
}

pub fn stop_audio() -> Result<()> {
    let _ = Command::new("net").args(["stop", "audiosrv", "/y"]).status();
    Ok(())
}

pub fn start_audio() -> Result<()> {
    // audiosrv has dependent services; `net start` starts it, the deps come back
    // with it. Best-effort.
    let _ = Command::new("net").args(["start", "audiosrv"]).status();
    Ok(())
}

// ---- Autostart (scheduled task: elevated at logon, no UAC) -------------------

fn gui_exe_path() -> Result<String> {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .context("resolve current exe dir")?;
    Ok(dir.join("tronteq.exe").to_string_lossy().into_owned())
}

/// Create a Scheduled Task that launches the GUI elevated at logon. Running with
/// highest privileges from a logon task means no UAC prompt.
pub fn register_autostart() -> Result<()> {
    let exe = gui_exe_path()?;
    let status = Command::new("schtasks")
        .args([
            "/create", "/tn", "TrontEQ", "/tr", &exe, "/sc", "onlogon", "/rl", "HIGHEST", "/f",
        ])
        .status()
        .context("run schtasks /create")?;
    if !status.success() {
        bail!("schtasks /create failed: {:?}", status);
    }
    Ok(())
}

pub fn unregister_autostart() -> Result<()> {
    let _ = Command::new("schtasks")
        .args(["/delete", "/tn", "TrontEQ", "/f"])
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_endpoint_reg_guid() {
        let id = "{0.0.0.00000000}.{76af72a1-a1af-42f8-88ea-7f5023c6e269}";
        assert_eq!(
            endpoint_reg_guid(id).unwrap(),
            "{76af72a1-a1af-42f8-88ea-7f5023c6e269}"
        );
    }

    #[test]
    fn rejects_malformed_id() {
        assert!(endpoint_reg_guid("garbage").is_err());
    }
}
