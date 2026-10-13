# TrontEQ 0.18.0: musical response and deck interaction

This release delivers the first three priorities from the approved brainstorm:
the full scene gauntlet, adaptive input response, and cross-deck distortion.
The remaining director, musical-memory, gesture, patch-view, and variation ideas
remain recorded in the daily app ask log for subsequent passes.

## Try the new controls

- **VIZ > More > Signals**: choose Musical, Percussive, or Flowing. Select an
  input and adjust its range adaptation, attack, and release. Original restores
  the prior response. The meter fill is the processed signal; its thin marker
  shows the original input. Learning memory, quiet floor, and Relearn are under
  Learning and quiet input.
- **VIZ > Perform > Cross-deck distortion**: try Flame flow, Spectral glass,
  or Prism light. These load both decks and put the crossfader at 35%.
- Choose Displace, Flow, or Light, then B shapes A, A shapes B, or Both ways.
  Strength, field scale, angle, and softness have the same React to controls as
  other parameters and appear in Patch bay.
- **VIZ > Style** exposes four controls each for Kaleido, Copper, and Laser.
  Their thumbnails have been regenerated from the production renderer.

Adaptive response is opt-in for existing looks. The new combination recipes
enable it on deck A. Each saved look includes its input settings, and deck B has
its own response editor. Learned ranges and current envelopes stay in memory;
playback does not write them into a look or produce visual undo entries.

Sustained bands expand around a moving range, with a minimum span to avoid
amplifying tiny fluctuations. Transients retain their zero baseline. Quiet input
stops range learning and lets energy envelopes release toward zero. Stereo
balance, correlation, width, and other absolute descriptors retain their meaning.
This is visual signal processing; it does not change Windows sound processing.

Distortion samples the two completed deck images before the final blend and
pulse controls. It does not enter scene feedback. Its influence fades to zero
at both crossfader ends, and Take B reverses its direction to preserve the
composition. Settings and visual undo retain the mixer configuration; resolved
performance takes retain the actual interaction values. Old takes default to Off.

## Scene pass

Six scenes received focused changes:

| Scene | Problem in the matched passage | Result |
|---|---|---|
| Kaleido | Mostly concentric rings | Mirrored, spectrum-lit lattice with distinct sectors |
| Copper | Overlapping bars formed a white band | Separate metallic ribbons with spectral thickness and beat accents |
| Laser | Broad beams and trails crowded the image | Narrow beams, readable crossings, adjustable haze and persistence |
| Julia | Broad beige glow hid fractal detail | Stable filament-rich orbit, narrower rim, colored orbit detail |
| Sonar | Additive history washed out the radar face | Decaying phosphor trails, smaller blips, readable grid |
| Pulsar | Large clipped white core | Smaller core, visible lighthouse sweeps and shockwaves |

All 36 other scenes have pixel-identical review frames against the baseline,
including Flame. The comparison used Original response, the same captured inputs,
and the same clock. This does not imply every remaining scene is finished; the
scene review records the next composition and motion candidates.

## Evidence and limits

The live capture attempt found no active music on the selected Windows output.
The gauntlet therefore replayed the previously captured 480-frame music passage
at 60 Hz through all 42 scenes, plus a matched silent pass for each. Every case
has a 30 fps silent video, four clean frames, four stacked-effect frames, and an
EQ-overlay frame. The three distortion recipes have their own videos and traces.
This is one musical passage, not a claim of coverage across whole songs or genres.

- Scene review: `target/gauntlet-018-review/index.html`.
- Baseline and final renders: `target/gauntlet-018-baseline/` and
  `target/gauntlet-018-final/`.
- Distortion combinations: `target/gauntlet-018-duets-final/`.
- Adaptive response music trace: `target/gauntlet-018-adaptive/`.
- GPU and UI previews: `target/audit-018-final.log` and `target/viz-review-*`.
- Final tests, build, install backup, and native capture:
  `target/update-0.18.0/`.

GPU checks cover 42 nonempty scenes, 48 scene controls, eight detail controls,
three interaction modes in three directions through five blends, clean endpoints,
zero strength, deck swaps, final pulse shaping, and viewport/scissor isolation.
Resolved distortion takes replay to identical pixels at 20, 60, and 144 fps.
Unit and UI tests cover legacy defaults, gain order, saved-look round trips,
range expansion, quiet input, frame-independent envelope timing, signal routing,
take validation, and scrolling to the new controls in a 310-pixel inspector.

The saved phrase's mid input spans 0.519 to 0.773; Musical response expands it
to 0.269 to 0.879 after warmup. These are normalized visual values, not audio gain.
White-pixel ratios and image differences are diagnostic measurements, not scores
of musical quality. Final verification passed 174 tests and 68 UI previews.
The installed 0.18.0 executable matches the release hash; its native framebuffer
shows the version in the title bar. User settings and sound controls remained
unchanged. A recorded distortion take exported to a verified 960x540, 30 fps
H264 MP4. The install receipt is `target/update-0.18.0/install.json`.
