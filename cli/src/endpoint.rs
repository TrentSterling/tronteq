//! Render-endpoint enumeration + FxProperties attach/detach.
//!
//! We set `PKEY_FX_StreamEffectClsid` on the endpoint's PropertyStore (opened
//! in READWRITE mode). This is the per-user "assign a SFX APO" mechanism that
//! Windows honors when building the audio pipeline.

use anyhow::{Context, Result};
use windows::core::{BSTR, GUID, HRESULT, PCWSTR, PROPVARIANT};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    STGM_READ, STGM_READWRITE,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY};

use crate::com_reg::CLSID_STR;

// PKEY_FX_StreamEffectClsid — the "install this APO on this endpoint" key.
// {D04E05A6-594B-4FB6-A80D-01AF5EED7D1D}, pid 3.
const PKEY_FX_STREAM_EFFECT_CLSID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xD04E05A6_594B_4FB6_A80D_01AF5EED7D1D),
    pid: 3,
};

// PKEY_Device_FriendlyName — {A45C254E-DF1C-4EFD-8020-67D146A850E0}, pid 14
const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xA45C254E_DF1C_4EFD_8020_67D146A850E0),
    pid: 14,
};

// InitPropVariantFromCLSID ships in propsys.dll; the windows crate doesn't
// re-export the single-value variant, so link it ourselves. `PROPVARIANT` is
// #[repr(transparent)] so a *mut PROPVARIANT == *mut imp::PROPVARIANT at ABI.
#[link(name = "propsys")]
extern "system" {
    fn InitPropVariantFromCLSID(clsid: *const GUID, ppropvar: *mut PROPVARIANT) -> HRESULT;
}

pub struct Endpoint {
    pub id: String,
    pub friendly_name: String,
    pub is_default: bool,
}

struct ComGuard;
impl ComGuard {
    fn new() -> Result<Self> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_err() && hr.0 as u32 != 0x8001_0106 {
                hr.ok().context("CoInitializeEx")?;
            }
        }
        Ok(ComGuard)
    }
}
impl Drop for ComGuard {
    fn drop(&mut self) { unsafe { CoUninitialize(); } }
}

fn default_render_id(enumerator: &IMMDeviceEnumerator) -> Option<String> {
    unsafe {
        let dev = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let id = dev.GetId().ok()?;
        id.to_string().ok()
    }
}

fn device_friendly_name(dev: &IMMDevice) -> String {
    unsafe {
        let Ok(props) = dev.OpenPropertyStore(STGM_READ) else {
            return String::from("<unnamed>");
        };
        let Ok(var) = props.GetValue(&PKEY_DEVICE_FRIENDLY_NAME) else {
            return String::from("<unnamed>");
        };
        match BSTR::try_from(&var) {
            Ok(b) => b.to_string(),
            Err(_) => String::from("<unnamed>"),
        }
    }
}

fn device_id_string(dev: &IMMDevice) -> Result<String> {
    unsafe {
        let id = dev.GetId().context("IMMDevice::GetId")?;
        id.to_string().context("PWSTR -> String")
    }
}

pub fn list_render_endpoints() -> Result<Vec<Endpoint>> {
    let _com = ComGuard::new()?;
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).context("IMMDeviceEnumerator")?;
        let default_id = default_render_id(&enumerator);
        let coll = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .context("EnumAudioEndpoints")?;
        let count = coll.GetCount().context("GetCount")?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let dev = coll.Item(i).context("Item")?;
            let id = device_id_string(&dev)?;
            let friendly_name = device_friendly_name(&dev);
            let is_default = default_id.as_deref() == Some(id.as_str());
            out.push(Endpoint { id, friendly_name, is_default });
        }
        Ok(out)
    }
}

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
