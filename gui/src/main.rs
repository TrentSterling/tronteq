//! TrontEQ GUI — draggable parametric curve. Writes the shared state file
//! the C++ APO reads every buffer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod color;
mod curve;
mod devices;
mod dsp_preview;
mod glstage;
mod inspector;
mod knob;
mod presets;
mod profiler;
mod profiles;
mod settings;
mod show;
mod spectrogram;
mod state_writer;
mod theme;
mod theme_window;
mod visualizer;
mod vizbus;
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

fn main() -> Result<()> {
    install_crash_logger();
    log_line(&format!("start pid={} main tid={}", std::process::id(), tid()));

    // Single instance: own the port, or hand the request to whoever does.
    let listener = match std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, SHOW_PORT)) {
        Ok(l) => l,
        Err(_) => {
            use std::io::Write as _;
            log_line("second instance: poking the running one to show, then exiting");
            if let Ok(mut s) =
                std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, SHOW_PORT))
            {
                let _ = s.write_all(b"show");
            }
            return Ok(());
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
        // Custom chrome: the toolbar is the title bar (window_chrome.rs owns the
        // caption buttons, dragging, and edge-resize the OS border used to give us).
        .with_decorations(false)
        .with_inner_size([1000.0, 460.0])
        .with_min_inner_size([800.0, 320.0]);
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
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(App::new(cc)?) as Box<dyn eframe::App>)
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
    spec_peaks: Vec<f32>, // peak-hold caps (instant rise, slow fall)
    loud_hist: Vec<f32>,  // rolling output RMS history
    last_step: std::time::Instant, // wall-clock pacing for the history visualizers
    spectro: spectrogram::Spectrogram,
    about_icon: Option<egui::TextureHandle>,

    // Tray (hide-to-tray on close so the control panel stays alive)
    _tray: Option<TrayIcon>,
    app_hwnd: isize,
    /// LATCHED: the close button hid us to the tray. Only the tray Show /
    /// DoubleClick handlers and a second launch clear it. A tray'd window gets
    /// no OS events, so nothing may infer this one.
    tray_hidden: Arc<AtomicBool>,
    /// DERIVED: false whenever nobody can see the window (tray'd, minimized,
    /// cloaked, collapsed) -> slow idle repaint. Recomputed every frame by
    /// `presentable`; read by the heartbeat thread.
    visible: Arc<AtomicBool>,
    /// Last inner size that was actually sane, to restore to when the window
    /// desyncs. See `heal_tiny_window`.
    good_size: egui::Vec2,
    /// Consecutive frames observed below the minimum inner size.
    small_frames: u32,
    fps_frames: u32,
    fps_t0: std::time::Instant,
    fps: f32, // measured delivered framerate (DATA/SETUP readout)

    // Output picker
    devices: Vec<devices::Device>,
    selected_device: usize,
    /// Receives the result of an in-flight `apply` (runs off-thread so the UI
    /// doesn't freeze during the audio-service restart).
    apply_rx: Option<Receiver<std::result::Result<String, String>>>,
    device_status: Option<(bool, String)>, // (ok, message)

    // Profiles + persisted UI settings
    store: profiles::ProfileStore,
    active_profile: Option<String>,
    modal: Modal,
    name_buf: String,
    modal_focus: bool,
    settings_cache: settings::AppSettings,
    /// The Theme window (gradient v2 controls): opened from the toolbar's
    /// accent swatch. Not persisted - always starts closed, same as `show_about`.
    show_theme_window: bool,
    /// Picking a gradient preset also rethemes the app (accent = the
    /// preset's most-saturated stop). Persisted; the `GradientCfg` itself
    /// lives in `theme.rs`'s own static, not here.
    gradient_preset_sync: bool,
    tab: inspector::Tab,
    show: show::ShowState,
    /// Auto-cycle through the GL SHOW modes while one is active.
    show_shuffle: bool,
    /// Stackable post-fx bitmask layered over any GL SHOW mode (see
    /// `glstage::FX_*`). 0 = off.
    show_fx: u32,
    /// Next scheduled shuffle switch (checked every frame, only acted on
    /// while `show_shuffle` is on and a GL mode is active).
    shuffle_next: std::time::Instant,
    /// Deterministic tiny LCG state (no rand dep) for picking the next mode.
    shuffle_rng: u32,
    frame_ms: f32, // EMA of eframe's reported per-frame CPU cost
    profiler: profiler::Profiler,
    vizbus: vizbus::VizBus,
    /// The shader stage (None if GL init failed; painter modes still work).
    gl_stage: Option<std::sync::Arc<std::sync::Mutex<glstage::GlStage>>>,
}

