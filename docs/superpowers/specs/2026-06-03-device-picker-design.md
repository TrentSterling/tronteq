# TrontEQ Device Picker — Design (2026-06-03)

## Goal
Pick the output endpoint to EQ from inside the TrontEQ GUI, instead of hardcoding
a device index and running `tronteq-cli install --device N` by hand. Minimise UAC
friction (Trent switches devices often; wants the fewest possible prompts).

## Decisions
- **GUI runs elevated** (admin manifest). One UAC prompt per launch, then unlimited
  in-process device switching with no further prompts. The usual elevated-app
  downside (no Explorer drag-drop) does not apply — TrontEQ accepts no dropped files.
- **Optional zero-UAC path:** `tronteq-cli register-autostart` creates a Scheduled
  Task (run with highest privileges at logon) so TrontEQ launches elevated with no
  prompt at all. `unregister-autostart` removes it. CLI-only; opt-in.

## Architecture
- **New crate `tronteq-core`** holds the Windows audio-endpoint code lifted out of
  the CLI binary. `shared` stays the pure 144-byte IPC contract (no Windows deps).
  - `endpoint.rs` (moved from `cli/src/`): `list_render_endpoints()`, `attach_fx(id)`,
    `detach_fx(id)`, plus new `current_eq_endpoint_ids()` (which endpoints carry our
    CLSID) and `is_apo_registered()` (HKCR CLSID present).
  - `CLSID_STR` + `clsid_guid()` move here (single source of truth). `cli/com_reg.rs`
    imports them from core.
- **CLI** depends on `tronteq-core`; `cli/src/endpoint.rs` is deleted, `main.rs` calls
  `tronteq_core::endpoint::*`. Adds `register-autostart` / `unregister-autostart`.
- **GUI** depends on `tronteq-core`; `build.rs` embeds a `requireAdministrator`
  manifest via the `embed-manifest` crate (pure Rust, no external tooling).

## GUI UI
- Top bar: a `ComboBox` of render endpoints (friendly name, `[default]` tag, ✓ on the
  one currently EQ'd) + a **Refresh** button (BT headset appearing mid-session).
- Selecting a device: detach any other endpoint carrying our CLSID, attach the chosen
  one, re-enumerate. Failures surface in the existing `last_error` slot (bottom panel).
- Status line: "EQ active on: <device>", or "APO not installed — run `tronteq-cli
  install`" when `is_apo_registered()` is false.
- After a switch, show the existing "toggle the device so audiodg reloads" hint.

## Data flow
GUI (elevated) → `tronteq_core::endpoint` → IMMDeviceEnumerator / IPropertyStore
(PKEY_FX_StreamEffectClsid). Same state.bin IPC to the APO is unchanged.

## Testing
- `core`: keep `clsid_guid_parses_registry_string`; add `list_render_endpoints` returns
  `Ok` (≥0 endpoints, no panic).
- Manual: launch elevated → pick device → confirm ✓ moves and `tronteq-cli list-devices`
  / `current_eq_endpoint_ids()` agree.

## Out of scope
- First-time install/sign/register stays in `tronteq-cli install`.
- The `IAudioSystemEffects2/3` loading gap (separate task; APO load still unverified).
- The intermittent GUI panic (`panic="abort"` confirms it was a panic; chase if it recurs).
