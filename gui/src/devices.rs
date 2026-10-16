//! Output-device picker plumbing. The GUI runs at MEDIUM integrity (asInvoker),
//! so the privileged work — `tronteq-cli install`, which writes HKLM FxProperties
//! and restarts audiosrv — is launched ELEVATED via ShellExecuteExW "runas" (one
//! UAC prompt per apply; device retargets are rare). Device enumeration queries
//! Windows directly. An elevated child's stdout can't be piped
//! back across the integrity boundary, so `apply` routes output through a log
//! file in ProgramData and reads it back after the child exits.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// Where the elevated install's combined stdout+stderr lands so we can read it
/// back from Medium. Lives beside state.bin, which install itself ACLs open.
const INSTALL_LOG: &str = r"C:\ProgramData\TrontEq\install.log";

pub use tronteq_shared::endpoints::{Endpoint as Device, selected_index};

fn cli_path() -> Result<PathBuf> {
    let path = std::env::current_exe().context("locate TrontEQ")?
        .parent().context("locate TrontEQ folder")?.join("tronteq-cli.exe");
    if !path.is_file() {
        bail!("Missing {}. Build or reinstall both TrontEQ executables before applying EQ.", path.display());
    }
    Ok(path)
}

pub fn installer_error() -> Option<String> {
    cli_path().err().map(|e| format!("{e:#}"))
}

/// Query Windows directly. A missing installer must never hide the outputs.
pub fn list() -> Result<Vec<Device>> {
    tronteq_shared::endpoints::list_render_endpoints()
}

/// Microphones for the mic stack.
pub fn list_mics() -> Result<Vec<Device>> {
    tronteq_shared::endpoints::list_capture_endpoints()
}

/// Apply the full APO recipe to a stable endpoint ID (runs `install --device-id`
/// ELEVATED — expect one UAC prompt; a declined prompt reads as a friendly error,
/// not a crash).
pub fn apply(endpoint_id: &str) -> Result<String> {
    run_install("install", endpoint_id)
}

/// Attach the mic stack to a capture endpoint (`install-mic --device-id`,
/// elevated, same one-UAC flow as outputs).
pub fn apply_mic(endpoint_id: &str) -> Result<String> {
    run_install("install-mic", endpoint_id)
}

fn run_install(subcommand: &str, endpoint_id: &str) -> Result<String> {
    let cli = cli_path()?;
    let _ = std::fs::remove_file(INSTALL_LOG);
    // cmd /c "<cli>" <subcommand> --device-id ID > <log> 2>&1; the outer quotes
    // are cmd's own re-quoting rule for a quoted program path plus redirection.
    let params = format!(
        "/c \"\"{}\" {} --device-id {} > \"{}\" 2>&1\"",
        cli.display(),
        subcommand,
        endpoint_id,
        INSTALL_LOG
    );
    let cmd = std::env::var("SystemRoot").context("SystemRoot")? + r"\System32\cmd.exe";
    let code = run_elevated(&cmd, &params)
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
