# TrontEQ: Adjustable A/V-sync delay + portable dev-mode

Date: 2026-07-21
Status: approved design, ready for implementation plan

## Motivation

Two asks:

1. **A/V-sync delay.** TrontEQ is a zero-latency system EQ. But when a video's
   audio runs *ahead* of its picture (e.g. Hulu lip-sync drift), a short,
   realtime-adjustable audio delay pulls it back into sync. Default 0 keeps the
   zero-latency promise intact; the delay is opt-in per viewing session.
2. **Portable dev-mode.** Get TrontEQ running on a second machine (Trent's
   wife's PC) with minimal fuss: drop the repo, open Claude, say "dev mode",
   and have it bootstrap. Ship prebuilt binaries so her machine needs no
   toolchain, plus documented full-source-build path and a Claude skill.

Both were confirmed via AskUserQuestion: max delay **2000 ms**, dev-mode scope
**"both" (prebuilt now + docs + skill + full-build path)**, and **deploy live on
the main PC now** (accepting the ~1-2s audiosrv-restart audio cut). The delay
control is a **knob** on the existing UI (no hotkeys).

---

## Part 1: A/V-sync delay

### 1.1 Where it runs

The delay is a ring buffer inside the APO (`apo/src/TrontEqApo.cpp`), applied as
the **final step** of `APOProcess`, after EQ + dynamics + the safety clamp. It
is the only place with the real interleaved-float audio buffer.

- `delay_ms == 0` -> the ring is skipped entirely. The zero-latency path is
  byte-for-byte identical to today. **This preserves the core promise.**
- The delay affects **all** audio on the target device (no per-app targeting;
  that remains a documented non-goal). Workflow: bump it up for a mis-synced
  video, reset to 0 for games/music.
- **Direction:** we can only ever *delay* audio, never advance it. That is
  exactly the mis-sync case here (audio ahead of video), so delaying is correct.
- Bypass = full passthrough, so delay is **off while bypassed** (the existing
  early-return in `APOProcess` already skips all post-input processing).

### 1.2 IPC / ABI change

`delay_ms` becomes a new GUI-written, seqlock-protected field, mirrored in
`shared/src/lib.rs` and `apo/src/EqState.h`. It sits right after `dynamics`,
before the APO-written telemetry block:

```
 offset  field         bytes
 0       version       8     (seqlock counter)
 8       bands[8]      128
 136     preamp_db     4
 140     bypass        1
 141     _pad[3]       3
 144     dynamics      48
 192     delay_ms      4     <-- NEW (f32, ms, seqlock'd like preamp)
 196     _reserved     4     <-- NEW (8-byte align + future headroom)
 200     telemetry     24    (APO-written; shifted down 4 bytes)
 224     = STATE_BYTES
```

Constants: `STATE_CORE_BYTES` 192 -> **200**, `STATE_BYTES` 216 -> **224**.
Update every `static_assert` / const assertion in both mirrors, plus
`layout_matches_abi`.

New Rust surface:
- `EqStateWrite::set_delay(&mut self, ms: f32)`
- `Snapshot.delay_ms: f32`
- `EqState::default_flat()` sets `delay_ms: 0.0`, `_reserved: [0;4]`
- `StateWriter::write_state(..)` gains a `delay_ms: f32` parameter

### 1.3 Backward-compatible migration

The old telemetry block lived at offset 192, exactly where `delay_ms` now sits.
A pre-224 `state.bin` therefore has stale telemetry bytes at the new delay
offset. `StateHandle::open_at` must not let that leak in as a phantom delay:

- Capture `old_len` before `set_len`.
- After mapping, if the file is a fresh/empty init, `default_flat` already sets
  delay 0.
- Else if `0 < old_len < STATE_BYTES` (an existing older file that just grew):
  under a seqlock write, force `delay_ms = 0.0` **and** zero the relocated
  telemetry block (so a stale `seq` is not misread as "APO is live").

The APO side (`SharedState.cpp::TryOpen`) already tolerates a short file via its
`telemetryFits` logic. With the new constants, a 216-byte file gives
`216 >= kStateCoreBytes(200)` true, `telemetryFits` false -> maps core only, no
telemetry write, and delay would read old bytes. The **deploy sequence
guarantees the file is 224 before the new APO maps it** (see 1.6), so the APO
always sees a clean `delay_ms`.

### 1.4 APO DSP (ring buffer)

New members on `TrontEqApo`:
```cpp
std::vector<float> m_delayBuf;    // interleaved frames * channels
uint32_t m_delayMaxFrames = 0;    // capacity in frames
uint32_t m_delayWrite = 0;        // write cursor (frames)
uint32_t m_delayFramesPrev = 0;   // for fresh-enable detection
```
`LocalState` gains `float delay_ms = 0.0f;`, and `SharedStateReader::Read`
copies `out.delay_ms = m_view->delay_ms;`.

**Allocation in `LockForProcess`** (NOT the RT thread, so heap alloc is safe):
```cpp
const double fs = m_framesPerSecond > 0 ? m_framesPerSecond : 48000;
m_delayMaxFrames = (uint32_t)std::ceil(kMaxDelayMs / 1000.0 * fs) + 1; // 2000 ms
m_delayBuf.assign((size_t)m_delayMaxFrames * m_channels, 0.0f);
m_delayWrite = 0;
m_delayFramesPrev = 0;
```
`kMaxDelayMs = 2000`. Worst case (192 kHz, 16 ch) ~= 24.6 MB; typical
(48 kHz, 2 ch) ~= 768 KB.

**Apply in `APOProcess`**, final step (after clamp + metering, RT-safe, no
syscalls):
```cpp
float dms = std::isfinite(m_cached.delay_ms) ? m_cached.delay_ms : 0.0f;
dms = std::clamp(dms, 0.0f, (float)kMaxDelayMs);
uint32_t delayFrames = (uint32_t)std::lround(dms / 1000.0f * fs);
if (delayFrames >= m_delayMaxFrames) delayFrames = m_delayMaxFrames - 1;

if (delayFrames > 0 && !m_delayBuf.empty()) {
    if (m_delayFramesPrev == 0) {
        std::fill(m_delayBuf.begin(), m_delayBuf.end(), 0.0f); // clean enable
    }
    const uint32_t cap = m_delayMaxFrames, ch = m_channels;
    for (uint32_t f = 0; f < in->u32ValidFrameCount; ++f) {
        // write current output frame, then read a delayed one
        float* slotW = &m_delayBuf[(size_t)m_delayWrite * ch];
        float* frame = &outBuf[(size_t)f * ch];
        std::memcpy(slotW, frame, ch * sizeof(float));
        uint32_t rp = (m_delayWrite + cap - delayFrames) % cap;
        std::memcpy(frame, &m_delayBuf[(size_t)rp * ch], ch * sizeof(float));
        m_delayWrite = (m_delayWrite + 1) % cap;
    }
}
m_delayFramesPrev = delayFrames;
```
When first enabled (0 -> N) the ring is zeroed, giving a clean `delay`-length
silence, which is the natural behavior of switching a delay on. Dragging the
knob mid-playback can click on large jumps; the nudge buttons keep trims small,
and this is acceptable for a manual sync tool (POC scope, not a smoothing DSP).

### 1.5 GUI

Reuse `knob::knob` (painter-drawn rotary; vertical drag to turn, Shift = fine,
scroll to nudge, returns `true` on change).

- **New "A/V SYNC" section at the top of the CHAIN tab** (`inspector.rs::chain_tab`),
  above SIGNAL CHAIN. CHAIN is the default tab and always visible in the right
  side panel, matching "open on top of the video and adjust live".
- Delay knob: `knob(ui, &mut delay, 0.0..=2000.0, "delay", " ms", 0, false, SYNC_ACCENT)`
  where `SYNC_ACCENT` is amber/gold (e.g. `Color32::from_rgb(255, 190, 90)`),
  distinct from cyan/red/green already in use. `log=false` (range starts at 0).
- Fine trims: `-25` `-5` `+5` `+25` ms buttons + a `Reset 0` (the knob's ~60 ms
  scroll step is too coarse for lip-sync alone; Shift-drag also gives fine).
- One-line hint (muted, small): "Delay audio to match late video. Affects all
  sound on this device."
- **Status-bar indicator:** when `delay_ms > 0`, show `DELAY 120 ms` in the
  bottom bar in `SYNC_ACCENT`, so it is never silently on. Plain ASCII text (no
  emoji/clock glyph: Rajdhani renders those as tofu boxes).

Wiring:
- `App` gains `delay_ms: f32`, loaded from the startup snapshot (rides
  `state.bin` exactly like `preamp_db`; persists across GUI restarts, stays
  active after Quit per the existing "chain keeps running" rule).
- `inspector::show` adds a `let mut delay = app.delay_ms;` copy-local, passes
  `&mut delay` into `chain_tab`, and writes it back on `chain_changed`.
- `chain_tab` signature gains `delay: &mut f32`.
- `App::commit()` / the `StateWriter::write_state` call include `delay_ms`.
- **Profiles are unaffected:** delay is not part of `Band`/`Dynamics`, so the
  profile JSON (bands + preamp + dynamics) automatically excludes it. Matches
  "profile == sound savedata"; A/V sync is a general/session setting.

### 1.6 Deploy live on the main PC (this session)

1. `cargo build --release --workspace` (shared, cli, gui).
2. `apo/build.bat` (rebuild `TrontEqApo.dll`; the ABI static_asserts guard the
   224-byte mirror at compile time).
3. Extend `state.bin` to 224 bytes and zero bytes 192..224 (PowerShell in the
   deploy script; old APO still mapped at 216, unaffected).
4. Stop `audiosrv` (unloads old APO, releases the DLL lock).
5. Sign + copy the new DLL over the installed one (same CLSID/EFX, no device
   re-assignment).
6. Start `audiosrv` (new APO loads, maps 224, reads delay 0). ~1-2s audio gap.
7. Relaunch the GUI via `schtasks /run /tn TrontEQ` (reads 224, shows the knob).
8. Verify: GUI up, telemetry `seq` advancing, set delay ~500 ms, confirm audible
   late-but-clean audio, reset to 0 confirms instant.

New script `deploy-delay.ps1`, modeled on `deploy-vumeter.ps1` (which did the
192 -> 216 growth) with size 224 + the zero step.

### 1.7 Testing

- `cargo test -p tronteq-shared`: extend `layout_matches_abi` (new sizes/offset),
  add delay to the seqlock roundtrip, add a migration test (write an old-size
  file with nonzero bytes at offset 192, open, assert `delay_ms == 0`).
- `cargo test -p tronteq-cli`: unchanged, must still pass.
- GUI builds (`cargo build -p tronteq`); `cargo test -p tronteq` stays skipped
  (admin-manifest test bin can't run non-elevated, known).
- APO: `build.bat` compiles (static_asserts). Audible verification is the live
  deploy above (Trent is the E2E tester).

---

## Part 2: Portable dev-mode for a second machine

Scope: **both** paths. Ship prebuilt so it works immediately; document + script
the full source build; add a Claude skill so "dev mode" bootstraps it.

### 2.1 De-hardcode paths (do here first, "fix it here")

Audit and remove `C:\trontstack\tronteq` assumptions so the repo runs from any
folder:
- **Autostart registration** (`cli/src/setup.rs` register-autostart): the
  scheduled-task action path must come from `std::env::current_exe()`, not a
  literal. Verify + fix if needed.
- **`apo/build.bat`**: already `%~dp0`-relative; add a `vswhere`-based fallback
  to locate `vcvars64.bat` instead of the hardcoded VS 2022 Community path.
- **`dev-cycle.ps1`** and any other helper: derive the repo root from
  `$PSScriptRoot`; the `TrontEQ` scheduled task's action path is machine-local
  and comes from register-autostart (so it is correct once that uses
  `current_exe`).

### 2.2 Prebuilt bundle

`make-dist.ps1` assembles `dist\TrontEQ\`:
- `TrontEqApo.dll` (signed with the dev cert)
- `tronteq.exe`, `tronteq-cli.exe`
- `dev-cert.cer` (public cert only; the private `.pfx` stays on the main PC)
- `bootstrap.ps1`, `SETUP.md`

Her machine needs **no Rust / VS / Windows SDK** for this path. The signed DLL
carries its Authenticode signature; her machine only needs to trust the cert and
enable test-signing. (Sharing a self-signed code-signing public cert to trust on
a machine you own is fine and authorized.)

### 2.3 `bootstrap.ps1`

Elevated, `$PSScriptRoot`-relative, idempotent, detects current state:
- **Phase 1 (needs admin, one reboot):** enable test-signing
  (`bcdedit /set testsigning on`); import `dev-cert.cer` into LocalMachine\Root
  + TrustedPublisher (prebuilt path) or create a fresh cert (full-build path);
  create `C:\ProgramData\TrontEq` with the APPLICATION-PACKAGES ACL; prompt for
  a reboot.
- **Phase 2 (post-reboot):** verify test-signing active; `tronteq-cli install`
  to the **default** device; register autostart; launch the GUI.
- Skips whatever is already done (checks `bcdedit`, cert store, install state).

The single test-signing reboot is unavoidable (kernel flag; audiodg refuses
unsigned APOs). Bootstrap batches all admin into one elevated pass = exactly one
reboot (per Trent's reboot-aversion preference).

### 2.4 `SETUP.md`

Fresh-machine walkthrough: prerequisites, the two-phase bootstrap around the one
reboot, both the prebuilt fast path and the full source-build path (install Rust
via rustup, VS 2022 Build Tools + Windows SDK, `cargo build --release
--workspace`, `apo/build.bat`, generate a local cert).

### 2.5 `.claude/skills/setup/SKILL.md` (project skill)

Triggers on "dev mode", "set up tronteq", "/setup", "get tronteq running". Drives
Claude through: detect machine state -> run the right bootstrap phase -> guide the
reboot -> install to default device -> launch + verify. Documents the
full-source-build branch as the alternative. Lives in the repo so opening Claude
in the folder on any machine makes it available.

### 2.6 Testing / verification

- Path audit: grep the repo for `C:\\trontstack` / `trontstack\\tronteq`; confirm
  none remain in build/run/autostart paths.
- `bootstrap.ps1`: dry-run the detection logic on the main PC (where test-signing
  is already on, cert already trusted, APO already installed) and confirm it
  reports "already set up" without redoing steps.
- `make-dist.ps1`: confirm `dist\TrontEQ\` contains all five artifacts and the
  DLL is signed (`Get-AuthenticodeSignature`).
- Real end-to-end on the wife's machine is Trent's to run (fresh install +
  reboot); the deliverables make it a one-command + one-reboot flow.

---

## Out of scope

- Per-app / per-video delay targeting (system-wide only).
- Delay smoothing/crossfade on knob jumps (manual tool, POC).
- Advancing audio (physically impossible; not needed for ahead-of-video).
- Hotkeys (Trent will pop the window over the video and adjust the knob live).
- Auto-detecting sync offset (manual dial by eye).
