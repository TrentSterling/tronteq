//! tronteq-cli — sign the APO DLL, register the CLSID, attach to an output
//! endpoint via `PKEY_FX_StreamEffectClsid`, or reverse all of that. The mic
//! stack is the same recipe on a capture endpoint, plus `mic` commands that edit
//! `mic.bin` without elevation.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tronteq_shared::mic::{self, MicHandle, MicSettings};

use crate::com_reg::ApoClass;

mod com_reg;
mod endpoint;
mod setup;
mod signing;

#[derive(Parser)]
#[command(name = "tronteq-cli")]
#[command(about = "TrontEQ APO installer / manager")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify test-signing is on and the dev cert + DLL are in place.
    Check,
    /// List active audio output endpoints (or microphones with --mics).
    ListDevices {
        /// List capture endpoints (microphones) instead of outputs.
        #[arg(long)]
        mics: bool,
    },
    /// Sign (if needed), register the CLSID, and attach the APO to one output device.
    Install {
        /// Index from `list-devices`. If omitted, uses the default output.
        #[arg(long, conflicts_with = "device_id")]
        device: Option<usize>,
        /// Stable Windows endpoint ID (safe across device reordering).
        #[arg(long)]
        device_id: Option<String>,
    },
    /// Detach the APO from all endpoints (outputs and mics) and unregister it.
    Uninstall,
    /// Attach the mic stack (gate, EQ, compressor, limiter) to one microphone.
    InstallMic {
        /// Index from `list-devices --mics`. If omitted, uses the default mic.
        #[arg(long, conflicts_with = "device_id")]
        device: Option<usize>,
        /// Stable Windows endpoint ID (safe across device reordering).
        #[arg(long)]
        device_id: Option<String>,
        /// Replace another effect already in the mic's slot.
        #[arg(long)]
        force: bool,
    },
    /// Detach the mic stack from every microphone. Outputs are left alone.
    UninstallMic,
    /// Show or change the mic stack settings (no elevation needed).
    Mic {
        #[command(subcommand)]
        action: MicAction,
    },
    /// Start TrontEQ in the system tray at logon (normal user, no UAC).
    RegisterAutostart,
    /// Remove the autostart Scheduled Task.
    UnregisterAutostart,
}

