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
- **GUI is Medium-integrity (asInvoker)** since v0.9.0 (privilege separation: GUI at user level, tronteq-cli elevated on demand). Kill = plain `Stop-Process`. Deploy: kill -> `cargo build --release -p tronteq` -> run (no UAC for GUI, only when CLI does installs). `cargo test -p tronteq` works; `-p tronteq-shared -p tronteq-cli` also available.
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

## Live feature-goblin queue (S56, requested mid-session — do after red team + modularization)
- **Curve editor handles / at-a-glance power** (newest ask): draw each band's INDIVIDUAL response bell faintly under the composite, colored by filter type, so you SEE how notch/shelf/peak/etc. stack into the total ("how they add up... more powerful at a glance"). Add draggable Q/width handles on the hovered/active node (Illustrator/Photoshop-style control points, alt to shift-drag/scroll). Per-band value tags / mini gain bars. "make simple vis better AND complex vis better."
- **Analyzer measurement modes**: spectrum TILT (Flat / +3 / +4.5 dB-oct pink) + volume-independent NORMALIZE first; then ballistics (fast/slow/avg), channel mode (mono/LR/MS), pre-vs-post-EQ ghost (divide measured spectrum by our known EQ magnitude response — only the EQ is invertible, not the nonlinear dynamics), reference/target-curve overlay (on-ramp to AutoEq). NOTE: viz is volume- AND compressor-dependent because WASAPI loopback taps post-volume / pre-mute, downstream of the whole chain.
- **Noise gate** (requested): Trent saw AGC boost the noise floor to audible hiss during long silence; a gate BEFORE auto-loudness fixes it. (Promote from Phase C.)
- **Modularize** (requested): split the large `main.rs` + tidy per the red-team modularity proposals. Do FIRST so features land on a clean base.
- **Options surface**: a Settings/Options window as the home for the growing option set ("more options for everything").
- **Red team in flight (S56):** 3 background adversarial reviewers — APO C++ (RT-safety / seqlock read / buffers / numerics), Rust IPC + elevated installer (privilege hygiene / command injection / repr(C) layout parity / ACL scope), GUI (unsafe audio reads / threading / panic=abort panics / modularization proposal). Fold fixes before/with the modularization.

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
