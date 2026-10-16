//! Render-endpoint enumeration + FxProperties attach/detach.
//!
//! We set `PKEY_FX_StreamEffectClsid` on the endpoint's PropertyStore (opened
//! in READWRITE mode). This is the per-user "assign a SFX APO" mechanism that
//! Windows honors when building the audio pipeline.

use anyhow::{Context, Result};
use windows::core::{GUID, HRESULT, PCWSTR, PROPVARIANT};
use windows::Win32::Media::Audio::{
    IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CLSCTX_ALL, STGM_READWRITE,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY};

use crate::com_reg::CLSID_STR;

// PKEY_FX_StreamEffectClsid — the "install this APO on this endpoint" key.
// {D04E05A6-594B-4FB6-A80D-01AF5EED7D1D}, pid 3.
const PKEY_FX_STREAM_EFFECT_CLSID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xD04E05A6_594B_4FB6_A80D_01AF5EED7D1D),
    pid: 3,
};

// InitPropVariantFromCLSID ships in propsys.dll; the windows crate doesn't
// re-export the single-value variant, so link it ourselves. `PROPVARIANT` is
// #[repr(transparent)] so a *mut PROPVARIANT == *mut imp::PROPVARIANT at ABI.
#[link(name = "propsys")]
extern "system" {
    fn InitPropVariantFromCLSID(clsid: *const GUID, ppropvar: *mut PROPVARIANT) -> HRESULT;
}

pub use tronteq_shared::endpoints::{list_capture_endpoints, list_render_endpoints};
use tronteq_shared::endpoints::ComGuard;

/// Parse our CLSID into a `GUID`.
///
/// `CLSID_STR` is registry format with braces (`{...}`), but windows-core's
/// `From<&str> for GUID` asserts the input is exactly the bare 36-char form
/// (`8-4-4-4-12`, no braces) and panics otherwise — and since only `From` is
/// implemented, `try_into()` cannot return `Err`, it just panics. Strip the
/// braces so we hand it the shape it expects.
fn clsid_guid() -> GUID {
    GUID::from(CLSID_STR.trim_matches(|c| c == '{' || c == '}'))
}

fn clsid_propvariant() -> Result<PROPVARIANT> {
    let clsid = clsid_guid();
    let mut var = PROPVARIANT::new();
    let hr = unsafe { InitPropVariantFromCLSID(&clsid, &mut var) };
    hr.ok().context("InitPropVariantFromCLSID")?;
    Ok(var)
}

pub fn attach_fx(endpoint_id: &str) -> Result<()> {
    let _com = ComGuard::new()?;
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let id_w: Vec<u16> = endpoint_id.encode_utf16().chain(std::iter::once(0)).collect();
        let dev = enumerator.GetDevice(PCWSTR(id_w.as_ptr())).context("GetDevice")?;
        let props: IPropertyStore = dev.OpenPropertyStore(STGM_READWRITE).context("OpenPropertyStore RW")?;
        let var = clsid_propvariant()?;
        props.SetValue(&PKEY_FX_STREAM_EFFECT_CLSID, &var).context("SetValue PKEY_FX_StreamEffectClsid")?;
        props.Commit().context("Commit")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clsid_guid_parses_registry_string() {
        // Must equal the same CLSID expressed numerically. If `clsid_guid`
        // fails to strip braces it panics inside windows-core's GUID parser.
        assert_eq!(
            clsid_guid(),
            GUID::from_u128(0xCA64E60A_A3C4_43B8_970F_0360055172F2)
        );
    }
}

pub fn detach_fx(endpoint_id: &str) -> Result<()> {
    let _com = ComGuard::new()?;
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let id_w: Vec<u16> = endpoint_id.encode_utf16().chain(std::iter::once(0)).collect();
        let dev = enumerator.GetDevice(PCWSTR(id_w.as_ptr())).context("GetDevice")?;
        let props: IPropertyStore = dev.OpenPropertyStore(STGM_READWRITE).context("OpenPropertyStore RW")?;
        let empty = PROPVARIANT::new();
        props.SetValue(&PKEY_FX_STREAM_EFFECT_CLSID, &empty).context("SetValue clear")?;
        props.Commit().context("Commit")?;
    }
    Ok(())
}
