# TrontEQ render checks

For restore regressions, run `tronteq-audit-probe --restore-only`. It exercises
the production renderer with a heavy effects stack and simulated 1-second,
1-hour, and 3-day gaps, without opening the installed app or its settings.
`--render-only` runs the full existing graphics checks without generating UI
previews. See `docs/2026-10-16-restore-lag.md` for the 0.18.1 repair receipt.

## Live music gauntlet

With music playing, run `powershell -File tools/run-viz-gauntlet.ps1`.
The opt-in gauntlet uses the production WASAPI loopback analyzer, captures eight
seconds after warmup, and replays that same passage through every scene at 60 Hz.
It leaves the installed GUI, sound controls, visual preferences, and favorites alone.

The new output directory contains an HTML review player, silent 30 fps scene
videos, four clean frames and four effect-stack frames per scene, an EQ-overlay
frame, signal data for replay, and metrics. The comparison stack is Mirror, CRT,
grain, edge glow, and zoom blur at 70% each. A matched-clock silent pass measures
how much audio changes each scene. These pixel differences and black/white ratios
are review aids, not aesthetic scores. GPU timings cover the 960x540 scene pass,
not the complete GUI or video encoder. Overlay previews use the production canvas
functions and all seven overlays; the shader background is a captured frame.

Repeat a comparison without needing the same song passage:

```powershell
powershell -File tools/run-viz-gauntlet.ps1 -Replay target/gauntlet-before/analysis.json -Scene tunnel
```

Omit `-Scene` to compare the entire roster. Each run requires a fresh output
directory. The recorded stereo and waveform samples are analyzer snapshots;
the gauntlet does not save a playable audio recording.

Run from the repository root:

```powershell
cargo run --offline --manifest-path tools/audit-probe/Cargo.toml --target-dir target
```

Creates a hidden native GL context, checks the production shaders/effect stack
and timing, then saves 46 UI previews under `target/viz-review-*.png`.
The preview uses synthetic audio and the real panel, canvas, and theme modules.
It does not open `App`, capture audio, write installed settings, or install an APO.
The preview toolbar is a small test shell; the inspector and canvas are production
components. GPU checks use exact pixel comparisons on the current machine.

Previews cover dark and Paper themes, compact Scenes/Style/Effects panels,
all five factory looks, presentation mode, three scene gallery layouts, and
expanded playback controls at full and half resolution. The numeric EQ editor
and history controls have large dark, compact, and Paper previews. Recovery
previews cover a saved sound, a compact window, and no valid backup in Paper.
The sidebar and expanded gallery share the production thumbnail browser.
Effects previews include the preserved stack while disabled, visual Undo, and
all ten effects in the Paper theme.

Use `--preview-only` after UI-only edits to regenerate the 46 screenshots
without repeating the shader and timing checks.

Renderer checks cover crossfade endpoints, midpoint, rapid selections, instant
switching, speed zero, Off, and equal fade timing at 20/60/144 fps. A reduced-scale
render is checked against a nonzero viewport origin and scissor rectangle.
Before/middle/after fade frames are saved as `target/transition-*.png`.
The probe also prints GPU query timings for a 1080p Nebula scene with Color split
and Edge glow at all three resolution settings. It waits for the GPU only in the
isolated measurement loop; the app reads available query results without waiting.

Use `--thumbnails-only` to bake the 39 production scenes into
`target/scene-atlas.png` and `target/scene-atlas.json`. Each scene runs for 180
fixed steps with synthetic audio. Review the atlas, then copy both files to
`gui/assets/` when changing shaders or the scene library. Stable scene IDs in
the metadata keep the UI independent of the atlas tile order. GUI tests check
that every scene has exactly one thumbnail and the image dimensions match.

To capture the actual gallery and verify Escape after launching the release GUI
with `--gallery`, run `python tools/check-presentation.py <pid> --gallery`.
This uses the installed GUI's screenshot pipe and sends Escape directly to the
verified TrontEQ window. It does not use global keyboard input or change focus.

Run `python tools/check-fullscreen.py <pid>` with a visible, windowed GUI and
shuffle disabled to exercise F11 entry, Left/Right scene navigation, Escape,
and F11 exit. It captures the canvas and verifies window bounds, persisted
preferences, and unchanged sound controls. Receipts and screenshots are saved
under `target/fullscreen-verification/`. Allow the GUI to finish loading settings
before starting the check, and leave its controls alone until it completes.

The optional `--seqlock` argument checks persisted-state recovery using a
disposable file beside the probe executable. Both even and abandoned odd reads
must return within two seconds. The probe verifies that a read does not mutate
the file, then explicitly restores a confirmed sound and checks its controls.
A timed-out child is terminated and the run fails. This path does not open GL
or access the installed state file.


The 0.16 performance pass uses the same production uniform constructor as the
app. Add `--performance` to `--live-gauntlet` for a second video exercising the
two decks, three-cue setlist and all five pads. Example:

```powershell
target/release/tronteq-audit-probe.exe --live-gauntlet --replay target/gauntlet-final/analysis.json --out target/performance-review-new --scene gl_flame --performance
```

New captures include the eight added signal values; old captures default those
to zero. The performance demo uses the existing bass/mid/high/beat/stereo inputs.
Render checks also cover independent deck endpoints, midpoint mixing, Take B
continuity, Freeze, framing and the explicit flash envelope. Perform and Signal
lab have dark/light/compact UI previews.


The 0.17 checks cover all five blend modes, endpoint purity and swap continuity
with unequal spectra, final pulse shaping and exact replay at 20/60/144 fps.
Patch bay, XY and recording have dark/light/compact previews. Three new scenes
bring the roster to 42. Use `--looks --look "Flame instrument"` with the live
gauntlet to audition the routed XY preset. Release export can be checked with
`tronteq.exe --render-take input.trontperf.json output.mp4`; it runs in a hidden
worker and never opens the normal application state or sound controls.

## 0.18 scene and signal pass

`--live-gauntlet --replay <analysis.json> --out <new-folder> --adaptive` runs
with Musical signal response and saves original/processed signal traces.
`--duets` auditions Flame flow, Spectral glass and Prism light with independent
live decks. Each case also saves its matched silent frame. The default gauntlet
keeps Original response for exact before/after comparisons of the 42 scenes.

The GPU probe now checks 48 scene controls, all three distortion modes in three
directions through all five blends, swap continuity, pure endpoints, zero mix,
and distortion take replay at 20/60/144 fps. Six new dark/light/compact previews
cover adaptive response and distortion, bringing the preview count to 68.
