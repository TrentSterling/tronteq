# TrontEQ Red-Team Findings (S56)

Three adversarial subagents reviewed the codebase by layer. **Verdict up front:** the 192-byte
seqlock IPC and the Rust/C++ `repr(C)` layout are byte-exact and correct (all three confirmed) —
the load-bearing contract is solid. The risk is in the orchestration around it.

Status key: ✅ fixed+shipped · ⏳ open

---

## FIXED — GUI pass 1 (commit 625f468, shipped)
- ✅ **NaN/Inf sample poison** — `visualizer.rs` now sanitizes mono + L/R samples at the source; one
  glitchy packet used to permanently poison the RMS meter (`msq` additive accumulator) + goniometer.
- ✅ **panic=abort crash on bad sample rate** — FFT bin `lo`/`hi` now clamped to `[1, nyq_bin]`; a
  tiny/garbage reported rate made `hz_per_bin~0` → index past `work.len()` → panic → process abort.
- ✅ **Channel bound** — `usable` now requires `channels <= 64` (absurd nChannels = OOB read).
- ✅ **Seqlock churn** — `curve.rs`/`knob.rs` only flag `changed` when the value actually moved
  (was writing to the APO every frame during a drag, even pinned at a rail).
- ✅ **Knob NaN guard** — log-taper `to_t` guarded against a corrupt `<=0` state value.
- ✅ **Device-index clamp** — `selected_device` clamped after a device-list refresh.

## OPEN — APO (C++ in audiodg) — HIGHEST PRIORITY (needs rebuild + reinstall)
- ⏳ **CRITICAL RT-thread logging** — `TrontEqApo.cpp` `ApoLog` does `OutputDebugStringA`×3 +
  `CreateFileW`/`WriteFile` from inside `APOProcess` (first buffer). Never do I/O on the audio
  thread. Remove from the RT path (lock-free POD ring drained off-thread if diagnostics needed).
- ⏳ **CRITICAL per-buffer `CreateFileW`** — `m_shared.TryOpen()` is called from `APOProcess` when
  `state.bin` is absent → full file-open+map syscall storm per buffer. Open only in `LockForProcess`;
  rate-limit reopen; pass-through if absent.
- ⏳ **CRITICAL FTZ/DAZ on wrong thread** — denormal flush set in `LockForProcess`, but MXCSR is
  per-thread and the RT mixer thread differs. Call `EnableDenormalFlushing()` at the top of
  `APOProcess` (the code comment already admits the doubt). Denormal silence-stall otherwise.
- ⏳ **CRITICAL use-after-unmap** — `UnlockForProcess` calls `m_shared.Close()` (UnmapViewOfFile) on
  a non-RT thread while `APOProcess` may still be reading the view → AV inside audiodg = system
  audio crash. Fix: keep the mapping open across lock cycles; only Close in the destructor.
- ⏳ **HIGH no param clamp / NaN guard** — `ComputeBiquad` doesn't bound `gain`; a corrupt/malicious
  `state.bin` → inf/NaN coeffs or unstable poles → full-scale noise to the user's ears. Clamp
  gain∈[-48,48], q∈[0.05,64], preamp∈[-24,24], reject non-finite coeffs, sanitize input samples,
  scrub DF-II-T state on non-finite.
- ⏳ **HIGH >8ch stride bug** — `min(channels, 8)` clamp used as buffer stride scrambles 7.1.4/Atmos
  (12ch). Reject >8ch in `LockForProcess` or raise `kMaxChannels` + `m_state`.
- ⏳ **HIGH BUFFER_SILENT** — with separate in/out buffers (the EFX case we install into), memset the
  output buffer on silent instead of trusting the consumer to honor the flag.

## OPEN — CLI / installer (Rust) — security, takes effect next install (release-blocker)
- ⏳ **HIGH cert disclosure** — `setup.rs` `icacls (RX)` grant is `(OI)(CI)/T` over the whole
  `C:\ProgramData\TrontEq`, making `dev-cert.pfx` (password hardcoded `"tronteq"` in `signing.rs`)
  readable by every AppContainer app. Scope the grant to `state.bin` only; move the cert to an
  admin-only path; stop hardcoding the password (env/prompt, or sign by thumbprint from the store).
- ⏳ **CRITICAL state.bin writable by standard users** — default ProgramData inheritance lets a
  standard local user write `state.bin` → inject EQ/dynamics into audiodg (pairs nastily with the
  missing APO clamp). Create the file with an explicit admins+SYSTEM-write / read-only-others DACL.
- ⏳ **HIGH PATH hijack** — `icacls`/`net`/`schtasks`/`bcdedit` invoked by bare name while elevated;
  resolve absolute `%SystemRoot%\System32\` paths. (`signtool` already does this right.)
- ⏳ **HIGH privilege lifetime** — SeBackup/SeRestore enabled in `open_fxproperties` and never
  disabled → ACL-bypass live for the whole session. Wrap in an RAII drop guard; enable per-op.
- ⏳ **CRITICAL seqlock init race / not panic-safe** (`shared/src/lib.rs`) — `open_or_init` fills
  bands with plain stores while `version` stays even (relies on the C++ reader's undocumented
  `v1 != 0` guard); and `write()` leaves `version` odd forever if the closure panics. Bracket init
  in the seqlock (odd sentinel via compare-exchange), add a drop guard restoring even parity.
- ⏳ **MEDIUM** — strict canonical-GUID validation on `endpoint_reg_guid` (registry path traversal
  under SeRestore); `cmd_uninstall` swallows enum failure and prints `[ok]` while leaving the APO
  attached; check `AdjustTokenPrivileges` for `ERROR_NOT_ALL_ASSIGNED`; resolve the DLL-to-sign from
  `current_exe()` not CWD; autostart task pinned to an admin-only location.

## OPEN — GUI perf (nice-to-have)
- ⏳ Spectrogram rebuilds + full-uploads the whole texture every frame (should scroll one row).
- ⏳ Per-frame Vec allocs/clones; composite curve + meshes rebuilt every frame even when unchanged.
- ⏳ Repaints at 60fps even when hidden-to-tray (burns a core 24/7) — gate repaint on visibility.
- ⏳ Tray Quit → `process::exit(0)` skips Drop → leaks the capture thread mid-WASAPI; use a graceful
  shutdown that joins the visualizer.

## Modularization (requested)
- `main.rs` (748 lines) is an 8-responsibility god object. Concrete split (from agent C): thin
  `main.rs` (entry) + `app.rs` (App + update orchestrator) + `tray.rs` + `win.rs` + `ui/{toolbar,
  device_bar, chain_panel, statusbar, canvas, about, meter}.rs`. The existing "mutate Copy locals,
  write back after the closure" pattern makes each panel a `fn panel(&mut needed_fields, ui) -> bool`.
- **APO**: lift the RT signal path (`APOProcess` body → seqlock read into scratch → EQ → dynamics)
  into a `DspChain` unit provably free of alloc/syscall/log; keep COM plumbing in `TrontEqApo.cpp`.
- **CLI**: a `system_tool(name) -> Command` helper (System32 abs path) + a privilege RAII guard.

## Doc rot
- `CLAUDE.md` IPC section + `EqState.h` line-2 comment say "144 bytes" / omit `dynamics`+`preamp_db`;
  the real contract is 192 bytes (code agrees with itself via static asserts). Update the docs.
