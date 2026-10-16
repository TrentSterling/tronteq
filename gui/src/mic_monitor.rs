//! Mic monitor: captures one microphone in shared mode (so it hears the mic
//! stack's output, like any other app would), analyzes it for the MIC tab's
//! curve and meters, and optionally plays it to the default output ("listen").
//!
//! The capture releases itself when nobody is looking and listen is off, so a
//! tray'd TrontEQ never keeps the Windows "microphone in use" indicator lit.
use crate::{
    audio_analysis::{Analyzer, Frame},
    audio_format::Format,
    visualizer::Status,
};
use anyhow::{Context, Result};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::core::{GUID, PCWSTR};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IAudioRenderClient, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

/// Stop capturing this long after the MIC tab was last drawn (unless listening).
const IDLE_RELEASE: Duration = Duration::from_secs(2);
/// Listen buffer. Small enough to hear yourself without a distracting echo.
const LISTEN_BUFFER_HNS: i64 = 400_000; // 40 ms

#[derive(Clone)]
pub struct Snapshot {
    pub status: Status,
    pub rate: u32,
    pub frame: Arc<Frame>,
    pub listen_error: Option<String>,
}

struct Control {
    stop: AtomicBool,
    listen: AtomicBool,
    volume: AtomicU32, // f32 bits, linear gain
    seen_ms: AtomicU64, // last time the UI asked, ms since `epoch`
    epoch: Instant,
}
impl Control {
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }
    fn wanted(&self) -> bool {
        self.listen.load(Ordering::Relaxed)
            || self.now_ms().saturating_sub(self.seen_ms.load(Ordering::Relaxed))
                < IDLE_RELEASE.as_millis() as u64
    }
}

pub struct MicMonitor {
    endpoint_id: String,
    shared: Arc<Mutex<Snapshot>>,
    ctl: Arc<Control>,
    handle: Option<JoinHandle<()>>,
}

