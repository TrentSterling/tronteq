//! Default-output loopback capture. Publishes coherent audio-clock snapshots.
use crate::{
    audio_analysis::{Analyzer, Frame},
    audio_format::Format,
};
use anyhow::{Context, Result};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Connecting,
    Live,
    Paused,
    Error(String),
}
#[derive(Clone)]
pub struct Source {
    pub endpoint_id: String,
    pub name: String,
    pub format: Format,
}
#[derive(Clone)]
pub struct Snapshot {
    pub revision: u64,
    pub source: Option<Source>,
    pub status: Status,
    pub frame: Arc<Frame>,
}
impl Snapshot {
    pub fn empty() -> Self {
        Self {
            revision: 0,
            source: None,
            status: Status::Connecting,
            frame: Arc::new(Analyzer::new(48_000, 2).snapshot()),
        }
    }
    pub fn description(&self) -> String {
        match (&self.status, &self.source) {
            (Status::Live, Some(s)) => format!(
                "{} Hz · {} channels · {}",
                s.format.rate,
                s.format.channels,
                s.format.label()
            ),
            (Status::Paused, _) => "Visualizer paused".into(),
            (Status::Error(e), _) => format!("Visualizer unavailable: {e}"),
            _ => "Connecting to Windows output...".into(),
        }
    }
}

pub struct Visualizer {
    shared: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}
impl Visualizer {
    /// A hidden-to-tray app releases its loopback client completely.
    pub fn start(paused: Arc<AtomicBool>) -> Self {
        let shared = Arc::new(Mutex::new(Snapshot::empty()));
        let stop = Arc::new(AtomicBool::new(false));
        let (output, stopping) = (shared.clone(), stop.clone());
        let handle = std::thread::spawn(move || {
            crate::log_line(&format!("thread: viz-capture tid={}", crate::tid()));
            if let Err(e) = unsafe { capture_loop(&output, &stopping, &paused) } {
                publish_status(&output, Status::Error(format!("{e:#}")));
            }
        });
        Self {
            shared,
            stop,
            handle: Some(handle),
        }
    }
    pub fn read(&self) -> Snapshot {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}
impl Drop for Visualizer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn publish_status(shared: &Mutex<Snapshot>, status: Status) {
    let mut snapshot = shared.lock().unwrap_or_else(|e| e.into_inner());
    if snapshot.status == status {
        return;
    }
    let revision = snapshot.revision.wrapping_add(1);
    *snapshot = Snapshot {
        revision,
        status,
        ..Snapshot::empty()
    };
}
fn publish(shared: &Mutex<Snapshot>, source: &Source, analysis: &Analyzer) {
    // FFTs and history updates never hold the shared lock.
    let frame = Arc::new(analysis.snapshot());
    let mut output = shared.lock().unwrap_or_else(|e| e.into_inner());
    *output = Snapshot {
        revision: output.revision.wrapping_add(1),
        source: Some(source.clone()),
        status: Status::Live,
        frame,
    };
}
unsafe fn endpoint_id(device: &IMMDevice) -> Result<String> {
    let raw = device.GetId()?;
    let value = raw.to_string();
    CoTaskMemFree(Some(raw.0.cast()));
    Ok(value?)
}
/// Frees GetMixFormat even when format validation or Initialize fails.
struct MixFormat(*mut WAVEFORMATEX);
impl Drop for MixFormat {
    fn drop(&mut self) {
        unsafe {
            CoTaskMemFree(Some(self.0.cast()));
        }
    }
}
struct RunningClient(IAudioClient);
impl Drop for RunningClient {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.Stop();
        }
    }
}

