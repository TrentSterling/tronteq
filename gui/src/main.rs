//! TrontEQ GUI — draggable parametric curve. Writes the shared state file
//! the C++ APO reads every buffer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod curve;
mod devices;
mod dsp_preview;
mod knob;
mod presets;
mod spectrogram;
mod state_writer;
mod theme;
mod visualizer;
mod win;

use anyhow::Result;
use eframe::egui;
use raw_window_handle::HasWindowHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tronteq_shared::{Band, BandKind, Dynamics, DEFAULT_FREQS, NUM_BANDS};

/// Write any Rust panic to a crash log before `panic = "abort"` kills us. The GUI
/// runs on the windows subsystem (no console), so without this a panic vanishes
/// as a bare 0xc0000409 in the Event Log with no message or location. The hook is
/// global (fires for any thread, incl. the WASAPI capture thread), and the process
/// is elevated, so it can append into ProgramData.
fn install_crash_logger() {
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = format!("[{ts}] {info}\n");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(r"C:\ProgramData\TrontEq\crash.log")
        {
            let _ = f.write_all(line.as_bytes());
        }
    }));
}

fn main() -> Result<()> {
    install_crash_logger();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("TrontEQ")
        .with_inner_size([1000.0, 460.0])
        .with_min_inner_size([800.0, 320.0]);
    // Window + taskbar icon = Trent's face (the tront.xyz favicon).
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "TrontEQ",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(App::new(cc)?) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e:?}"))?;
    Ok(())
}

struct App {
    state: state_writer::StateWriter,
    bands: [Band; NUM_BANDS],
    bypass: bool,
    sample_rate: f32,
    last_error: Option<String>,

    // Signal chain (preamp + dynamics)
    preamp_db: f32,
    dynamics: Dynamics,
    show_about: bool,
    rainbow: bool,
    viz: visualizer::Visualizer,
    layers: curve::Layers,
    spec_peaks: Vec<f32>, // peak-hold caps (instant rise, slow fall)
    loud_hist: Vec<f32>,  // rolling output RMS history
    spectro: spectrogram::Spectrogram,
    about_icon: Option<egui::TextureHandle>,

    // Tray (hide-to-tray on close so the control panel stays alive)
    _tray: Option<TrayIcon>,
    app_hwnd: isize,
    visible: Arc<AtomicBool>, // false while hidden-to-tray -> slow idle repaint

    // Output picker
    devices: Vec<devices::Device>,
    selected_device: usize,
    /// Receives the result of an in-flight `apply` (runs off-thread so the UI
    /// doesn't freeze during the audio-service restart).
    apply_rx: Option<Receiver<std::result::Result<String, String>>>,
    device_status: Option<(bool, String)>, // (ok, message)
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

        let devices = devices::list().unwrap_or_default();
        let selected_device = devices.iter().position(|d| d.is_default).unwrap_or(0);

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

        // Tracks shown vs hidden-to-tray so update() can drop to a slow idle
        // repaint instead of burning a core at 60fps with no window on screen.
        let visible = Arc::new(AtomicBool::new(true));

