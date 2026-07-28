# Changelog

## [0.12.4] - 2026-07-27

Two findings from an adversarial review, both in the window-state machine.

- **A restore the app did not perform left the window permanently blank.**
  `tray_hidden` was documented as "only the close button sets it, only the tray
  handlers clear it, nothing else may touch it". That premise is false on Win32:
  `hide_window` sets WS_EX_TOOLWINDOW and minimizes, it never clears WS_VISIBLE,
  and WS_EX_TOOLWINDOW governs shell presentation rather than the window APIs. A
  lingering taskbar button, an Alt-Tab entry, or any same-integrity process that
  finds the HWND and calls `ShowWindow(SW_RESTORE)` (a window tiler, for
  instance) restores it without running any of the three clearing sites. The
  window was then genuinely on screen and focused while `presentable` still
  returned false, so `update` returned before building a single widget; with
  `decorations(false)` there is no native caption either, so the result was a
  fully visible window containing nothing, recoverable only via the tray icon.
  `presentable` now reconciles the latch against `is_on_screen` first: if the OS
  says we are on screen, we are not tray'd, whatever the latch believes.
- **`good_size` was seeded in the wrong unit space.** It is POINTS (what
  `ViewportCommand::InnerSize` takes) but was initialised to the literal
  `1000x460` "matching with_inner_size", which is LOGICAL pixels applied at
  window creation. Those agree only at zoom 1.0, so with a saved zoom of 1.25 the
  seed was 25% wrong and a heal firing before the first sane frame would resize
  the window to a size the user never chose. It is now `Option`, learned from the
  first sane frame and never guessed.

## [0.12.3] - 2026-07-27

The window would not drag small. Reported as "min size is quite large"; it was
two things, and the second was a failsafe fighting the user.

- **`min_inner_size` lowered from 800x320 to 640x300** (logical px), now via
  named `MIN_INNER_W` / `MIN_INNER_H` constants so the OS floor and the
  collapsed-window failsafe cannot drift apart again.
- **`heal_tiny_window` was comparing the wrong units.** `content_rect()` is in
  egui POINTS (physical / (dpi_scale * zoom_factor)); `min_inner_size` is applied
  once at creation in LOGICAL pixels (physical / dpi_scale) and never re-applied
  when zoom changes. They agree only at zoom 1.0. At 110% DPI with a saved zoom
  of 1.1 the OS allowed an 880px window, which reads as 727 points, which the
  hardcoded 790 threshold called "desynced" and snapped back. The failsafe was
  contesting a ~76px band the OS had already permitted, and because `good_size`
  is stored in points it snapped BIGGER the further you zoomed in. It now
  converts points to logical via `zoom_factor()` before comparing, so the two
  agree at any zoom.

## [0.12.2] - 2026-07-26

TrontEQ was burning a full CPU core around the clock. Reported as "why is this
using 4-5% when the window isn't even open", measured at 643 minutes of CPU over
14.7 hours of uptime. Two separate causes, neither of them the audio engine.

- **The tray was the expensive part.** Hiding to tray used `SW_HIDE`. winit
  never delivers a redraw to a hidden window, and an outstanding repaint request
  keeps its event loop in `Poll` rather than `Wait`, so the main thread spun at
  100% of a core servicing a request that could never be answered. It was not
  rendering anything; per-thread sampling put 98.6% on the main thread and 0.5%
  on the audio capture. The tray state is now **minimized + `WS_EX_TOOLWINDOW`**,
  which keeps it out of the taskbar and out of alt-tab while leaving winit in the
  one state where it genuinely idles. Tray'd cost went from 99% of a core to 0%.
- **The idle pulse aimed at a window that could not receive it.** The heartbeat
  thread pulsed `request_repaint()` every 500ms regardless of window state, which
  is what kept the request outstanding. It now pulses only when Win32 says the
  window is on screen. Reading the OS rather than our own flags is deliberate:
  the second-launch path restores the window from a thread with no egui context,
  so polling real state is what makes every wake route work.
- **The window could collapse to 15x15 and keep rendering.** After a tray round
  trip `SW_RESTORE` could bring the window back far below its minimum inner size,
  parked in the corner where it read as "not open" while still costing a full
  core. `heal_tiny_window` (ported from TrontSnap, which hit this first) restores
  the last sane size after four consecutive undersized frames.
- **Minimize and virtual desktops now idle too.** `visible` used to mean only
  "the close button has not hidden us", so a minimized window still ran at a
  locked 60fps. Presentability is now derived every frame from tray state,
  minimized state, `DWMWA_CLOAKED` (window parked on another virtual desktop) and
  a collapsed-size failsafe.
- **Fixed a livelock introduced while fixing the above:** the derived flag was
  briefly its own input, so minimizing cleared it and restoring had nothing to set
  it back, leaving the window on screen frozen at 2fps. Latched tray state and
  derived presentability are now separate values.