/// Which name-entry modal is open.
enum Modal {
    None,
    SaveAs,
    Rename { old: String },
}

/// Deferred profile-chip actions (can't mutate the store while iterating it).
enum ProfileAction {
    Load(String),
    Overwrite(String),
    StartRename(String),
    Delete(String),
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Result<Self> {
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

        let devices = devices::list().unwrap_or_default();
        let selected_device = devices.iter().position(|d| d.is_default).unwrap_or(0);

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
            .clone()
            .filter(|n| store.get(n).is_some());

        // System tray: close hides to tray (keeps the control panel alive, avoids
        // a UAC relaunch since the GUI is elevated).
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
            .with_tooltip("TrontEQ")
            .with_menu(Box::new(tray_menu));
        if let Some(icon) = tray_icon {
            tray_builder = tray_builder.with_icon(icon);
        }
        let tray = tray_builder.build().ok();
        log_line(if tray.is_some() {
            "tray: built ok"
        } else {
            "tray: FAILED to build (close will exit instead of hiding)"
        });

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
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if e.id == show {
                    win::show_window(app_hwnd);
                    th.store(false, Ordering::Relaxed);
                    v.store(true, Ordering::Relaxed);
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
                    c.request_repaint();
                }
            }));
        }

        // Push an initial committed state so APO stops bypassing.
        let mut me = App {
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
            viz: visualizer::Visualizer::start(),
            layers: ui_settings.layers,
            spec_peaks: Vec::new(),
            loud_hist: Vec::new(),
            last_step: std::time::Instant::now(),
            spectro: spectrogram::Spectrogram::new(),
            about_icon: None,
            _tray: tray,
            app_hwnd,
            tray_hidden,
            visible,
            good_size: egui::vec2(1000.0, 460.0), // matches with_inner_size
            small_frames: 0,
            fps_frames: 0,
            fps_t0: std::time::Instant::now(),
            fps: 0.0,
            devices,
            selected_device,
            apply_rx: None,
            device_status: None,
            store,
            active_profile,
            modal: Modal::None,
            name_buf: String::new(),
            modal_focus: false,
            tab: inspector::Tab::from_str(&ui_settings.inspector_tab),
            show: show::ShowState::new(show::ShowMode::from_str(&ui_settings.show_mode)),
            show_shuffle: ui_settings.show_shuffle,
            show_fx: ui_settings.show_fx,
            shuffle_next: std::time::Instant::now() + SHUFFLE_PERIOD,
            shuffle_rng: 0x9E37_79B9,
            frame_ms: 0.0,
            profiler: profiler::Profiler::default(),
            vizbus: vizbus::VizBus::new(),
            gl_stage,
            show_theme_window: std::env::var_os("TRONTEQ_THEME_WINDOW").is_some(),
            gradient_preset_sync: ui_settings.gradient_preset_sync,
            settings_cache: ui_settings,
        };
        me.commit();
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
        // Just under the 800x320 min_inner_size, so only genuine breakage trips it.
        const MIN_W: f32 = 790.0;
        const MIN_H: f32 = 310.0;
        // content_rect, not the deprecated screen_rect: we want the drawable area,
        // which on this decorations(false) window is the inner size min_inner_size
        // constrains.
        let sz = ctx.input(|i| i.content_rect()).size();
        if sz.x >= MIN_W && sz.y >= MIN_H {
            self.good_size = sz; // remember what the user actually chose
            self.small_frames = 0;
        } else if sz.x > 1.0 && sz.y > 1.0 {
            // Non-degenerate but sub-minimum: confirm it persists, then fix.
            self.small_frames += 1;
            if self.small_frames % 4 == 0 {
                log_line(&format!(
                    "window desynced to {:.0}x{:.0}, restoring to {:.0}x{:.0}",
                    sz.x, sz.y, self.good_size.x, self.good_size.y
                ));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.good_size));
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
        if self.tray_hidden.load(Ordering::Relaxed) {
            return false;
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

    /// Apply the APO to the selected output on a background thread.
    fn start_apply(&mut self) {
        let Some(dev) = self.devices.get(self.selected_device) else { return };
        let idx = dev.index;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(devices::apply(idx).map_err(|e| e.to_string()));
        });
        self.apply_rx = Some(rx);
        self.device_status = Some((true, "applying…".to_string()));
    }

    fn poll_apply(&mut self) {
        if let Some(rx) = &self.apply_rx {
            if let Ok(res) = rx.try_recv() {
                self.device_status = Some(match res {
                    Ok(_) => (true, "✓ EQ active on this output".to_string()),
                    Err(e) => (false, format!("✗ {}", e.lines().last().unwrap_or("failed"))),
                });
                self.apply_rx = None;
                self.devices = devices::list().unwrap_or_default();
                self.selected_device =
                    self.selected_device.min(self.devices.len().saturating_sub(1));
            }
        }
    }

    fn commit(&mut self) {
        self.state.write_state(
            &self.bands,
            self.bypass,
            self.preamp_db,
            &self.dynamics,
            self.delay_ms,
        );
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
    fn save_current_as(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
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
        });
        self.active_profile = Some(name.to_string());
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

        // Number keys 1-9 quickswap profiles (skipped while a text field has focus).
        if !ctx.wants_keyboard_input() {
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
        if self._tray.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            win::hide_window(self.app_hwnd);
            self.tray_hidden.store(true, Ordering::Relaxed);
            self.visible.store(false, Ordering::Relaxed);
            log_line("hide to tray (close intercepted; process stays alive)");
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
        let tel = self.state.telemetry();
        let t_panels = std::time::Instant::now();

        // Discord-style background wash, painted into the background layer
        // before any panel draws (no-op when the toggle is off). Panels
        // above it go slightly translucent (`theme::build_visuals`) so it
        // reads through the chrome; the EQ canvas + OUT meter paint their
        // own opaque fills and stay solid dark regardless (house rule).
        theme::paint_gradient(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
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
                ui.separator();
                // Profile chips: click = load (keys 1-9 too), right-click = manage,
                // `*` = tweaked since save. Factory chips are ordinary profiles.
                let mut act: Option<ProfileAction> = None;
                for e in &self.store.entries {
                    let p = &e.profile;
                    let active = self.active_profile.as_deref() == Some(p.name.as_str());
                    let dirty = active && !p.matches(&self.bands, self.preamp_db, &self.dynamics);
                    let label = if dirty { format!("{}*", p.name) } else { p.name.clone() };
                    let resp = ui.selectable_label(active, label);
                    let resp = if dirty {
                        resp.on_hover_text("Tweaked since save. Click = reload saved, right-click = overwrite")
                    } else {
                        resp
                    };
                    if resp.clicked() {
                        act = Some(ProfileAction::Load(p.name.clone()));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Overwrite with current").clicked() {
                            act = Some(ProfileAction::Overwrite(p.name.clone()));
                            ui.close();
                        }
                        if ui.button("Rename...").clicked() {
                            act = Some(ProfileAction::StartRename(p.name.clone()));
                            ui.close();
                        }
                        ui.menu_button("Delete", |ui| {
                            if ui.button(format!("Really delete \"{}\"", p.name)).clicked() {
                                act = Some(ProfileAction::Delete(p.name.clone()));
                                ui.close();
                            }
                        });
                    });
                }
                if ui
                    .button("+")
                    .on_hover_text("Save current sound as a new profile")
                    .clicked()
                {
                    self.modal = Modal::SaveAs;
                    self.name_buf.clear();
                    self.modal_focus = true;
                }
                if let Some(a) = act {
                    match a {
                        ProfileAction::Load(n) => self.apply_profile(&n),
                        ProfileAction::Overwrite(n) => self.save_current_as(&n),
                        ProfileAction::StartRename(n) => {
                            self.name_buf = n.clone();
                            self.modal = Modal::Rename { old: n };
                            self.modal_focus = true;
                        }
                        ProfileAction::Delete(n) => {
                            self.store.delete(&n);
                            if self.active_profile.as_deref() == Some(n.as_str()) {
                                self.active_profile = None;
                            }
                        }
                    }
                }
                let mut bypass = self.bypass;
                if ui.checkbox(&mut bypass, "Bypass").changed() {
                    self.bypass = bypass;
                    self.commit();
                }
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
                // The APO heartbeats telemetry every buffer; silence means the EQ
                // isn't processing (not applied to the default output, or an old
                // APO build without telemetry support).
                if tel.seq == 0 {
                    let warn = egui::RichText::new("! EQ silent")
                        .color(egui::Color32::from_rgb(255, 170, 90));
                    if ui
                        .button(warn)
                        .on_hover_text(
                            "No telemetry from the EQ on this output.\nOpen SETUP and hit \"Apply EQ here\".",
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
                    ui.add_space(window_chrome::CAPTION_W);
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
        let (vrms, vpeak) = self.viz.level();
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let hint = "drag = gain  ·  shift-drag / scroll = Q  ·  ctrl-drag = frequency  ·  dbl-click = reset band  ·  right-click = type";
                ui.label(hint);
                if let Some(e) = &self.last_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("· {e}"));
                }
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
                    if tel.seq > 0 {
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
                        ui.label(egui::RichText::new("OUT").color(theme::muted()).small());
                    }
                });
            });
        });

        // Right-side inspector: CHAIN / VIZ / SETUP tabs.
        inspector::show(self, ctx);
        self.profiler.add(profiler::Scope::Panels, t_panels.elapsed());

        // Shuffle: auto-cycle the GL SHOW modes every ~18s while one is
        // active. GL modes only (painter modes are excluded on purpose -
        // shuffle is a "let it ride" showcase mode for the shader stage).
        if self.show_shuffle && self.show.mode.gl_mode().is_some() {
            let now = std::time::Instant::now();
            if now >= self.shuffle_next {
                let gl_modes: Vec<show::ShowMode> = show::ShowMode::ALL
                    .iter()
                    .copied()
                    .filter(|m| m.gl_mode().is_some())
                    .collect();
                if gl_modes.len() > 1 {
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
                self.shuffle_next = now + SHUFFLE_PERIOD;
            }
        } else {
            // Not actively shuffling: keep the deadline fresh so switching
            // shuffle back on (or landing on a GL mode) doesn't fire an
            // instant switch from a long-stale timestamp.
            self.shuffle_next = std::time::Instant::now() + SHUFFLE_PERIOD;
        }

        let t_hist = std::time::Instant::now();
        let wave = self.viz.snapshot();
        let spectrum = self.viz.spectrum();
        let stereo = self.viz.stereo();
        let (rms, peak) = self.viz.level();
        // Feed the data pipes (self-gates to 60 Hz; DATA tab + viz consume it).
        self.vizbus.step(&spectrum, &stereo, rms, peak);

        // Advance the GUI-side history visualizers on a fixed ~60Hz wall-clock
        // cadence rather than once-per-frame. This decouples scroll speed from how
        // fast frames actually arrive: unfocused we render at ~20fps, and on focus
        // regain winit can deliver a burst of coalesced repaints — a per-frame
        // advance would rip the waterfall / loudness / peak-fall forward ("lag then
        // fast-forward catch-up"). We step at most once per 16ms and snap the clock
        // to now after a gap, so a backlog is dropped, not replayed.
        const STEP: std::time::Duration = std::time::Duration::from_millis(16);
        let stepped = self.last_step.elapsed() >= STEP;
        if stepped {
            self.last_step = std::time::Instant::now();
        }

        // Peak-hold caps: rise tracks the live spectrum every frame (caps never sit
        // below a current bar); the slow fall is what we time-gate.
        if self.spec_peaks.len() != spectrum.len() {
            self.spec_peaks = spectrum.clone();
        } else {
            for (p, &s) in self.spec_peaks.iter_mut().zip(spectrum.iter()) {
                *p = if s >= *p {
                    s
                } else if stepped {
                    (*p - 0.012).max(s)
                } else {
                    *p
                };
            }
        }

        if stepped {
            // Loudness history ring (~8s at 60Hz).
            self.loud_hist.push(rms);
            if self.loud_hist.len() > 480 {
                let excess = self.loud_hist.len() - 480;
                self.loud_hist.drain(0..excess);
            }
            if self.layers.waterfall {
                self.spectro.push(&spectrum);
            }
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
            let b = &self.vizbus;
            let acc = theme::viz_accent();
            let mut spec_arr = [0.0f32; glstage::SPEC_W];
            for (i, s) in spectrum.iter().take(glstage::SPEC_W).enumerate() {
                spec_arr[i] = *s;
            }
            let mut wave_arr = [0.0f32; glstage::WAVE_W];
            if wave.len() >= 2 {
                let step = (wave.len() / glstage::WAVE_W).max(1);
                for (i, w) in wave.iter().step_by(step).take(glstage::WAVE_W).enumerate() {
                    wave_arr[i] = *w;
                }
            }
            glstage::Uniforms {
                mode: glstage::GlMode::Warp, // overwritten by the active mode
                bass: b.bass,
                mid: b.mid,
                treble: b.treble,
                pulse: b.pulse,
                beat_phase: b.beat_phase,
                bright: b.centroid,
                rainbow: self.rainbow,
                accent: [
                    acc.r() as f32 / 255.0,
                    acc.g() as f32 / 255.0,
                    acc.b() as f32 / 255.0,
                ],
                spec: spec_arr,
                wave: wave_arr,
                bpm: b.bpm,
                bpm_conf: b.bpm_conf,
                flux: b.flux,
                loud: ((b.loud_db + 60.0) / 60.0).clamp(0.0, 1.0),
                crest: (b.crest_db / 24.0).clamp(0.0, 1.0),
                corr: b.corr,
                width: b.width,
                fx: self.show_fx,
            }
        };
        let mut eq_changed = false;
        {
            let viz = curve::VizData {
                layers: self.layers,
                spectrum: &spectrum,
                peaks: &self.spec_peaks,
                waveform: &wave,
                stereo: &stereo,
                loudness: &self.loud_hist,
                spectro_tex,
            };
            let bands = &mut self.bands;
            let show = &mut self.show;
            let gl = self.gl_stage.as_ref();
            let (sample_rate, rainbow) = (self.sample_rate, self.rainbow);
            egui::CentralPanel::default().show(ctx, |ui| {
                eq_changed =
                    curve::draw(ui, bands, sample_rate, rainbow, &viz, show, gl, uni).changed;
            });
        }
        self.profiler.add(profiler::Scope::Canvas, t_canvas.elapsed());
        if eq_changed {
            self.commit();
        }

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
                !n.is_empty() && self.store.get(n).is_some() && rename_old.as_deref() != Some(n)
            };
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.name_buf)
                            .hint_text("profile name")
                            .desired_width(200.0),
                    );
                    if self.modal_focus {
                        edit.request_focus();
                        self.modal_focus = false;
                    }
                    let name_ok = !self.name_buf.trim().is_empty() && !(is_rename && name_taken);
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
                match rename_old {
                    Some(old) => {
                        let keep_active = self.active_profile.as_deref() == Some(old.as_str());
                        self.store.rename(&old, &new_name);
                        if keep_active {
                            self.active_profile = Some(new_name);
                        }
                    }
                    None => self.save_current_as(&new_name),
                }
                self.modal = Modal::None;
            } else if do_cancel {
                self.modal = Modal::None;
            }
        }

        about::show(ctx, &mut self.show_about, &mut self.about_icon);
        theme_window::show(self, ctx);

        // Persist UI settings on change. There is no clean shutdown hook (tray
        // Quit is process::exit), so save-on-change is the only reliable path.
        let pal = theme::current();
        let cur = settings::AppSettings {
            schema: 1,
            dark_mode: pal.dark,
            rainbow: self.rainbow,
            layers: self.layers,
            zoom: ctx.zoom_factor(),
            active_profile: self.active_profile.clone(),
            inspector_tab: self.tab.as_str().to_string(),
            show_mode: self.show.mode.as_str().to_string(),
            show_shuffle: self.show_shuffle,
            show_fx: self.show_fx,
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
        if cur != self.settings_cache && !ctx.input(|i| i.pointer.primary_down()) {
            cur.save();
            self.settings_cache = cur;
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

