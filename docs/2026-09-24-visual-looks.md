# Visual looks and presentation, September 24, 2026

VIZ now keeps complete visual arrangements as named looks. Open **VIZ > Style**
to choose Neon drift, Solar tide, Phosphor, Night drive, or Still water. Each
combines a scene, speed, audio response, brightness, Spectrum/Theme color mode,
and ordered effects with their enable states and mixes.

**Save as** opens an inline name field. Saved looks appear above built-in looks
in the selector. A modified look gets an asterisk; **Update look** explicitly
replaces its saved settings. **Remove look** offers an undo. Duplicate names
are rejected instead of silently overwriting another look.

Looks preserve sound, window state, measurement overlays, favorites, and shuffle
preferences. Theme color mode follows the app's current theme; a look does not
replace the app theme. Saved looks live in the existing settings file and older
settings continue to load.

**Present** opens the current scene fullscreen with the editor, meters, toolbar,
and inspector hidden. A return control appears on input and hides after three
seconds; the idle cursor hides too. **Escape** restores the previous window
state and controls. F11 exits presentation to a window. Launch with
`tronteq.exe --visualizer --present` to start directly in presentation.

The short inspector now keeps Shuffle visible, including when egui's default
scroll minimum would have forced it offscreen. Style's main controls fit in the
compact layout. Settings write failures surface in the footer and retry every
two seconds; the app only considers settings saved after the write succeeds.

## Verification

- `cargo test --offline -p tronteq`: **50 passed, 0 failed**. Tests include the
  actual save, recall, update, remove, and undo UI; disk persistence and failed
  writes; compact Shuffle placement; presentation return button and Escape.
- Release GUI build passed. Existing three warnings remain.
- Native GL preview harness produced **12 previews**, including all five factory
  looks, compact panels, Paper theme, and presentation. Logs are in
  `target/looks-preview.log`; images are `target/viz-review-*.png`.
- The release GUI was launched in presentation on the live desktop. Its own
  screenshot pipe captured the fullscreen scene with idle controls hidden.
  Escape sent directly to its verified window restored the normal VIZ panel.
  Captures: `target/looks-live-presentation.png` and
  `target/looks-live-return.png`. `tools/check-presentation.py <pid>` records
  the repeatable capture/return sequence for a GUI launched with `--present`.
- Audio control bytes matched the pre-update backup; APO telemetry advanced at
  100 buffers per second. The user's bypass state was preserved.
- `git -c core.safecrlf=false diff --check` passed.

The renderer has no shader changes in this follow-up. The earlier 37-scene GPU
and timing verification is recorded in
[the unification report](2026-09-24-viz-unification.md). Gallery thumbnails are
now implemented in [the next UI pass](2026-09-24-scene-gallery.md). Transitions
and measured render-quality controls are covered in
[the playback follow-up](2026-09-24-playback.md).
