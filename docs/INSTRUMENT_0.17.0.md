# TrontEQ 0.17.0: visual instrument

The visualizer now has 42 scenes and a performance path from audio signals to
editable response curves, XY snapshots, two-deck blends, final pulse shaping,
and saved performances. Audio processing and the 224-byte APO contract are unchanged.

## Try it

- Choose **VIZ > Style > Flame instrument**. Its Ember, Ribbon, Storm and Eruption
  snapshots react to beats and stereo width through the XY pad.
- **VIZ > Perform** links to **Patch bay**, **XY pad** and **Record**. These pages
  are also available through **More**.
- Patch bay connects any of the 18 audio sources to a parameter. Add a second
  source with Mix, Add, Multiply, Maximum or Difference, set its weight, then
  drag the five response-curve points. Depth, smoothing, inversion and Intensity
  ranges remain available. Live cables show the shaped input.
- XY snapshots blend continuous parameters of one scene. Seed four corners or
  capture authored sliders into each corner. Signal routes apply after the morph.
  Audio pilot connects bass/beats to X and width to Y; Manual XY removes those
  two routes. The pad can glide home after release. Saved looks include snapshots
  and routes, while live values do not alter their authored settings.
- Perform adds **Additive**, **Screen**, **Difference** and **Frequency mask**
  deck blends. Both ends are clean A/B. Frequency masking samples both spectra
  and reverses its orientation during Take B to preserve the composite.
- **Effects > Global pulse control** applies after both decks and all effects.
  Amount, punch, ceiling and recovery shape the final artwork. Amount zero
  preserves the original render. This setting is global across look changes.
- New scenes: **Spectral City**, **Cymatics**, and **Stereo Orbit**, each with
  four routable controls. New looks include Afterhours district, Resonant gold,
  Binary stars and Flame instrument. Cymatics is a stylized plate pattern,
  not a physical simulation.

## Recording

Record stores resolved inputs, controls and deck handoffs for up to 60 seconds
or 3,600 frames. Starting a take clears renderer history so the take can be
reproduced independently. Replay renders every recorded step, including when
the GUI refreshes more slowly than the recorded performance. It does not replace
or replay the sound coming from Windows.

Save a `.trontperf.json` take to preserve it across sessions. Load validates
the schema, frame count, duration and numeric inputs. Export runs in a separate
hidden instance of TrontEQ, before its normal single-instance handling. It uses
FFmpeg on PATH to produce a 960x540, 30 fps H.264 MP4 containing artwork only.
Default destination: `%USERPROFILE%/Videos/TrontEQ`. Files are not overwritten.

## Verification

Receipts are in `target/tests-final-0.17.log`, `target/audit-final-0.17.log`,
`target/performance-017/`, and `target/update-0.17.0/`.

- Workspace: 168 tests passed; the optional live-capture unit test is ignored.
- GPU: 42 scenes, 36 scene controls and eight detail controls; ordered effects;
  all five blend endpoints and swap continuity with different deck spectra;
  final brightness ceiling and softened attack.
- Recorded replay: identical final pixels at 20, 60 and 144 fps, including a
  deck swap and final pulse history. Existing viewport/scissor, render-scale,
  feedback timing and transition checks also pass.
- UI: real egui interactions exercise route creation and curve presets, XY
  seeding, Audio pilot/manual switching, and recording controls in a 310-pixel
  panel. 62 offscreen previews cover dark, light and compact layouts.
- Export is exercised through the release executable and inspected with ffprobe.
  The MP4 and probe result live in `target/performance-017/`.

The installed executable hash, prior executable backup, settings backup and
sound-control comparison are recorded in `target/update-0.17.0/install.json`.

The final live capture attempt found no active music on the selected output.
The three new scenes and Flame instrument were reviewed against the saved
480-frame music passage instead. The latter also passed three deck handoffs
and all five pads. Review players are in `target/gauntlet-017-reviewed-*`.

Installed verification: PID 117888; SHA256
`F7F0F1AAE643435CBAFAD6B6240C76819E9CACE62975A99048D5765B75A54559`.
The app's own framebuffer was reviewed at 60 fps. Top-level settings and sound
controls matched their pre-install copies. The export probe reports exactly
60 H.264 frames at 30 fps, 960x540, duration 2.000 seconds.
