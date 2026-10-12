# Sound editing and comparison, September 24, 2026

The CHAIN inspector now has **EQ**, **Dynamics**, and **A/V sync** pages. Sound
history and A/B controls remain visible above the page content. The EQ page
provides eight band selectors and exact frequency, filter type, gain, and Q
values. Clicking a curve node selects the corresponding fields; typing a value
commits on Enter or focus loss, and dragging adjusts it directly. Gain is disabled
for filter types that do not use it. A selected node has an additional ring.

Curve drags now pick the band at the original press position. Previously, moving
quickly past the node's hit radius before the drag threshold registered could
cause a drag to do nothing.

**Undo/Redo** keeps up to 128 session edits and groups a continuous drag into one
step. Ctrl+Z, Ctrl+Y, and Ctrl+Shift+Z operate when a text field or modal is not
using keyboard input. Sound-profile recall, band edits, preamp, dynamics, and
sound reset participate in history. File operations are separate from sound
history; Undo does not recreate or delete profile files.

**A/B** holds two independently editable sounds for the current session. Both
start with the sound loaded at launch. A > B or B > A copies the current sound
into the other slot; Undo also restores a comparison overwritten this way.
Save a sound profile to retain a comparison after closing the app. Bypass and
A/V sync delay remain independent of history, comparison, and profile recall.

Reset actions say what they affect:

- **Reset band:** default frequency, zero gain, Q 1, and peak type for that band.
- **Flat EQ:** resets all eight bands, preserving preamp and dynamics.
- **Preamp 0:** resets only preamp gain.
- **SETUP > Reset sound:** resets EQ, preamp, and dynamics; preserves A/V delay.
- **A/V sync > Reset 0:** resets the output delay separately.

Profile save, overwrite, rename, and delete return disk errors to the UI. Failed
save/rename leaves the name window open; other failures appear in the footer
with the full error on hover. The store updates in-memory entries only after
the disk operation succeeds. Names are trimmed, limited to 80 characters, and
matched without ASCII case differences. Windows reserved file names receive a
safe filename prefix.

## Verification

- `cargo test --workspace --offline`: **69 GUI + 4 CLI + 9 shared + 30 UI-harness
  tests passed, 0 failed** (`target/editing-tests.log`). Tests exercise exact typed
  frequency commit, selected-band reset/type changes, fast curve drags, history
  grouping, independent comparisons, undoing comparison copies, history limits,
  and profile operations failing against disposable disk fixtures.
- `cargo run --offline -p tronteq-uitest`: **18 scenes**, ending in `[uishot] done`
  (`target/editing-uishot.log`).
- The native GL preview probe produced **20 previews**
  (`target/editing-preview.log`). The new EQ previews cover large dark, compact
  1000-by-600, and Paper layouts. These use the production numeric editor and
  history controls with synthetic audio; the toolbar is a test shell.
- Current screenshots: `target/viz-review-eq-{editor,compact,paper}.png`.
  The selected band, numeric fields, eight selectors, and history controls fit
  all three reviewed layouts.

- Release build passed (`target/editing-release-build.log`), with the same three
  pre-existing GUI warnings. The updated GUI was launched and left running
  (PID 175840). Its own screenshot pipe captured the production GL output at
  60 fps (`target/editing-live.png`).
- Audio control bytes 8 through 199 matched the immediate pre-restart backup
  `target/state.pre-editing.bin`. Telemetry advanced 101 buffers in the one-second
  sample; the user's bypass value remained 1 (`target/editing-live-check.json`).
  No audio installation, endpoint, service, or registry changes were made.
- Native tab-click automation did not switch the live app away from VIZ. The
  release capture therefore verifies startup and rendering; EQ interaction is
  verified by the UI harness, and its layout by the three native previews.
  Follow-up: the [audio analysis pass](2026-09-24-audio-analysis.md) repaired the
  test's message timing and captured the actual CHAIN panel. It also fixed shared
  inspector scroll positions that hid the band selectors after visiting SETUP.
- `git -c core.safecrlf=false diff --check` passed.

The shader pipeline was unchanged in this pass. Renderer verification remains
documented in [the playback note](2026-09-24-playback.md).
