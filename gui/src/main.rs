//! TrontEQ GUI — draggable parametric curve. Writes the shared state file
//! the C++ APO reads every buffer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod audio_status;
mod color;
mod curve;
mod devices;
mod dsp_preview;
mod glstage;
mod inspector;
mod knob;
mod mic_monitor;
mod mic_panel;
mod presets;
mod profiler;
mod profiles;
mod settings;
mod show;
mod spectrogram;
mod state_writer;
mod state_recovery;
mod theme;
mod theme_window;
mod visualizer;
mod audio_format;
mod audio_analysis;
mod vizbus;
mod viz_settings;
mod viz_edit;
mod viz_panel;
mod viz_looks;
mod presentation;
mod scene_gallery;
mod render_timer;
mod sound_edit;
mod eq_editor;
mod profile_picker;
use profile_picker::Action as ProfileAction;
mod win;
mod window_chrome;

use anyhow::Result;
use eframe::egui;
use raw_window_handle::HasWindowHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tronteq_shared::{Band, BandKind, Dynamics, DEFAULT_FREQS, NUM_BANDS};

/// Append a timestamped line to the diagnostic log. The GUI runs on the windows
/// subsystem (no console) with `panic = "abort"`, so any panic OR clean-but-
/// unexpected exit (e.g. eframe/wgpu returning Err on GPU device-loss) otherwise
/// vanishes silently: a panic shows only as a bare 0xc0000409 in the Event Log,
/// and an Err exit leaves no trace at all. Elevated, so it can append to ProgramData.
pub(crate) fn log_line(msg: &str) {
    use std::io::Write;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(r"C:\ProgramData\TrontEq\crash.log")
    {
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

/// Process start time, used by the panic hook to decide whether a crash is a
/// recoverable mid-session fault (relaunch) or a startup crash-loop (give up).
static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Self-heal: re-spawn ourselves so a transient render fault doesn't leave the
/// user with no control panel + no tray icon (which is exactly what happened on
/// 2026-06-06: an egui-wgpu staging-buffer allocation failed → panic=abort →
/// silent 0xc0000409, and nothing relaunched it). Render-side faults that abort
/// the process this way — GPU device-loss on monitor sleep, a driver TDR, a
/// momentary GPU/host OOM — are nearly always transient, so a fresh process
/// comes straight back up healthy.
///
/// Guarded by uptime so we DON'T spin: a crash within the first 20s (e.g. a
/// genuine startup bug, or a relaunch that immediately re-crashes because the
/// fault is persistent) does not relaunch. So we recover once from a transient
/// fault and stop if the relaunch can't survive — never a fork-bomb.
fn maybe_relaunch() {
    let up = START.get().map(|s| s.elapsed().as_secs()).unwrap_or(0);
    if up < 20 {
        log_line(&format!("self-heal: crash after {up}s uptime — NOT relaunching (looks like a startup loop)"));
        return;
    }
    match std::env::current_exe().and_then(|exe| std::process::Command::new(exe).spawn()) {
        // Medium-integrity parent spawns a Medium child — no UAC involved.
        Ok(child) => log_line(&format!("self-heal: relaunched pid={} after {up}s uptime", child.id())),
        Err(e) => log_line(&format!("self-heal: relaunch FAILED: {e}")),
    }
}

fn install_crash_logger() {
    START.get_or_init(std::time::Instant::now);
    std::panic::set_hook(Box::new(|info| {
        log_line(&format!("PANIC: {info}"));
        // Runs before the abort, so the replacement is spawned as we go down.
        maybe_relaunch();
    }));
}

/// Loopback port used to poke an already-running instance into showing itself.
/// Belt and suspenders for "TrontEQ is unlocatable": the process can be alive
/// and hidden to tray while its tray icon has gone stale (the shell drops it on
/// an Explorer restart), and `schtasks /run` REFUSES to help because the task is
/// already Running. Launching the exe again now always surfaces the window
/// instead of silently doing nothing.
const SHOW_PORT: u16 = 48762;

/// Set by the acceptor thread when a second launch asks us to surface.
static SHOW_REQUEST: AtomicBool = AtomicBool::new(false);
/// The main window handle, published so the acceptor thread can raise the window
/// itself without waiting on `update()` (which is skipped while tray'd).
static APP_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// OS thread id, for correlating a hot thread in Task Manager with the code that
/// spawned it.
fn tid() -> u32 {
    unsafe { windows::Win32::System::Threading::GetCurrentThreadId() }
}

/// Smallest window the OS will let you drag to, in LOGICAL pixels (what
/// `min_inner_size` speaks). Lowered from 800x320 on 2026-07-27: the EQ curve
/// and the inspector both survive a much smaller window than that, and the old
/// floor made TrontEQ awkward to park in a corner.
///
/// `heal_tiny_window` derives its own thresholds from these, so the failsafe and
/// the OS floor can never disagree again. See the note there.
const MIN_INNER_W: f32 = 640.0;
const MIN_INNER_H: f32 = 300.0;

fn main() -> Result<()> {
    install_crash_logger();
    log_line(&format!("start pid={} main tid={}", std::process::id(), tid()));
    let startup = std::env::args().any(|arg| arg == "--startup");

    // Single instance: own the port, or hand the request to whoever does.
    let listener = match std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, SHOW_PORT)) {
        Ok(l) => l,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            // A repeated logon/startup request must not open a window over the
            // user's desktop. An ordinary second launch still opens the editor.
            if startup {
                log_line("startup: instance already running; leaving it in place");
                return Ok(());
            }
            use std::io::Write as _;
            log_line("second instance: poking the running one to show, then exiting");
            if let Ok(mut s) =
                std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, SHOW_PORT))
            {
                let _ = s.write_all(b"show");
            }
            return Ok(());
        }
        Err(error) => {
            log_line(&format!("single instance: cannot bind show port: {error}"));
            return Err(error.into());
        }
    };
    std::thread::spawn(move || {
        log_line(&format!("thread: show-acceptor tid={}", tid()));
        for stream in listener.incoming() {
            if stream.is_ok() {
                // Raise it from HERE (Win32, no eframe involvement) so this works
                // even if the UI thread is wedged, then flag it so update() puts
                // the app's own `visible` state back in sync.
                let h = APP_HWND.load(Ordering::Relaxed);
                if h != 0 {
                    win::show_window(h);
                }
                SHOW_REQUEST.store(true, Ordering::Relaxed);
            }
        }
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("TrontEQ")
        .with_active(!startup)
        // Custom chrome: the toolbar is the title bar (window_chrome.rs owns the
        // caption buttons, dragging, and edge-resize the OS border used to give us).
        .with_decorations(false)
        .with_inner_size([1000.0, 460.0])
        .with_min_inner_size([MIN_INNER_W, MIN_INNER_H]);
    // Window + taskbar icon = Trent's face (the tront.xyz favicon).
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        // Force the OpenGL (glow) backend. Both of this app's lifetime crashes were
        // inside egui-wgpu on the DX12 path (write_buffer_with staging-buffer failure
        // + an invalid Texture::create_view), and the app uses only renderer-agnostic
        // egui APIs (painter + load_texture, zero wgpu callbacks/RenderState). glow
        // uploads via glBufferSubData/glTexSubImage with no per-frame wgpu staging
        // buffer, removing that entire failure class. Pinned explicitly so the choice
        // can't silently flip back to wgpu if the wgpu feature is ever re-added (with
        // both features on, Renderer::default() resolves to Wgpu).
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let run = eframe::run_native(
        "TrontEQ",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(App::new(cc, startup)?) as Box<dyn eframe::App>)
        }),
    );
    match &run {
        Ok(()) => log_line("exit: run_native -> Ok (window closed / normal return)"),
        Err(e) => log_line(&format!("exit: run_native -> Err: {e:?}")),
    }
    run.map_err(|e| anyhow::anyhow!("eframe: {e:?}"))?;
    Ok(())
}

/// How long a shuffled GL SHOW mode stays up before auto-cycling to another.
const SHUFFLE_PERIOD: std::time::Duration = std::time::Duration::from_secs(18);

struct App {
    state: state_writer::StateWriter,
    show_recovery: bool,
    bands: [Band; NUM_BANDS],
    bypass: bool,
    sample_rate: f32,
    last_error: Option<String>,

    // Signal chain (preamp + dynamics)
    preamp_db: f32,
    dynamics: Dynamics,
    delay_ms: f32,
    show_about: bool,
    rainbow: bool,
    viz: visualizer::Visualizer,
    layers: curve::Layers,
    capture: visualizer::Snapshot,
    spectro_revision: u64,
    spectro: spectrogram::Spectrogram,
    about_icon: Option<egui::TextureHandle>,

