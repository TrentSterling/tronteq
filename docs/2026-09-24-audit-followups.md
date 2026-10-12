# Follow-ups from the September 24 audit

The immediate work is the unified visualizer described in
[the implementation note](2026-09-24-viz-unification.md). These are separate tasks,
not claims that this UI change fixes the audio engine or installer.

| Priority | Finding | Evidence and next step |
| --- | --- | --- |
| Implemented | Undo/redo, numeric band editor, and A/B slots | [Sound editing pass](2026-09-24-sound-editing.md); session history, precise selected-band fields, and independent comparison sounds. |
| Implemented | Profile save/delete/rename failures reached only the log | Disk failures now reach the UI and preserve in-memory entries; failed save/rename keeps the dialog open. |
| Clarified | Reset whole chain leaves A/V delay unchanged | Renamed Reset sound, with an explicit delay-preservation note and a separate A/V sync page. |
| Implemented for capture | GUI sample rate was fixed at 48 kHz | [Audio analysis pass](2026-09-24-audio-analysis.md); capture format is exposed and drives the EQ preview. APO-specific rate/endpoint telemetry remains separate work. |
| Audio correctness | Telemetry is shared by all attached APO instances | `SharedState.h` and `state.bin`; add endpoint identity before presenting per-device processing status. |
| Audio correctness | Silent buffers return before advancing the delay ring | `apo/src/TrontEqApo.cpp`; test impulse, silent buffers, resume, and bypass transitions offline. This is source analysis, not a live-audio reproduction. |
| Implemented | An abandoned odd seqlock version hung the GUI snapshot reader | [Bounded reads, coordinated writers, confirmed-sound backups, and explicit recovery](2026-09-24-state-recovery.md). Fixture and cross-process checks preserve the sound and telemetry. |
| Installer maintenance | Uninstall clears endpoint EFX values without checking ownership | `cli/src/main.rs::cmd_uninstall` and `setup::clear_endpoint_efx`; preserve prior values and remove only owned registrations. Uninstall was not executed for this audit. |
| Installer maintenance | Installed DLL inherits broad Modify access | Read-only ACL inspection confirmed it. Separate installed binaries from writable state during a future installer pass. No ACLs were changed. |

The capture subformat bug, opposite-phase cancellation, and UI-dependent analyzer
histories are fixed in [the audio analysis pass](2026-09-24-audio-analysis.md).
Fixtures verify PCM/float decoding, multichannel signal handling, and identical
history results with different GUI rates. A separate live loopback check verified
format, 60 Hz analysis, and pause/resume on Beyond TV. Capture timing and shader
simulation timing are now both independent of GUI repaint cadence.

Existing planning docs describe older renderers and IPC layouts. Use the current
code and verification notes when deciding the next changes.
