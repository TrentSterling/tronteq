//! MIC tab: the input-side chain (gain -> EQ -> gate -> compressor -> limiter)
//! that the capture APO runs on one microphone. Settings live in `mic.bin`,
//! separate from the output sound, its profiles, and its undo history. Anything
//! that opens the mic in shared mode (Axela, Discord, OBS) hears the result.
//! The tab's monitor (`mic_monitor.rs`) draws the mic's live spectrum under the
//! same draggable curve as the main EQ and can play the mic back ("listen").

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use eframe::egui;
use tronteq_shared::mic::{self, MicHandle, MicSettings};
use tronteq_shared::Telemetry;

use crate::mic_monitor::MicMonitor;
use crate::{curve, devices, knob, theme};

const CURVE_HEIGHT: f32 = 180.0;

pub struct MicPanel {
    handle: Option<MicHandle>,
    open_error: Option<String>,
    write_error: Option<String>,
    settings: MicSettings,
    telemetry: Telemetry,
    next_reload: Instant,
    devices: Vec<devices::Device>,
    selected: usize,
    list_rx: Option<Receiver<Result<Vec<devices::Device>, String>>>,
    next_list: Instant,
    apply_rx: Option<Receiver<Result<String, String>>>,
    status: Option<(bool, String)>,
    monitor: Option<MicMonitor>,
    listen: bool,
    listen_db: f32,
    selected_band: usize,
    scene: crate::show::ShowState,
}

impl Default for MicPanel {
    fn default() -> Self {
        let now = Instant::now();
        MicPanel {
            handle: None,
            open_error: None,
            write_error: None,
            settings: MicSettings::default(),
            telemetry: Telemetry::default(),
            next_reload: now,
            devices: Vec::new(),
            selected: 0,
            list_rx: None,
            next_list: now,
            apply_rx: None,
            status: None,
            monitor: None,
            listen: false,
            listen_db: 0.0,
            selected_band: 0,
            scene: crate::show::ShowState::new(crate::show::ShowMode::Off),
        }
    }
}

impl MicPanel {
    /// Open mic.bin on first use, and pick up changes made elsewhere (the CLI,
    /// another TrontEQ) about once a second.
    fn sync(&mut self) {
        let now = Instant::now();
        if self.handle.is_none() && now >= self.next_reload {
            match MicHandle::open_or_init() {
                Ok(h) => { self.handle = Some(h); self.open_error = None; }
                Err(e) => self.open_error = Some(format!("{e:#}")),
            }
            self.next_reload = now + Duration::from_secs(2);
        }
        if let Some(h) = &self.handle {
            self.telemetry = h.telemetry();
            if now >= self.next_reload {
                if let Ok(s) = h.snapshot() { self.settings = s; }
                self.next_reload = now + Duration::from_secs(1);
            }
        }
    }

    fn commit(&mut self) {
        let Some(h) = &self.handle else { return };
        self.write_error = h.write(&self.settings).err().map(|e| e.to_string());
        // Don't let the periodic reload fight a knob mid-drag.
        self.next_reload = Instant::now() + Duration::from_secs(1);
    }

    fn poll_devices(&mut self) {
        if let Some(rx) = &self.list_rx {
            match rx.try_recv() {
                Ok(Ok(list)) => {
                    let previous = self.devices.get(self.selected).map(|d| d.id.clone());
                    self.selected = devices::selected_index(&list, previous.as_deref());
                    self.devices = list;
                    self.list_rx = None;
                }
                Ok(Err(e)) => { self.status = Some((false, format!("Could not list microphones: {e}"))); self.list_rx = None; }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.list_rx = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.list_rx.is_none() && self.apply_rx.is_none() && Instant::now() >= self.next_list {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || { let _ = tx.send(devices::list_mics().map_err(|e| format!("{e:#}"))); });
            self.list_rx = Some(rx);
            self.next_list = Instant::now() + Duration::from_secs(5);
        }
        if let Some(rx) = &self.apply_rx {
            let done = match rx.try_recv() {
                Ok(r) => Some(r),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("Installer stopped unexpectedly".into())),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            };
            if let Some(r) = done {
                self.status = Some(match r { Ok(m) => (true, m), Err(e) => { crate::log_line(&format!("mic install: {e}")); (false, e) } });
                self.apply_rx = None;
                self.handle = None; // the installer may have just created mic.bin
                self.next_reload = Instant::now();
            }
        }
    }