    // Tray (hide-to-tray on close so the control panel stays alive)
    _tray: Option<TrayIcon>,
    startup_pending: bool,
    app_hwnd: isize,
    /// LATCHED: the close button hid us to the tray. Only the tray Show /
    /// DoubleClick handlers and a second launch clear it. A tray'd window gets
    /// no OS events, so nothing may infer this one.
    tray_hidden: Arc<AtomicBool>,
    /// Mirrors `tray_hidden` for the visualizer's capture thread: while set, the
    /// WASAPI loopback session is released, so a tray'd TrontEQ captures no audio
    /// and runs no FFT. Set/cleared at the same sites as `tray_hidden`.
    viz_paused: Arc<AtomicBool>,
    /// DERIVED: false whenever nobody can see the window (tray'd, minimized,
    /// cloaked, collapsed) -> slow idle repaint. Recomputed every frame by
    /// `presentable`; read by the heartbeat thread.
    visible: Arc<AtomicBool>,
    /// Last inner size that was actually sane, in POINTS (what
    /// `ViewportCommand::InnerSize` takes), to restore to when the window
    /// desyncs. See `heal_tiny_window`.
    ///
    /// `None` until a sane frame has actually been observed. It used to be
    /// seeded with the literal `vec2(1000.0, 460.0)` "matching with_inner_size",
    /// but `with_inner_size` is LOGICAL pixels applied at window creation while
    /// this is POINTS, and those only agree at zoom 1.0. With a saved zoom of
    /// 1.25 the seed was 25% wrong, so a heal firing before the first sane frame
    /// would resize the window to a size the user never chose. Refusing to guess
    /// is simpler than converting.
    good_size: Option<egui::Vec2>,
    /// Consecutive frames observed below the minimum inner size.
    small_frames: u32,
    fps_frames: u32,
    fps_t0: std::time::Instant,
    fps: f32, // measured delivered framerate (DATA/SETUP readout)

    // Output picker
    devices: Vec<devices::Device>,
    selected_device: usize,
    device_error: Option<String>,
    installer_error: Option<String>,
    device_rx: Option<Receiver<std::result::Result<Vec<devices::Device>, String>>>,
    next_device_refresh: std::time::Instant,
    audio_status: audio_status::AudioStatus,
    audio_live: bool,
    /// Receives the result of an in-flight `apply` (runs off-thread so the UI
    /// doesn't freeze during the audio-service restart).
    apply_rx: Option<Receiver<std::result::Result<String, String>>>,
    device_status: Option<(bool, String)>, // (ok, message)

    // Profiles + persisted UI settings
    store: profiles::ProfileStore,
    active_profile: Option<String>,
    sound_editor: sound_edit::Editor,
    selected_band: usize,
    chain_page: inspector::ChainPage,
    profile_error: Option<String>,
    modal: Modal,
    name_buf: String,
    modal_focus: bool,
    settings_cache: settings::AppSettings,
    settings_error: Option<String>,
    settings_retry_at: std::time::Instant,
    presentation: presentation::Presentation,
    /// The Theme window (gradient v2 controls): opened from the toolbar's
    /// accent swatch. Not persisted - always starts closed, same as `show_about`.
    show_theme_window: bool,
    /// Picking a gradient preset also rethemes the app (accent = the
    /// preset's most-saturated stop). Persisted; the `GradientCfg` itself
    /// lives in `theme.rs`'s own static, not here.
    gradient_preset_sync: bool,
    tab: inspector::Tab,
    /// MIC tab: input-side mic stack, independent of the output sound.
    mic: mic_panel::MicPanel,
    show: show::ShowState,
    /// Auto-cycle through the GL SHOW modes while one is active.
    show_shuffle: bool,
    /// Stackable post-fx bitmask layered over any GL SHOW mode (see
    /// `glstage::FX_*`). 0 = off.
    viz_settings: viz_settings::VizSettings,
    viz_browser: viz_panel::Browser,
    /// Next scheduled shuffle switch (checked every frame, only acted on
    /// while `show_shuffle` is on and a GL mode is active).
    shuffle_next: std::time::Instant,
    /// Deterministic tiny LCG state (no rand dep) for picking the next mode.
    shuffle_rng: u32,
    frame_ms: f32, // EMA of eframe's reported per-frame CPU cost
    profiler: profiler::Profiler,
    /// The shader stage (None if GL init failed; painter modes still work).
    gl_stage: Option<std::sync::Arc<std::sync::Mutex<glstage::GlStage>>>,
}

/// Which name-entry modal is open.
enum Modal {
    None,
    SaveAs,
    Rename { old: String },
}

impl App {
    fn new(cc: &eframe::CreationContext, startup: bool) -> Result<Self> {
        let ctx = &cc.egui_ctx;
        // Native window handle for raw-Win32 hide/show (eframe's Visible command
        // can't restore a hidden window — proven in trontclicker/PSM).
        let app_hwnd: isize = cc
            .window_handle()
            .ok()
            .and_then(|wh| match wh.as_raw() {
                raw_window_handle::RawWindowHandle::Win32(h) => Some(h.hwnd.get() as isize),
                _ => None,
            })
            .unwrap_or(0);
        // Publish it so the single-instance acceptor thread can raise the window
        // without going through eframe (see SHOW_PORT).
        APP_HWND.store(app_hwnd, Ordering::Relaxed);

        let state = state_writer::StateWriter::open()?;
        // If the file already had a committed version, prefer that state.
        let snap = state.snapshot();
        let bands = if snap.version > 0 {
            snap.bands
        } else {
            let mut b = [Band::flat(0.0); NUM_BANDS];
            for (i, f) in DEFAULT_FREQS.iter().enumerate() {
                b[i] = Band {
                    freq: *f,
                    gain: 0.0,
                    q: 1.0,
                    kind: BandKind::Peak as u32,
                };
            }
            b
        };
        let bypass = if snap.version > 0 { snap.bypass != 0 } else { false };
        let preamp_db = if snap.version > 0 { snap.preamp_db } else { 0.0 };
        // ratio < 1.0 means the dynamics block is uninitialized (e.g. an old
        // 144-byte state file that was zero-extended) — fall back to defaults.
        let dynamics = if snap.version > 0 && snap.dynamics.comp_ratio >= 1.0 {
            snap.dynamics
        } else {
            Dynamics::default_passive()
        };
        let delay_ms = if snap.version > 0 { snap.delay_ms } else { 0.0 };

        let (devices, device_error) = match devices::list() {
            Ok(devices) => (devices, None),
            Err(e) => {
                log_line(&format!("outputs: {e:#}"));
                (Vec::new(), Some(format!("Could not list outputs: {e:#}")))
            }
        };
        let selected_device = devices::selected_index(&devices, None);

        // Compile the GL viz stage once (glow backend exposes the context here).
        let gl_stage = cc.gl.as_ref().and_then(|gl| match glstage::GlStage::new(gl) {
            Ok(s) => {
                log_line("glstage: shaders compiled");
                Some(std::sync::Arc::new(std::sync::Mutex::new(s)))
            }
            Err(e) => {
                log_line(&format!("glstage: init FAILED ({e}) - GL modes disabled"));
                None
            }
        });

        // Persisted UI settings + saved sound profiles. Theme/zoom apply before
        // the first frame; the live chain still comes from state.bin (the APO is
        // already running with it) - the active profile is just the UI marker.
        let ui_settings = settings::AppSettings::load();
        if ui_settings.theme_name.is_empty() {
            // Pre-theme settings file: honor the old dark/light flag.
            theme::set_mode(ctx, ui_settings.dark_mode);
        } else {
            theme::set_palette(
                ctx,
                theme::Palette::resolve(
                    &ui_settings.theme_name,
                    &ui_settings.theme_colors,
                    ui_settings.dark_mode,
                ),
            );
        }
        theme::set_gradient(ctx, ui_settings.gradient);
        // Gradient v2 knobs + frost, loaded before the first frame same as
        // the palette above (no default-look flash on startup).
        let mut grad_cfg = theme::GradientCfg {
            angle_deg: ui_settings.gradient_angle,
            intensity: ui_settings.gradient_intensity.clamp(0.0, 1.0),
            pegs: ui_settings.gradient_pegs.clamp(1, 4),
            harmony: ui_settings.gradient_harmony,
            preset: ui_settings.gradient_preset,
            ..Default::default()
        };
        for (i, hex) in ui_settings.gradient_custom.split(',').take(4).enumerate() {
            if let Some(rgb) = color::hex_to_rgb(hex) {
                grad_cfg.custom[i] = rgb;
            }
        }
        theme::set_gradient_cfg(grad_cfg);
        theme::set_frost(true, ui_settings.frost_dark);
        theme::set_frost(false, ui_settings.frost_light);
        ctx.set_zoom_factor(ui_settings.zoom);
        let store = profiles::ProfileStore::load();
        let active_profile = ui_settings
            .active_profile
            .as_ref()
            .and_then(|n| store.get(n).map(|p| p.name.clone()));

        // Stable shell identity preserves the user's tray pin across launches.
        // tray-icon >= 0.25 keeps its message window alive if Explorer is not
        // ready yet, then registers the icon when TaskbarCreated arrives.
        let tray_menu = Menu::new();
        let show_item = MenuItem::new("Show TrontEQ", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        let _ = tray_menu.append(&show_item);
        let _ = tray_menu.append(&quit_item);
        let tray_show_id = show_item.id().clone();
        let tray_quit_id = quit_item.id().clone();
        let tray_icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
            .ok()
            .and_then(|d| Icon::from_rgba(d.rgba, d.width, d.height).ok());
        let mut tray_builder = TrayIconBuilder::new()
            .with_guid(0x63b19802_492e_4f5f_9cc1_6ba48e46271d)
            .with_tooltip("TrontEQ")
            .with_menu(Box::new(tray_menu));
        if let Some(icon) = tray_icon {
            tray_builder = tray_builder.with_icon(icon);
        }
        let tray = match tray_builder.build() {
            Ok(tray) => {
                log_line("tray: built ok (Explorer registration recovers automatically)");
                Some(tray)
            }
            Err(error) => {
                log_line(&format!("tray: FAILED to build: {error}; keeping window available"));
                None
            }
        };

        // TWO flags, because conflating them is a livelock:
        //
        // `tray_hidden` is LATCHED state. Only the close-button intercept sets
        // it; only the tray Show/DoubleClick handlers (and a second launch)
        // clear it. Nothing else may touch it, because a hidden window gets no
        // OS events to recover from on its own.
        //
        // `visible` is DERIVED state, recomputed every frame by `presentable`
        // and read by the heartbeat thread to pick 60fps vs a 2fps idle pulse.
        // Minimize/cloak/collapse feed only this one, so they self-recover the
        // moment the condition clears.
        let tray_hidden = Arc::new(AtomicBool::new(false));
        let visible = Arc::new(AtomicBool::new(true));

        // Tracks `tray_hidden`, but owned separately because the visualizer thread
        // must read it from outside egui: while tray'd the loopback capture session
        // is released entirely, so the process holds no WASAPI client and does no
        // FFT for a window nobody can see. Set at the same four sites.
        let viz_paused = Arc::new(AtomicBool::new(false));

        // Repaint heartbeat thread (Boxel pattern): winit DEFERS
        // request_repaint_after deadlines so pacing from update() drifts; a
        // repaint request from another thread is delivered immediately.
        //
        // VISIBLE = LOCKED 60 regardless of focus. This is a visualizer — the
        // whole point is watching it while another window has focus; the old
        // focus throttle made fps depend on mouse motion (egui's reactive
        // repaints filled the gaps only while the pointer moved). Two more
        // pieces of the jank: Sleep(16) without raised timer resolution
        // rounds to ~31ms on Windows (timeBeginPeriod(1) fixes it), and
        // relative sleeps drift (absolute schedule fixes that).
        {
            let c = ctx.clone();
            let v = visible.clone();
            std::thread::spawn(move || {
                log_line(&format!("thread: heartbeat tid={}", tid()));
                unsafe {
                    let _ = windows::Win32::Media::timeBeginPeriod(1);
                }
                let tick = std::time::Duration::from_micros(16_666);
                let mut next = std::time::Instant::now() + tick;
                loop {
                    if !v.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        next = std::time::Instant::now() + tick;
                        // Pulse ONLY at a window the OS says is on screen.
                        //
                        // winit never services a repaint request for a window that
                        // is hidden or minimized, and an outstanding request keeps
                        // its event loop in Poll instead of Wait. So an idle pulse
                        // aimed at a tray'd window does not merely go to waste, it
                        // pins the main thread at 100% of a core. That is what
                        // TrontEQ did for the entire time it sat in the tray
                        // (measured 2026-07-26: 643 minutes of CPU over 14 hours).
                        //
                        // Reading Win32 instead of our own flags is what makes
                        // waking up reliable: the second-launch path restores the
                        // window from a thread with no egui context, so there is
                        // nobody to request the repaint that would let update()
                        // notice. Polling the real state covers every route back.
                        if win::is_on_screen(app_hwnd) {
                            c.request_repaint();
                        }
                        continue;
                    }
                    let now = std::time::Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    }
                    next += tick;
                    // Fell behind (system stall): resync, don't burst-catch-up.
                    if next < std::time::Instant::now() {
                        next = std::time::Instant::now() + tick;
                    }
                    c.request_repaint();
                }
            });
        }

