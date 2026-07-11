# TrontEQ

Zero-latency Windows system EQ with a hand-drawable curve. Our own Audio Processing Object (APO) lives inside the Windows audio pipeline; our Rust GUI lets you grab 8 band nodes and hear the change on everything — YouTube, Discord, games — in real time.

No VB-Cable. No Equalizer APO. No drivers. Our code, our install, our stack.

## Status

POC. See `CLAUDE.md` for architecture and `CHANGELOG.md` for what shipped.

## Components

- **`apo/TrontEqApo.dll`** (C++ COM) — the real-time processor, loaded by `audiodg.exe`
- **`cli/tronteq-cli`** (Rust) — sign, register, attach to an output endpoint, uninstall
- **`gui/tronteq`** (Rust + eframe) — the draggable curve
- **`shared/`** (Rust) — the `EqState` struct shared with the C++ APO via file-backed mmap

## One-time setup (do once, never again)

APOs are loaded into a protected Windows process (`audiodg.exe`) that refuses unsigned DLLs. For development we use test-signing + a self-signed cert.

**Run from an elevated PowerShell once:**

```powershell
bcdedit /set testsigning on
# Reboot Windows

# Create signing cert:
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=TrontEQ Dev" `
    -KeyUsage DigitalSignature -FriendlyName "TrontEQ Dev" `
    -CertStoreLocation "Cert:\CurrentUser\My"
$pwd = ConvertTo-SecureString -String "tronteq" -Force -AsPlainText
mkdir C:\ProgramData\TrontEq -Force
Export-PfxCertificate -Cert $cert -FilePath C:\ProgramData\TrontEq\dev-cert.pfx -Password $pwd
Export-Certificate  -Cert $cert -FilePath C:\ProgramData\TrontEq\dev-cert.cer
Import-Certificate -FilePath C:\ProgramData\TrontEq\dev-cert.cer -CertStoreLocation Cert:\LocalMachine\Root
Import-Certificate -FilePath C:\ProgramData\TrontEq\dev-cert.cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher
```

## Build

```bash
# Rust crates
cd C:\trontstack\tronteq
cargo build --release --workspace

# C++ APO DLL (uses MSVC via vcvars64)
cd apo
build.bat
```

## Install to an output device

```bash
cd C:\trontstack\tronteq
cargo run -p tronteq-cli -- check
cargo run -p tronteq-cli -- list-devices
cargo run -p tronteq-cli -- install --device 0
```

## Run the GUI

```bash
cargo run -p tronteq
```

Drag a band up, hear the change instantly.

## Uninstall

```bash
cargo run -p tronteq-cli -- uninstall
```

## Credits

- **aFoolsDuty**: ideas + inspiration; the hardware-dongle escape hatch from the signing problem was his call first.

## License

MIT. See `LICENSE`.
