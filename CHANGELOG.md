# Changelog

## [0.10.1] - 2026-07-24

- Discord-style background gradient behind the chrome, derived live from the
  palette (accent hue wash, deeper shifted corner), with a Gradient toggle in
  SETUP. The EQ canvas and OUT meter stay solid dark - scope look is law.
- Caption overlay fill pinned to the solid theme panel color so toolbar
  content can't ghost through the window buttons now that panels are
  translucent under the gradient.

## [0.10.0] - 2026-07-23

THE VIZ MEGAPASS. 13 GL modes became 33, plus a stackable post-FX layer.

- **20 new shader modes** (winamp/milkdrop/shadertoy energy): Matrix rain, Scope
  ring, Sky (the VR SimpleClouds port — clouds thicken on bass, sun flares on
  beats), Aurora, Outrun, Spectrum city, Wormhole, Spirograph, Laser show,
  Disco ball, Hex pulse, Lightning, DNA helix, Bubbles, Copper bars, LED wall,
  Sonar, Pulsar, Black hole, Moonlit ocean. Every one beat-reactive, every one
  AI-vision-verified live before shipping.
- **3 reworks:** Starfield is a real layered warp field now (hundreds of stars,
  hyperspace lurch on kicks), Flame got fierce (licking tongues, embers on
  beats), Julia breathes through a slow deep-zoom cycle with smooth (banding-
  free) iteration coloring.
- **Post-FX overlay stack** — mix-and-match bits over ANY shader mode, feedback
  loop stays clean: Mirror, Zoom blur, Chromatic aberration, Pixelate,
  Halftone, CRT scanlines, Film grain + vignette, Strobe-on-downbeat, Edge
  glow, Thermal. Toggle chips live in VIZ > FX.
- **Every VizBus signal now reaches the shaders:** BPM + confidence, spectral
  flux, momentary loudness, crest, stereo correlation (gonio) + width join
  bass/mid/treble/pulse/beat-phase/centroid as uniforms for all 33 modes.
- **Shuffle** toggle auto-advances through GL modes every ~18s, plus a Random
  button. VIZ-IDEAS.md added: the 150-idea master palette, tracked.
- Custom window chrome: the toolbar is the title bar now (frameless window,
  painter-drawn min/max/close, drag anywhere on the empty strip, edge resize).
  Same treatment as TrontSnap/Boxel.

## [0.9.0] - 2026-07-23

- **The GUI is no longer elevated.** Manifest went requireAdministrator ->
  asInvoker, so TrontEQ now runs as a normal Medium-integrity window. Why it
  matters: Windows refuses to deliver modifier-less global hotkeys registered
  by Medium apps while an elevated window has focus — bare PrintScreen in
  TrontSnap (and ShareX!) went dead whenever TrontEQ was focused. Elevation
  also UIPI-broke drag/drop onto the window and focus handoff from normal apps.
  The GUI never needed admin for its actual job (it drives the APO through the
  state.bin memory map); only installs did.
- "Apply EQ here" now launches tronteq-cli elevated on demand (one UAC prompt
  per device retarget — rare). Output comes back via
  C:\ProgramData\TrontEq\install.log; declining the prompt is a clean error.
- install now grants BUILTIN\Users Modify on C:\ProgramData\TrontEq so the
  Medium GUI can keep writing state.bin / settings.json / profiles.
- The TrontEQ logon task registers at LIMITED run level (was HIGHEST).

## [0.8.0] - 2026-07-21

- **A/V-sync delay.** A new amber knob at the top of the CHAIN tab (0 to 2000 ms)
  plus -25/-5/+5/+25 ms fine trims and a status-bar DELAY readout. For when a
  video's audio runs ahead of the picture: open TrontEQ over it and dial in a
  delay live until the lips line up. delay = 0 keeps the zero-latency path
  byte-for-byte identical, so it's a per-session opt-in, applied as a ring
  buffer in the APO after the whole chain. Skipped while bypassed.
- IPC grew 216 to 224 bytes (delay_ms at offset 192 + reserved pad). Old
  state.bin files migrate to delay 0, and the APO refuses to trust delay bytes
  from a pre-224 file, so a phantom delay can't sneak in during the DLL swap.
- **Portable dev-mode.** bootstrap.ps1 + SETUP.md + a /setup Claude skill +
  make-dist.ps1, so TrontEQ drops onto a fresh machine (prebuilt path needs no
  Rust/VS/SDK) and comes up with a single reboot. Build/run/autostart code
  paths de-hardcoded off C:\trontstack\tronteq.

## [0.7.3] - 2026-07-12

- Julia is alive: ~3.5x faster c-orbit (the shape morph), a mid-frequency
  shimmer epicycle, bass pushes the c-point directly, slow plane rotation,
  bass-breathing zoom, palette cycles with time + beat phase. Verified by
  two harness frames 3s apart showing entirely different fractal forms.

## [0.7.2] - 2026-07-12

The mouse-position fps mystery, dissected and killed:

- egui repaints reactively on pointer events (mouse motion = bonus frames),
  the old focus throttle dropped unfocused-but-visible to 20fps (wrong for a
  visualizer you watch while working), and Sleep(16) without raised timer
  resolution rounds to ~31ms. Net effect: smooth while hovering, janky when
  still. All three fixed.
- Heartbeat now: timeBeginPeriod(1) + absolute-schedule 16.6ms ticks with
  stall resync; VISIBLE = full rate regardless of focus; tray = 2fps.
- Delivered-FPS counter in the bottom bar + DATA ENGINE (cadence is a number).
  Verified by self-capture: 120 fps, mouse untouched, window unfocused
  (vsync-locked to the high-refresh panel; dt-based sim keeps speeds correct).

## [0.7.1] - 2026-07-12

Three shader fixes, all caught by the self-capture verify pipe (AI vision
review), none by the user: Terrain (camera spawned inside the terrain — flat
purple wash; now flies above a lower landscape with a finer 64-step march),
Starfield (feedback accumulation blew out white; decay + bias-down + dimmer
stars), Julia (non-escaping interior rendered max-bright; now near-black with
a glowing boundary).

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