        // Tray events fire on the message thread even while the window is hidden;
        // act via raw Win32 (works without the eframe update loop running).
        {
            let c = ctx.clone();
            let show = tray_show_id.clone();
            let quit = tray_quit_id.clone();
            let v = visible.clone();
            let th = tray_hidden.clone();
            let vp = viz_paused.clone();
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if e.id == show {
                    win::show_window(app_hwnd);
                    th.store(false, Ordering::Relaxed);
                    v.store(true, Ordering::Relaxed);
                    vp.store(false, Ordering::Relaxed);
                    c.request_repaint();
                } else if e.id == quit {
                    log_line("exit: tray Quit -> process::exit(0)");
                    std::process::exit(0);
                }
            }));
        }
        {
            let c = ctx.clone();
            let v = visible.clone();
            let th = tray_hidden.clone();
            let vp = viz_paused.clone();
            TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                // Double-click shows the window. Single/right clicks fall through
                // so tray-icon can open its context menu (stealing focus here
                // would make the menu flicker/close).
                if let TrayIconEvent::DoubleClick {
                    button: tray_icon::MouseButton::Left,
                    ..
                } = e
                {
                    win::show_window(app_hwnd);
                    th.store(false, Ordering::Relaxed);
                    v.store(true, Ordering::Relaxed);
                    vp.store(false, Ordering::Relaxed);
                    c.request_repaint();
                }
            }));
        }

        // Push an initial committed state so APO stops bypassing.
        let mut me = App {
            show_recovery: state.problem.is_some(),
            state,
            bands,
            bypass,
            sample_rate: 48_000.0,
            last_error: None,
            preamp_db,
            dynamics,
            delay_ms,
            show_about: false,
            rainbow: ui_settings.rainbow,
            viz: visualizer::Visualizer::start(viz_paused.clone()),
            layers: ui_settings.layers,
            capture: visualizer::Snapshot::empty(),
            spectro_revision: u64::MAX,
            spectro: spectrogram::Spectrogram::new(),
            about_icon: None,
            _tray: tray,
            startup_pending: startup,
            app_hwnd,
            tray_hidden,
            viz_paused,
            visible,
            good_size: None, // learned from the first sane frame; never guessed
            small_frames: 0,
            fps_frames: 0,
            fps_t0: std::time::Instant::now(),
            fps: 0.0,
            devices,
            selected_device,
            device_error,
            installer_error: devices::installer_error(),
            device_rx: None,
            next_device_refresh: std::time::Instant::now() + std::time::Duration::from_secs(5),
            audio_status: audio_status::AudioStatus::default(),
            audio_live: false,
            apply_rx: None,
            device_status: None,
            store,
            sound_editor: sound_edit::Editor::new(sound_edit::Sound { bands, preamp: preamp_db, dynamics, profile: active_profile.clone() }),
            active_profile,
            selected_band: 0,
            chain_page: inspector::ChainPage::Eq,
            profile_error: None,
            modal: Modal::None,
            name_buf: String::new(),
            modal_focus: false,
            tab: if std::env::args().any(|a| a == "--visualizer") { inspector::Tab::Viz }
                else { inspector::Tab::from_str(&ui_settings.inspector_tab) },
            mic: mic_panel::MicPanel::default(),
            show: show::ShowState::new(show::ShowMode::from_str(&ui_settings.show_mode)),
            show_shuffle: ui_settings.show_shuffle,
            viz_settings: {
                let mut v = ui_settings.visualizer.clone().unwrap_or_else(|| viz_settings::VizSettings::from_legacy(ui_settings.show_fx, ui_settings.rainbow));
                v.sanitize(); v
            },
            viz_browser: viz_panel::Browser::default(),
            shuffle_next: std::time::Instant::now() + SHUFFLE_PERIOD,
            shuffle_rng: 0x9E37_79B9,
            frame_ms: 0.0,
            profiler: profiler::Profiler::default(),
            gl_stage,
            show_theme_window: std::env::var_os("TRONTEQ_THEME_WINDOW").is_some(),
            gradient_preset_sync: ui_settings.gradient_preset_sync,
            settings_cache: ui_settings,
            settings_error: None,
            settings_retry_at: std::time::Instant::now(),
            presentation: presentation::Presentation::default(),
        };
        if me.state.problem.is_none() { me.commit(); }
        if std::env::args().any(|a| a == "--gallery") {
            me.tab = inspector::Tab::Viz;
            me.viz_browser.gallery.open = true;
        }
        if std::env::args().any(|a| a == "--present") && me.show.mode != show::ShowMode::Off && me.gl_stage.is_some() {
            let (fullscreen, time) = ctx.input(|i| (i.viewport().fullscreen.unwrap_or(false), i.time));
            me.presentation.enter(fullscreen, time);
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        Ok(me)
    }

    /// Detect and repair the window collapsing below its minimum inner size.
    ///
    /// Observed 2026-07-26: after a hide-to-tray round trip, `show_window`'s
    /// SW_RESTORE brought the window back at 15x15 in the top-left corner. Win32
    /// reported it visible, unminimized and uncloaked, so every "is anyone
    /// looking at this" check said yes and the heartbeat held a locked 60fps
    /// rebuilding the whole UI for a 15-pixel speck. 643 minutes of CPU over 14
    /// hours of uptime. Ported from trontsnap, which hit the same desync first.
    ///
    /// Runs BEFORE the presentability gate on purpose: a window that is idling
    /// because it is too small still has to get frames, or it could never resize
    /// itself back and would stay broken until restarted.
    fn heal_tiny_window(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().minimized).unwrap_or(false) {
            return; // reported size is meaningless while minimized
        }
        // UNITS MATTER HERE, and getting them wrong is what made the window
        // refuse to be dragged small (reported 2026-07-27).
        //
        // `content_rect()` is in egui POINTS: physical pixels divided by
        // (native DPI scale x zoom_factor). `min_inner_size` is applied once at
        // window creation in LOGICAL pixels, i.e. physical divided by the DPI
        // scale alone, and is never re-applied when zoom changes. The two only
        // agree at zoom 1.0.
        //
        // At 110% DPI with a saved zoom of 1.1, the OS happily allows an 880px
        // window, but that reads as 727 points, which the old hardcoded 790
        // threshold called "desynced" and snapped back. The failsafe was fighting
        // the user for a ~76px band the OS had already allowed, and because
        // `good_size` is in points it snapped BIGGER the further you zoomed in.
        //
        // Converting points -> logical with zoom_factor puts the comparison in
        // the same space `min_inner_size` constrains, so the failsafe now trips
        // only below what the OS itself permits, at any zoom.
        let sz = ctx.input(|i| i.content_rect()).size();
        let logical = sz * ctx.zoom_factor();
        // A hair under the real floor, so only genuine breakage trips it.
        let min_w = MIN_INNER_W - 10.0;
        let min_h = MIN_INNER_H - 10.0;
        if logical.x >= min_w && logical.y >= min_h {
            // Stored in POINTS, because ViewportCommand::InnerSize takes points.
            self.good_size = Some(sz);
            self.small_frames = 0;
        } else if sz.x > 1.0 && sz.y > 1.0 && self.good_size.is_some() {
            // Non-degenerate but sub-minimum: confirm it persists, then fix.
            self.small_frames += 1;
            if self.small_frames % 4 == 0 {
                if let Some(good) = self.good_size {
                    log_line(&format!(
                        "window desynced to {:.0}x{:.0}, restoring to {:.0}x{:.0}",
                        sz.x, sz.y, good.x, good.y
                    ));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(good));
                }
            }
        }
    }

    /// Whether a human could actually see this window right now.
    ///
    /// `visible` alone used to mean only "the close button has not hidden us",
    /// which is why a minimized or cloaked or collapsed window still burned a
    /// full core. Every state that means "not on screen" now lands here, and the
    /// heartbeat thread reads the result.
    /// Reads `tray_hidden`, never `visible`. Reading its own output was the
    /// livelock: minimize cleared `visible`, then restore had nothing to set it
    /// back, so the window came back on screen frozen at 2fps.
    fn presentable(&self, ctx: &egui::Context) -> bool {
        // RECONCILE THE LATCH WITH REALITY FIRST.
        //
        // `tray_hidden` was documented as "only the close button sets it, only
        // the tray Show/DoubleClick handlers and a second launch clear it,
        // nothing else may touch it". That premise is false on Win32.
        // `hide_window` only sets WS_EX_TOOLWINDOW and minimizes; it never
        // clears WS_VISIBLE, and WS_EX_TOOLWINDOW governs shell presentation,
        // not the window APIs. Any same-integrity process that finds the HWND
        // (EnumWindows, FindWindow) can call ShowWindow(SW_RESTORE) on it, and
        // Trent's own window tilers are exactly that class of tool. A lingering
        // taskbar button or an Alt-Tab entry does it too.
        //
        // When that happened, none of the three clearing sites ran, so the
        // window sat genuinely on screen and focused while `presentable` still
        // returned false, and `update` returned before building a single
        // widget. With decorations(false) there is no native caption either, so
        // the result was a fully visible, permanently blank window with nothing
        // in it to click. The heartbeat kept pulsing forever because
        // `is_on_screen` correctly said true. Only the tray icon could recover
        // it, which is easy to miss when the window looks open.
        //
        // So: if the OS says we are on screen, we are not tray'd, whatever the
        // latch believes. This covers every restore route rather than the three
        // the app knows how to perform itself.
        if self.tray_hidden.load(Ordering::Relaxed) {
            if win::is_on_screen(self.app_hwnd) {
                self.tray_hidden.store(false, Ordering::Relaxed);
                // Whatever route brought us back, the capture has to come back
                // with us, or the window returns with a permanently dead
                // spectrum, waveform and meters.
                self.viz_paused.store(false, Ordering::Relaxed);
            } else {
                return false;
            }
        }
        if ctx.input(|i| i.viewport().minimized).unwrap_or(false) {
            return false;
        }
        if win::is_cloaked(self.app_hwnd) {
            return false; // another virtual desktop
        }
        // Failsafe: a window stuck below the minimum despite repeated heal
        // attempts is not worth 60fps. heal_tiny_window still runs on the idle
        // pulse, so it can recover on its own, just at 2fps instead of 60.
        self.small_frames < 30
    }

    fn refresh_devices(&mut self) {
        if self.device_rx.is_some() {
            return;
        }
        self.installer_error = devices::installer_error();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(devices::list().map_err(|e| format!("{e:#}")));
        });
        self.device_rx = Some(rx);
        self.next_device_refresh = std::time::Instant::now() + std::time::Duration::from_secs(5);
    }

    fn poll_devices(&mut self) {
        if let Some(rx) = &self.device_rx {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("Device scan stopped unexpectedly".into())),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                match result {
                    Ok(devices) => {
                        let previous = self.devices.get(self.selected_device).map(|d| d.id.as_str());
                        self.selected_device = devices::selected_index(&devices, previous);
                        self.devices = devices;
                        self.device_error = None;
                    }
                    Err(error) => {
                        let message = format!("Could not list outputs: {error}");
                        if self.device_error.as_ref() != Some(&message) {
                            log_line(&message);
                        }
                        // Retain the last good list, but block Apply until a scan succeeds.
                        self.device_error = Some(message);
                    }
                }
                self.device_rx = None;
            }
        }
        if self.apply_rx.is_none() && std::time::Instant::now() >= self.next_device_refresh {
            self.refresh_devices();
        }
    }

    /// Apply the APO to the selected endpoint on a background thread.
    fn start_apply(&mut self) {
        if self.apply_rx.is_some() || self.device_error.is_some() {
            return;
        }
        let Some(dev) = self.devices.get(self.selected_device) else { return };
        let id = dev.id.clone();
        let name = dev.friendly_name.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = devices::apply(&id)
                .map(|_| format!("Installed on {name}. Play audio to verify processing."))
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
        });
        self.apply_rx = Some(rx);
        self.device_status = Some((true, "Applying EQ...".to_string()));
    }

    fn poll_apply(&mut self) {
        if let Some(rx) = &self.apply_rx {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("Installer stopped unexpectedly".into())),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.device_status = Some(match result {
                    Ok(message) => (true, message),
                    Err(error) => {
                        log_line(&format!("apply: {error}"));
                        (false, error)
                    }
                });
                self.apply_rx = None;
                self.refresh_devices();
            }
        }
    }

    fn commit(&mut self) {
        self.sound_editor.observe(self.current_sound());
        if let Err(error) = self.state.write_state(
            &self.bands,
            self.bypass,
            self.preamp_db,
            &self.dynamics,
            self.delay_ms,
        ) {
            log_line(&format!("sound state: {error}"));
            self.show_recovery = true;
        }
    }

    fn current_sound(&self) -> sound_edit::Sound {
        sound_edit::Sound { bands: self.bands, preamp: self.preamp_db, dynamics: self.dynamics, profile: self.active_profile.clone() }
    }
    fn restore_sound(&mut self, sound: sound_edit::Sound) {
        self.bands = sound.bands;
        self.preamp_db = sound.preamp;
        self.dynamics = sound.dynamics;
        self.active_profile = sound.profile.and_then(|name| self.store.get(&name).map(|p| p.name.clone()));
        self.commit();
    }

    /// Reset the whole chain to defaults: flat EQ, 0 preamp, passive dynamics.
    fn reset_all(&mut self) {
        for (i, f) in DEFAULT_FREQS.iter().enumerate() {
            self.bands[i] = Band {
                freq: *f,
                gain: 0.0,
                q: 1.0,
                kind: BandKind::Peak as u32,
            };
        }
        self.preamp_db = 0.0;
        self.dynamics = Dynamics::default_passive();
        self.active_profile = None;
        self.commit();
    }

    /// Load a saved profile into the live chain (bands + preamp + dynamics) and
    /// push it to the APO.
    fn apply_profile(&mut self, name: &str) {
        let Some(p) = self.store.get(name).cloned() else { return };
        self.bands = p.bands;
        self.preamp_db = p.preamp_db;
        self.dynamics = p.dynamics;
        self.active_profile = Some(p.name);
        self.commit();
    }

    /// Save the current sound as `name` (overwrite if it exists) and make it active.
    fn save_current_as(&mut self, name: &str) -> std::result::Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a profile name.".into());
        }
        let order = self
            .store
            .get(name)
            .map(|p| p.order)
            .unwrap_or_else(|| self.store.next_order());
        self.store.save(profiles::Profile {
            schema: 1,
            name: name.to_string(),
            order,
            bands: self.bands,
            preamp_db: self.preamp_db,
            dynamics: self.dynamics,
        })?;
        self.active_profile = Some(name.to_string());
        Ok(())
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll_apply();

        // Live frame-cost EMA (CPU side) for the SETUP readout.
        if let Some(cpu) = frame.info().cpu_usage {
            let ms = cpu * 1000.0;
            self.frame_ms =
                if self.frame_ms <= 0.0 { ms } else { self.frame_ms * 0.95 + ms * 0.05 };
        }
        let t_update = std::time::Instant::now();
        if ctx.input(|i| i.key_pressed(egui::Key::F10)) {
            self.profiler.overlay = !self.profiler.overlay;
        }
        if !self.viz_browser.gallery.open { self.presentation.input(ctx); }

        let visual_before_input = self.show.mode;
        let shortcuts_blocked = self.presentation.active || self.viz_browser.gallery.open
            || self.show_recovery || !matches!(self.modal, Modal::None);
        if let Some((target, redo)) = viz_edit::shortcut(ctx, self.tab == inspector::Tab::Viz, shortcuts_blocked) {
            match target {
                viz_edit::Target::Visual => {
                    let edit = if redo { self.viz_browser.editor.redo() } else { self.viz_browser.editor.undo() };
                    if let Some(edit) = edit { edit.apply(&mut self.show.mode, &mut self.viz_settings, &mut self.viz_browser.selected_look); }
                }
                viz_edit::Target::Sound if self.state.problem.is_none() => {
                    let sound = if redo { self.sound_editor.redo() } else { self.sound_editor.undo() };
                    if let Some(sound) = sound { self.restore_sound(sound); }
                }
                _ => {}
            }
        }
        // Number keys 1-9 quickswap profiles (skipped while a text field has focus).
        if self.state.problem.is_none() && !ctx.wants_keyboard_input() && !self.presentation.active && !self.viz_browser.gallery.open {
            const NUM_KEYS: [egui::Key; 9] = [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
                egui::Key::Num6,
                egui::Key::Num7,
                egui::Key::Num8,
                egui::Key::Num9,
            ];
            let mut hit: Option<String> = None;
            for (i, k) in NUM_KEYS.iter().enumerate() {
                if ctx.input(|inp| inp.key_pressed(*k)) {
                    if let Some(e) = self.store.entries.get(i) {
                        hit = Some(e.profile.name.clone());
                    }
                }
            }
            if let Some(n) = hit {
                self.apply_profile(&n);
            }
        }

        // Hide-to-tray on close (tray Show/Quit handled in the tray handlers).
        // Only if a tray exists, else we'd trap the window with no way back.
        // eframe shows the native window after its first paint. Parking during
        // that first update is undone by the runner, so wait for the next frame.
        let startup_hide = self.startup_pending && ctx.cumulative_frame_nr() > 0;
        if startup_hide {
            self.startup_pending = false;
        }
        if self._tray.is_some() && (startup_hide || ctx.input(|i| i.viewport().close_requested())) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.tray_hidden.store(true, Ordering::Relaxed);
            self.visible.store(false, Ordering::Relaxed);
            self.viz_paused.store(true, Ordering::Relaxed);
            win::hide_window(self.app_hwnd);
            log_line(if startup_hide {
                "startup: parked in system tray"
            } else {
                "hide to tray (close intercepted; process stays alive)"
            });
        }

        // While hidden to tray, do NOT build any UI or upload textures. Rendering
        // and re-uploading egui-managed textures (the spectrogram waterfall, the
        // About icon) to the hidden window's wgpu surface is what killed the GUI on
        // 2026-06-04: a managed texture went invalid and `Texture::create_view`
        // panicked (wgpu validation error → 0xc0000409 via panic=abort, ~50min into
        // being tray'd). Skipping the frame entirely removes the trigger and saves a
        // core while tray'd. The tray Show/DoubleClick handlers flip `visible` back
        // on and call request_repaint(), so showing is still instant.
        // A second launch (shortcut, Start menu, `tronteq.exe`) asked us to
        // surface. The acceptor thread already raised the OS window; this puts
        // our own hidden/visible bookkeeping back in sync so the UI resumes.
        if SHOW_REQUEST.swap(false, Ordering::Relaxed) {
            win::show_window(self.app_hwnd);
            self.tray_hidden.store(false, Ordering::Relaxed);
            self.visible.store(true, Ordering::Relaxed);
            self.viz_paused.store(false, Ordering::Relaxed);
            log_line("show request from a second launch");
        }

        // Repair a collapsed window before deciding whether to render it.
        self.heal_tiny_window(ctx);

        // Fold every "nobody can see this" state into the one flag the heartbeat
        // thread reads. This is a pure recompute from `tray_hidden` + live window
        // state, so un-minimizing or un-cloaking restores 60fps by itself on the
        // next idle pulse (worst case 500ms).
        let presentable = self.presentable(ctx);
        self.visible.store(presentable, Ordering::Relaxed);
        if !presentable {
            self.sound_editor.finish_gesture();
            self.state.checkpoint();
            // Build no UI and upload no textures. Beyond the CPU saving this is
            // also what stopped the 2026-06-04 crash: re-uploading egui-managed
            // textures (spectrogram waterfall, About icon) to an off-screen
            // surface eventually invalidated one and panicked in create_view.
            //
            // Pacing is NOT this function's job. Whether the loop actually idles
            // is decided by the window state we park in (see win::hide_window)
            // and by the heartbeat declining to pulse at a window that cannot
            // service a repaint. Sleeping here was tried and changed nothing,
            // because while tray'd this function is not called at all.
            return;
        }

        // Read APO telemetry up front: the toolbar warning chip and the bottom
        // meter both use it.
        self.poll_devices();
        let tel = self.state.telemetry();
        self.audio_live = self.audio_status.observe(tel.seq, std::time::Instant::now());
        self.capture = self.viz.read();
        if let Some(source) = &self.capture.source { self.sample_rate = source.format.rate as f32; }
        let t_panels = std::time::Instant::now();

        // Discord-style background wash, painted into the background layer
        // before any panel draws (no-op when the toggle is off). Panels
        // above it go slightly translucent (`theme::build_visuals`) so it
        // reads through the chrome; the EQ canvas + OUT meter paint their
        // own opaque fills and stay solid dark regardless (house rule).
        theme::paint_gradient(ctx);

        if !self.presentation.active {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(2.0);
            let toolbar_width = (ui.available_width() - window_chrome::CAPTION_W).max(1.0);
            ui.allocate_ui_with_layout(
                egui::vec2(toolbar_width, 0.0),
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                |ui| {
                let title_color = if self.rainbow {
                    theme::viz_hsv((ui.input(|i| i.time) as f32 * 0.2).fract())
                } else {
                    theme::cyan()
                };
                // App icon + wordmark double as a window drag handle (custom
                // chrome: no OS title bar), so a packed toolbar always has a
                // grab point.
                if let Some(tex) = about::icon_texture(ctx, &mut self.about_icon) {
                    let ic = ui.add(
                        egui::Image::new(egui::load::SizedTexture::new(
                            tex.id(),
                            egui::vec2(22.0, 22.0),
                        ))
                        .corner_radius(5.0)
                        .sense(egui::Sense::click_and_drag()),
                    );
                    window_chrome::drag_window(ctx, &ic);
                }
                if toolbar_width >= 600.0 {
                let wm = ui.add(
                    egui::Label::new(
                        egui::RichText::new("TrontEQ")
                            .family(egui::FontFamily::Name("display".into()))
                            .color(title_color)
                            .strong()
                            .size(23.0),
                    )
                    .sense(egui::Sense::click_and_drag()),
                );
                window_chrome::drag_window(ctx, &wm);
                }
                ui.separator();
                let choices: Vec<_> = self.store.entries.iter().map(|e| {
                    let p = &e.profile;
                    let active = self.active_profile.as_deref() == Some(p.name.as_str());
                    profile_picker::Choice {
                        name: &p.name, active,
                        modified: active && !p.matches(&self.bands, self.preamp_db, &self.dynamics),
                    }
                }).collect();
                let act = ui.add_enabled_ui(self.state.problem.is_none(), |ui| profile_picker::show(ui, &choices, (toolbar_width * 0.3).clamp(110.0, 180.0))).inner;
                if let Some(a) = act {
                    match a {
                        ProfileAction::SaveAs => {
                            self.profile_error = None;
                            self.modal = Modal::SaveAs;
                            self.name_buf.clear();
                            self.modal_focus = true;
                        }
                        ProfileAction::Load(n) => self.apply_profile(&n),
                        ProfileAction::Overwrite(n) => self.profile_error = self.save_current_as(&n).err(),
                        ProfileAction::StartRename(n) => {
                            self.profile_error = None;
                            self.name_buf = n.clone();
                            self.modal = Modal::Rename { old: n };
                            self.modal_focus = true;
                        }
                        ProfileAction::Delete(n) => {
                            match self.store.delete(&n) {
                                Ok(()) => {
                                    self.profile_error = None;
                                    if self.active_profile.as_deref() == Some(n.as_str()) { self.active_profile = None; }
                                }
                                Err(error) => self.profile_error = Some(error),
                            }
                        }
                    }
                }
                let mut bypass = self.bypass;
                if ui.add_enabled(self.state.problem.is_none(), egui::Checkbox::new(&mut bypass, "Bypass")).changed() {
                    self.bypass = bypass;
                    self.commit();
                }
                if ui.selectable_label(self.viz_settings.stage_view, "Stage").on_hover_text("Toggle a clear visualizer canvas").clicked() {
                    self.viz_settings.stage_view = !self.viz_settings.stage_view;
                    self.tab = inspector::Tab::Viz;
                }
                if toolbar_width < 700.0 {
                    ui.menu_button("App", |ui| {
                        if ui.button("Theme...").clicked() { self.show_theme_window = true; ui.close(); }
                        if ui.button(if theme::dark_mode() { "Light mode" } else { "Dark mode" }).clicked() {
                            theme::set_mode(ctx, !theme::dark_mode()); ui.close();
                        }
                        if ui.button("About").clicked() { self.show_about = true; ui.close(); }
                    });
                } else {
                // Theme window trigger: a small accent-colored swatch (click
                // to open the full gradient v2 control panel — colors,
                // dark/light, wash). Sits right before the quick Light/Dark
                // flip, which stays as its own one-click shortcut.
                let (srect, sresp) =
                    ui.allocate_exact_size(egui::vec2(26.0, 18.0), egui::Sense::click());
                // Show the colour the USER picked, not `theme::cyan()` (the
                // enforced ink accessor). This swatch answers "what is my theme
                // colour", so painting the readability-corrected value made a
                // yellow pick read as brown right next to the picker holding
                // 255,246,0.
                let seed = theme::accent_seed();
                ui.painter()
                    .rect_filled(srect, 4.0, egui::Color32::from_rgb(seed[0], seed[1], seed[2]));
                let ring = if sresp.hovered() || self.show_theme_window {
                    egui::Stroke::new(1.5, theme::text())
                } else {
                    egui::Stroke::new(1.0, theme::muted())
                };
                ui.painter().rect_stroke(srect, 4.0, ring, egui::StrokeKind::Middle);
                if sresp.on_hover_text("Theme: colors, dark/light, gradient").clicked() {
                    self.show_theme_window = !self.show_theme_window;
                }
                if ui
                    .button(if theme::dark_mode() { "Light" } else { "Dark" })
                    .on_hover_text("Toggle light / dark theme")
                    .clicked()
                {
                    theme::set_mode(ctx, !theme::dark_mode());
                }
                if ui.button("About").clicked() {
                    self.show_about = true;
                }
                }
                // The APO heartbeats telemetry every buffer; silence means the EQ
                // isn't processing (not applied to the default output, or an old
                // APO build without telemetry support).
                if !self.audio_live {
                    let warn = egui::RichText::new("! EQ not responding")
                        .color(egui::Color32::from_rgb(255, 170, 90));
                    if ui
                        .button(warn)
                        .on_hover_text(
                            "No live EQ telemetry. The visualizer captures Windows output independently.\nOpen SETUP to check the output and apply EQ.",
                        )
                        .clicked()
                    {
                        self.tab = inspector::Tab::Setup;
                    }
                }
                // The caption buttons live in a floating top-right overlay (see
                // caption_overlay below) so a cramped toolbar can never draw
                // through them. Here we only reserve their footprint and turn
                // the leftover gap into a window drag region.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let gap = ui.available_rect_before_wrap();
                    window_chrome::drag_region(ui, gap, "toolbar-drag-gap");
                });
            });
        });
        window_chrome::caption_overlay(ctx);
        window_chrome::edge_resize(ctx);

        // Meter source: prefer the APO's real pre/post-chain telemetry (true in
        // vs out; read above the toolbar). Fall back to the WASAPI-loopback
        // output level when the APO isn't publishing so the meter still shows
        // something.
        let (vrms, vpeak) = (self.capture.frame.rms, self.capture.frame.peak);
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let hint = if self.viz_settings.stage_view { "Stage view  |  F11 fullscreen  |  EQ continues processing" }
                    else { "drag = gain  ·  shift-drag / scroll = Q  ·  ctrl-drag = frequency  ·  dbl-click = reset band  ·  right-click = type" };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Delivered fps, always visible (pacing proof lives here).
                    ui.label(
                        egui::RichText::new(format!("{:.0} fps", self.fps))
                            .color(theme::muted())
                            .small(),
                    );
                    if self.delay_ms > 0.0 {
                        ui.label(
                            egui::RichText::new(format!("DELAY {:.0} ms", self.delay_ms))
                                .color(egui::Color32::from_rgb(255, 190, 90))
                                .small(),
                        );
                    }
                    if self.audio_live {
                        inout_meter(ui, tel.in_rms, tel.in_peak, tel.out_rms, tel.out_peak);
                        // ASCII on purpose: U+2192 has no coverage in Rajdhani
                        // and renders as a tofu box.
                        ui.label(egui::RichText::new("IN>OUT").color(theme::muted()).small());
                        if tel.gr_db > 0.2 {
                            ui.label(
                                egui::RichText::new(format!("GR -{:.1} dB", tel.gr_db))
                                    .color(egui::Color32::from_rgb(255, 150, 90))
                                    .small(),
                            )
                            .on_hover_text("Gain reduction from the dynamics stage");
                        }
                    } else {
                        out_meter(ui, vrms, vpeak);
                        ui.label(egui::RichText::new("LOOPBACK").color(theme::muted()).small());
                    }
                    if let Some(error) = &self.settings_error {
                        ui.colored_label(egui::Color32::LIGHT_RED, "Settings not saved").on_hover_text(error);
                    }
                    if let visualizer::Status::Error(error) = &self.capture.status {
                        ui.colored_label(egui::Color32::LIGHT_RED, "Visualizer unavailable").on_hover_text(error);
                    }
                    if let Some(error) = &self.profile_error {
                        ui.colored_label(egui::Color32::LIGHT_RED, "Profile operation failed").on_hover_text(error);
                    }
                    if let Some(problem) = &self.state.problem {
                        if ui.button(egui::RichText::new("EQ editing paused").color(egui::Color32::LIGHT_RED)).on_hover_text(&problem.message).clicked() { self.show_recovery = true; }
                    }
                    if let Some(error) = &self.state.backup_error {
                        ui.colored_label(egui::Color32::LIGHT_RED, "Sound backup not saved").on_hover_text(error);
                    }
                    if let Some(error) = &self.last_error {
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                    let help = if self.viz_settings.stage_view { "F11 fullscreen" } else { "EQ controls" };
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(help).small().color(theme::muted())).on_hover_text(hint);
                    });
                });
            });
        });

        // Right-side inspector: CHAIN / VIZ / SETUP tabs.
        inspector::show(self, ctx);
        }
        self.profiler.add(profiler::Scope::Panels, t_panels.elapsed());

        // Shuffle every scene, or just the saved favorites, on the chosen interval.
        let shuffle_period = std::time::Duration::from_secs_f32(self.viz_settings.shuffle_seconds);
        if self.show.mode != visual_before_input {
            self.shuffle_next = std::time::Instant::now() + shuffle_period;
        }
        if self.show_shuffle && self.show.mode != show::ShowMode::Off {
            let now = std::time::Instant::now();
            if now >= self.shuffle_next {
                let gl_modes = self.viz_settings.scene_pool();
                if gl_modes.len() == 1 { self.show.mode = gl_modes[0]; }
                else if gl_modes.len() > 1 {
                    let current = self.show.mode;
                    let mut next = current;
                    while next == current {
                        // Deterministic tiny xorshift32 (no rand dep).
                        let mut x = self.shuffle_rng;
                        x ^= x << 13;
                        x ^= x >> 17;
                        x ^= x << 5;
                        self.shuffle_rng = x;
                        next = gl_modes[x as usize % gl_modes.len()];
                    }
                    self.show.mode = next;
                }
                self.viz_browser.editor.follow_scene(self.show.mode);
                self.shuffle_next = now + shuffle_period;
            }
        } else {
            // Not actively shuffling: keep the deadline fresh so switching
            // shuffle back on does not fire an
            // instant switch from a long-stale timestamp.
            self.shuffle_next = std::time::Instant::now() + shuffle_period;
        }

        let t_hist = std::time::Instant::now();
        let analyzed = self.capture.frame.clone();
        let wave = &analyzed.wave;
        let spectrum = &analyzed.spectrum;
        let stereo = &analyzed.stereo;
        if self.layers.waterfall && self.spectro_revision != self.capture.revision {
            self.spectro.replace_rows(&analyzed.waterfall);
            self.spectro_revision = self.capture.revision;
        }
        let spectro_tex = if self.layers.waterfall {
            self.spectro.texture(ctx, self.rainbow)
        } else {
            None
        };
        self.profiler.add(profiler::Scope::Histories, t_hist.elapsed());

        // Split borrows: VizData reads the history fields while curve::draw
        // mutates bands + show — disjoint fields, so no clones. (The old code
        // cloned both history Vecs EVERY FRAME just to satisfy borrowck.)
        let t_canvas = std::time::Instant::now();
        // Uniforms for the GL stage: VizBus stats + theme accent + the audio
        // textures' source data, all by value (Copy) for the paint callback.
        let uni = {
            let b = &analyzed.bus;
            let energy = self.viz_settings.energy;
            let (fx_order, fx_mix, fx_count) = self.viz_settings.effect_uniforms();
            let acc = theme::viz_accent();
            let mut spec_arr = [0.0f32; glstage::SPEC_W];
            for (i, s) in spectrum.iter().take(glstage::SPEC_W).enumerate() {
                spec_arr[i] = (*s * energy).clamp(0.0, 1.0);
            }
            let mut wave_arr = [0.0f32; glstage::WAVE_W];
            if wave.len() >= 2 {
                let step = (wave.len() / glstage::WAVE_W).max(1);
                for (i, w) in wave.iter().step_by(step).take(glstage::WAVE_W).enumerate() {
                    wave_arr[i] = (*w * energy).clamp(-1.0, 1.0);
                }
            }
            glstage::Uniforms {
                mode: glstage::GlMode::Warp, // overwritten by the active mode
                bass: (b.bass * energy).min(1.0),
                mid: (b.mid * energy).min(1.0),
                treble: (b.treble * energy).min(1.0),
                pulse: (b.pulse * energy).min(1.0),
                beat_phase: b.beat_phase,
                bright: b.centroid,
                rainbow: self.viz_settings.rainbow,
                accent: [
                    acc.r() as f32 / 255.0,
                    acc.g() as f32 / 255.0,
                    acc.b() as f32 / 255.0,
                ],
                spec: spec_arr,
                wave: wave_arr,
                bpm: b.bpm,
                bpm_conf: b.bpm_conf,
                flux: (b.flux * energy).min(1.0),
                loud: (((b.loud_db + 60.0) / 60.0).clamp(0.0, 1.0) * energy).min(1.0),
                crest: (b.crest_db / 24.0).clamp(0.0, 1.0),
                corr: b.corr,
                width: b.width,
                fx: 0, fx_order, fx_mix, fx_count,
                speed: self.viz_settings.speed,
                brightness: self.viz_settings.brightness,
                render_scale: self.viz_settings.quality.scale(),
                transition_seconds: self.viz_settings.transition_seconds,
                draw_editor: !self.presentation.active && !self.viz_settings.stage_view,
                draw_overlays: !self.presentation.active && (!self.viz_settings.stage_view || self.viz_settings.overlays_in_stage),
            }
        };
        let mut eq_changed = false;
        {
            let viz = curve::VizData {
                layers: self.layers,
                spectrum,
                peaks: &analyzed.peaks,
                waveform: wave,
                stereo,
                loudness: &analyzed.loudness,
                spectro_tex,
            };
            let bands = &mut self.bands;
            let selected = &mut self.selected_band;
            let show = &mut self.show;
            let gl = self.gl_stage.as_ref();
            let (sample_rate, rainbow) = (self.sample_rate, self.rainbow);
            let panel = egui::CentralPanel::default();
            let panel = if self.presentation.active { panel.frame(egui::Frame::NONE) } else { panel };
            panel.show(ctx, |ui| {
                if self.state.problem.is_some() { ui.disable(); }
                let response = curve::draw(ui, bands, *selected, sample_rate, rainbow, &viz, show, gl, uni);
                eq_changed = response.changed;
                if let Some(index) = response.selected_band { *selected = index; }
            });
        }
        self.profiler.add(profiler::Scope::Canvas, t_canvas.elapsed());
        if eq_changed {
            self.commit();
        }

        self.presentation.controls(ctx, self.show.mode.label());
        let before_gallery = self.show.mode;
        viz_panel::gallery(ctx, &mut self.show.mode, &mut self.viz_settings, &mut self.viz_browser);
        if self.show.mode != before_gallery { self.shuffle_next = std::time::Instant::now() + shuffle_period; }

        if !self.presentation.active {
        // Save-as / rename modal (small centered window with a name field).
        let modal_kind = match &self.modal {
            Modal::None => None,
            Modal::SaveAs => Some(None),
            Modal::Rename { old } => Some(Some(old.clone())),
        };
        if let Some(rename_old) = modal_kind {
            let is_rename = rename_old.is_some();
            let title = if is_rename { "Rename profile" } else { "Save profile as" };
            let mut do_apply = false;
            let mut do_cancel = false;
            let name_taken = {
                let n = self.name_buf.trim();
                !n.is_empty() && self.store.get(n).is_some() && !rename_old.as_deref().is_some_and(|old| old.eq_ignore_ascii_case(n))
            };
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.name_buf)
                            .hint_text("profile name")
                            .char_limit(80)
                            .desired_width(200.0),
                    );
                    if self.modal_focus {
                        edit.request_focus();
                        self.modal_focus = false;
                    }
                    if edit.changed() { self.profile_error = None; }
                    let name_ok = profiles::validate_name(&self.name_buf).is_ok() && !(is_rename && name_taken);
                    if is_rename && name_taken {
                        ui.colored_label(egui::Color32::LIGHT_RED, "name already taken");
                    } else if name_taken {
                        ui.label(
                            egui::RichText::new("existing profile - will overwrite")
                                .color(theme::muted())
                                .small(),
                        );
                    }
                    let submit = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if let Some(error) = &self.profile_error {
                        ui.set_max_width(340.0);
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                    ui.horizontal(|ui| {
                        let verb = if is_rename { "Rename" } else { "Save" };
                        if ui.add_enabled(name_ok, egui::Button::new(verb)).clicked()
                            || (submit && name_ok)
                        {
                            do_apply = true;
                        }
                        if ui.button("Cancel").clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Escape))
                        {
                            do_cancel = true;
                        }
                    });
                });
            if do_apply {
                let new_name = self.name_buf.trim().to_string();
                let result = match rename_old {
                    Some(old) => {
                        let keep_active = self.active_profile.as_deref() == Some(old.as_str());
                        let result = self.store.rename(&old, &new_name);
                        if result.is_ok() && keep_active {
                            self.active_profile = Some(new_name);
                        }
                        result
                    }
                    None => self.save_current_as(&new_name),
                };
                match result {
                    Ok(()) => { self.modal = Modal::None; self.profile_error = None; }
                    Err(error) => self.profile_error = Some(error),
                }
            } else if do_cancel {
                self.modal = Modal::None;
            }
        }

        about::show(ctx, &mut self.show_about, &mut self.about_icon);
        theme_window::show(self, ctx);
        }

        self.sound_editor.observe(self.current_sound());
        if !ctx.input(|i| i.pointer.primary_down()) { self.sound_editor.finish_gesture(); self.state.checkpoint(); }
        if self.show_recovery {
            if let Some(problem) = &self.state.problem {
                let decision = state_recovery::show(ctx, problem, self.state.recovery_sound(), &self.state.recovery_label());
                match decision {
                    Some(state_recovery::Decision::Later) => self.show_recovery = false,
                    Some(state_recovery::Decision::Apply(action)) => {
                        if let Ok(s) = self.state.recover(action) {
                            self.bands = s.bands; self.preamp_db = s.preamp_db; self.dynamics = s.dynamics; self.bypass = s.is_bypassed(); self.delay_ms = s.delay_ms;
                            self.active_profile = None;
                            self.sound_editor = sound_edit::Editor::new(self.current_sound());
                            self.show_recovery = false;
                        }
                    }
                    None => {}
                }
            }
        }

        // Persist UI settings on change. There is no clean shutdown hook (tray
        // Quit is process::exit), so save-on-change is the only reliable path.
        let pal = theme::current();
        let cur = settings::AppSettings {
            schema: 2,
            dark_mode: pal.dark,
            rainbow: self.rainbow,
            layers: self.layers,
            zoom: ctx.zoom_factor(),
            active_profile: self.active_profile.clone(),
            inspector_tab: self.tab.as_str().to_string(),
            show_mode: self.show.mode.as_str().to_string(),
            show_shuffle: self.show_shuffle,
            show_fx: self.viz_settings.legacy_mask(),
            visualizer: Some(self.viz_settings.clone()),
            theme_name: pal.name,
            theme_colors: pal.source,
            gradient: theme::gradient_enabled(),
            gradient_angle: theme::gradient_cfg().angle_deg,
            gradient_intensity: theme::gradient_cfg().intensity,
            gradient_pegs: theme::gradient_cfg().pegs,
            gradient_harmony: theme::gradient_cfg().harmony,
            gradient_preset: theme::gradient_cfg().preset,
            gradient_custom: theme::gradient_cfg()
                .custom
                .iter()
                .map(|rgb| color::rgb_to_hex(*rgb))
                .collect::<Vec<_>>()
                .join(","),
            frost_dark: theme::frost(true),
            frost_light: theme::frost(false),
            gradient_preset_sync: self.gradient_preset_sync,
        };
        // Only hit the disk once a drag ends: the Theme window's
        // Direction/Intensity/Frost sliders change `cur` every single frame
        // mid-drag (Slider fires `changed()` continuously), and without this
        // gate that means a save (temp-write + rename) per frame for as long
        // as the mouse button is held - avoidable SSD churn a plain "did the
        // value change" diff doesn't catch (SpaceView precedent).
        if cur == self.settings_cache {
            self.settings_error = None;
        } else if !ctx.input(|i| i.pointer.primary_down()) && std::time::Instant::now() >= self.settings_retry_at {
            match cur.save() {
                Ok(()) => { self.settings_cache = cur; self.settings_error = None; }
                Err(error) => {
                    log_line(&format!("settings: {error}"));
                    self.settings_error = Some(error);
                    self.settings_retry_at = std::time::Instant::now() + std::time::Duration::from_secs(2);
                }
            }
        }

        // Self-screenshot pipe: the harness drops shot.req and we ReadPixels
        // our own final GL frame to shot.png. This is the ONLY reliable
        // capture of this app: eframe's Screenshot command is dropped on GL,
        // PrintWindow is UIPI-blocked (we're elevated), and screen copies race
        // monitors/z-order. A Foreground-layer callback runs after every panel
        // has tessellated, so the grab is the real composed frame.
        const SHOT_REQ: &str = r"C:\ProgramData\TrontEq\shot.req";
        const SHOT_PNG: &str = r"C:\ProgramData\TrontEq\shot.png";
        if std::path::Path::new(SHOT_REQ).exists() {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("tronteq-selfshot"),
            ));
            let rect = ctx.content_rect();
            painter.add(egui::PaintCallback {
                rect,
                callback: std::sync::Arc::new(eframe::egui_glow::CallbackFn::new(
                    move |info, painter| {
                        use eframe::glow::HasContext;
                        let gl = painter.gl();
                        let w = info.screen_size_px[0] as i32;
                        let h = info.screen_size_px[1] as i32;
                        if w <= 0 || h <= 0 {
                            return;
                        }
                        let mut buf = vec![0u8; (w * h * 4) as usize];
                        unsafe {
                            gl.read_pixels(
                                0,
                                0,
                                w,
                                h,
                                eframe::glow::RGBA,
                                eframe::glow::UNSIGNED_BYTE,
                                eframe::glow::PixelPackData::Slice(Some(&mut buf)),
                            );
                        }
                        // GL rows are bottom-up; flip + force opaque alpha.
                        let stride = (w * 4) as usize;
                        let mut out = vec![0u8; buf.len()];
                        for row in 0..h as usize {
                            let src = &buf[(h as usize - 1 - row) * stride..][..stride];
                            out[row * stride..][..stride].copy_from_slice(src);
                        }
                        for px in out.chunks_exact_mut(4) {
                            px[3] = 255;
                        }
                        let _ = image::save_buffer(
                            SHOT_PNG,
                            &out,
                            w as u32,
                            h as u32,
                            image::ColorType::Rgba8,
                        );
                        let _ = std::fs::remove_file(SHOT_REQ);
                        crate::log_line("selfshot: wrote shot.png");
                    },
                )),
            });
        }

        // Close out the profiler frame + draw the F10 overlay (drawn after
        // commit so it never measures itself; Boxel rule).
        self.profiler.add(profiler::Scope::Update, t_update.elapsed());
        self.profiler.commit_frame();
        self.profiler.draw_overlay(ctx, self.frame_ms);

        // Delivered-FPS counter (the heartbeat thread owns pacing; this just
        // measures what actually arrives so the jank is a number, not a vibe).
        self.fps_frames += 1;
        let el = self.fps_t0.elapsed().as_secs_f32();
        if el >= 1.0 {
            self.fps = self.fps_frames as f32 / el;
            self.fps_frames = 0;
            self.fps_t0 = std::time::Instant::now();
        }
    }
}

