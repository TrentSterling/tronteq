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

*(fill in: screenshots before/after, final commit list, what actually made 5pm)*
