# Restore lag investigation, 0.18.2

Trent reported severe desktop lag when restoring the installed TrontEQ after a
long minimized/tray interval. The old process had been running since October 13.
It remained alive and was force-stopped at Trent's request. No stack trace was
captured before termination, so the exact original stall is not proven.

## Findings and repair

- The repaint worker trusted a visibility flag maintained by `update()`. Windows
  can stop redraw delivery immediately on minimize, before that flag is cleared.
  The worker now checks actual native visibility/minimized/cloaked state before
  requesting another repaint. It also pauses capture without needing a UI frame.
- Normal minimize previously did not consistently release the loopback capture
  session. Every non-presentable state now pauses it; restore re-enables it.
- Cached egui minimized metadata no longer overrides a restored native window.
- Live animation clocks discard gaps longer than 250 ms and restart with one
  normal frame. Restore also resets frame-rate accounting and the GL wall clock.
  Offline explicit-time replay/export remains unchanged.
- The fixed-step renderer already limited elapsed time, so no unbounded replay
  of days of animation was found. However, it repeated all post effects and
  composition up to six times per displayed frame. Intermediate steps now run
  feedback simulation only; the effects stack runs once on the final result.
- Two frame-rate/CPU/GPU reports are logged after each start/restore to help
  diagnose a recurrence. No continuous per-frame logging is enabled.

The change preserves authored looks, effect order/mixes, audio controls, and the
APO/IPC format. It does not reinstall the APO or change the startup task.

## Verification

- 138 GUI tests passed; the opt-in live capture test also passed separately.
  Initial sandbox temp-file failures were resolved by using `target/restore-test-temp`.
- `tronteq-audit-probe --restore-only`: a nine-effect Warp at 2x speed produces
  pixel-identical output with one final effects stack versus six full passes.
  The measured isolated batch was 0.894 ms versus 3.538 ms for six full passes.
- Simulated 1-second, 1-hour, and 3-day gaps resumed in 0.641, 0.526, and 0.589 ms
  respectively, with no timing debt in the following frames. This tests the
  production rendering code, not three days of actual Windows suspension.
- `tronteq-audit-probe --render-only`: all 42 scenes, 48 controls, effect order,
  scene transitions, dual decks, distortion, and exact 20/60/144 Hz replay passed.

Receipts are in `target/restore-*.log` and `target/restore-verification-2026-10-16/`.
Native minimize/restore and installed-binary verification are recorded below
after the release build is ready.

## Native 0.18.1 result: insufficient

Trent confirmed the entire desktop still became sluggish while the window was
visible. The 0.18.1 timing fixes alone did not solve his reported problem.
The bounded test was stopped. It must not be represented as a successful fix.

The process consumed 1.4 to 2.05 CPU seconds per two-second visible sample,
despite reports of 60 fps, about 2.1 ms CPU frame time and 0.066 ms shader time.
Minimized samples dropped to zero CPU. Windows screen capture failed twice with
`FrameArrived timed out: timed out waiting on channel`; no visual QA was obtained.

eframe 0.33.3 explicitly excludes `gl_surface.swap_buffers()` from its reported
CPU frame time. Driver-controlled vsync is therefore a candidate for the missing
CPU time. Version 0.18.2 disables that second pacer and retains the existing
application-owned 60 Hz heartbeat. It does not change any Windows/NVIDIA settings.
Its native CPU comparison is recorded after building and testing below.

## Native 0.18.2 and Qwen isolation: still failing

Trent reported desktop lag in the seven-second 0.18.2 comparison too. The
process consumed 1.8125 CPU seconds in 5.016 seconds. The frame log delivered
about 120 FPS despite the intended 60 Hz timer: egui's immediate repaint
request explicitly schedules two frames. Version 0.18.3 uses a nonzero
repaint delay from the timer to request one frame instead. This is a concrete
pacing defect, but its relationship to the desktop stall is still unverified.