        // Tray events fire on the message thread even while the window is hidden;
        // act via raw Win32 (works without the eframe update loop running).
        {
            let c = ctx.clone();
            let show = tray_show_id.clone();
            let quit = tray_quit_id.clone();
            let v = visible.clone();
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                if e.id == show {
                    win::show_window(app_hwnd);
                    v.store(true, Ordering::Relaxed);
                    c.request_repaint();
                } else if e.id == quit {
                    std::process::exit(0);
                }
            }));
        }
        {
            let c = ctx.clone();
            let v = visible.clone();
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
            show_about: false,
            rainbow: true,
            viz: visualizer::Visualizer::start(),
            layers: curve::Layers::default(),
            spec_peaks: Vec::new(),
            loud_hist: Vec::new(),
            spectro: spectrogram::Spectrogram::new(),
            about_icon: None,
            _tray: tray,
            app_hwnd,
            visible,
            devices,
            selected_device,
            apply_rx: None,
            device_status: None,
        };
        me.commit();
        Ok(me)
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
        self.state
            .write_state(&self.bands, self.bypass, self.preamp_db, &self.dynamics);
    }

    fn reset_flat(&mut self) {
        for (i, f) in DEFAULT_FREQS.iter().enumerate() {
            self.bands[i] = Band {
                freq: *f,
                gain: 0.0,
                q: 1.0,
                kind: BandKind::Peak as u32,
            };
        }
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
        self.commit();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_apply();

        // Hide-to-tray on close (tray Show/Quit handled in the tray handlers).
        // Only if a tray exists, else we'd trap the window with no way back.
        if self._tray.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            win::hide_window(self.app_hwnd);
            self.visible.store(false, Ordering::Relaxed);
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let title_color = if self.rainbow {
                    theme::hsv((ui.input(|i| i.time) as f32 * 0.2).fract(), 0.9, 1.0)
                } else {
                    theme::cyan()
                };
                ui.label(
                    egui::RichText::new("TrontEQ")
                        .family(egui::FontFamily::Name("display".into()))
                        .color(title_color)
                        .strong()
                        .size(23.0),
                );
                ui.separator();
                if ui.button("Flat").clicked() {
                    self.reset_flat();
                }
                if ui
                    .button("Reset")
                    .on_hover_text("Reset the whole chain (EQ + preamp + dynamics) to defaults")
                    .clicked()
                {
                    self.reset_all();
                }
                for name in presets::EQ_PRESETS {
                    if ui.button(name).clicked() {
                        presets::apply_eq(&mut self.bands, name);
                        self.commit();
                    }
                }
                let mut bypass = self.bypass;
                if ui.checkbox(&mut bypass, "Bypass").changed() {
                    self.bypass = bypass;
                    self.commit();
                }
                ui.checkbox(&mut self.rainbow, "Rainbow");
                ui.label(egui::RichText::new("Viz").color(theme::muted()));
                let g = &mut self.layers;
                ui.toggle_value(&mut g.spectrum, "Spec").on_hover_text("Spectrum bars");
                ui.toggle_value(&mut g.peak_hold, "Peak").on_hover_text("Peak-hold caps");
                ui.toggle_value(&mut g.analyzer, "Line").on_hover_text("Analyzer line");
                ui.toggle_value(&mut g.waterfall, "Fall").on_hover_text("Spectrogram waterfall");
                ui.toggle_value(&mut g.waveform, "Wave").on_hover_text("Waveform");
                ui.toggle_value(&mut g.goniometer, "Gonio").on_hover_text("Stereo goniometer");
                ui.toggle_value(&mut g.loudness, "Loud").on_hover_text("Loudness history");
                if ui
                    .button(if theme::dark_mode() { "Light" } else { "Dark" })
                    .on_hover_text("Toggle light / dark theme")
                    .clicked()
                {
                    theme::set_mode(ctx, !theme::dark_mode());
                }
                ui.separator();
                ui.label(format!("{} Hz", self.sample_rate as u32));
                ui.label(format!("v{}", self.state.version()));
                if ui.button("About").clicked() {
                    self.show_about = true;
                }
                ui.separator();
                // UI zoom: -, current %, + (middle resets to 100%).
                let zoom = ctx.zoom_factor();
                if ui.button("-").on_hover_text("Zoom out").clicked() {
                    ctx.set_zoom_factor((zoom - 0.1).max(0.5));
                }
                if ui
                    .button(format!("{}%", (zoom * 100.0).round() as i32))
                    .on_hover_text("Reset zoom to 100%")
                    .clicked()
                {
                    ctx.set_zoom_factor(1.0);
                }
                if ui.button("+").on_hover_text("Zoom in").clicked() {
                    ctx.set_zoom_factor((zoom + 0.1).min(2.0));
                }
            });

            // Output device picker: pick where the EQ runs and apply it there.
            ui.horizontal(|ui| {
                let busy = self.apply_rx.is_some();
                ui.label("Output:");
                let selected = self
                    .devices
                    .get(self.selected_device)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| "—".to_string());
                egui::ComboBox::from_id_salt("device_picker")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (i, d) in self.devices.iter().enumerate() {
                            let label = if d.is_default {
                                format!("{}  (default)", d.name)
                            } else {
                                d.name.clone()
                            };
                            ui.selectable_value(&mut self.selected_device, i, label);
                        }
                    });
                if ui
                    .add_enabled(!busy, egui::Button::new("Apply EQ here"))
                    .on_hover_text("Install the APO onto this output (elevated)")
                    .clicked()
                {
                    self.start_apply();
                }
                if ui.add_enabled(!busy, egui::Button::new("⟳")).on_hover_text("Refresh device list").clicked() {
                    self.devices = devices::list().unwrap_or_default();
                    self.selected_device =
                        self.selected_device.min(self.devices.len().saturating_sub(1));
                }
                if busy {
                    ui.spinner();
                }
                if let Some((ok, msg)) = &self.device_status {
                    let color = if *ok {
                        egui::Color32::from_rgb(140, 220, 140)
                    } else {
                        egui::Color32::LIGHT_RED
                    };
                    ui.colored_label(color, msg);
                }
            });
        });

        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let hint = "drag = gain  ·  shift-drag / scroll = Q  ·  ctrl-drag = frequency  ·  dbl-click = reset band  ·  right-click = type";
                ui.label(hint);
                if let Some(e) = &self.last_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("· {e}"));
                }
                let (rms, peak) = self.viz.level();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    out_meter(ui, rms, peak);
                    ui.label(egui::RichText::new("OUT").color(theme::muted()).small());
                });
            });
        });

        // Mutate Copy locals inside the panel closure, then write back + commit
        // after it closes (avoids borrowing self inside nested egui closures).
        let mut preamp = self.preamp_db;
        let mut d = self.dynamics;
        let mut changed = false;
        egui::SidePanel::right("chain")
            .resizable(false)
            .default_width(264.0)
            .show(ctx, |ui| {
                let comp_acc = theme::cyan();
                let lim_acc = egui::Color32::from_rgb(255, 120, 120);
                let agc_acc = theme::ok();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("SIGNAL CHAIN").color(theme::cyan()).strong());
                    ui.separator();

                    // Preamp
                    ui.horizontal(|ui| {
                        ui.label("Preamp");
                        if ui.button("↺").on_hover_text("reset preamp").clicked() {
                            preamp = 0.0;
                            changed = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        changed |= knob::knob(ui, &mut preamp, -24.0..=24.0, "gain", " dB", 1, false, theme::cyan());
                    });
                    ui.separator();

                    // Compressor
                    let mut comp_on = d.comp_enabled != 0;
                    if ui.checkbox(&mut comp_on, "Compressor").changed() {
                        d.comp_enabled = comp_on as u32;
                        changed = true;
                    }
                    ui.horizontal_wrapped(|ui| {
                        for name in presets::COMP_PRESETS {
                            if ui.button(name).clicked() { presets::apply_comp(&mut d, name); changed = true; }
                        }
                        if ui.button("↺").on_hover_text("reset compressor").clicked() { presets::reset_comp(&mut d); changed = true; }
                    });
                    comp_on = d.comp_enabled != 0;
                    ui.add_enabled_ui(comp_on, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            changed |= knob::knob(ui, &mut d.comp_threshold_db, -60.0..=0.0, "thresh", " dB", 0, false, comp_acc);
                            changed |= knob::knob(ui, &mut d.comp_ratio, 1.0..=20.0, "ratio", ":1", 1, false, comp_acc);
                            changed |= knob::knob(ui, &mut d.comp_attack_ms, 0.1..=200.0, "attack", " ms", 1, true, comp_acc);
                            changed |= knob::knob(ui, &mut d.comp_release_ms, 10.0..=1000.0, "release", " ms", 0, true, comp_acc);
                            changed |= knob::knob(ui, &mut d.comp_makeup_db, 0.0..=24.0, "makeup", " dB", 1, false, comp_acc);
                        });
                    });
                    ui.separator();

                    // Limiter
                    let mut lim_on = d.limiter_enabled != 0;
                    if ui.checkbox(&mut lim_on, "Limiter").changed() {
                        d.limiter_enabled = lim_on as u32;
                        changed = true;
                    }
                    ui.horizontal_wrapped(|ui| {
                        for name in presets::LIMITER_PRESETS {
                            if ui.button(name).clicked() { presets::apply_limiter(&mut d, name); changed = true; }
                        }
                        if ui.button("↺").on_hover_text("reset limiter").clicked() { presets::reset_limiter(&mut d); changed = true; }
                    });
                    lim_on = d.limiter_enabled != 0;
                    ui.add_enabled_ui(lim_on, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            changed |= knob::knob(ui, &mut d.limiter_ceiling_db, -12.0..=0.0, "ceiling", " dB", 1, false, lim_acc);
                        });
                    });
                    ui.separator();

                    // Auto-loudness
                    let mut agc_on = d.agc_enabled != 0;
                    if ui.checkbox(&mut agc_on, "Auto-loudness").changed() {
                        d.agc_enabled = agc_on as u32;
                        changed = true;
                    }
                    ui.horizontal_wrapped(|ui| {
                        for name in presets::AGC_PRESETS {
                            if ui.button(name).clicked() { presets::apply_agc(&mut d, name); changed = true; }
                        }
                        if ui.button("↺").on_hover_text("reset auto-loudness").clicked() { presets::reset_agc(&mut d); changed = true; }
                    });
                    agc_on = d.agc_enabled != 0;
                    ui.add_enabled_ui(agc_on, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            changed |= knob::knob(ui, &mut d.agc_target_db, -30.0..=-6.0, "target", " dB", 0, false, agc_acc);
                            changed |= knob::knob(ui, &mut d.agc_max_gain_db, 0.0..=36.0, "max gain", " dB", 0, false, agc_acc);
                        });
                    });
                    ui.add_space(8.0);
                });
            });
        if changed {
            self.preamp_db = preamp;
            self.dynamics = d;
            self.commit();
        }

        let wave = self.viz.snapshot();
        let spectrum = self.viz.spectrum();
        let stereo = self.viz.stereo();
        let (rms, _peak) = self.viz.level();

        // Peak-hold caps: instant rise, slow fall.
        if self.spec_peaks.len() != spectrum.len() {
            self.spec_peaks = spectrum.clone();
        } else {
            for (p, &s) in self.spec_peaks.iter_mut().zip(spectrum.iter()) {
                *p = if s >= *p { s } else { (*p - 0.012).max(s) };
            }
        }

        // Loudness history ring (~8s at 60fps).
        self.loud_hist.push(rms);
        if self.loud_hist.len() > 480 {
            let excess = self.loud_hist.len() - 480;
            self.loud_hist.drain(0..excess);
        }

        if self.layers.waterfall {
            self.spectro.push(&spectrum);
        }
        let spectro_tex = if self.layers.waterfall {
            self.spectro.texture(ctx, self.rainbow)
        } else {
            None
        };

        // Clone the persistent histories into locals so VizData borrows locals,
        // not self (the panel closure needs &mut self for commit()).
        let peaks_local = self.spec_peaks.clone();
        let loud_local = self.loud_hist.clone();
        let viz = curve::VizData {
            layers: self.layers,
            spectrum: &spectrum,
            peaks: &peaks_local,
            waveform: &wave,
            stereo: &stereo,
            loudness: &loud_local,
            spectro_tex,
        };
        egui::CentralPanel::default().show(ctx, |ui| {
            let response = curve::draw(
                ui,
                &mut self.bands,
                self.sample_rate,
                self.rainbow,
                &viz,
            );
            if response.changed {
                self.commit();
            }
        });

        about::show(ctx, &mut self.show_about, &mut self.about_icon);

        // Repaint fast (~60fps) when visible; drop to a slow idle tick when hidden
        // to tray so we don't burn a core 24/7 with no window on screen.
        let interval = if self.visible.load(Ordering::Relaxed) { 16 } else { 1000 };
        ctx.request_repaint_after(std::time::Duration::from_millis(interval));
    }
}

/// Compact horizontal output meter: RMS fill (green/yellow/red by level) + peak tick.
fn out_meter(ui: &mut egui::Ui, rms: f32, peak: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(130.0, 12.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, theme::BG);
    let db = |v: f32| 20.0 * v.max(1e-5).log10();
    let norm = |d: f32| ((d + 54.0) / 54.0).clamp(0.0, 1.0);
    let rt = norm(db(rms));
    let pt = norm(db(peak));
    let col = if pt > norm(-3.0) {
        egui::Color32::from_rgb(255, 90, 90)
    } else if pt > norm(-12.0) {
        egui::Color32::from_rgb(255, 210, 90)
    } else {
        theme::OK
    };
    if rt > 0.0 {
        let fill =
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * rt, rect.height()));
        p.rect_filled(fill, 3.0, col.gamma_multiply(0.85));
    }
    let px = rect.left() + rect.width() * pt;
    p.line_segment(
        [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
        egui::Stroke::new(2.0, egui::Color32::WHITE),
    );
}

