# Theme audit sweep, 2026-10-14

Source: `THEME-AUDIT-2026-08-01.md` in the TrontStack root (outside this repository).
This note records what the unattended sweep did inside TrontEQ and what is still
open, so the audit can be updated by hand.

## Status of the audit's remaining-work list

| # | Item | Where | State |
|---|---|---|---|
| 1 | Port AUTO AWESOME into TrontEQ | tronteq | `from_accent` was ported in d514abd. This sweep closes the last gap on that path: the Light/Dark flip. Built-ins and `from_colors` are still literals (see below). |
| 2 | Boxel missing `accent_readable` | Boxel | Not touched: another repository. |
| 3 | SpaceView raw `step(9)` as `accent_readable` | SpaceView | Not touched: another repository. |
| 4 | TrontSnap tags v0.13 to v0.22 | TrontSnap | Not touched: another repository. |
| 5 | `trontsnap/dist/` stale binaries | TrontSnap | Not touched: another repository. |

## What changed

- **The flip keeps the pick.** `theme::set_mode` used to swap to the Electric
  Cyan or Paper literals, and `set_palette` then overwrote the recorded accent
  seed with Paper's accent, so a custom accent was gone after one click of the
  toolbar Light/Dark button, the App menu, or the Theme window chips. It now
  goes through `Palette::for_mode`:
  - Electric Cyan and Paper (no source list) swap to each other, exact literals.
  - Anything else re-derives from its own accent via `from_accent`: custom
    accents, preset-synced accents (the label is kept; they are keyed on a
    single stored hex, not on the name), premades, Random and Synthwave.
  - Asking for the mode a palette is already in returns it unchanged.
- **Hue-0 guard in `generate_harmony`.** `rgb_to_hsl` reports hue 0 for every
  achromatic colour. The Monochromatic and Complementary rules ADD saturation
  (`s + 10`, `s + 20`), so a black or gray accent grew reddish pegs in the
  gradient. Boosts are now skipped for an achromatic base. The `s.max(..)`
  floors the audit named were already gone.
- **Hyperlinks are ink.** `hyperlink_color` was the raw accent. It now goes
  through the same readable-ink walk as `theme::cyan()`, against the window fill.
- **Which binary is this?** About shows the running exe path and its file
  modified time (read at runtime, so an incremental build cannot leave a stale
  stamp). `install-user-startup.ps1` prints `installed vX -> incoming vY (built
  ..., path)` before it copies. Log only: it does not refuse a downgrade.
  `dev-cycle.ps1` already always builds.

## Not done, on purpose

- **Built-ins and `from_colors` stay literal/old-path.** Re-deriving Electric
  Cyan, Paper, Synthwave and the 32 premades from the ladder changes the default
  look and every premade. It needs a human looking at the result; the sweep
  could not launch the GUI. `enforce_readability` still guarantees every one of
  them clears the text and muted floors.
- `on_color` exists in `color.rs` and is only exercised by its own test. The app
  draws no label on a solid accent fill today (active and selected widgets use a
  22 to 30 percent accent over the panel, and egui text is pinned to
  `override_text_color`). Wire it in the first time a filled accent button is
  added.
- The installer does not refuse downgrades (TrontSnap's bootstrap does).

## Verification

`cargo test -p tronteq`: 127 passed before, 136 passed after (9 new tests: 2 in
`color`, 6 in `theme`, 1 for the About date formatter), 0 failed. The exact
output is in the run's PROGRESS.md. The installer change was parse-checked only;
the script was not run because it touches the real install and scheduled task.
