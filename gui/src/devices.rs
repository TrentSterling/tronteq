//! Output-device picker plumbing. The GUI runs at MEDIUM integrity (asInvoker),
//! so the privileged work — `tronteq-cli install`, which writes HKLM FxProperties
//! and restarts audiosrv — is launched ELEVATED via ShellExecuteExW "runas" (one
//! UAC prompt per apply; device retargets are rare). Unprivileged commands like
//! `list-devices` still spawn plainly. An elevated child's stdout can't be piped
//! back across the integrity boundary, so `apply` routes output through a log
//! file in ProgramData and reads it back after the child exits.

use anyhow::{bail, Context, Result};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

const CREATE_NO_WINDOW: u32 = 0x0800_0000; // keep the CLI's console from flashing

/// Where the elevated install's combined stdout+stderr lands so we can read it
/// back from Medium. Lives beside state.bin, which install itself ACLs open.
const INSTALL_LOG: &str = r"C:\ProgramData\TrontEq\install.log";

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

/// Parse `tronteq-cli list-devices` into structured rows. Device enumeration is
/// unprivileged, so this stays a plain spawn (no UAC).
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

/// Apply the full APO recipe to the device at `index` (runs `install --device`
/// ELEVATED — expect one UAC prompt; a declined prompt reads as a friendly error,
/// not a crash).
pub fn apply(index: usize) -> Result<String> {
    let _ = std::fs::remove_file(INSTALL_LOG);
    // cmd /c "<cli>" install --device N > <log> 2>&1  — the outer quotes are
    // cmd's own re-quoting rule for a quoted program path plus redirection.
    let params = format!(
        "/c \"\"{}\" install --device {} > \"{}\" 2>&1\"",
        cli_path().display(),
        index,
        INSTALL_LOG
    );
    let code = run_elevated("cmd.exe", &params)
        .context("launch elevated install (UAC declined?)")?;
    let log = std::fs::read_to_string(INSTALL_LOG).unwrap_or_default();
    if code != 0 {
        bail!("install exited with code {code}\n{log}");
    }
    Ok(log)
}

/// ShellExecuteExW "runas": launch `exe params` elevated, hidden, wait for it to
/// exit, and return its exit code. Fails (rather than hangs) if the user cancels
/// the UAC prompt.
fn run_elevated(exe: &str, params: &str) -> Result<u32> {
    let verb = HSTRING::from("runas");
    let file = HSTRING::from(exe);
    let parameters = HSTRING::from(params);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info).context("ShellExecuteExW")?;
        let proc = info.hProcess;
        if proc.is_invalid() {
            bail!("elevated process launched but no handle returned");
        }
        let wait = WaitForSingleObject(proc, INFINITE);
        let mut code = 1u32;
        let _ = GetExitCodeProcess(proc, &mut code);
        let _ = CloseHandle(proc);
        if wait != WAIT_OBJECT_0 {
            bail!("wait on elevated install failed: {wait:?}");
        }
        Ok(code)
    }
}
