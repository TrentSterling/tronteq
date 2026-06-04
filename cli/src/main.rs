//! tronteq-cli — sign the APO DLL, register the CLSID, attach to an output
//! endpoint via `PKEY_FX_StreamEffectClsid`, or reverse all of that.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

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
    /// List active audio output endpoints.
    ListDevices,
    /// Sign (if needed), register the CLSID, and attach the APO to one output device.
    Install {
        /// Index from `list-devices`. If omitted, uses the default output.
        #[arg(long)]
        device: Option<usize>,
    },
    /// Detach the APO from all endpoints and unregister the CLSID.
    Uninstall,
    /// Launch TrontEQ elevated at logon with no UAC (Scheduled Task).
    RegisterAutostart,
    /// Remove the autostart Scheduled Task.
    UnregisterAutostart,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check => cmd_check(),
        Command::ListDevices => cmd_list_devices(),
        Command::Install { device } => cmd_install(device),
        Command::Uninstall => cmd_uninstall(),
        Command::RegisterAutostart => cmd_register_autostart(),
        Command::UnregisterAutostart => cmd_unregister_autostart(),
    }
}

fn cmd_register_autostart() -> Result<()> {
    setup::register_autostart()?;
    println!("[ok]  autostart task 'TrontEQ' created — launches elevated at logon, no UAC.");
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

fn cmd_list_devices() -> Result<()> {
    let devices = endpoint::list_render_endpoints()?;
    println!("== Render endpoints ==");
    for (i, d) in devices.iter().enumerate() {
        let tag = if d.is_default { " [default]" } else { "" };
        println!("[{i}] {}{tag}", d.friendly_name);
        println!("    id: {}", d.id);
    }
    Ok(())
}

fn cmd_install(device_idx: Option<usize>) -> Result<()> {
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
    let endpoints = endpoint::list_render_endpoints()?;
    let target = match device_idx {
        Some(i) => endpoints.get(i).with_context(|| format!("no device at index {i}"))?,
        None => endpoints.iter().find(|d| d.is_default).context("no default output device")?,
    };
    let ep_guid = setup::endpoint_reg_guid(&target.id)?;
    let target_name = target.friendly_name.clone();

    let installed = signing::INSTALL_DIR.to_owned();
    std::fs::create_dir_all(&installed).context("create install dir")?;
    setup::grant_programdata_acl(&installed)?;
    println!("[ok]  settings dir ACL (ALL APPLICATION PACKAGES read)");

    setup::register_apo()?;
    println!("[ok]  APO registered in AudioProcessingObjects store");

    // Stop audio so audiodg releases the DLL (a re-install otherwise can't overwrite it).
    setup::stop_audio()?;

    let dest = format!("{installed}\\TrontEqApo.dll");
    std::fs::copy(&built, &dest).with_context(|| format!("copy DLL to {dest}"))?;
    println!("[ok]  copied DLL -> {dest}");

    signing::sign_dll(&dest)?;
    println!("[ok]  signed");

    com_reg::register_inproc(&dest)?;
    println!("[ok]  CLSID registered (HKCR)");

    setup::set_endpoint_efx(&ep_guid)?;
    println!("[ok]  EFX slot -> TrontEQ on: {target_name}");

    setup::start_audio()?;
    println!("[ok]  audio service restarted");
    println!();
    println!("If you don't hear EQ yet, disable + re-enable \"{target_name}\" once in");
    println!("Sound settings (or reconnect it) so the endpoint re-reads the effect.");
    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    let endpoints = endpoint::list_render_endpoints().unwrap_or_default();
    for d in &endpoints {
        let _ = endpoint::detach_fx(&d.id); // legacy Properties-store cleanup
        if let Ok(g) = setup::endpoint_reg_guid(&d.id) {
            if let Err(e) = setup::clear_endpoint_efx(&g) {
                eprintln!("  warn: clear EFX on {}: {e}", d.friendly_name);
            }
        }
    }
    println!("[ok]  EFX slots cleared from all endpoints");

    match setup::unregister_apo() {
        Ok(()) => println!("[ok]  APO unregistered from AudioProcessingObjects"),
        Err(e) => eprintln!("  warn: unregister APO: {e}"),
    }
    match com_reg::unregister_inproc() {
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
