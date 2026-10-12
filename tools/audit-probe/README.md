# TrontEQ render checks

Run from the repository root:

```powershell
cargo run --offline --manifest-path tools/audit-probe/Cargo.toml --target-dir target
```

Creates a hidden native GL context, checks the production shaders/effect stack
and timing, then saves 25 UI previews under `target/viz-review-*.png`.
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

Use `--preview-only` after UI-only edits to regenerate the 25 screenshots
without repeating the shader and timing checks.

Renderer checks cover crossfade endpoints, midpoint, rapid selections, instant
switching, speed zero, Off, and equal fade timing at 20/60/144 fps. A reduced-scale
render is checked against a nonzero viewport origin and scissor rectangle.
Before/middle/after fade frames are saved as `target/transition-*.png`.
The probe also prints GPU query timings for a 1080p Nebula scene with Color split
and Edge glow at all three resolution settings. It waits for the GPU only in the
isolated measurement loop; the app reads available query results without waiting.

Use `--thumbnails-only` to bake the 37 production scenes into
`target/scene-atlas.png` and `target/scene-atlas.json`. Each scene runs for 180
fixed steps with synthetic audio. Review the atlas, then copy both files to
`gui/assets/` when changing shaders or the scene library. Stable scene IDs in
the metadata keep the UI independent of the atlas tile order. GUI tests check
that every scene has exactly one thumbnail and the image dimensions match.

To capture the actual gallery and verify Escape after launching the release GUI
with `--gallery`, run `python tools/check-presentation.py <pid> --gallery`.
This uses the installed GUI's screenshot pipe and sends Escape directly to the
verified TrontEQ window. It does not use global keyboard input or change focus.

The optional `--seqlock` argument checks persisted-state recovery using a
disposable file beside the probe executable. Both even and abandoned odd reads
must return within two seconds. The probe verifies that a read does not mutate
the file, then explicitly restores a confirmed sound and checks its controls.
A timed-out child is terminated and the run fails. This path does not open GL
or access the installed state file.
