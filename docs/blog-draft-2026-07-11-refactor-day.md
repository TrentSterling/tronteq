# TrontEQ Refactor Day — profiles, tabs, a real light mode (draft)

*Raw beats for a future blog post / trontstack journal entry. Written live during the session, 2026-07-11.*

## The setup

TrontEQ v0.2 worked. Custom APO living inside audiodg, zero added latency, full signal chain (preamp, 8-band EQ, compressor, limiter, auto-loudness), rainbow spectrum + waterfall + goniometer, real in/out VU from APO telemetry. Community verdict: "winamp era UI and i love it."

But: the toolbar was 2 rows and ~25 controls deep. Light theme skipped the entire viz canvas (deliberate "scope look" decision that stopped feeling deliberate). And the big one — no saved profiles. I always adjust from V-shape into some super-V and lose it on the next preset click. Presets are good but saved profiles are better.

Gave the AI exactly one hour (then two, quota runs out tomorrow, gotta blast). Max effort Fable, phased plan, launch-verify every phase because of the Extraction Curse (see below).

## The Extraction Curse, debunked

Last month, extracting the toolbar + chain panel out of main.rs compiled clean, passed a static red-team, and instantly crashed on launch. Reverted, wrote it off as "never trust an egui refactor you haven't launched."

Today's autopsy: both extraction diffs were verbatim moves. No bug in them. The crash predated crash.log (couldn't diagnose, panic=abort + windows subsystem = invisible death) AND the app was still on the wgpu/DX12 renderer at the time — the same renderer later proven to crash the app on its own (staging-buffer alloc failure, device-loss on RTX 50-series), and since ripped out entirely for OpenGL/glow.

The refactor was innocent the whole time. The renderer did it.

## What shipped today

- **Sound profiles**: the full audible chain (8 bands + preamp + comp/limiter/AGC) saved as JSON, one file per profile, in ProgramData. Toolbar chips: click = load, right-click = overwrite/rename/delete, `*` when you've tweaked away from the saved version, keys 1-9 quickswap. Factory presets (Flat/Bass/Vocal/V-shape/Treble/Tinny) got demoted to ordinary seed profiles — edit them, delete them, whatever. Profile == sound save data. Nothing else rides along.
- **settings.json**: theme, viz layers, rainbow, zoom, active profile — all finally persist across restarts. (Previously: reset every launch. Nobody noticed because the app never gets closed, it lives in the tray.)
- **Inspector tabs**: CHAIN / VIZ / SETUP in the right panel. Toolbar dropped from 2 rows to 1. Device picker + zoom + resets moved into SETUP. Viz toggles got full names in VIZ.
- **True light canvas**: the viz canvas, grid, tooltips, meters, waterfall all follow the theme now. Pale scope, dark ink, recolored rainbow.
- **Peak-hold fix**: peak caps used to silently require spectrum bars to be on. Now they draw standalone.

## Viz roadmap that came out of the chat

Two buckets, decided today:
- **ANALYZE** (data viz): the current layers + queued stats — BPM detection, LUFS, crest factor, stereo correlation. Siblings of the loudness meter.
- **SHOW** (pretty viz): winamp / windows media player energy. Mirrored bars XL, phosphor scope trails, radial spectrum tunnel, maybe milkdrop-style feedback someday. Mutually exclusive full-canvas modes, separate from the data layers.

## Hardware sidebar (from the Discord chat)

Checked boards for the bluetooth man-in-the-middle / handheld sound driver dongle idea: most ESP32s don't have all the hardware in one package, two boards do, could roll a custom board but kicad skills not there yet. Non-ESP32 options exist. Verdict for now: too much effort when Windows dev test mode already works. ¯\_(ツ)_/¯

## Live bug reports mid-session

- "in out vu meter has a white rectangle" — the IN→OUT meter label used a real
  `→` (U+2192), which has zero coverage in Rajdhani. The tofu box strikes again
  (same gotcha as the About-tab arrow last month). Fixed to ASCII `IN>OUT`.
  Standing rule reaffirmed: egui + custom font = ASCII or proven glyphs only.
- Performance: "I generally have a good framerate its not always perfectly
  stable and Im on a BEAST RIG... I got boxel running on a mullins apu from a
  decade or more ago lmao." Perf pass queued (per-frame Vec clones, waterfall
  texture uploads, repaint timer granularity are the suspects).
- Tauri thought: "frankly Id do a tauri rewrite if it wasnt for the fact I like
  egui" — egui stays, but the itch is on record.

## Stray quotes

- "we cookin more trontEQ even though it will never release (might port to linux tho)"
- "presets are good but saved profiles are better"
- "I love the EQ software more than boxel tbh"

## The receipts

Four commits, one afternoon:

- `6f96998` — baseline checkpoint (turns out the entire glow-renderer fix + vumeter
  telemetry work from June was never committed; refactor day started by rescuing it)
- `c247b59` — profiles + settings persistence
- `ec24668` — tabbed inspector + single-row toolbar + peak fix + tofu fix
- `49b7f23` — true light canvas + v0.3.0

Every phase compiled clean with zero warnings on the first check. The launch
verify went through dev-cycle.ps1 (a self-elevating kill/build/relaunch/verify
script — the exe is locked while running, so the build has to happen inside the
elevated window). Biggest human-factors discovery of the day: UAC prompts expire
after two minutes, and a dev deep in Discord will out-wait every single one.
chord.wav became the official "click the dialog" bat-signal.

Launch verdict: v0.3.0 verified live same afternoon (crash.log showed the
factory profiles seeding — new code provably running), and the UAC saga ended
with a discovery: the self-elevating deploy script had been LOOPING (every
approval spawned a fresh prompt instead of running — "I hit yes dude, Im
telling you" was true the whole time). The fix that killed UAC forever: drive
the existing elevated scheduled task — `schtasks /end` unlocks the exe,
`cargo build`, `schtasks /run` relaunches elevated. Zero prompts, fully
autonomous deploys from that moment on.

## Arc 2, same day: SHOW modes, colormagic themes, and the eyeball-killer

The evening run went bigger:

- **SHOW modes** — the winamp-era eye candy landed: Bars XL (fat mirrored
  bars), Scope trails (phosphor ghosts), Tunnel (rotating radial spectrum with
  kick-triggered echo rings), Particles (spectral fountain + beat bursts). All
  beat-reactive off a bass envelope + pulse detector.
- **Dynamic themes** — the old "colormagic" experiment came home. TrontColors'
  color math had already been ported to Rust inside Boxel; TrontEQ vendored it
  and grew a runtime palette system: Dracula, Tokyo Night, Gruvbox, Hades Fire,
  plus a Randomize button that rolls flavor/harmony/premade palettes through an
  auto-theme deriver with WCAG contrast enforcement. Random, but always
  readable. The morning's accessor refactor made it a drop-in.
- **A real profiler** — Boxel's zero-alloc scoped stopwatch pattern, F10
  overlay, and the first genuine perf fix: two Vecs were being cloned every
  single frame just to satisfy the borrow checker. Split field borrows, gone.
- **The headless harness** — a `uitest` crate renders the UI to PNGs via
  egui_kittest: every SHOW mode, both classic themes, the About window at 100%
  and 200%, and a sheet of theme rolls. The AI reviews the screenshots with
  vision instead of asking the human to eyeball scales. It caught a real bug
  on its FIRST run (a font-family panic) and verified the About fix at 200%
  without a single manual screenshot.

Four minor versions in one day: 0.2 -> 0.5. The app has profiles, tabs, themes,
eye candy, a profiler, and a test harness it did not have at breakfast.
