# CLAUDE.md — TrontEQ

Zero-latency Windows system EQ. Our own Audio Processing Object plus a Rust eframe GUI that drives it through a memory-mapped state file.

## Architecture

Three artifacts. One IPC file.

```
 Rust GUI (tronteq.exe)  ──writes──▶  C:\ProgramData\TrontEq\state.bin
                                              ▲
                                              │ reads every buffer (seqlock)
                                              │
 tronteq-cli.exe                       TrontEqApo.dll
   ├─ sign                              (loaded by audiodg.exe)
   ├─ register CLSID                     ├─ 8-band biquad cascade
   ├─ assign PKEY_FX_StreamEffectClsid   ├─ RBJ peak/lowshelf/highshelf
   └─ uninstall                          └─ DF-II-T, FTZ+DAZ, zero latency
```

**Zero added latency.** APOs run inline in the Windows audio graph. The only cost is the biquad math itself (~48 muls + 40 adds per sample for 8 bands = negligible at 48 kHz).

## Components

| Path | Role |
|---|---|
| `apo/` | C++ COM DLL (`TrontEqApo.dll`). Subclasses `CBaseAudioProcessingObject`. RBJ biquads, DF-II-T, FTZ/DAZ. Reads `EqState` from file-backed mmap. |
| `cli/` | Rust installer. `check` / `list-devices` / `install --device` / `uninstall`. Uses `windows` crate for `IMMDeviceEnumerator` + `IPropertyStore`. |
| `gui/` | Rust eframe app (glow renderer). Draggable 8-band curve + viz layers (`curve.rs`), tabbed inspector CHAIN/VIZ/SETUP (`inspector.rs`), saved sound profiles (`profiles.rs`, JSON in `C:\ProgramData\TrontEq\profiles\`), persisted UI settings (`settings.rs`, settings.json), knobs/theme/spectrogram/visualizer/devices/tray modules. Writes the state file. Runs Medium (asInvoker) since v0.9.0; spawns tronteq-cli elevated for installs. |
| mic stack | Same DLL, second class `TrontEqMicApo` `{68FF0293-9E2F-414B-834B-BE2ED82F607E}` on a capture endpoint. Reads `mic.bin`; chain is preamp -> EQ -> gate (`apo/src/Gate.cpp`) -> AGC/comp/limiter, no delay. Rust side: `shared/src/mic.rs` (layout, presets, `MicHandle`), CLI `install-mic` / `mic ...`, GUI `mic_panel.rs` (MIC tab). |
| `shared/` | Rust crate. `#[repr(C)] EqState` — the IPC contract. **224 bytes** (200 seqlock state + 24 APO-written telemetry). Serde on `Band`/`Dynamics` for profile JSON. Mirrored in `apo/src/EqState.h`. |

## IPC Contract

File: `C:\ProgramData\TrontEq\state.bin` (224 bytes, file-backed memory map).

```
EqState {
  version:   AtomicU64     // offset 0, 8 bytes. Seqlock counter (GUI writes).
  bands[8]:  Band          // offset 8, 128 bytes
  preamp_db: f32           // offset 136 (was bass_boost)
  bypass:    u8            // offset 140
  _pad:      [u8; 3]       // offset 141..=143
  dynamics:  Dynamics      // offset 144, 48 bytes (comp/limiter/AGC params)
  delay_ms:  f32           // offset 192, 4 bytes (A/V-sync delay, GUI-written)
  _reserved: [u8; 4]       // offset 196, 4 bytes (alignment + headroom)
  telemetry: Telemetry     // offset 200, 24 bytes — APO WRITES, GUI reads
}                          //   (seq, in/out peak+rms, gr_db; not seqlocked)

Band {
  freq: f32     // Hz
  gain: f32     // dB
  q:    f32     // dimensionless
  kind: u32     // 0=Peak 1=LowShelf 2=HighShelf 3=HP 4=LP 5=BP 6=Notch 7=AllPass
}
```

Old 144/192-byte files are zero-extended on open; telemetry `seq == 0` means an
APO build that never wrote it. Sound profiles serialize `bands + preamp_db +
dynamics` to JSON (`profiles/*.json`) — same structs, separate from this file.

**Mic stack file:** `C:\ProgramData\TrontEq\mic.bin`, 256 bytes: an `EqState`
(same 224-byte layout, `delay_ms` ignored) followed by `Gate` at offset 224
(`enabled, threshold_db, hysteresis_db, range_db, attack_ms, hold_ms,
release_ms, _reserved`). The gate commits under the same seqlock version. The
capture APO opens it only if it exists; `install-mic` creates it on the `whisper`
preset. Gate ramps are linear in dB (attack/release = time to fully open/close).
Portable gate test: `apo/tests/gate_test.cpp`.

**Seqlock protocol:** The writer stages a complete draft, publishes an odd
version while copying it into the mapping, then commits an even version. The
APO remains lock-free. Rust clients coordinate with nonblocking OS file locks
and bounded snapshot retries; an abandoned odd version returns an error rather
than hanging. The GUI offers explicit recovery from a confirmed sound and
checkpoints completed edits to `state.last-good.json`. Recovery never runs
implicitly and preserves APO telemetry. See `docs/2026-09-24-state-recovery.md`.

**Why file-backed and not `Local\`/`Global\`**: `audiodg.exe` runs in Session 0 (services), GUI runs in the user session. `Local\*` names don't cross sessions. `Global\*` requires `SeCreateGlobalPrivilege` the GUI doesn't have. A file-backed mapping bridges sessions cleanly.

## Build

```bash
# Rust
cd C:\trontstack\tronteq
cargo build --release --workspace

# C++ APO
cd apo
build.bat
```

`apo/build.bat` shells through `vcvars64.bat` (VS 2022 Community) then runs `cl` + `link` against Windows SDK 10.0.26100 headers. Output: `apo/build/TrontEqApo.dll`.

## One-time dev setup

The live GUI is installed at `%LOCALAPPDATA%\Programs\TrontEQ\tronteq.exe`.
The `TrontEQ` task launches it with `--startup` ten seconds after user logon,
at Limited integrity, with no execution limit and ten one-minute failure retries.
Use `install-user-startup.ps1` to deploy/register it; `dev-cycle.ps1` also copies
new GUI/CLI builds into that installation before restarting the task. Building
`target/release` alone does not update the installed copy.

Tray startup uses `tray-icon` 0.25.1 to survive Explorer not being ready. Keep
its fixed tray GUID stable so the Windows pin persists. Defer startup parking
past eframe's first paint, which makes the native window visible. Native checks:
`tools/verify-startup.ps1`; receipt: `target/startup-verification.json`.

Test-signing must be ON (audiodg refuses unsigned APOs). Self-signed cert must be trusted. See `README.md` for the one-shot PowerShell commands.

## CLSID

`{CA64E60A-A3C4-43B8-970F-0360055172F2}` — generated once via `[guid]::NewGuid()`, hard-coded in both `apo/src/Guids.h` and `cli/src/com_reg.rs`. Never regenerate.

## Design Decisions

- **APO flags:** `APO_FLAG_INPLACE | APO_FLAG_SAMPLESPERFRAME_MUST_MATCH | APO_FLAG_FRAMESPERSECOND_MUST_MATCH`. In-place means one buffer pointer, no extra allocation.
- **Audio format:** `KSDATAFORMAT_SUBTYPE_IEEE_FLOAT`, 32-bit float, any channel count, any sample rate. Windows engine converts into/out-of this format for us.
- **Lock-free coefficient updates:** Seqlock on the `version` counter. No mutex in `APOProcess`.
- **Denormals:** `_mm_setcsr(_mm_getcsr() | _MM_FLUSH_ZERO_ON | _MM_DENORMALS_ZERO_ON)` in `APOInitialize`.
- **Biquad form:** Direct Form II Transposed. Best FP stability for boosted/cut peaks.
- **RBJ cookbook:** canonical coefficients for Peak / LowShelf / HighShelf. That's all we implement in POC.
- **Filter recompute only when bands change.** Dirty-flag check at buffer start; skip RBJ math 99% of the time.

## Current GUI

The inspector has CHAIN, VIZ, DATA, and SETUP tabs. CHAIN provides precise band
editing, dynamics, separate A/V sync, session undo/redo, and A/B comparison. VIZ
has a unified 37-scene library, gallery, saved looks, ordered effects, transitions,
render quality, and presentation mode. Visual undo/redo is separate from sound
history; Ctrl+Z/Y follows the active inspector. The complete effect stack can be
enabled/disabled without losing its order or mixes. Sound profiles and visual
looks are separate. See `docs/2026-09-24-handoff.md` for the completed UI pass.

`audio_format.rs` validates the WASAPI mix format; `audio_analysis.rs` runs
multichannel analysis and fixed 60 Hz histories on the capture thread.
`visualizer.rs` handles session lifetime and publishes immutable snapshots. The
GUI renders those snapshots without advancing the analysis clock. See the dated
verification notes under `docs/` for behavior and test receipts.

Per-app EQ, INF packaging, and independently controlled simultaneous outputs
remain outside the current implementation.

## Reference Projects (SAMPLES/, gitignored)

- `microsoft/Windows-driver-samples` audio/sysvad/APO/SwapAPO — APO scaffolding reference
- Equalizer APO — per-user registry + endpoint PropertyStore install pattern
- EasyEffects — drawable-curve UX inspiration
- RBJ Audio-EQ-Cookbook text — biquad coefficient math