- Each thread logs its OS thread id at startup, which is what made the main
  thread identifiable as the spinner instead of the audio capture.

## [0.11.1] - 2026-07-25

Gradient v2 was half-reachable. Three real bugs, all found by capturing the
window and reading pixels rather than trusting that the port looked right.

- **Frost did nothing.** `set_frost` stored the value and stopped there, while
  every sibling setter (`set_palette` / `set_mode` / `set_gradient`)
  re-applies egui Visuals. Frost lives in `build_visuals`' panel alpha, so it
  was read correctly and simply never reached the screen. New
  `theme::refresh_visuals` is called on change and on Reset.
- **Frost, the Harmony/Presets/Custom chips, the peg count and every peg
  picker were unreachable.** The panel was ported from SpaceView, whose window
  is 800px tall; TrontEQ's is 460. The content needed ~750px and simply ran off
  the bottom. Fixed by making it fit AND scroll: the big color picker moved into
  a collapsed "Accent color" header, slider labels sit inline with their
  sliders, this panel uses compact row metrics, and the scrollbar is now
  non-floating and accent-colored so it is actually visible - egui's default
  floating bar fades out, which is why `ScrollBarVisibility::AlwaysVisible`
  appeared to do nothing.
- **A UTF-8 BOM in settings.json silently wiped every setting.** serde_json
  rejects a BOM with "expected value at line 1 column 1"; the whole file then
  fell back to Default and save-on-change wrote those defaults over the user's
  real config. Notepad, VS Code and PowerShell's `Set-Content -Encoding utf8`
  all produce BOMs. `load()` now trims it.
- `TRONTEQ_THEME_WINDOW=1` opens the Theme window at startup - a no-injection
  hook for screenshot-verifying this panel.

## [0.11.0] - 2026-07-24

Gradient v2 (SpaceView port): the flat 4-corner wash becomes a real
Discord-style multi-stop ramp, with a full Theme window to drive it.

- **Multi-stop ramp:** 1-4 color pegs (not 4 fixed corners), any angle
  (Direction dial), 0-100% Color Intensity, and end-hold easing so the
  outer ~12% of the ramp sits at the pure first/last peg instead of a fade.
  Painted as a 16x16 vertex grid so an arbitrary angle stays crisp.
- **Three peg sources:** Harmony (1-4 pegs derived from the live accent via
  colormagic's harmony rules), 27 curated named Presets (Galaxy Punch,
  Vaporwave, Chrome Sunset, Aurora Sky, and 23 more; cycle with `<`/`>` or
  the dropdown), or Custom (pick up to 4 colors yourself; slot 1 is always
  the live accent, linked).
- **Theme window:** opened from a new accent swatch next to the Light/Dark
  toolbar toggle. Big inline color picker, Dark/Light chips, a full premade
  combo (all 32 palettes) + Random, the gradient controls above, a
  raw-wash/frosted split preview strip, Magic (roll a colormagic flavor into
  custom pegs), and Reset (the wayback machine: known-good ramp + frost).
- **Frost knob:** panel opacity over the wash is now a slider (0-100%, per
  mode) instead of the old fixed 216-alpha constant. Dark defaults to 85%
  (matches the old look pixel-for-pixel), light gets its own 59% default
  since white bleaches color faster than dark preserves it. The EQ canvas
  and OUT meter are untouched by this knob and stay solid dark in both
  modes, same as v1.
- `Palette::from_accent` + `Palette::resolve`'s new `dark` parameter: a
  single picked accent can now be retargeted at dark or light without
  losing the pick, independent of the multi-color `from_colors` path.

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
  ring, Sky (the VR SimpleClouds port â€” clouds thicken on bass, sun flares on
  beats), Aurora, Outrun, Spectrum city, Wormhole, Spirograph, Laser show,
  Disco ball, Hex pulse, Lightning, DNA helix, Bubbles, Copper bars, LED wall,
  Sonar, Pulsar, Black hole, Moonlit ocean. Every one beat-reactive, every one
  AI-vision-verified live before shipping.
- **3 reworks:** Starfield is a real layered warp field now (hundreds of stars,
  hyperspace lurch on kicks), Flame got fierce (licking tongues, embers on
  beats), Julia breathes through a slow deep-zoom cycle with smooth (banding-
  free) iteration coloring.
- **Post-FX overlay stack** â€” mix-and-match bits over ANY shader mode, feedback
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
  by Medium apps while an elevated window has focus â€” bare PrintScreen in
  TrontSnap (and ShareX!) went dead whenever TrontEQ was focused. Elevation
  also UIPI-broke drag/drop onto the window and focus handoff from normal apps.
  The GUI never needed admin for its actual job (it drives the APO through the
  state.bin memory map); only installs did.
