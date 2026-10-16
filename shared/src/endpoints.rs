//! Read-only Windows output and input enumeration, shared by the GUI and installer.
//! Listing devices must not depend on the installer executable being present.

use anyhow::{Context, Result};
use windows::core::{BSTR, GUID};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

const FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xA45C254E_DF1C_4EFD_8020_67D146A850E0),
    pid: 14,
};

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub id: String,
    pub friendly_name: String,
    pub is_default: bool,
}

pub struct ComGuard(bool);

impl ComGuard {
    pub fn new() -> Result<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // The GUI can already own an STA. Use it, without uninitializing the
        // apartment we did not acquire (RPC_E_CHANGED_MODE).
        if hr.0 as u32 == 0x8001_0106 {
            return Ok(Self(false));
        }
        hr.ok().context("CoInitializeEx")?;
        Ok(Self(true))
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

fn device_id(device: &IMMDevice) -> Result<String> {
    unsafe {
        let raw = device.GetId().context("IMMDevice::GetId")?;
        let id = raw.to_string().context("output endpoint ID");
        CoTaskMemFree(Some(raw.0.cast()));
        id
    }
}

pub fn list_render_endpoints() -> Result<Vec<Endpoint>> {
    list_endpoints(eRender)
}

/// Active microphones / line inputs, for the mic stack.
pub fn list_capture_endpoints() -> Result<Vec<Endpoint>> {
    list_endpoints(eCapture)
}

fn list_endpoints(flow: EDataFlow) -> Result<Vec<Endpoint>> {
    let _com = ComGuard::new()?;
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("IMMDeviceEnumerator")?;
        let default_id = enumerator.GetDefaultAudioEndpoint(flow, eConsole)
            .ok().and_then(|d| device_id(&d).ok());
        let collection = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
            .context("EnumAudioEndpoints")?;
        let mut devices = Vec::new();
        for i in 0..collection.GetCount().context("device count")? {
            let device = collection.Item(i).context("audio device")?;
            let id = device_id(&device)?;
            let friendly_name = device.OpenPropertyStore(STGM_READ)
                .and_then(|props| props.GetValue(&FRIENDLY_NAME))
                .and_then(|value| BSTR::try_from(&value))
                .map(|name| name.to_string())
                .unwrap_or_else(|_| id.clone());
            let is_default = default_id.as_deref() == Some(id.as_str());
            devices.push(Endpoint { id, friendly_name, is_default });
        }
        Ok(devices)
    }
}

/// Keep a user's selection across refresh/reordering; fall back to the current
/// default if the selected endpoint was unplugged.
pub fn selected_index(devices: &[Endpoint], previous_id: Option<&str>) -> usize {
    previous_id.and_then(|id| devices.iter().position(|d| d.id == id))
        .or_else(|| devices.iter().position(|d| d.is_default))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_survives_reordering_and_disconnect() {
        let outputs = vec![
            Endpoint { id: "b".into(), friendly_name: "Same name".into(), is_default: false },
            Endpoint { id: "a".into(), friendly_name: "Same name".into(), is_default: true },
        ];
        assert_eq!(selected_index(&outputs, Some("b")), 0);
        assert_eq!(selected_index(&outputs, Some("a")), 1);
        assert_eq!(selected_index(&outputs, Some("unplugged")), 1);
        assert_eq!(selected_index(&outputs, None), 1);
        assert_eq!(selected_index(&[], Some("a")), 0);
    }
}