/// Overlaid input-vs-output VU. Output is the filled bar (RMS, colored by its peak
/// level); input is drawn as a cyan cap line + peak tick over the top. Reading it:
/// the cyan line is where the signal entered, the filled bar is where the chain put
/// it — line past the fill = net cut, fill past the line = net boost. Both share one
/// dB scale (~ -54 dB .. 0).
fn inout_meter(ui: &mut egui::Ui, in_rms: f32, in_peak: f32, out_rms: f32, out_peak: f32) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(150.0, 16.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, theme::meter_track());
    let db = |v: f32| 20.0 * v.max(1e-5).log10();
    let norm = |d: f32| ((d + 54.0) / 54.0).clamp(0.0, 1.0);
    let x_at = |v: f32| rect.left() + rect.width() * norm(db(v));

    // OUT: filled RMS bar, colored by output peak (green/yellow/red).
    let opk = norm(db(out_peak));
    let col = if opk > norm(-3.0) {
        egui::Color32::from_rgb(255, 90, 90)
    } else if opk > norm(-12.0) {
        egui::Color32::from_rgb(255, 210, 90)
    } else {
        theme::ok()
    };
    let orms = norm(db(out_rms));
    if orms > 0.0 {
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * orms, rect.height()));
        p.rect_filled(fill, 3.0, col.gamma_multiply(0.85));
    }
    // OUT peak tick (white).
    let oxp = x_at(out_peak);
    p.line_segment(
        [egui::pos2(oxp, rect.top()), egui::pos2(oxp, rect.bottom())],
        egui::Stroke::new(2.0, theme::ink(255)),
    );

    // IN: a cyan cap line along the top to the input RMS extent + a half-height
    // input peak tick. Sits over the OUT fill so both are always visible.
    let inc = theme::cyan();
    let irms_x = x_at(in_rms);
    p.line_segment(
        [egui::pos2(rect.left(), rect.top() + 2.0), egui::pos2(irms_x, rect.top() + 2.0)],
        egui::Stroke::new(2.0, inc),
    );
    let ixp = x_at(in_peak);
    p.line_segment(
        [egui::pos2(ixp, rect.top()), egui::pos2(ixp, rect.top() + rect.height() * 0.5)],
        egui::Stroke::new(1.5, inc.gamma_multiply(0.9)),
    );

    resp.on_hover_text(format!(
        "IN   rms {:.0} dB   peak {:.0} dB\nOUT  rms {:.0} dB   peak {:.0} dB",
        db(in_rms),
        db(in_peak),
        db(out_rms),
        db(out_peak),
    ));
}

/// Compact horizontal output meter: RMS fill (green/yellow/red by level) + peak tick.
fn out_meter(ui: &mut egui::Ui, rms: f32, peak: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(130.0, 12.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, theme::meter_track());
    let db = |v: f32| 20.0 * v.max(1e-5).log10();
    let norm = |d: f32| ((d + 54.0) / 54.0).clamp(0.0, 1.0);
    let rt = norm(db(rms));
    let pt = norm(db(peak));
    let col = if pt > norm(-3.0) {
        egui::Color32::from_rgb(255, 90, 90)
    } else if pt > norm(-12.0) {
        egui::Color32::from_rgb(255, 210, 90)
    } else {
        theme::ok()
    };
    if rt > 0.0 {
        let fill =
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * rt, rect.height()));
        p.rect_filled(fill, 3.0, col.gamma_multiply(0.85));
    }
    let px = rect.left() + rect.width() * pt;
    p.line_segment(
        [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
        egui::Stroke::new(2.0, theme::ink(255)),
    );
}

