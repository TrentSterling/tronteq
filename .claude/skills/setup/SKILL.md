---
name: setup
description: Get TrontEQ running on this machine — bootstraps test-signing, the dev cert, APO install, and autostart. Triggers on "dev mode", "set up tronteq", "get tronteq running", "/setup", or when TrontEQ isn't installed/running yet on a machine.
---

# TrontEQ setup

Drives a fresh (or partially-set-up) Windows machine through
`bootstrap.ps1` to a working TrontEQ install. Read `SETUP.md` in the repo
root once for full background — this skill is the short operational loop.

## 0. Figure out which layout you're in

```powershell
Get-ChildItem . -Name
```

- If you see `TrontEqApo.dll`, `tronteq.exe`, `tronteq-cli.exe`, `dev-cert.cer`,
  `bootstrap.ps1` flat in the current folder → **prebuilt dist** layout
  (this is `dist\TrontEQ\`, copied from another machine).
- If you see `apo\`, `cli\`, `gui\`, `shared\`, `Cargo.toml` → **repo root**,
  full source-build layout. Check whether it's already built:
  `Test-Path apo\build\TrontEqApo.dll` and
  `Test-Path target\release\tronteq-cli.exe`.
  - Not built yet → go to **step 1 (source build)** below first.
  - Already built → skip straight to **step 2**.

**No toolchain and don't want to build?** Grab the prebuilt release instead
(needs GitHub read access to the private repo + `gh auth login`):
```powershell
gh release download v0.8.0 -R TrentSterling/tronteq
Expand-Archive .\TrontEQ.zip -DestinationPath .\TrontEQ ; cd .\TrontEQ
```
That extracted folder is the **prebuilt dist** layout — the user can just
double-click `Install TrontEQ.cmd`, or continue with **step 2** below.

## 1. Source build (only if you're in a repo root with no prior build)

Confirm the toolchain is present, then build:

```powershell
rustc --version   # if this fails: point the user to https://rustup.rs, MSVC toolchain
cargo build --release --workspace
cmd /c apo\build.bat
```

`apo\build.bat` tries the hardcoded VS2022 Community path first, then falls
back to `vswhere.exe`. If it errors with "vcvars64.bat not found", the user
needs the **Desktop development with C++** workload from VS 2022 Build Tools
(or full VS 2022) — tell them, don't try to install it yourself.

If `cargo build` fails on a missing toolchain, direct the user to install
Rust via rustup (`https://rustup.rs`) with the default MSVC target, then retry.

## 2. Run bootstrap — must be elevated

`bootstrap.ps1` self-checks for admin rights and refuses to run un-elevated
(no self-elevation — a UAC prompt needs a human to click "Yes", which you
can't do). **You cannot run this from an unprivileged shell.** Ask the user
to open an elevated PowerShell (Win key → type "PowerShell" → Ctrl+Shift+Enter,
or right-click → *Run as administrator*) and either:

- run the commands there themselves and paste you the output, or
- if your shell tooling supports it, launch one for you and wait for the
  human to approve the UAC prompt.

Then, from that elevated shell, in the folder identified in step 0:

```powershell
.\bootstrap.ps1
```

## 3. Read the result

- **`REBOOT REQUIRED`** → Phase 1 (test-signing + cert + ACL) finished but
  test-signing needs a reboot to take effect. Tell the user to reboot, then
  come back and run `.\bootstrap.ps1` again from the same elevated shell /
  folder. This is the ONE reboot the whole flow needs — don't ask for more.
- **`RESULT: already set up. Nothing to do.`** → done, nothing to change.
  Skip to step 4 (verify).
- **`RESULT: bootstrap complete...`** → Phase 2 ran (install / autostart /
  launch, whichever weren't already done). Go to step 4.
- **`ERROR: ...`** → read the message, it names the exact missing file or
  failed step (e.g. missing `apo\build\TrontEqApo.dll` → go back to step 1;
  `tronteq-cli install failed` → check
  `C:\ProgramData\TrontEq\bootstrap-install.err.log` for the reason, most
  often test-signing not really active yet, or no active output device).

## 4. Verify

```powershell
Get-Process tronteq -ErrorAction SilentlyContinue
Get-Content C:\ProgramData\TrontEq\bootstrap.log -Tail 20
```

The GUI window should be visible with the 8-band curve. Ask the user to drag
a band and confirm they hear the change on system audio — that's the real
end-to-end check; nothing else proves the APO is actually wired into the
audio graph.

## Notes

- `bootstrap.ps1` is idempotent — safe to re-run any time as a health check;
  it reports and skips whatever's already done (see SETUP.md's table).
- Never run `tronteq.exe` directly yourself to "test" it, and never touch
  `C:\ProgramData\TrontEq\state.bin` by hand — let `bootstrap.ps1` /
  `tronteq-cli.exe` own that file.
- This skill only *drives* the scripts; it does not reimplement their logic.
  If `bootstrap.ps1`'s behavior needs to change, edit the script, not this
  skill.
