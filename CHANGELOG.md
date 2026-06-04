# Changelog

## [0.1.0] - 2026-04-17

Initial POC. Drag an 8-band parametric curve, hear it on any Windows output device, zero added latency.

- Own APO (`TrontEqApo.dll`, C++ COM) loaded into `audiodg.exe`
- Installer CLI (`tronteq-cli`) with `check`, `list-devices`, `install --device`, `uninstall`
- GUI (`tronteq`, eframe + egui 0.33) with draggable curve
- Shared state via file-backed memory map at `C:\ProgramData\TrontEq\state.bin`
- RBJ biquad cascade (peak / low-shelf / high-shelf), Direct Form II Transposed, FTZ+DAZ
- Seqlock IPC protocol (no locks in the audio thread)