With Trent's explicit approval, Snow Crash's verified Qwen worker (PID 497148)
was suspended briefly and resumed in a finally block. The same installed
TrontEQ 0.18.2 still lagged, per Trent. Its comparison used 2.375 CPU seconds
in 5.013 seconds. GPU utilization sampled 16% both before and during TrontEQ;
VRAM remained allocated while Qwen was suspended. The worker resumed successfully
and subsequently used 2.016 CPU seconds over two seconds. TrontEQ was closed.
This rules out active Qwen generation as a necessary trigger; it does not rule
out driver or memory-pressure interactions. No Axela process was found.

Version 0.18.3 also adds a controls-only diagnostic launch: `--controls-only`,
or a `tronteq.controls-only` marker beside the executable. This skips shader
construction and WASAPI capture, uses a 10 Hz telemetry heartbeat, and draws a
plain editable EQ curve. It does not replace the saved visualizer look.

## Zoom and compact-window comparison

The first controls-only test accidentally launched the installed 0.18.2 binary
because the desktop launcher resolved the app identity to its installed path.
That instance was stopped immediately after discovery. The diagnostic was then
installed at the verified launch path and its hash/version checked before retry.

With 0.18.3 in controls-only mode and the original saved zoom of 230%, Trent
reported severe lag again. The log showed 171 FPS despite a 10 Hz heartbeat,
with no GL stage or capture thread running. CPU use was 1.203 seconds over
5.014 seconds. Trent observed the oversized UI inside a shrunken window.

Only settings.json's zoom was reset to 100%, after a backup and a parsed
all-other-values comparison. The next run settled at 10 FPS and used 0.172 CPU
seconds over 5.008 seconds (about 86% less). Trent did not catch that short run,
so this is measured improvement, not a user confirmation of resolved desktop lag.
At Trent's request it was opened again for 30 seconds: 0.8125 CPU seconds over
28.016 seconds measured, with the log again settling at 10 FPS. Both tests
closed automatically; Qwen remained running throughout.

The small-window chrome-only headless reproduction did not reproduce the
continuous repaint at 230%; the exact low-level trigger is not proven. 0.18.4
adds a window-size-aware zoom guard before building the layout. It reduces zoom
when the resulting content would be smaller than 640x300 points, without
automatically raising it when the window grows. Large windows retain high zoom.
The native window is not enlarged and sound/visualizer settings are preserved.
141 GUI tests passed, including the zoom guard, single-heartbeat repaint,
and compact chrome at 100% and 230%. The live-capture opt-in test is excluded
from that count; it passed separately earlier in this investigation.

## Final owner correction: still unresolved, do not reopen automatically

Trent did not see the timed 30-second comparison. At his request the 0.18.3
controls-only GUI was reopened at 100% and left up. He reported that it rendered
a few frames then severely lagged the desktop again, and minimized it to recover.
The new log climbed from 46.9 to 133.5 FPS despite the 10 Hz timer. Earlier low
CPU samples were therefore not sufficient proof of a successful repair.

At Trent's explicit request PID 69640 was stopped; a subsequent host check
reported zero TrontEQ processes. Do not reopen it for further visible testing
without new direction. Qwen's worker was resumed earlier and was not stopped.

Trent now reports the same severe symptom in TrontSnap. Bounded source inspection
shows both GUI builds use egui/eframe with glow/OpenGL (TrontEQ 0.33.3; TrontSnap
0.29.1, locally patched). This is source evidence, not a runtime backend trace of
TrontSnap. It strengthens the shared presentation/driver hypothesis, but does
not prove the language, library, driver, or either application is at fault.

Read-only host checks: NVIDIA 591.86; last boot October 4; approximately 25.5 GiB
RAM available; GPU 48 C with 10.5/15.9 GiB VRAM used. System log had nvlddmkm
events 13/153 on October 14, with no rendered message text; this is historical
evidence, not a crash correlated with today's test. The Virtual Desktop Monitor
adapter reported Error, while NVIDIA, Intel, Meta and Parsec reported OK. No
driver setting, adapter, service or reboot was changed.

0.18.4 was built successfully and its 141 tests passed, but it is NOT installed
and is NOT claimed to fix the desktop problem. The installed GUI is 0.18.3,
closed, with the controls-only marker and 100% saved zoom. Original executable
and settings backups remain. Next isolation should start after Trent saves
active jobs and chooses a reboot, then compare shared rendering/presentation
state before making more speculative app changes.
