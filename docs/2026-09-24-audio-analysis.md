# Visualizer signal and timing, September 24, 2026

The unified visualizer now reads a coherent snapshot from the capture thread.
Spectrum, waveform, stereo scope, levels, peak holds, waterfall, DATA histories,
and rhythm statistics share the same input and analysis clock. GUI repaint rate
no longer determines how quickly those histories move.

## Signal correctness

- FFT power is averaged across channels after transforming each channel. RMS
  uses mean channel energy, and peak uses the largest channel magnitude. Inverting
  one stereo channel therefore does not cancel the spectrum or output meter.
- The waveform follows the strongest channel, selected per FFT window with
  hysteresis. This also shows right-only and surround-only material. The stereo
  scope and correlation continue to use the first two channels, duplicated for
  mono input. They do not claim to represent every channel of surround audio.
- The FFT frequency scale uses the negotiated capture rate; bars above Nyquist
  are zero. The EQ preview now uses that rate too. Before capture connects, it
  retains its previous rate (48 kHz at launch), identified in SETUP. The installed
  APO still does not publish its own rate or endpoint identity; this does not
  establish per-device APO telemetry.
- SETUP shows the actual visualizer source, rate, channel count, and encoding.
  Capture errors are visible in VIZ, SETUP, and the footer while reconnection
  retries. Failed or paused capture clears all published signal data.
- DATA labels its unweighted output level in dBFS. It is not a LUFS meter.
- Each inspector tab/subpage keeps its own scroll position. Native review found
  that scrolling SETUP previously hid the first controls when opening CHAIN or
  VIZ; separate scroll identities fix that. SETUP's reset button is labeled
  **Reset sound**, matching the preserved A/V sync delay.

Mix-format decoding checks the actual subformat, size, channel count, rate, and
block alignment. Supported inputs are float32 and 8/16/24/32-bit PCM, including
PCM with fewer valid bits left aligned in a larger container. Unknown or invalid
formats report an error. Nonfinite samples become zero and extreme float values
are bounded to +/-16 for visualization only. The format handling follows
[Microsoft's WAVEFORMATEXTENSIBLE definition](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible)
and [valid-bit alignment rules](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/extensible-wave-format-descriptors).

## Timing and lifetime

The analyzer advances exactly 60 history/rhythm ticks per second of audio using
the capture rate and frame count. FFT updates remain every 512 audio frames;
spectrum and meter decay coefficients account for the sample rate. Waveform
sampling uses an 8 kHz clock; the scope uses up to 12 kHz. Waterfall history holds
three seconds, level history eight seconds, and DATA sparklines four seconds.
Snapshots contain complete bounded histories, so an unfocused GUI can skip
frames without losing their time scale. No raw captured audio is saved to disk.

Silent packets feed zeros through every analysis product. If Windows produces
no packets, the capture thread starts advancing silence after a 100 ms buffer
grace period. Catch-up work is capped at 250 ms after a long thread stall. Device
discontinuities clear previous FFT/history state rather than blending unrelated
audio. A default-output change starts a fresh session. The handling of empty,
silent, and discontinuous buffers follows
[Microsoft's capture-buffer contract](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer).

Mix-format memory is released on failure paths as well as success. A running
capture client is stopped when its session ends. Hiding to tray releases capture;
resume reacquires the Windows default output. The GUI copies an immutable snapshot
handle, and FFT/history processing runs outside the shared snapshot lock.

## Verification

- `cargo test --workspace --offline`: **80 GUI + 4 CLI + 9 shared + 30 UI-harness
  tests passed, 0 failed** (`target/analysis-tests.log`). One hardware-dependent
  test is ignored by default and was run explicitly below.
- Fixtures cover phase inversion, right-only, mono, surround-only, silent input,
  8/44.1/48/96 kHz rates, frequencies above Nyquist, float and PCM formats,
  invalid format headers, nonfinite samples, bounded no-packet catch-up, stale
  state after errors/pause, immutable snapshots, and 120 BPM rhythm detection.
- Snapshots sampled at simulated **20/60/144 fps** produced identical spectra,
  peak holds, waterfall rows, level history, and pulse history.
- Explicit live test passed on **Beyond TV (NVIDIA High Definition Audio)**:
  **48,000 Hz, 2 channels, 32-bit float**, with **60 analysis ticks** over the
  one-second sample. Measured peak was 0.03837 and RMS 0.01349. Pause cleared
  the published source and signal; resume reconnected successfully.
  Receipt: `target/analysis-live-capture.log`.
- The UI screenshot harness completed all 18 scenes with `[uishot] done`
  (`target/analysis-uishot.log`).
- Release builds passed. Native captures show the real capture format in SETUP,
  DATA histories, numeric CHAIN controls, and the return to VIZ. Scrolling SETUP
  no longer transfers its offset to those pages. Captures are
  `target/analysis-live-{status,data,chain,final}.png`.
- The live window rendered at approximately 60 fps; the final captured SETUP frame
  reported 1.5 ms CPU frame time. This is one scene/workload sample.
- Audio control bytes remained identical to `target/state.pre-analysis.bin`
  across GUI restarts and navigation. The user's bypass setting remained 1.
  No APO, service, endpoint, or registry changes were made.
- Final GUI PID: **165532**, left open on VIZ. The final receipt records telemetry
  advancing 102 buffers over the approximately one-second sample
  (`target/analysis-live-check.json`). The final release build receipt is
  `target/analysis-final-build.log`; the same three pre-existing GUI warnings remain.
- `git -c core.safecrlf=false diff --check` passed.

Run the optional live check with:

```powershell
cargo test --offline -p tronteq live_capture_publishes_audio_clock_and_releases_while_paused -- --ignored --nocapture
```

It reads the current output briefly and exercises its own capture client's
pause/resume. It does not alter playback, EQ, devices, services, or registry state.
Physical device removal and changing Windows' output format were not performed;
their error/reset logic is covered with fixtures, not claimed as live tests.
