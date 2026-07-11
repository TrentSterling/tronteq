# TrontEQ v0.3 — UI Refactor: Profiles + Inspector Tabs + True Light Canvas

## Context

TrontEQ v0.2 works end-to-end (APO + glow-renderer GUI, stable at ~178MB). The UI has outgrown its layout: the toolbar is 2 rows / ~25 controls, the light theme skips the viz canvas entirely (fixed dark consts in `curve.rs`), and there is no way to save a tweaked state — Trent always adjusts from V-shape into "Super V" and loses it. Nothing (theme, viz layers, zoom) persists across restarts except the IPC state itself.

This refactor ships three things in priority order: **(1) saveable, one-click-swappable sound profiles**, **(2) a tabbed right-side inspector that unclutters the toolbar**, **(3) a real light-mode canvas**. Plus a persistence layer (settings.json) that finally remembers UI state, and a structural home for the upcoming viz expansion.

**Decisions locked with Trent:** profile = sound only (8 bands + preamp + comp/limiter/AGC incl. enabled flags; bypass excluded — it's the A/B toggle). UI state persists globally, not per profile ("profile == sound savedata, general == general savedata"). Quickswap = toolbar chips. Light canvas = true reskin (pale bg, dark ink, recolored viz).

**De-risk finding:** the S57 "extraction crash" (toolbar.rs/chain_panel.rs, reverted) — I read both diffs (8f2a5bd, 5d21348): verbatim moves, no bug visible. The crash predated crash.log AND the app was still on wgpu/DX12, which was later proven (S59) to crash this app on its own and was removed (glow swap). Almost certainly misattributed. We still launch-verify every phase.

## Phase 0 — Baseline safety net (~5 min)

1. **Commit the uncommitted working tree** (14 modified files = the S59 glow swap + vumeter telemetry + viz-retry work, never committed) as a checkpoint commit. This is the restore point.
2. Write `dev-cycle.ps1` (pattern: existing `heal-launch.ps1`): self-elevating — kill running tronteq, launch fresh `target\release\tronteq.exe`, sleep ~10s, verify process alive, tail `C:\ProgramData\TrontEq\crash.log`, log to `dev-cycle.log`. **One UAC click per checkpoint** (3-4 total this session).
3. Drop this design into `tronteq/docs/2026-07-11-ui-refactor-design.md`, commit with baseline.

## Phase 1 — Profiles (~20 min) ← the headline feature

**`shared/src/lib.rs`:** add `serde` derives (`Serialize, Deserialize` + `#[serde(default)]`) to `Band` and `Dynamics`; impl `Default` for both (`Band::flat(1000)`, `Dynamics::default_passive()`). Add `serde` dep to shared. Roundtrip unit test (runnable — shared has no admin manifest).

**`gui/src/profiles.rs` (NEW):**
- `Profile { schema: u32 = 1, name: String, order: u32, bands: [Band; 8], preamp_db: f32, dynamics: Dynamics }`
- `ProfileStore`: dir `C:\ProgramData\TrontEq\profiles\` (inherits the already-ACL'd TrontEq dir; GUI is elevated). `load_all()` (skip + `log_line` corrupt files), `save()` (slugified filename, write `.tmp` then rename), `delete()`, `rename()`, `seed_if_empty()` → factory profiles **Flat, Bass, Vocal, V-shape, Treble, Tinny** from `presets::apply_eq` gains + passive dynamics (normal user profiles: editable, deletable).
- `matches(bands, preamp, dynamics) -> bool` for the dirty check (exact f32 compare is safe — values only flow through these paths).

**`gui/src/settings.rs` (NEW):** `AppSettings { schema, dark_mode, rainbow, layers (7 bools), zoom, active_profile: Option<String>, inspector_tab }`, JSON at `C:\ProgramData\TrontEq\settings.json`, save-on-change (required: tray Quit is `process::exit(0)`, there is no clean shutdown hook). Wire into `App::new`: restore theme (before first frame), layers, rainbow, zoom, active profile.

**`main.rs` toolbar (minimal touch this phase):** replace the Flat/Reset + 5 preset buttons with **profile chips**: click = load (applies bands+preamp+dynamics, `commit()`), active chip highlighted, `*` suffix when current state diverges from saved, `+` chip opens a save-as name popup, right-click chip → Overwrite with current / Rename / Delete (2-step confirm). Number keys 1-9 quickswap (guarded by `!ctx.wants_keyboard_input()`). Glyph rule: only proven glyphs (`↺ ⟳ ·`), ASCII elsewhere (egui tofu gotcha).

**CHECKPOINT 1:** `cargo build --release -p tronteq` + `cargo test -p tronteq-shared -p tronteq-cli` + dev-cycle (UAC) + crash.log clean + Trent saves a real "Super V".

## Phase 2 — Inspector tabs + toolbar slim + viz fixes (~15 min)

**`gui/src/inspector.rs` (NEW):** right `SidePanel` (keep id `"chain"`) with a tab strip — **CHAIN | VIZ | SETUP** (selectable labels; active tab persisted in settings).
- **CHAIN**: the existing signal-chain content, moved verbatim (same mutate-Copy-locals/write-back pattern, per-component preset chips + knobs unchanged).
- **VIZ**: "ANALYZE" section — the 7 layer toggles with full names (Spectrum bars, Peak-hold caps, Analyzer line, Waterfall, Waveform, Goniometer, Loudness history) + Rainbow. Section header structure leaves room for the future "SHOW" (pretty viz) section.
- **SETUP**: output device picker + Apply EQ here + refresh + status (moved verbatim from toolbar row 2), zoom -/100%/+, Reset all (whole chain), sample rate + state-version readouts, "Open profiles folder", About.

**`main.rs` toolbar final form (single row):** `TrontEQ | [chips…][+] | Bypass | …spacer… | ⚠"EQ inactive" chip when telemetry.seq==0 (click → SETUP tab) | Light/Dark | About`.

**`curve.rs` fix:** Peak-hold caps draw even when Spectrum bars are off (today the Peak toggle is dead without Spec — hoist the peak-cap drawing out of the `if layers.spectrum` branch).

**CHECKPOINT 2 (UAC):** tabs all render, device apply works from SETUP, chips + keys still work, crash.log clean.

## Phase 3 — True light canvas (~12 min)

**`theme.rs`:** add canvas-scoped mode-aware accessors: `canvas_bg()`, `canvas_grid()`, `canvas_zero()`, `canvas_label()`, `ink()` (white in dark / near-black `#16202A` in light — replaces every hardcoded white: node rings, peak caps, spectrum cap lines, meter peak ticks, loudness line core), `glow_core()` (the `150,240,255` family), `tooltip_bg()`, `inset_bg()` (goniometer box), `spectrum_base()`, and `viz_hsv(h)` (dark: `hsv(h,.85,1)`; light: `hsv(h,.9,.72)` so rainbow reads on pale). Light canvas bg `#E9EEF3`, grid = dark ink at low alpha.

**Sweep:** `curve.rs` (~20 hardcoded Color32 sites → accessors), `main.rs` meters (`theme::BG` track + white ticks), `spectrogram.rs` (`color_for` gains a dark-mode flag; cache key extends `last_rainbow` pattern with `last_dark`; darker ramp on light).

**CHECKPOINT 3 (UAC):** toggle Light — canvas, grid, tooltips, gonio inset, meters, waterfall all reskin; both modes eyeballed by Trent; screenshot for the record.

## Phase 4 — Wrap (~8 min)

- `CHANGELOG.md` v0.3.0 entry; version bump (workspace `version` in root `Cargo.toml` — gui uses `version.workspace = true`).
- `tronteq/CLAUDE.md`: add profiles.rs / settings.rs / inspector.rs to the GUI file list.
- Commit per phase throughout (each phase = restore point). Final dev-cycle launch, leave app running.
- Update memory (`tronteq.md`, session history).

## Parked — designed today, built next session ("SHOW" viz + analyzer stats)

Two-bucket viz model agreed with Trent: **ANALYZE** = data layers (current 7; queued: **BPM detection readout**, LUFS, crest factor, stereo correlation — siblings of the loudness meter) vs **SHOW** = mutually-exclusive full-canvas eye candy (Winamp/WMP energy: mirrored Bars XL, phosphor scope art, radial spectrum tunnel; Milkdrop-style feedback later). The VIZ tab ships with the ANALYZE section header today so SHOW slots straight in.

## Verification

- Per phase: `cargo build --release -p tronteq`; tests via `cargo test -p tronteq-shared -p tronteq-cli` (NOT `-p tronteq` — admin-manifest test binary can't run).
- Per checkpoint: `dev-cycle.ps1` elevated (one UAC click) → process alive >15s, `crash.log` tail clean, Trent eyeballs.
- Invariants that must not break: `state.bin` 216-byte IPC contract (profiles never touch the layout — they serialize the same structs to JSON, separately), tray hide/show, autostart task, elevated Apply flow, curve-drag snappiness.

## Time budget (60 min hard target)

P0 5' → P1 20' → P2 15' → P3 12' → P4 8'. If overrunning: **Phase 3 (light canvas) slides** to a follow-up — profiles + tabs ship first (value order per Trent's emphasis: "I need saveable profiles make it easy man").
