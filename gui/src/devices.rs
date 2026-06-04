//! Output-device picker plumbing. The GUI runs elevated (admin manifest), so it
//! drives the privileged work by spawning `tronteq-cli` — which inherits the
//! elevation, so there's no extra UAC prompt per action.

use anyhow::{bail, Context, Result};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000; // keep the CLI's console from flashing

#[derive(Clone)]
pub struct Device {
    pub index: usize,
    pub name: String,
    pub is_default: bool,
}

fn cli_path() -> PathBuf {
    // tronteq-cli.exe sits next to tronteq.exe in the build/release dir.
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tronteq-cli.exe")))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("tronteq-cli.exe"))
}

/// Parse `tronteq-cli list-devices` into structured rows.
pub fn list() -> Result<Vec<Device>> {
    let out = Command::new(cli_path())
        .arg("list-devices")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("run list-devices")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut devices = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        // "[0] Headphones (Smokin' Buds) [default]"
        let Some(rest) = l.strip_prefix('[') else { continue };
        let Some((idx, name)) = rest.split_once(']') else { continue };
        let Ok(index) = idx.trim().parse::<usize>() else { continue };
        let name = name.trim();
        let is_default = name.ends_with("[default]");
        let name = name.trim_end_matches("[default]").trim().to_string();
        devices.push(Device { index, name, is_default });
    }
    if devices.is_empty() {
        bail!("could not parse any output devices from tronteq-cli");
    }
    Ok(devices)
}

/// Apply the full APO recipe to the device at `index` (runs `install --device`).
pub fn apply(index: usize) -> Result<String> {
    let out = Command::new(cli_path())
        .args(["install", "--device", &index.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("run install")?;
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        bail!("{s}");
    }
    Ok(s)
}
