//! HKCR\CLSID registration for TrontEqApo. This is the in-process COM server
//! registration — it tells Windows where the DLL lives for a given CLSID.

use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CLASSES_ROOT,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ,
};

pub const CLSID_STR: &str = "{CA64E60A-A3C4-43B8-970F-0360055172F2}";
const FRIENDLY_NAME: &str = "TrontEQ Stream Effect APO";

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn set_string_value(parent: HKEY, subkey: &str, value_name: Option<&str>, value: &str) -> Result<()> {
    let subkey_w = to_wide(subkey);
    let mut key = HKEY::default();
    let sam: REG_SAM_FLAGS = KEY_WRITE;
    let status = unsafe {
        RegCreateKeyExW(
            parent,
            PCWSTR(subkey_w.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            sam,
            None,
            &mut key,
            None,
        )
    };
    status.ok().with_context(|| format!("RegCreateKeyExW {subkey}"))?;

    let name_w = value_name.map(to_wide);
    let value_w = to_wide(value);
    let value_bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(
            value_w.as_ptr() as *const u8,
            value_w.len() * std::mem::size_of::<u16>(),
        )
    };

    let result = unsafe {
        RegSetValueExW(
            key,
            match &name_w {
                Some(n) => PCWSTR(n.as_ptr()),
                None => PCWSTR::null(),
            },
            0,
            REG_SZ,
            Some(value_bytes),
        )
    };
    unsafe { let _ = RegCloseKey(key); }
    result.ok().with_context(|| format!("RegSetValueExW {subkey}"))?;
    Ok(())
}

pub fn register_inproc(dll_path: &str) -> Result<()> {
    let clsid_key = format!("CLSID\\{CLSID_STR}");
    let inproc_key = format!("CLSID\\{CLSID_STR}\\InprocServer32");

    set_string_value(HKEY_CLASSES_ROOT, &clsid_key, None, FRIENDLY_NAME)?;
    set_string_value(HKEY_CLASSES_ROOT, &inproc_key, None, dll_path)?;
    set_string_value(HKEY_CLASSES_ROOT, &inproc_key, Some("ThreadingModel"), "Both")?;
    Ok(())
}

pub fn unregister_inproc() -> Result<()> {
    let clsid_key = format!("CLSID\\{CLSID_STR}");
    let clsid_key_w = to_wide(&clsid_key);
    let r = unsafe { RegDeleteTreeW(HKEY_CLASSES_ROOT, PCWSTR(clsid_key_w.as_ptr())) };
    if r.0 != 0 && r.0 != 2 {
        anyhow::bail!("RegDeleteTreeW failed: {:?}", r);
    }
    Ok(())
}
