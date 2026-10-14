# TrontEQ 0.16.0 performance tools

VIZ > Perform has the mixer and pads. A is the current look. Load/copy a look
into B; Edit deck B has its own scene, effect stack, flash, framing and routes.
Drag A/B, route it through React to, or Fade to A/B in beats. Take B preserves
its existing feedback and history. B is disabled by default and uses a second
scene/effect pass when enabled.

Hold Punch, Scatter, Freeze, Sweep or Color burst. Keys 1-5 work in Perform and
presentation while focused, except during text editing. Return controls release.
Drive pads with music provides individual routes. Freeze holds each deck frame;
the mixer remains independently controllable. Pads leave the saved look intact.

Effects > Effect racks saves the stack and its effect/flash mappings separately
from the scene. Initial racks: Dirty CRT, Liquid neon, Print crush. Loading a rack
preserves the scene, palette and non-effect mappings.

Flash control adjusts the Beat flash effect source, strength, threshold, decay
and maximum trigger rate. Held-high signals cannot continually retrigger it.
Disable Beat flash affects the deck being edited. It does not disable lighting
changes authored into individual scenes.

More > Signals shows 18 sources. Sub-bass covers roughly 20-59 Hz and kick onset
60-190 Hz. Kick is an onset proxy, not instrument recognition. Tonal/noisy uses
FFT flatness; balance maps left/center/right to 0/0.5/1. Three band transients
are separately available. Frame movement and crossfade are routable targets.

Mutation has scene/palette/motion/effect locks and a strength slider. Visual Undo
restores the look. Randomization never introduces Beat flash as a new effect.

Visual setlist captures complete looks with order, removal and per-cue hold/fade
beats. Start begins at cue one, Next advances, Stop holds the current mix. Tempo
uses detected BPM with manual fallback when confidence is low. Setlist playback
stops scene shuffle. Config, racks and cues persist; playback clocks, envelopes
and held pads are runtime state. Animated fades save only when finished.

## Verification

156 workspace tests passed, one optional capture test ignored. GPU audit passed
39 scene renders, 24 controls, 8 details, palettes/variants, timing, clipping and
ordered effects. Added pixel comparisons passed for both independent deck
endpoints, 50% mixing, B-to-A continuity, Freeze, framing and flash envelopes.
46 offscreen UI previews include compact/Paper Perform and Signal lab.

The recorded-music performance review passed 480 frames, three continuous deck
commits and five held/released pads without GL errors. This reuses the existing
eight-second analysis capture. Old captures contain zero for the new sources;
new captures store them. Video is silent, with no playable audio recording.

Evidence: target/performance-tests.log, target/performance-render.log,
target/performance-preview-final.log, target/performance-replay.log and
[target/performance-review-0.16/index.html](../target/performance-review-0.16/index.html).