#[derive(Subcommand)]
enum MicAction {
    /// Print the current mic chain and meter levels.
    Show,
    /// Load a built-in preset: whisper (speech recognition), voice, broadcast, clean.
    Preset { name: String },
    /// Turn the whole mic chain off (pass-through) or back on.
    Bypass {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Turn the noise gate on or off, optionally moving its threshold.
    Gate {
        #[arg(value_parser = ["on", "off"])]
        state: String,
        /// Open threshold in dBFS (for example -50).
        #[arg(long, allow_hyphen_values = true)]
        threshold: Option<f32>,
        /// Attenuation while closed in dB (for example -24).
        #[arg(long, allow_hyphen_values = true)]
        range: Option<f32>,
    },
    /// Head-of-chain gain in dB.
    Gain {
        #[arg(allow_hyphen_values = true)]
        db: f32,
    },
    /// Save the mic chain as JSON.
    Export { path: std::path::PathBuf },
    /// Load a mic chain from JSON (missing fields keep their defaults).
    Import { path: std::path::PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check => cmd_check(),
        Command::ListDevices { mics } => cmd_list_devices(mics),
        Command::Install { device, device_id } => {
            cmd_install(&com_reg::OUTPUT, device, device_id.as_deref(), true)
        }
        Command::Uninstall => cmd_uninstall(),
        Command::InstallMic { device, device_id, force } => {
            cmd_install(&com_reg::MIC, device, device_id.as_deref(), force)
        }
        Command::UninstallMic => cmd_uninstall_mic(),
        Command::Mic { action } => cmd_mic(action),
        Command::RegisterAutostart => cmd_register_autostart(),
        Command::UnregisterAutostart => cmd_unregister_autostart(),
    }
}

fn cmd_register_autostart() -> Result<()> {
    setup::register_autostart()?;
    println!("[ok]  autostart task 'TrontEQ' created: starts in the tray at logon, no UAC or runtime limit.");
    Ok(())
}

fn cmd_unregister_autostart() -> Result<()> {
    setup::unregister_autostart()?;
    println!("[ok]  autostart task removed.");
    Ok(())
}

fn cmd_check() -> Result<()> {
    println!("== TrontEQ setup check ==");

    match signing::test_signing_state()? {
        signing::TestSigningState::On => println!("[ok]  test-signing enabled"),
        signing::TestSigningState::Off => {
            println!("[!!]  test-signing is OFF");
            println!("      Run in an elevated PowerShell:");
            println!("        bcdedit /set testsigning on");
            println!("      Then reboot.");
        }
        signing::TestSigningState::Unknown(note) => {
            println!("[??]  test-signing state unknown: {note}");
        }
    }

    if signing::dev_cert_present() {
        println!("[ok]  dev cert present at {}", signing::DEV_CERT_PFX);
    } else {
        println!("[!!]  dev cert missing. See README.md setup.");
    }

    let dll = signing::dll_build_path();
    if dll.exists() {
        println!("[ok]  built DLL at {}", dll.display());
    } else {
        println!("[!!]  DLL not built yet. Run `apo\\build.bat`.");
    }

    Ok(())
}

fn cmd_list_devices(mics: bool) -> Result<()> {
    let devices = if mics { endpoint::list_capture_endpoints()? } else { endpoint::list_render_endpoints()? };
    println!("{}", if mics { "== Capture endpoints (mics) ==" } else { "== Render endpoints ==" });
    for (i, d) in devices.iter().enumerate() {
        let tag = if d.is_default { " [default]" } else { "" };
        println!("[{i}] {}{tag}", d.friendly_name);
        println!("    id: {}", d.id);
    }
    Ok(())
}

fn list_for(class: &ApoClass) -> Result<Vec<tronteq_shared::endpoints::Endpoint>> {
    if class.flow == com_reg::MIC.flow { endpoint::list_capture_endpoints() } else { endpoint::list_render_endpoints() }
}

/// `force` only matters for the mic: an output install has always replaced the
/// slot, but a mic may carry a vendor effect worth keeping.
fn cmd_install(class: &ApoClass, device_idx: Option<usize>, device_id: Option<&str>, force: bool) -> Result<()> {
    let mic = class.flow == com_reg::MIC.flow;
    let state = signing::test_signing_state()?;
    if !matches!(state, signing::TestSigningState::On) {
        anyhow::bail!(
            "Test-signing must be enabled first. Run `tronteq-cli check` for instructions."
        );
    }

    if !signing::dev_cert_present() {
        anyhow::bail!(
            "Dev cert missing at {}. See README.md setup.",
            signing::DEV_CERT_PFX
        );
    }

    let built = signing::dll_build_path();
    if !built.exists() {
        anyhow::bail!(
            "DLL not built. From `apo/` run `build.bat`. Expected: {}",
            built.display()
        );
    }

    // Resolve the target endpoint up front (need its registry GUID for the EFX slot).
    let kind = if mic { "microphone" } else { "output" };
    let endpoints = list_for(class)?;
    let target = if let Some(id) = device_id {
        endpoints.iter().find(|d| d.id == id).with_context(|| format!("selected {kind} is no longer connected; refresh the device list"))?
    } else { match device_idx {
        Some(i) => endpoints.get(i).with_context(|| format!("no device at index {i}"))?,
        None => endpoints.iter().find(|d| d.is_default).with_context(|| format!("no default {kind}"))?,
    } };
    let ep_guid = setup::endpoint_reg_guid(&target.id)?;
    let target_name = target.friendly_name.clone();

    if !force {
        if let Some(existing) = setup::endpoint_efx(class, &ep_guid)? {
            if !existing.eq_ignore_ascii_case(class.clsid) {
                anyhow::bail!(
                    "\"{target_name}\" already runs another effect ({existing}). \
                     Re-run with --force to replace it."
                );
            }
        }
    }

    let installed = signing::INSTALL_DIR.to_owned();
    std::fs::create_dir_all(&installed).context("create install dir")?;
    if mic {
        // The capture APO only opens an existing mic.bin; create it (voice preset)
        // before the ACL pass so audiodg gets the same access as state.bin.
        MicHandle::open_or_init().context("create mic.bin")?;
    }
    setup::grant_programdata_acl(&installed)?;
    println!("[ok]  settings dir ACL (ALL APPLICATION PACKAGES read)");

    setup::register_apo(class)?;
    println!("[ok]  APO registered in AudioProcessingObjects store");

    // Stop audio so audiodg releases the DLL (a re-install otherwise can't overwrite it).
    let audio = setup::AudioRestartGuard::stop()?;

    let dest = format!("{installed}\\TrontEqApo.dll");
    std::fs::copy(&built, &dest).with_context(|| format!("copy DLL to {dest}"))?;
    println!("[ok]  copied DLL -> {dest}");

    signing::sign_dll(&dest)?;
    println!("[ok]  signed");

    com_reg::register_inproc(class, &dest)?;
    println!("[ok]  CLSID registered (HKCR)");

    setup::set_endpoint_efx(class, &ep_guid)?;
    println!("[ok]  EFX slot -> {} on: {target_name}", class.store_name);

    audio.restart()?;
    println!("[ok]  audio service restarted");
    println!();
    if mic {
        println!("Programs that open \"{target_name}\" now hear the mic stack (whisper preset).");
        println!("Change it with `tronteq-cli mic preset broadcast` or the MIC tab. If nothing");
        println!("changes, turn \"Audio enhancements\" on for this mic in Sound settings and");
        println!("disable + re-enable it once.");
    } else {
        println!("If you don't hear EQ yet, disable + re-enable \"{target_name}\" once in");
        println!("Sound settings (or reconnect it) so the endpoint re-reads the effect.");
    }
    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    let endpoints = endpoint::list_render_endpoints().unwrap_or_default();
    for d in &endpoints {
        let _ = endpoint::detach_fx(&d.id); // legacy Properties-store cleanup
        if let Ok(g) = setup::endpoint_reg_guid(&d.id) {
            if let Err(e) = setup::clear_endpoint_efx(&com_reg::OUTPUT, &g) {
                eprintln!("  warn: clear EFX on {}: {e}", d.friendly_name);
            }
        }
    }
    println!("[ok]  EFX slots cleared from all endpoints");
    // The DLL is about to go away, so the mic class must not stay attached either.
    detach_mics();
    unregister(&com_reg::MIC);

    match setup::unregister_apo(&com_reg::OUTPUT) {
        Ok(()) => println!("[ok]  APO unregistered from AudioProcessingObjects"),
        Err(e) => eprintln!("  warn: unregister APO: {e}"),
    }
    match com_reg::unregister_inproc(&com_reg::OUTPUT) {
        Ok(()) => println!("[ok]  CLSID unregistered (HKCR)"),
        Err(e) => eprintln!("  warn: unregister CLSID: {e}"),
    }

    setup::stop_audio()?; // release the loaded DLL so it can be deleted
    let dest = format!("{}\\TrontEqApo.dll", signing::INSTALL_DIR);
    let _ = std::fs::remove_file(&dest);
    println!("[ok]  DLL removed from {}", signing::INSTALL_DIR);
    setup::start_audio()?;

    println!();
    println!("Done. Disable + re-enable affected outputs once to fully drop the APO.");
    Ok(())
}

/// Clear the mic class from every capture endpoint that has it. Vendor effects
/// on other mics are left in place.
fn detach_mics() {
    for d in endpoint::list_capture_endpoints().unwrap_or_default() {
        let Ok(g) = setup::endpoint_reg_guid(&d.id) else { continue };
        match setup::clear_endpoint_efx_if_ours(&com_reg::MIC, &g) {
            Ok(true) => println!("[ok]  mic stack removed from: {}", d.friendly_name),
            Ok(false) => {}
            Err(e) => eprintln!("  warn: clear mic EFX on {}: {e}", d.friendly_name),
        }
    }
}

fn unregister(class: &ApoClass) {
    if let Err(e) = setup::unregister_apo(class) {
        eprintln!("  warn: unregister {}: {e}", class.store_name);
    }
    if let Err(e) = com_reg::unregister_inproc(class) {
        eprintln!("  warn: unregister {} CLSID: {e}", class.store_name);
    }
}

fn cmd_uninstall_mic() -> Result<()> {
    let audio = setup::AudioRestartGuard::stop()?;
    detach_mics();
    unregister(&com_reg::MIC);
    audio.restart()?;
    println!("[ok]  mic stack detached; output EQ unchanged");
    Ok(())
}

fn cmd_mic(action: MicAction) -> Result<()> {
    let handle = MicHandle::open_or_init().context("open mic.bin (run once as administrator if access is denied)")?;
    let mut s = handle.snapshot()?;
    match action {
        MicAction::Show => {
            print_mic(&s);
            let t = handle.telemetry();
            if t.seq == 0 {
                println!("meters: no audio processed yet (is the mic stack installed and the mic open?)");
            } else {
                let db = |x: f32| if x > 1e-6 { 20.0 * x.log10() } else { -120.0 };
                println!("meters: in {:.1} dBFS rms, out {:.1} dBFS rms, gain reduction {:.1} dB",
                    db(t.in_rms), db(t.out_rms), t.gr_db);
            }
            return Ok(());
        }
        MicAction::Preset { name } => {
            s = mic::preset(&name).with_context(|| format!("unknown preset {name}; try {}", mic::PRESETS.join(", ")))?;
        }
        MicAction::Bypass { state } => s.bypass = state == "on",
        MicAction::Gate { state, threshold, range } => {
            s.gate.enabled = u32::from(state == "on");
            if let Some(t) = threshold { s.gate.threshold_db = t; }
            if let Some(r) = range { s.gate.range_db = r; }
        }
        MicAction::Gain { db } => s.preamp_db = db,
        MicAction::Export { path } => {
            std::fs::write(&path, serde_json::to_string_pretty(&s)?)
                .with_context(|| format!("write {}", path.display()))?;
            println!("[ok]  saved {}", path.display());
            return Ok(());
        }
        MicAction::Import { path } => {
            let text = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            s = serde_json::from_str::<MicSettings>(&text).context("parse mic settings")?;
        }
    }
    handle.write(&s)?;
    print_mic(&s);
    Ok(())
}

fn print_mic(s: &MicSettings) {
    let on = |b: bool| if b { "on" } else { "off" };
    println!("mic stack: {}", if s.bypass { "BYPASSED" } else { "active" });
    println!("  gain      {:+.1} dB", s.preamp_db);
    let g = &s.gate;
    println!("  gate      {} (open {:.0} dBFS, close {:.0}, range {:.0} dB, hold {:.0} ms, release {:.0} ms)",
        on(g.enabled != 0), g.threshold_db, g.threshold_db - g.hysteresis_db, g.range_db, g.hold_ms, g.release_ms);
    let eq: Vec<String> = s.bands.iter()
        .filter(|b| b.gain != 0.0 || b.kind == tronteq_shared::BandKind::HighPass as u32 || b.kind == tronteq_shared::BandKind::LowPass as u32)
        .map(|b| format!("{} {:.0} Hz {:+.1} dB", tronteq_shared::BandKind::from_u32(b.kind).short_label(), b.freq, b.gain))
        .collect();
    println!("  eq        {}", if eq.is_empty() { "flat".to_string() } else { eq.join(", ") });
    let d = &s.dynamics;
    println!("  comp      {} ({:.0} dB, {:.1}:1, makeup {:+.1} dB)", on(d.comp_enabled != 0), d.comp_threshold_db, d.comp_ratio, d.comp_makeup_db);
    println!("  limiter   {} ({:.1} dB)", on(d.limiter_enabled != 0), d.limiter_ceiling_db);
    println!("  auto-loud {}", on(d.agc_enabled != 0));
}
