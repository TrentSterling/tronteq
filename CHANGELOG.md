# Changelog

## [0.3.0] - 2026-07-11

The UI refactor: saved profiles, tabbed inspector, true light mode, persistence.

- **Sound profiles**: full audible chain (8 bands + preamp + compressor/limiter/
  auto-loudness incl. on/off flags) saved as JSON per profile in
  `C:\ProgramData\TrontEq\profiles\`. Toolbar chips: click = load, right-click =
  overwrite / rename / delete, `*` = tweaked since save, `+` = save-as, number
  keys 1-9 quickswap. Factory presets became ordinary seed profiles.
- **settings.json**: theme, viz layers, rainbow, zoom, active profile, inspector
  tab all persist across restarts (previously reset every launch).
- **Tabbed inspector** (right panel): CHAIN (signal chain), VIZ (ANALYZE layer
  toggles with full names; SHOW section reserved for eye-candy modes), SETUP
  (output device install flow, zoom, reset, profiles folder, status readouts).
- **Single-row toolbar**: wordmark, profile chips, Bypass, Light/Dark, About,
  plus an "! EQ silent" warning chip when APO telemetry is quiet.
- **True light-mode canvas**: viz background, grid, tooltips, meters, waterfall
  all follow the theme (was: fixed dark scope in both modes).
- Peak-hold caps draw standalone (Peak toggle was dead without Spec on).
- `IN>OUT` meter label: replaced a U+2192 arrow that rendered as a tofu box.
- shared: serde on `Band`/`Dynamics` (forward-compatible profiles), ABI untouched.

## [0.2.0] - 2026-06 (retroactive)

The "full signal chain + viz" era, previously unchangelogged (S56-S59):

- Chain became Preamp -> EQ -> AGC -> Compressor -> Limiter (IPC 144 -> 192 B);
  8 filter types; custom rotary knobs; per-component presets.
- Viz suite: FFT spectrum bars, peak-hold, analyzer line, spectrogram waterfall,
  waveform, stereo goniometer, loudness history — stackable layers. Rajdhani font,
  rainbow theme, light/dark chrome, About, tray, autostart, UI zoom.
- APO telemetry block (IPC 192 -> 216 B): true in/out VU + gain-reduction readout.
- Renderer swapped wgpu/DX12 -> glow/OpenGL after DX12 device-loss crashes +
  a staging-buffer leak (3.7 GB -> ~178 MB steady). Self-heal relaunch on panic,
  crash.log, WASAPI loopback auto-reacquire.

## [0.1.0] - 2026-04-17

Initial POC. Drag an 8-band parametric curve, hear it on any Windows output device, zero added latency.

- Own APO (`TrontEqApo.dll`, C++ COM) loaded into `audiodg.exe`
- Installer CLI (`tronteq-cli`) with `check`, `list-devices`, `install --device`, `uninstall`
- GUI (`tronteq`, eframe + egui 0.33) with draggable curve
- Shared state via file-backed memory map at `C:\ProgramData\TrontEq\state.bin`
- RBJ biquad cascade (peak / low-shelf / high-shelf), Direct Form II Transposed, FTZ+DAZ
- Seqlock IPC protocol (no locks in the audio thread)