    fn start_install(&mut self) {
        let Some(dev) = self.devices.get(self.selected) else { return };
        let (id, name) = (dev.id.clone(), dev.friendly_name.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = devices::apply_mic(&id)
                .map(|_| format!("Mic stack installed on {name}. Talk to see the meters move."))
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(r);
        });
        self.apply_rx = Some(rx);
        self.status = Some((true, "Installing mic stack...".into()));
    }

    /// Keep the monitor on the selected mic.
    fn follow_selection(&mut self) {
        let id = self.devices.get(self.selected).map(|d| d.id.clone());
        if self.monitor.as_ref().map(|m| m.endpoint_id().to_string()) != id {
            self.monitor = id.map(|id| MicMonitor::start(id, self.listen, db_to_gain(self.listen_db)));
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.sync();
        self.poll_devices();
        self.follow_selection();
        let live = self.monitor.as_ref().map(|m| m.read());
        // Keep the analysis moving while the tab is open.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        let acc = theme::cyan();
        let gate_acc = theme::ok();
        let lim_acc = theme::readable_ink(egui::Color32::from_rgb(255, 120, 120));
        let warn = egui::Color32::from_rgb(255, 170, 90);

        ui.label(egui::RichText::new("MIC STACK").color(acc).strong());
        ui.label(egui::RichText::new("Cleans up one microphone for every app that records it, including Axela.")
            .color(theme::muted()).small());
        ui.separator();

        // Device + install
        let busy = self.apply_rx.is_some();
        let selected = self.devices.get(self.selected).map(|d| d.friendly_name.clone())
            .unwrap_or_else(|| "No active microphones".into());
        ui.add_enabled_ui(!busy && !self.devices.is_empty(), |ui| {
            egui::ComboBox::from_id_salt("mic_picker")
                .selected_text(selected)
                .truncate()
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for (i, d) in self.devices.iter().enumerate() {
                        let label = if d.is_default { format!("{}  (default)", d.friendly_name) } else { d.friendly_name.clone() };
                        ui.selectable_value(&mut self.selected, i, label).on_hover_text(&d.id);
                    }
                });
        });
        if ui.add_enabled(!busy && !self.devices.is_empty(), egui::Button::new("Install on this mic"))
            .on_hover_text("Attach the mic stack to this microphone (asks for administrator once)")
            .clicked()
        {
            self.start_install();
        }
        if let Some((ok, msg)) = &self.status {
            ui.colored_label(if *ok { theme::muted() } else { egui::Color32::LIGHT_RED }, msg);
        }
        if let Some(e) = &self.open_error {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("Mic settings unavailable: {e}"));
            return;
        }
        if let Some(e) = &self.write_error {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
        ui.separator();

        // Live mic (what apps hear) and listen-back
        let db = |x: f32| if x > 1e-6 { 20.0 * x.log10() } else { -120.0 };
        if let Some(snap) = &live {
            match &snap.status {
                crate::visualizer::Status::Live => {
                    let frac = ((db(snap.frame.rms) + 60.0) / 60.0).clamp(0.0, 1.0);
                    ui.horizontal(|ui| {
                        ui.label("mic ");
                        ui.add(egui::ProgressBar::new(frac).desired_width(150.0)
                            .text(format!("{:.0} dB", db(snap.frame.rms))));
                    });
                }
                crate::visualizer::Status::Error(e) => { ui.colored_label(warn, format!("Mic monitor: {e}")); }
                _ => { ui.small("Connecting to the microphone..."); }
            }
        }
        ui.horizontal(|ui| {
            if ui.checkbox(&mut self.listen, "Listen")
                .on_hover_text("Play this mic back through your current output, processing included")
                .changed()
            {
                if let Some(m) = &self.monitor { m.set_listen(self.listen); }
            }
            let resp = ui.add_enabled(self.listen,
                egui::Slider::new(&mut self.listen_db, -30.0..=12.0).suffix(" dB").text("volume"));
            if resp.changed() {
                if let Some(m) = &self.monitor { m.set_volume(db_to_gain(self.listen_db)); }
            }
        });
        if self.listen {
            ui.small("Use headphones: speakers will feed back into the mic.");
        }
        if let Some(e) = live.as_ref().and_then(|s| s.listen_error.as_ref()) {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("Listen: {e}"));
        }

        // Meters from the capture APO
        let t = self.telemetry;
        if t.seq == 0 {
            ui.colored_label(warn, "No mic audio processed yet. Install on a mic, then open it in any app.");
        } else {
            let db = |x: f32| if x > 1e-6 { 20.0 * x.log10() } else { -120.0 };
            let bar = |ui: &mut egui::Ui, label: &str, v: f32| {
                ui.horizontal(|ui| {
                    ui.label(format!("{label:<4}"));
                    let frac = ((db(v) + 60.0) / 60.0).clamp(0.0, 1.0);
                    ui.add(egui::ProgressBar::new(frac).desired_width(150.0).text(format!("{:.0} dB", db(v))));
                });
            };
            bar(ui, "in", t.in_rms);
            bar(ui, "out", t.out_rms);
            ui.small(format!("gain reduction {:.1} dB", t.gr_db));
        }
        ui.separator();

        let mut changed = false;
        let s = &mut self.settings;
        ui.horizontal(|ui| {
            let mut active = !s.bypass;
            if ui.checkbox(&mut active, "Active").on_hover_text("Off passes the mic through untouched").changed() {
                s.bypass = !active;
                changed = true;
            }
            for name in mic::PRESETS {
                if ui.button(name).on_hover_text(preset_hint(name)).clicked() {
                    if let Some(p) = mic::preset(name) { *s = p; changed = true; }
                }
            }
        });
        ui.add_enabled_ui(!s.bypass, |ui| {

            // The main EQ's curve, over the mic's live spectrum.
            let (spectrum, peaks, rate) = match &live {
                Some(l) if l.status == crate::visualizer::Status::Live =>
                    (&l.frame.spectrum[..], &l.frame.peaks[..], l.rate as f32),
                _ => (&[][..], &[][..], 48_000.0),
            };
            let viz = curve::VizData { layers: curve::Layers::default(), spectrum, peaks,
                waveform: &[], stereo: &[], loudness: &[], spectro_tex: None };
            let width = ui.available_width();
            ui.allocate_ui(egui::vec2(width, CURVE_HEIGHT), |ui| {
                ui.set_min_size(egui::vec2(width, CURVE_HEIGHT));
                let r = curve::draw_with_id(ui, &mut s.bands, self.selected_band, rate, false, &viz,
                    &mut self.scene, None, crate::glstage::Uniforms::default(), "tronteq-mic-node");
                changed |= r.changed;
                if let Some(i) = r.selected_band { self.selected_band = i; }
            });
            changed |= crate::eq_editor::show(ui, &mut s.bands, &mut self.selected_band, &mut s.preamp_db);
            ui.separator();

            let mut gate_on = s.gate.enabled != 0;
            if ui.checkbox(&mut gate_on, "Noise gate").on_hover_text("Turns room noise down between words").changed() {
                s.gate.enabled = u32::from(gate_on);
                changed = true;
            }
            ui.add_enabled_ui(gate_on, |ui| {
                ui.horizontal_wrapped(|ui| {
                    changed |= knob::knob(ui, &mut s.gate.threshold_db, -80.0..=-10.0, "open at", " dB", 0, false, gate_acc);
                    changed |= knob::knob(ui, &mut s.gate.range_db, -80.0..=0.0, "range", " dB", 0, false, gate_acc);
                    changed |= knob::knob(ui, &mut s.gate.hold_ms, 0.0..=1000.0, "hold", " ms", 0, false, gate_acc);
                    changed |= knob::knob(ui, &mut s.gate.release_ms, 10.0..=1000.0, "release", " ms", 0, true, gate_acc);
                });
            });
            ui.separator();

            let d = &mut s.dynamics;
            let mut comp_on = d.comp_enabled != 0;
            if ui.checkbox(&mut comp_on, "Compressor").changed() {
                d.comp_enabled = u32::from(comp_on);
                changed = true;
            }
            ui.add_enabled_ui(comp_on, |ui| {
                ui.horizontal_wrapped(|ui| {
                    changed |= knob::knob(ui, &mut d.comp_threshold_db, -60.0..=0.0, "thresh", " dB", 0, false, acc);
                    changed |= knob::knob(ui, &mut d.comp_ratio, 1.0..=20.0, "ratio", ":1", 1, false, acc);
                    changed |= knob::knob(ui, &mut d.comp_makeup_db, 0.0..=24.0, "makeup", " dB", 1, false, acc);
                });
            });
            let mut lim_on = d.limiter_enabled != 0;
            if ui.checkbox(&mut lim_on, "Limiter").changed() {
                d.limiter_enabled = u32::from(lim_on);
                changed = true;
            }
            ui.add_enabled_ui(lim_on, |ui| {
                ui.horizontal_wrapped(|ui| {
                    changed |= knob::knob(ui, &mut d.limiter_ceiling_db, -12.0..=0.0, "ceiling", " dB", 1, false, lim_acc);
                });
            });
        });
        if changed {
            self.commit();
        }
    }
}

fn preset_hint(name: &str) -> &'static str {
    match name {
        "whisper" => "Speech recognition (Axela): low cut, slow soft gate, no compression",
        "voice" => "Listeners: tone shaping, soft gate, light compression",
        "broadcast" => "Streaming and calls: firmer gate and compression",
        "clean" => "Low cut and limiter only",
        _ => "",
    }
}

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
