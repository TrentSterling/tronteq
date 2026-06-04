# TrontEQ Polish + Feature Roadmap (phased)

Status: APP IS FEATURE-COMPLETE + WORKING (S56). This plan is the "make it sexy + go deep"
roadmap. Trent wants: polish the hell out of it, spectrum bars + spectrogram + everything,
"sexy pass now in phases, Phase A = fastest to sexy, then hammer hard."

## Hard context (so a fresh session can execute without re-discovery)

- **Stack:** `cli/` (Rust, install/setup), `gui/` (Rust eframe + wgpu), `apo/` (C++ APO DLL),
  `shared/` (Rust IPC contract). Build APO via `apo/build.bat` (PowerShell `cmd /c`, not bash).
- **GUI files:** `gui/src/main.rs` (App + panels + tray), `curve.rs` (curve + waveform render),
  `theme.rs` (cyan theme + `hsv`), `presets.rs`, `visualizer.rs` (WASAPI loopback ring),
  `devices.rs` (picker), `state_writer.rs`, `dsp_preview.rs` (RBJ mirror for the drawn curve).
- **GUI is ELEVATED** (admin manifest via winresource in `build.rs`). Replacing the running exe
  needs an elevated `Stop-Process`. Deploy cycle: kill elevated -> `cargo build --release -p tronteq`
  -> `Start-Process` the exe (UAC). `cargo test -p tronteq` FAILS (manifest) -> use
  `-p tronteq-shared -p tronteq-cli`.
- **Visualizer already exists**: `gui/src/visualizer.rs` captures the default render endpoint via
  WASAPI loopback on a thread into a ring of decimated mono samples; GUI reads `snapshot()` and
  `curve.rs::draw_waveform` paints it. ALL new meters/spectrum/spectrogram build on this.
- **APO cannot feed data back to the GUI** (audiodg restricted token, proven). So gain-reduction /
  AGC meters can't read the APO's internal values; derive them GUI-side from loopback or defer.
- **Do not break:** native curve-drag snappiness, the `state.bin` 192-byte seqlock contract, the
  tray (double-click show / right-click menu, raw-Win32 ShowWindow), autostart, the durable install.
- See `[[tronteq]]` memory for the full working recipe + gotchas.

## UI tech decision (DONE)
Stay **egui**, push it hard (custom knobs, meters, font, layout). NOT Electron. Tauri is a possible
v2 (their stack: Device History + TrontColors are Tauri v2) but native egui keeps the curve/viz
snappy + zero rewrite. Phase A = egui glow-up.

---

## PHASE A — fastest to sexy ✅ DONE (S56)
Shipped: Rajdhani HUD font (`gui/assets/Rajdhani-*.ttf`, embedded in `theme.rs`); FFT spectrum
bars (rustfft on the loopback ring in `visualizer.rs`, drawn in `curve.rs::draw_spectrum`);
spectrogram waterfall (`gui/src/spectrogram.rs`, freq->X / time->Y texture) + a "Viz:" toolbar
button cycling Spectrum / Waterfall / Waveform / Clean (`curve::Backdrop`); custom rotary knobs
(`gui/src/knob.rs`, drag/scroll/log, section accent colors) replacing every dynamics slider;
output level meter (peak+RMS from `visualizer::level()`, drawn bottom-right via `out_meter`);
chrome (SemiBold display wordmark, color-coded sections, scroll area). Community reaction:
"winamp era UI and i love it". egui-stays decision vindicated.

Goal (orig): it should *look* like pro audio gear, plus the spectrum + spectrogram Trent asked for.

1. **Real font** : embed a .ttf (Inter / Geist / a techy mono-ish display face) via
   `ctx.set_fonts(FontDefinitions)` in `theme.rs`. Single biggest "not programmery" win.
2. **FFT spectrum bars behind the curve** : add `rustfft` dep. In `visualizer.rs` keep a
   contiguous window (1024/2048, NOT over-decimated) for FFT; compute magnitude spectrum, map to
   log-frequency bins matching the curve's X axis (20 Hz..20 kHz), draw faint glowing bars in
   `curve.rs` behind the curve (theme-aware / rainbow). Peak-hold optional.
3. **Spectrogram / waterfall** : scrolling history of the FFT (a texture or a column ring); a
   toggle to swap the backdrop between waveform / spectrum bars / spectrogram.
4. **Custom rotary knobs** : painter-drawn knob widget (replaces the slider+numberbox+label rows in
   the Signal Chain panel). Drag-to-turn, value below. This is the core "audio gear" look.
5. **Output level meter** (+ peak hold) from the loopback ring. Optional: estimate compressor
   gain-reduction GUI-side (mirror the envelope on loopback) for a GR meter; defer if fiddly.
6. **Layout + chrome** : section cards with subtle depth/shadow, generous spacing, iconography, a
   small logo/wordmark, filled/hover button states, nicer combo. Make the toolbar breathe.

Phase A verify: deploy, eyeball it, confirm spectrum tracks audio + knobs drag + still snappy.

## PHASE B — workflow killers
- Global named preset save/load + import/export (serde + serde_json; `C:\ProgramData\TrontEq\presets\*.json`).
- **Per-output profiles + auto-apply on device switch** (remember settings per endpoint GUID;
  watch default-device change; auto-write EFX on the new one). Kills the "swap headphones, redo it" pain.
- Global hotkeys (bypass / cycle presets) via RegisterHotKey (pattern in trontclicker).
- Tray quick-presets (right-click tray -> pick preset, no GUI).
- A/B compare (two snapshots, toggle).

## PHASE C — DSP expansion
- **AutoEq headphone profiles** (load a target curve; AutoEq database) — transformative for headphones.
- **Crossfeed** (headphone music).
- **Noise gate** (fixes AGC boosting silence/hiss).
- De-esser (dynamic sibilance notch).
- Saturation / tube / exciter (harmonic color).
- Stereo width / mid-side.
- LUFS / loudness meter.

## PHASE D — ambitious
- Per-app EQ (per-session routing; hard).
- Match EQ (analyze a reference track -> curve).
- ESP32 audio dongle (separate moonshot `[[tronteq-esp32-dongle]]`).

## Distribution (the release blocker)
- Real code-signing certificate so it loads WITHOUT Windows test-signing mode. This is the only
  thing between the current build and handing it to friends / a public GitHub release.

## Sequencing rec
Phase A first (the sexy pass, with spectrum + spectrogram). Then B (per-output auto-apply + presets
+ hotkeys make it indispensable). Then C (AutoEq + crossfeed = headphone killer). D + signing later.
