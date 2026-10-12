# Unified visualizer, September 24, 2026

The old VIZ panel exposed its implementation: analyzer checkboxes, four painter
scenes, a separate list of 33 shaders, and a post-effect bitmask that only affected
the shaders. Several effects silently replaced earlier effects in that bitmask.

## What changed

- VIZ has focused Scenes, Style, Effects, and Meters pages instead of one long
  column of unrelated controls. Compact layouts keep the page navigation visible.
- One searchable library of 37 scenes, grouped into Atmosphere, Space, Geometry,
  Audio, Retro, and Motion. Stable saved scene IDs are preserved.
- Favorites, a Next button, and shuffle with a configurable interval and an
  explicit choice between all scenes and favorites.
- EQ view and Stage view. Stage hides the grid, curve, nodes, and analyzer overlays
  together. Switching back restores the editor and existing overlay preferences.
- A Stage shortcut in the main toolbar, fullscreen via F11 or Full, and Escape to
  leave fullscreen. The inspector can be resized.
- Common speed, audio response, brightness, and Spectrum/Theme color controls.
  Visual colors are independent of the EQ curve's rainbow preference.
- An ordered effect list. Add an effect, enable/disable it, adjust its mix, move
  it earlier in the stack, or remove it. Effects process the scene; measurement
  overlays and the editor remain readable above them.
- Bars XL, Scope trails, Tunnel, and Particles now render through the same GPU
  stage as the other scenes. Their painter implementations remain as fallbacks.
- Every scene uses the same fixed simulation clock, including feedback. Speed 0
  holds the scene; speed 2 advances it twice as quickly. Rendering stalls discard
  excess elapsed time rather than replaying an unbounded backlog.
- Scene switches reset feedback instead of inheriting the previous scene's image.
- Frequency labels now include 50, 100, 200, 500, 1k, 2k, 5k, and 10k. The old
  octave loop could never reach most of the frequencies it intended to label.

## Effect correction

The previous post shader ran every selected effect against the same source
texture. Aberration replaced zoom blur; pixelation replaced aberration; scanlines
replaced halftone. A GPU probe confirmed that adding the earlier effect changed
**zero pixels** in each of those combinations.

The renderer now uses one pass per active effect and alternates two post targets.
Every pass reads its predecessor. Scene feedback stays separate from post effects.
The same probes now produce different output when both effects are enabled.

## Persistence

Settings schema 2 stores the visual controls, effect order/mixes, favorites, and
shuffle interval. Existing settings without this object migrate the previous
effect bitmask and rainbow preference. Sound profiles, audio parameters, output
attachment, and the APO binary are outside this change.

The settings file and previous GUI executable were backed up under `target/`
before replacing the GUI. The launch flag `--visualizer` opens the VIZ tab.

## Verification

```powershell
cargo test --workspace --offline
cargo run --offline --manifest-path tools/audit-probe/Cargo.toml --target-dir target
cargo run --offline -p tronteq-uitest
```

The first unification pass passed 40 GUI tests, 4 CLI tests, 9 shared tests, and the
UI harness's 29 reused color/theme tests. New tests cover settings migration,
effect enable state, shuffle scope, scene selection, favorites, Stage view, and
adding/disabling an effect through the actual UI. A compact-layout test checks
that scene controls and page navigation remain inside a 310-point-wide panel.

The native GPU probe uses the production renderer and an offscreen framebuffer:

- 37/37 scene shaders compile, link, and render nonempty frames without GL errors.
- Three previously broken effect combinations retain the earlier effect.
- Changing effect order changes the result; 0% mix preserves the source.
- Warp, Scope trails, and Terrain produce identical frames at 20, 60, and 144 fps
  for equal simulated time. Speed 0 and 2 are checked against reference runs.
- Dark, compact, Paper, compact Effects, and compact Style previews render the real panel and GL canvas to
  `target/viz-review-*.png`. No installed settings are loaded by this probe.

The existing UI screenshot harness also completed all 18 scenes. A screenshot
of the running GUI confirmed the VIZ tab opens correctly and the live audio
telemetry still advances. The sound-control bytes matched the pre-update backup.

## Next useful additions

Named visual looks and fullscreen presentation are now implemented and verified
in [the visual looks follow-up](2026-09-24-visual-looks.md).

The [scene gallery follow-up](2026-09-24-scene-gallery.md) adds thumbnail browsing
and a compact sound-profile toolbar.

Scene transitions, resolution controls, and measured GPU timing are implemented
in [the playback follow-up](2026-09-24-playback.md). Analyzer freeze/reference
traces and distinct measurement axes remain useful additions.

Further audio and installer audit findings are recorded separately so they do not
hold up visual work on the working test-mode installation.