- "Apply EQ here" now launches tronteq-cli elevated on demand (one UAC prompt
  per device retarget â€” rare). Output comes back via
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
review), none by the user: Terrain (camera spawned inside the terrain â€” flat
purple wash; now flies above a lower landscape with a finer 64-step march),
Starfield (feedback accumulation blew out white; decay + bias-down + dimmer
stars), Julia (non-escaping interior rendered max-bright; now near-black with
a glowing boundary).

## [0.7.0] - 2026-07-11/12 (night arc, wave 3 â€” ten more FX + the fixes)

- **VIEWPORT FIX**: GL modes no longer paint over the panels/toolbar. The
  present pass now restores egui_glow's own viewport + scissor and draws a
  plain fullscreen triangle (v0.6.0 disabled scissor and reprojected the rect
  by hand â€” wrong on both counts).
- **Timestep decoupled**: shader clock is a sim clock advanced by CLAMPED dt
  (max 50ms/frame) â€” frame stalls no longer fast-forward the FX. Repaint
  pacing moved to a heartbeat thread (Boxel pattern: winit defers
  request_repaint_after deadlines; thread-side requests are immediate):
  60fps focused / 20 unfocused / 2 tray-hidden.
- **Ten new GL modes**: Plasma, Starfield (feedback streaks), Kaleido
  (feedback fold), Ray tunnel, Metaballs (band-driven blobs), Voronoi
  (cells lit by FFT bins), Nebula (fbm clouds), **Terrain** (tron wireframe
  mountains raymarched over a scrolling spectrum-HISTORY texture â€” the audio
  is literally the landscape), Ripples (beat-phase rings refracting the
  feedback), Julia (bass-orbited fractal). SHOW UI regrouped into painter +
  GL chip rows.
- **Live-capture verification** (`livecap.ps1` + `verify-gl.sh`): PrintWindow
  captures of the running app per GL mode, reviewed by AI vision before the
  user ever tests â€” closes the kittest/GL coverage gap.

## [0.6.0] - 2026-07-11 (night arc, wave 2 â€” the shader leap)

- **GL viz stage** (`glstage.rs`): real GLSL under the canvas via egui
  PaintCallback on the glow backend. Ping-pong RGBA16F feedback rendertextures
  (the Milkdrop trick: prev frame re-sampled through a warp, new content
  splatted on top), FFT buckets + waveform uploaded as R32F textures, VizBus
  stats (bass/mid/treble, pulse, beat phase, brightness) as uniforms.
- **Three shader modes** in SHOW: **Warp (GL)** â€” feedback zoom/rotate/decay
  with a beat-breathing spectrum ring; **Flame (GL)** â€” rising heat field fed
  by the FFT at the floor, noisy cooling, kick flares; **Smoke (GL)** â€” curl-
  noise advected density with buoyancy and beat bursts. Painter layers + the
  EQ curve composite on top, so everything cross-pollinates.
- GL init failure degrades gracefully (painter modes unaffected, logged).

## [0.5.1] - 2026-07-11 (night arc, wave 1)

- **VizBus** (`vizbus.rs`): the data pipes, unified â€” band energies, spectral
  centroid + flux, beat pulse, **realtime BPM** (onset autocorrelation, 60-180,
  confidence + beat phase), momentary loudness, crest, stereo corr/width. Every
  signal keeps a 4s history. Zero steady-state allocation.
- **DATA tab** (inspector): living pipe inspector â€” each signal as label +
  sparkline + value; BPM hero readout with a beat-phase blinker; ENGINE section
  exposes the profiler scopes. What wiggles in DATA is what viz can be fed.
- Versioning policy: patch bumps for incremental work; minor = real leaps.

## [0.5.0] - 2026-07-11 (evening arc)

Make-it-shine pass: dynamic themes, a real profiler, and a headless UI harness.

- **Dynamic themes**: `color.rs` (colormagic â€” TrontColors' math, vendored via
  Boxel's tested Rust port) + `theme.rs` rebuilt around a runtime Palette.
  Built-ins (Electric Cyan / Paper / Synthwave), 6 featured premades (Dracula,
  Tokyo Night, Gruvbox, Hades Fire, Deep Ocean, Arctic Aurora), and "Roll a
  random theme" â€” flavor/harmony/premade rolls through an AutoTheme deriver
  with enforced WCAG contrast, so random is always readable. Persisted.
- **Scoped profiler** (`profiler.rs`, Boxel pattern): update/panels/histories/
  canvas stopwatches, EMA + last-frame spike columns, F10 overlay. Zero
  per-frame heap. Killed the two per-frame history-Vec clones in update()
  (split borrows instead).
- **Headless UI harness** (`uitest/` crate, `cargo run -p tronteq-uitest`):
  egui_kittest + wgpu renders 19 PNG snapshots â€” canvas dark/light, every SHOW
  mode, About at 100%/200%, and a theme sheet incl. random rolls â€” reviewable
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
  waveform, stereo goniometer, loudness history â€” stackable layers. Rajdhani font,
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