unsafe fn capture_loop(
    shared: &Mutex<Snapshot>,
    stop: &AtomicBool,
    paused: &AtomicBool,
) -> Result<()> {
    let _com = tronteq_shared::endpoints::ComGuard::new()?;
    while !stop.load(Ordering::Relaxed) {
        if paused.load(Ordering::Relaxed) {
            publish_status(shared, Status::Paused);
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        match run_session(shared, stop, paused) {
            Err(e) => {
                let message = format!("{e:#}");
                crate::log_line(&format!("viz: {message}"));
                publish_status(shared, Status::Error(message));
            }
            Ok(()) => publish_status(
                shared,
                if paused.load(Ordering::Relaxed) {
                    Status::Paused
                } else {
                    Status::Connecting
                },
            ),
        }
        for _ in 0..5 {
            if stop.load(Ordering::Relaxed) || paused.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}
/// No packets can arrive when Windows has no playing streams. Advance silence
/// after a buffer grace period, bounded after a long thread stall.
#[derive(Default)]
struct IdleClock {
    emitted: u64,
}
impl IdleClock {
    fn frames(&mut self, elapsed: Duration, rate: u32) -> usize {
        if elapsed < Duration::from_millis(100) {
            return 0;
        }
        let total = (elapsed.as_secs_f64() * rate as f64) as u64;
        let frames = total.saturating_sub(self.emitted).min(rate as u64 / 4);
        self.emitted = total;
        frames as usize
    }
}
unsafe fn run_session(
    shared: &Mutex<Snapshot>,
    stop: &AtomicBool,
    paused: &AtomicBool,
) -> Result<()> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator
        .GetDefaultAudioEndpoint(eRender, eConsole)
        .context("Windows output is unavailable")?;
    let id = endpoint_id(&device)?;
    let name = tronteq_shared::endpoints::list_render_endpoints()
        .ok()
        .and_then(|devices| devices.into_iter().find(|d| d.id == id))
        .map(|d| d.friendly_name)
        .unwrap_or_else(|| id.clone());
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    let raw = MixFormat(client.GetMixFormat()?);
    anyhow::ensure!(!raw.0.is_null(), "Windows returned an empty audio format");
    let bytes = std::slice::from_raw_parts(raw.0.cast::<u8>(), 18 + (*raw.0).cbSize as usize);
    let format = Format::parse(bytes).map_err(anyhow::Error::msg)?;
    client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK,
        2_000_000,
        0,
        raw.0,
        None,
    )?;
    drop(raw);
    let capture: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    let _running = RunningClient(client);
    let source = Source {
        endpoint_id: id,
        name,
        format,
    };
    crate::log_line(&format!(
        "viz: {} at {} Hz, {} channels, {}",
        source.name,
        format.rate,
        format.channels,
        format.label()
    ));
    let mut analysis = Analyzer::new(format.rate, format.channels);
    publish(shared, &source, &analysis);
    let mut published_tick = 0;
    let mut had_packet = false;
    let mut idle = IdleClock::default();
    let mut last_packet = Instant::now();
    let mut last_default_poll = Instant::now();
    let mut decoded = [0.0; 64];
    while !stop.load(Ordering::Relaxed) && !paused.load(Ordering::Relaxed) {
        if last_default_poll.elapsed() >= Duration::from_millis(500) {
            last_default_poll = Instant::now();
            let current = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            if endpoint_id(&current)? != source.endpoint_id {
                return Ok(());
            }
        }
        while capture.GetNextPacketSize()? > 0 {
            if stop.load(Ordering::Relaxed) || paused.load(Ordering::Relaxed) {
                return Ok(());
            }
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
            capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            if frames == 0 {
                break;
            } // AUDCLNT_S_BUFFER_EMPTY: nothing to release.
            if flags & 1 != 0 && had_packet && idle.emitted == 0 {
                analysis = Analyzer::new(format.rate, format.channels);
                published_tick = u64::MAX;
            }
            if flags & 2 != 0 || data.is_null() {
                analysis.silence(frames as usize);
            } else {
                let bytes = std::slice::from_raw_parts(data, frames as usize * format.stride);
                for frame in bytes.chunks_exact(format.stride) {
                    format.decode(frame, &mut decoded);
                    analysis.feed(&decoded);
                }
            }
            capture.ReleaseBuffer(frames)?;
            had_packet = true;
            last_packet = Instant::now();
            idle = IdleClock::default();
        }
        analysis.silence(idle.frames(last_packet.elapsed(), format.rate));
        if analysis.ticks != published_tick {
            published_tick = analysis.ticks;
            publish(shared, &source, &analysis);
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Read-only capture of the real Windows default output, including pause/resume"]
    fn live_capture_publishes_audio_clock_and_releases_while_paused() {
        let paused = Arc::new(AtomicBool::new(false));
        let viz = Visualizer::start(paused.clone());
        let wait = |wanted: Status| {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                let s = viz.read();
                if s.status == wanted && (wanted != Status::Live || s.frame.ticks > 5) {
                    return s;
                }
                assert!(Instant::now() < deadline, "capture timeout: {:?}", s.status);
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        let first = wait(Status::Live);
        println!(
            "CAPTURE {}: {}",
            first.source.as_ref().unwrap().name,
            first.description()
        );
        std::thread::sleep(Duration::from_secs(1));
        let second = viz.read();
        let elapsed = second.frame.ticks.saturating_sub(first.frame.ticks);
        assert!(
            (50..=75).contains(&elapsed),
            "audio ticks in one second: {elapsed}"
        );
        assert!(second.revision > first.revision);
        assert!(second.frame.spectrum.iter().all(|v| v.is_finite()));
        println!(
            "CLOCK {elapsed} ticks; peak {:.5}, rms {:.5}",
            second.frame.peak, second.frame.rms
        );
        paused.store(true, Ordering::Relaxed);
        let hidden = wait(Status::Paused);
        assert!(hidden.source.is_none());
        assert_eq!(hidden.frame.peak, 0.0);
        std::thread::sleep(Duration::from_millis(250));
        assert_eq!(viz.read().revision, hidden.revision);
        paused.store(false, Ordering::Relaxed);
        let resumed = wait(Status::Live);
        assert!(resumed.revision > hidden.revision);
        println!("PAUSE cleared state; RESUME {}", resumed.description());
    }
    #[test]
    fn absent_packets_advance_silence_once_and_bound_stall_work() {
        let mut idle = IdleClock::default();
        assert_eq!(idle.frames(Duration::from_millis(90), 48_000), 0);
        assert_eq!(idle.frames(Duration::from_millis(100), 48_000), 4800);
        assert_eq!(idle.frames(Duration::from_millis(100), 48_000), 0);
        assert_eq!(idle.frames(Duration::from_millis(110), 48_000), 480);
        assert_eq!(idle.frames(Duration::from_secs(60), 48_000), 12_000);
        assert_eq!(idle.frames(Duration::from_secs(60), 48_000), 0);
    }
    #[test]
    fn failed_or_paused_capture_discards_stale_analysis_and_source() {
        let shared = Mutex::new(Snapshot::empty());
        let format = Format {
            rate: 48_000,
            channels: 2,
            stride: 8,
            encoding: crate::audio_format::Encoding::Float32,
        };
        let source = Source {
            endpoint_id: "fixture".into(),
            name: "Fixture".into(),
            format,
        };
        let mut analysis = Analyzer::new(48_000, 2);
        for _ in 0..800 {
            analysis.feed(&[0.25, -0.25]);
        }
        publish(&shared, &source, &analysis);
        let live = shared.lock().unwrap().clone();
        assert!(live.frame.peak > 0.2);
        for status in [Status::Error("device removed".into()), Status::Paused] {
            publish_status(&shared, status.clone());
            let read = shared.lock().unwrap().clone();
            assert_eq!(read.status, status);
            assert!(read.source.is_none());
            assert_eq!(read.frame.peak, 0.0);
            assert!(read.frame.wave.iter().all(|v| *v == 0.0));
            assert!(read.revision > live.revision);
        }
        assert!(live.frame.peak > 0.2, "Published snapshots are immutable");
    }
}
