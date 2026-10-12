# Parked ideas

Saved at Trent's request on September 24, 2026. The UI overhaul is complete;
these are options for a future session, not active work.

## Visualizer and UI

- Freeze the analyzer and retain a reference trace for comparisons.
- Give measurement overlays clearer, independent axes where they share the EQ
  canvas. Keep the artwork and measurement controls separate.
- Preserve an unreadable settings file and offer recovery instead of falling
  back silently. Source observation: `AppSettings::load` currently returns
  defaults on a parse error; this path has not been reproduced with live settings.

## Audio and installation

- Add APO endpoint identity and sample rate to telemetry before claiming
  per-device processing status for simultaneous outputs.
- Test and fix delay-ring advancement through silent buffers and bypass changes.
- Make uninstall preserve unowned endpoint effects and restore previous values.
- Separate installed binaries from writable state and tighten binary ACLs if
  installation/distribution becomes a priority again.

The audio/installer findings and their evidence are in
[the audit](2026-09-24-audit-followups.md). None requires changing the current
working test-mode installation to enjoy the UI improvements.
