//! Read the actual Windows mixer independently of persisted APO telemetry.
use anyhow::Result;
use windows::core::{Interface, HSTRING};
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, IAudioSessionManager2, IAudioSessionControl2, ISimpleAudioVolume};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

pub fn print_status() -> Result<()> {
    let _com = tronteq_shared::endpoints::ComGuard::new()?;
    let endpoints = tronteq_shared::endpoints::list_render_endpoints()?;
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        for endpoint in endpoints {
            println!("{}{}\n  {}", endpoint.friendly_name, if endpoint.is_default { " [default]" } else { "" }, endpoint.id);
            let device = enumerator.GetDevice(&HSTRING::from(&endpoint.id))?;
            let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            let meter: IAudioMeterInformation = device.Activate(CLSCTX_ALL, None)?;
            println!("  volume={:.1}% mute={} peak={:.6}", volume.GetMasterVolumeLevelScalar()? * 100.0, volume.GetMute()?.as_bool(), meter.GetPeakValue()?);
            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let sessions = manager.GetSessionEnumerator()?;
            for index in 0..sessions.GetCount()? {
                let session = sessions.GetSession(index)?;
                let control: IAudioSessionControl2 = session.cast()?;
                let volume: ISimpleAudioVolume = session.cast()?;
                let peak = session.cast::<IAudioMeterInformation>().and_then(|m| m.GetPeakValue()).unwrap_or(0.0);
                println!("  session pid={} state={:?} volume={:.1}% mute={} peak={:.6}", control.GetProcessId()?, session.GetState()?, volume.GetMasterVolume()? * 100.0, volume.GetMute()?.as_bool(), peak);
            }
        }
    }
    Ok(())
}
