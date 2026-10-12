# Bounded sound-state access, September 24, 2026

An interrupted write could leave `state.bin` with an odd version. The old GUI
reader spun forever on that file, so restarting could hang before the window
opened. The baseline probe reproduced this against a disposable file and killed
only its own child after two seconds (`target/recovery-baseline.log`).

Rust reads now have a bounded retry count. Writes stage a complete draft before
changing the mapped data. A local mutex and nonblocking OS file lock coordinate
current Rust clients; a panic while preparing a draft leaves the mapped state
unchanged. The 224-byte ABI and the APO's lock-free reads are unchanged. Old
binaries that write mapped memory without the file lock do not participate in
this coordination.

If a read or write cannot complete, the GUI pauses sound editing and offers
Reload current state, Restore confirmed sound, or Keep visualizer open. The
dialog displays the candidate's bands, preamp, bypass, dynamics, and A/V delay.
A separate collapsed action can explicitly reset to flat with bypass enabled
and delay cleared. Nothing automatically overwrites the interrupted sound.

Recovery requires both the writer lock and the exact observed version. A stale
decision requires a reload. Recovery preserves the APO-owned telemetry and
reserved bytes. A missing or invalid backup disables Restore confirmed sound.

Successful sound states are checkpointed to `state.last-good.json` beside the
state file after a completed gesture. The checkpoint is synced and renamed into
place. Save errors leave the previous backup intact, show a footer message, and
retry after a delay. The app never saves its display-only fallback as a recovery
candidate.

## Verification

- Shared fixtures cover abandoned odd versions, bounded reads, unchanged data on
  failed reads/writes, stale recovery decisions, competing writer locks, invalid
  recovery values, panicking drafts, and unchanged telemetry/reserved bytes.
- GUI fixtures cover interrupted startup with a valid backup, missing/corrupt
  backup, explicit restoration and reset, failed checkpoint replacement, retry,
  and the recovery dialog's explicit buttons.
- Cross-process probe: `SEQLOCK PASS: even snapshot; bounded odd snapshot without
  mutation; explicit recovery preserved controls`.
  Receipt: `target/recovery-process-probe.log`.
- Workspace tests and actual GL recovery previews are included in
  [the UI verification](2026-09-24-ui-polish.md).
- The released GUI created its recovery checkpoint successfully. Re-encoding its
  JSON fields matched every live band, preamp, bypass, dynamics, and delay byte;
  its version matched the confirmed live version (68734). Receipt:
  `target/ui-polish-backup-check.json`.

Recovery faults were injected only into disposable fixtures. The installed
sound state was not deliberately corrupted or reset.
