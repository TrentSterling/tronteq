# Scene gallery and compact toolbar, September 24, 2026

**VIZ > Scenes > Gallery** shows previews for all 37 scenes. Search, category,
and favorites are shared with the inspector's scene list. Choosing a thumbnail
changes the scene and closes the gallery; Done, Escape, and clicking outside
return without choosing. The current scene has an accent outline. Empty results
offer Clear filters.

Thumbnails come from the actual production shaders, rendered with sample audio.
The bundled atlas loads into one texture on first use; browsing does not run
37 extra renderers. The gallery adjusts its column count and scroll area to the
window, and follows the app's dark or light theme. `--gallery` opens it at launch.

The toolbar's growing row of sound-profile chips is now one **Sound** picker.
It retains modified-state markers, reload, Save sound as, Manage active profile,
and the existing number shortcuts. Long names truncate inside a fixed allocation.
Small windows put theme and About controls in an App menu, keeping Bypass and
Stage directly accessible. The visual-look picker now also constrains long names.

On the running 1500-by-690 GUI, the toolbar occupies one aligned row and the scene
list has more vertical room. Both the gallery and its Escape return were captured
from the GUI's own final GL framebuffer.

## Verification

- `cargo test --workspace --offline`: **57 GUI + 4 CLI + 9 shared + 29 UI-harness
  tests passed, 0 failed** (`target/completion-tests.log`). New cases cover atlas
  coverage, scene selection, favorites, empty filters, Escape, the actual Gallery
  button, long sound/look names, profile selection/save, and toolbar alignment.
- Release build passed (`target/gallery-release-build.log`). Three existing GUI
  warnings remain.
- **37/37 thumbnail renders** produced nonempty frames without GL errors
  (`target/gallery-thumbnails.log`). The reviewed atlas and ID metadata are in
  `gui/assets/scene-atlas.{png,json}`.
- **15 native GL UI previews** rendered successfully (`target/gallery-preview.log`),
  including large, compact, and Paper galleries. Actual production panel, gallery,
  and profile-picker code is used; the preview toolbar remains a test shell.
- Native release captures: `target/gallery-live-open.png`,
  `target/gallery-live-return.png`, and the final aligned toolbar in
  `target/gallery-live-final.png`. The final release was left running in VIZ.
- Audio control bytes matched the immediate pre-launch backup. Telemetry advanced
  by 101 buffers over the final approximately one-second sample; bypass remained
  at the user's value of 1. No APO, endpoint, service, or registry changes.
- The current CLI enumerated five outputs and identified Beyond TV as default.
- Whitespace checks and Python helper compilation passed.

## Review against the requested overhaul

| Request | Evidence |
| --- | --- |
| Repair the empty output list and explain the inactive EQ | Native endpoint enumeration, interactive picker test, current five-device CLI result, and [the repair report](2026-09-23-output-repair.md). The approved installation previously verified nonzero pre/post levels with bypass off. |
| Audit gaps and possible additions | [Audit findings](2026-09-24-audit-followups.md) distinguish reproduced failures, source observations, and proposed features. |
| Unify visualizer scenes and controls | One 37-scene library, shared feel/color controls, common renderer and clock, and [GPU checks](2026-09-24-viz-unification.md) for effect order and equal simulated time. |
| Preserve the artwork while improving the UI | Searchable gallery, focused inspector pages, saved looks, Stage and presentation views; reviewed native previews and live captures. |
| Make smaller windows usable | Visible Shuffle, fixed-width selectors, aligned compact toolbar, resizable inspector, and compact layout tests/captures. |
| Keep the working test-mode audio installation | Control-byte comparison and advancing telemetry; this UI pass did not modify the audio installation. |

Further work remains in the audit backlog, including analyzer accuracy and
recovery from abandoned shared-state writes. Scene transitions and render-quality
controls are implemented in [the playback follow-up](2026-09-24-playback.md).
Profile error reporting, numeric band editing, A/B, and undo are implemented in
[the sound editing follow-up](2026-09-24-sound-editing.md).
