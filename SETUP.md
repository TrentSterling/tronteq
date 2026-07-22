# TrontEQ — fresh-machine setup

Two paths. Both end at `bootstrap.ps1` and the same one-reboot flow. Pick one:

- **[Prebuilt](#prebuilt-fast-path)** — no Rust, no Visual Studio, no Windows SDK. Just Windows.
- **[Full source build](#full-source-build-path)** — you have (or want) the toolchain and want to build from source.

## Why a reboot at all?

`audiodg.exe` (the Windows audio engine process) refuses to load an
unsigned/self-signed Audio Processing Object unless Windows **test-signing
mode** is on. That's a kernel boot flag (`bcdedit /set testsigning on`) — it
only takes effect after a reboot. There's no way around this for a
self-signed dev cert, so `bootstrap.ps1` batches every other admin step into
the *same* elevated pass and asks for exactly **one** reboot.

## Prerequisites (both paths)

- Windows 10 or 11, x64.
- An account with administrator rights (you'll need one UAC / elevated shell).
- Nothing else for the prebuilt path. VS Build Tools + Windows SDK for the source path (below).

---

## Prebuilt fast path

1. Get `dist\TrontEQ\` onto the machine (zip it from the main PC via
   `make-dist.ps1`, copy over, extract). You should have:
   ```
   dist\TrontEQ\
     TrontEqApo.dll
     tronteq.exe
     tronteq-cli.exe
     dev-cert.cer          <- PUBLIC cert only. The private key never leaves the main PC.
     bootstrap.ps1
     SETUP.md               (this file)
     Install TrontEQ.cmd    <- one-click installer
     Launch TrontEQ.cmd     <- one-click launcher
   ```

### Easiest: one-click

2. **Double-click `Install TrontEQ.cmd`.** It asks for admin once (click *Yes*),
   then runs the whole bootstrap. If it needs the one reboot, **just reboot** —
   TrontEQ finishes installing and launches by itself on the next login. To open
   it any time after that, double-click `Launch TrontEQ.cmd` (it also starts on
   its own every login). That's it — skip the rest of this section.

### Or do it by hand

2. Open an **elevated** PowerShell (right-click Start → *Terminal (Admin)*,
   or *Windows PowerShell (Admin)*).
3. `cd` into the extracted `dist\TrontEQ\` folder.
4. Run:
   ```powershell
   .\bootstrap.ps1
   ```
5. **Phase 1** runs: enables test-signing, generates a machine-local
   signing cert (trusted alongside the shipped `dev-cert.cer`), sets up
   `C:\ProgramData\TrontEq`. If test-signing was already off, it prints
   `REBOOT REQUIRED` and stops.
6. **Reboot.**
7. Re-open an elevated PowerShell, `cd` back into `dist\TrontEQ\`, run
   `.\bootstrap.ps1` again. This time it verifies test-signing is on, then
   runs **Phase 2**: installs the APO onto your default output device,
   registers autostart, and launches the GUI.
8. You should see the TrontEQ window with the 8-band curve. Drag a band —
   you should hear it on your system audio immediately.

Re-running `bootstrap.ps1` at any point is safe — it detects what's already
done and skips it (see "Idempotency" below).

---

## Full source-build path

1. **Install Rust** via [rustup](https://rustup.rs) (`rustup-init.exe`,
   accept defaults — stable toolchain, MSVC target).
2. **Install Visual Studio 2022 Build Tools** (or full VS 2022 — Community
   is free): during setup, select the **"Desktop development with C++"**
   workload. This pulls in the MSVC toolset and the Windows 10/11 SDK that
   `apo\build.bat` needs (targets Windows SDK 10.0.26100 headers; a recent
   SDK from the workload is fine — the build script doesn't pin an exact SDK
   version).
3. Clone/copy the `tronteq` repo to this machine (any path — nothing in the
   build is hardcoded to a location anymore).
4. Open an elevated PowerShell, `cd` into the repo root.
5. Build everything:
   ```powershell
   cargo build --release --workspace
   cmd /c apo\build.bat
   ```
   `apo\build.bat` tries the known VS2022 Community path first, then falls
   back to `vswhere.exe` to locate whatever VS2022+ edition you installed
   (Community/Professional/Enterprise/Build Tools all work).
6. Run:
   ```powershell
   .\bootstrap.ps1
   ```
7. Same as the prebuilt path from here: Phase 1 enables test-signing and
   (since no cert exists yet) generates a fresh local dev cert; reboot if
   prompted; run `.\bootstrap.ps1` again for Phase 2.

If you'd rather build the dist bundle yourself for a *third* machine, run
`.\make-dist.ps1` from the repo root after step 5 — it assembles
`dist\TrontEQ\` from your local build plus your local
`C:\ProgramData\TrontEq\dev-cert.cer` (public half only).

---

## Idempotency — what re-running `bootstrap.ps1` does

Every step is gated behind a state check:

| Check | Skips if... |
|---|---|
| test-signing | `bcdedit /enum {current}` already shows `testsigning Yes` |
| dev cert | `C:\ProgramData\TrontEq\dev-cert.pfx` already exists |
| APO install | the CLSID is already registered *and* `TrontEqApo.dll` is already at `C:\ProgramData\TrontEq\` |
| autostart | the `TrontEQ` scheduled task already exists |
| launch | a `tronteq` process is already running |

On a machine that's fully set up already, `bootstrap.ps1` prints
`RESULT: already set up. Nothing to do.` and makes no changes — safe to run
again any time (e.g. after pulling a repo update) as a health check.

## Troubleshooting

- **"vcvars64.bat not found"** — install the "Desktop development with C++"
  workload (source path only).
- **GUI won't launch / no sound change** — run `tronteq-cli.exe check` for a
  readout of test-signing / cert / DLL-build state, and
  `tronteq-cli.exe list-devices` to confirm your default output is what you
  expect.
- **Changed your default output device later** — re-run
  `tronteq-cli.exe install` (bootstrap only auto-attaches on first install).
- **Full log** — `C:\ProgramData\TrontEq\bootstrap.log` (also
  `bootstrap-install.out.log` / `.err.log` for the install step specifically).