impl MicMonitor {
    pub fn start(endpoint_id: String, listen: bool, volume: f32) -> Self {
        let shared = Arc::new(Mutex::new(empty(Status::Connecting)));
        let ctl = Arc::new(Control {
            stop: AtomicBool::new(false),
            listen: AtomicBool::new(listen),
            volume: AtomicU32::new(volume.to_bits()),
            seen_ms: AtomicU64::new(0),
            epoch: Instant::now(),
        });
        let (out, c, id) = (shared.clone(), ctl.clone(), endpoint_id.clone());
        let handle = std::thread::spawn(move || {
            crate::log_line(&format!("thread: mic-monitor tid={}", crate::tid()));
            unsafe { monitor_loop(&id, &out, &c) };
        });
        Self { endpoint_id, shared, ctl, handle: Some(handle) }
    }
    pub fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }
    pub fn set_listen(&self, on: bool) {
        self.ctl.listen.store(on, Ordering::Relaxed);
    }
    pub fn set_volume(&self, linear: f32) {
        self.ctl.volume.store(linear.clamp(0.0, 4.0).to_bits(), Ordering::Relaxed);
    }
    /// Read the latest analysis; also marks the monitor as being watched.
    pub fn read(&self) -> Snapshot {
        self.ctl.seen_ms.store(self.ctl.now_ms(), Ordering::Relaxed);
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Drop for MicMonitor {
    fn drop(&mut self) {
        self.ctl.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn empty(status: Status) -> Snapshot {
    Snapshot { status, rate: 48_000, frame: Arc::new(Analyzer::new(48_000, 1).snapshot()), listen_error: None }
}

fn set_status(shared: &Mutex<Snapshot>, status: Status) {
    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
    if s.status != status {
        *s = empty(status);
    }
}

unsafe fn monitor_loop(id: &str, shared: &Mutex<Snapshot>, ctl: &Control) {
    let _com = match tronteq_shared::endpoints::ComGuard::new() {
        Ok(c) => c,
        Err(e) => return set_status(shared, Status::Error(format!("{e:#}"))),
    };
    while !ctl.stop.load(Ordering::Relaxed) {
        if !ctl.wanted() {
            set_status(shared, Status::Paused);
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        if let Err(e) = run_session(id, shared, ctl) {
            let message = format!("{e:#}");
            crate::log_line(&format!("mic monitor: {message}"));
            set_status(shared, Status::Error(message));
            for _ in 0..10 {
                if ctl.stop.load(Ordering::Relaxed) { break; }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

struct MixFormat(*mut WAVEFORMATEX);
impl Drop for MixFormat {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast())) }
    }
}
struct Running(IAudioClient);
impl Drop for Running {
    fn drop(&mut self) {
        unsafe { let _ = self.0.Stop(); }
    }
}

/// Default-output playback of the mic, always stereo float at the mic's rate;
/// Windows converts to the output's own format.
struct Listen {
    _running: Running,
    render: IAudioRenderClient,
    client: IAudioClient,
    capacity: u32,
    endpoint: String,
}

unsafe fn endpoint_id(device: &IMMDevice) -> Result<String> {
    let raw = device.GetId()?;
    let value = raw.to_string();
    CoTaskMemFree(Some(raw.0.cast()));
    Ok(value?)
}

const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

unsafe fn open_listen(enumerator: &IMMDeviceEnumerator, rate: u32) -> Result<Listen> {
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).context("no default output to listen on")?;
    let endpoint = endpoint_id(&device)?;
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    let fmt = WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: 0xFFFE, // WAVE_FORMAT_EXTENSIBLE
            nChannels: 2,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * 8,
            nBlockAlign: 8,
            wBitsPerSample: 32,
            cbSize: 22,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 { wValidBitsPerSample: 32 },
        dwChannelMask: 0x3, // front left + right
        SubFormat: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
    };
    client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        LISTEN_BUFFER_HNS,
        0,
        &fmt.Format,
        None,
    ).context("open the output for listening")?;
    let capacity = client.GetBufferSize()?;
    let render: IAudioRenderClient = client.GetService()?;
    client.Start()?;
    Ok(Listen { _running: Running(client.clone()), render, client, capacity, endpoint })
}

unsafe fn run_session(id: &str, shared: &Mutex<Snapshot>, ctl: &Control) -> Result<()> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let id_w: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
    let device = enumerator.GetDevice(PCWSTR(id_w.as_ptr())).context("microphone is unavailable")?;
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    let raw = MixFormat(client.GetMixFormat()?);
    anyhow::ensure!(!raw.0.is_null(), "Windows returned an empty microphone format");
    let bytes = std::slice::from_raw_parts(raw.0.cast::<u8>(), 18 + (*raw.0).cbSize as usize);
    let format = Format::parse(bytes).map_err(anyhow::Error::msg)?;
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, Default::default(), 1_000_000, 0, raw.0, None)
        .context("open the microphone")?;
    drop(raw);
    let capture: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    let _running = Running(client);

    let mut analysis = Analyzer::new(format.rate, format.channels);
    let mut listen: Option<Listen> = None;
    let mut listen_error: Option<String> = None;
    let mut next_listen_try = Instant::now();
    let mut last_default_poll = Instant::now();
    let mut decoded = [0.0f32; 64];
    let mut mono: Vec<f32> = Vec::with_capacity(4096);
    let mut published = u64::MAX;
    let mut published_error: Option<String> = None;

    while !ctl.stop.load(Ordering::Relaxed) && ctl.wanted() {
        // Open/close/retarget the listen path.
        let want_listen = ctl.listen.load(Ordering::Relaxed);
        if !want_listen {
            listen = None;
            listen_error = None;
        } else {
            if listen.is_some() && last_default_poll.elapsed() >= Duration::from_millis(500) {
                last_default_poll = Instant::now();
                let current = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()
                    .and_then(|d| endpoint_id(&d).ok());
                if current.as_deref() != listen.as_ref().map(|l| l.endpoint.as_str()) {
                    listen = None; // default output changed: follow it
                }
            }
            if listen.is_none() && Instant::now() >= next_listen_try {
                match open_listen(&enumerator, format.rate) {
                    Ok(l) => { listen = Some(l); listen_error = None; }
                    Err(e) => {
                        listen_error = Some(format!("{e:#}"));
                        next_listen_try = Instant::now() + Duration::from_secs(2);
                    }
                }
            }
        }

        mono.clear();
        while capture.GetNextPacketSize()? > 0 {
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
            capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            if frames == 0 {
                break;
            }
            if flags & 2 != 0 || data.is_null() {
                analysis.silence(frames as usize);
                mono.extend(std::iter::repeat(0.0).take(frames as usize));
            } else {
                let bytes = std::slice::from_raw_parts(data, frames as usize * format.stride);
                for frame in bytes.chunks_exact(format.stride) {
                    format.decode(frame, &mut decoded);
                    analysis.feed(&decoded);
                    let ch = &decoded[..format.channels];
                    mono.push(ch.iter().sum::<f32>() / ch.len() as f32);
                }
            }
            capture.ReleaseBuffer(frames)?;
        }

        if let Some(l) = &listen {
            if let Err(e) = write_listen(l, &mono, f32::from_bits(ctl.volume.load(Ordering::Relaxed))) {
                listen_error = Some(format!("{e:#}"));
                listen = None;
                next_listen_try = Instant::now() + Duration::from_secs(2);
            }
        }

        if analysis.ticks != published || listen_error != published_error {
            published = analysis.ticks;
            published_error = listen_error.clone();
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            *s = Snapshot {
                status: Status::Live,
                rate: format.rate,
                frame: Arc::new(analysis.snapshot()),
                listen_error: listen_error.clone(),
            };
        }
        std::thread::sleep(Duration::from_millis(3));
    }
    Ok(())
}

/// Write what fits and drop the rest: latency stays bounded by the buffer
/// instead of growing whenever the two clocks drift.
unsafe fn write_listen(l: &Listen, mono: &[f32], volume: f32) -> Result<()> {
    if mono.is_empty() {
        return Ok(());
    }
    let padding = l.client.GetCurrentPadding()?;
    let room = l.capacity.saturating_sub(padding) as usize;
    let n = room.min(mono.len());
    if n == 0 {
        return Ok(());
    }
    let skip = mono.len() - n; // keep the newest samples
    let buf = l.render.GetBuffer(n as u32)?.cast::<f32>();
    let out = std::slice::from_raw_parts_mut(buf, n * 2);
    for (i, &x) in mono[skip..].iter().enumerate() {
        let v = (x * volume).clamp(-1.0, 1.0);
        out[2 * i] = v;
        out[2 * i + 1] = v;
    }
    l.render.ReleaseBuffer(n as u32, 0)?;
    Ok(())
}
