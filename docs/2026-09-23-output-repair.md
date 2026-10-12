# Output picker and inactive EQ diagnosis (2026-09-23)

The running GUI had no adjacent `tronteq-cli.exe`. It used that executable for
device enumeration and discarded every enumeration error with
`unwrap_or_default()`. The result was an empty dropdown and an Apply button
that did nothing. Windows itself reported five active render endpoints.

The default endpoint was Beyond TV (NVIDIA High Definition Audio), ID
`{0.0.0.00000000}.{5331eff1-1c22-4b00-bc42-1221a5476971}`. Its EFX registration
was the Microsoft effect, not TrontEQ. TrontEQ was registered on older
endpoints. WASAPI loopback visualization followed the current default and
therefore continued independently of the EQ.

The persisted telemetry sequence was 454868506 and did not advance between
samples. All meter values were zero. The GUI treated any nonzero sequence as
live forever, hiding the warning and displaying a stale IN/OUT meter. The
saved chain also had bypass enabled and preamp at approximately +6.90 dB.

## Changes

- Enumerate Windows endpoints directly in a shared module, without a CLI
  subprocess. Free endpoint ID allocations and balance COM initialization.
- Refresh the list in the background, retain selections by endpoint ID, and
  expose scan/installer failures. Disable Apply when there is no usable target.
- Pass the stable endpoint ID to installation instead of a stale numeric index.
- Require observed telemetry sequence changes, with a two-second expiry, before
  showing the APO meter. Otherwise show the independently captured LOOPBACK
  output and a visible explanation. Show a bypass explanation in CHAIN/SETUP.
- Resolve the installer DLL relative to its executable, including portable and
  source build layouts. An elevated launch from System32 now finds the DLL.
- Check audio service command results and restart audio when installation exits
  with an error after stopping it. Successful installation no longer claims
  verified live processing.
- Build both executables in `dev-cycle.ps1`. Keep the toolbar controls and
  footer meter readable at narrow widths; truncate long selected device names.

## Verification

`cargo test -p tronteq -p tronteq-cli -p tronteq-shared --quiet`:

```text
33 passed; 0 failed (GUI, including output selection and delay interaction)
4 passed; 0 failed (CLI)
9 passed; 0 failed (shared state, migration, device selection)
```

`cargo build --release -p tronteq -p tronteq-cli` completed successfully.
The existing `uishot` harness rendered all 18 scenes and ended `[uishot] done`
with exit code 0. The running GUI's own GL screenshot confirmed that Beyond TV
appears in the selector and stale telemetry produces the EQ warning.
`tronteq-cli check` from System32 successfully located the source build DLL.

The first Windows administrator consent request was canceled before the
installer launched. The user approved a fresh request on September 24, and
installation completed with exit code 0. Beyond TV's EFX registration now points
to TrontEQ (`{CA64E60A-A3C4-43B8-970F-0360055172F2}`), and Windows Audio restarted
successfully. Its previous FxProperties are backed up in
`target/beyond-tv-before-repair.reg`.

Live verification after installation observed the telemetry sequence advance
from 454869192 to 454869393 over two seconds, with nonzero input and output
levels. For example, one buffer reported input RMS 0.275624 and output RMS
0.087726. Bypass was off (the user disabled it before installation), confirming
the current EQ/dynamics chain was processing. The user also reported hearing
the change. No sound-profile parameters were changed by the repair.
