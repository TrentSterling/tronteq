# Changelog

## [0.7.0] - 2026-07-11/12 (night arc, wave 3 — ten more FX + the fixes)

- **VIEWPORT FIX**: GL modes no longer paint over the panels/toolbar. The
  present pass now restores egui_glow's own viewport + scissor and draws a
  plain fullscreen triangle (v0.6.0 disabled scissor and reprojected the rect
  by hand — wrong on both counts).
- **Timestep decoupled**: shader clock is a sim clock advanced by CLAMPED dt
  (max 50ms/frame) — frame stalls no longer fast-forward the FX. Repaint
  pacing moved to a heartbeat thread (Boxel pattern: winit defers
  request_repaint_after deadlines; thread-side requests are immediate):
  60fps focused / 20 unfocused / 2 tray-hidden.
- **Ten new GL modes**: Plasma, Starfield (feedback streaks), Kaleido
  (feedback fold), Ray tunnel, Metaballs (band-driven blobs), Voronoi
  (cells lit by FFT bins), Nebula (fbm clouds), **Terrain** (tron wireframe
  mountains raymarched over a scrolling spectrum-HISTORY texture — the audio
  is literally the landscape), Ripples (beat-phase rings refracting the
  feedback), Julia (bass-orbited fractal). SHOW UI regrouped into painter +
  GL chip rows.
- **Live-capture verification** (`livecap.ps1` + `verify-gl.sh`): PrintWindow
  captures of the running app per GL mode, reviewed by AI vision before the
  user ever tests — closes the kittest/GL coverage gap.

## [0.6.0] - 2026-07-11 (night arc, wave 2 — the shader leap)

- **GL viz stage** (`glstage.rs`): real GLSL under the canvas via egui
  PaintCallback on the glow backend. Ping-pong RGBA16F feedback rendertextures
  (the Milkdrop trick: prev frame re-sampled through a warp, new content
  splatted on top), FFT buckets + waveform uploaded as R32F textures, VizBus
  stats (bass/mid/treble, pulse, beat phase, brightness) as uniforms.
- **Three shader modes** in SHOW: **Warp (GL)** — feedback zoom/rotate/decay
  with a beat-breathing spectrum ring; **Flame (GL)** — rising heat field fed
  by the FFT at the floor, noisy cooling, kick flares; **Smoke (GL)** — curl-
  noise advected density with buoyancy and beat bursts. Painter layers + the
  EQ curve composite on top, so everything cross-pollinates.
- GL init failure degrades gracefully (painter modes unaffected, logged).

## [0.5.1] - 2026-07-11 (night arc, wave 1)

- **VizBus** (`vizbus.rs`): the data pipes, unified — band energies, spectral
  centroid + flux, beat pulse, **realtime BPM** (onset autocorrelation, 60-180,
  confidence + beat phase), momentary loudness, crest, stereo corr/width. Every
  signal keeps a 4s history. Zero steady-state allocation.
- **DATA tab** (inspector): living pipe inspector — each signal as label +
  sparkline + value; BPM hero readout with a beat-phase blinker; ENGINE section
  exposes the profiler scopes. What wiggles in DATA is what viz can be fed.
- Versioning policy: patch bumps for incremental work; minor = real leaps.

## [0.5.0] - 2026-07-11 (evening arc)

Make-it-shine pass: dynamic themes, a real profiler, and a headless UI harness.

- **Dynamic themes**: `color.rs` (colormagic — TrontColors' math, vendored via
  Boxel's tested Rust port) + `theme.rs` rebuilt around a runtime Palette.
  Built-ins (Electric Cyan / Paper / Synthwave), 6 featured premades (Dracula,
  Tokyo Night, Gruvbox, Hades Fire, Deep Ocean, Arctic Aurora), and "Roll a
  random theme" — flavor/harmony/premade rolls through an AutoTheme deriver
  with enforced WCAG contrast, so random is always readable. Persisted.
- **Scoped profiler** (`profiler.rs`, Boxel pattern): update/panels/histories/
  canvas stopwatches, EMA + last-frame spike columns, F10 overlay. Zero
  per-frame heap. Killed the two per-frame history-Vec clones in update()
  (split borrows instead).
- **Headless UI harness** (`uitest/` crate, `cargo run -p tronteq-uitest`):
  egui_kittest + wgpu renders 19 PNG snapshots — canvas dark/light, every SHOW
  mode, About at 100%/200%, and a theme sheet incl. random rolls — reviewable
  without launching the app. Lives outside the gui crate because the admin
  manifest makes gui test binaries unrunnable. Caught its first real bug on
  run one (About's display-font family panics if fonts aren't installed).

## [0.4.0] - 2026-07-11 (afternoon arc)

- **SHOW eye-candy modes** (VIZ tab): Bars XL (mirrored WMP-style), Scope
  trails (phosphor ghosts), Tunnel (rotating radial spectrum + kick echo
  rings), Particles (spectral fountain + beat bursts). Beat-reactive via a
  bass envelope + pulse detector; mesh-batched, fixed pools, wall-clock paced.
- Live frame-cost readout in SETUP status.
- About window scrolls instead of overflowing the screen at high zoom.

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
