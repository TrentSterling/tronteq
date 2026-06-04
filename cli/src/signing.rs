//! Test-signing probe + signtool wrapper + install-path helpers.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub const INSTALL_DIR: &str = r"C:\ProgramData\TrontEq";
pub const DEV_CERT_PFX: &str = r"C:\ProgramData\TrontEq\dev-cert.pfx";
pub const DEV_CERT_PASS: &str = "tronteq";

/// The Windows SDK signtool. We search a few plausible install paths.
fn signtool_path() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe",
        r"C:\Program Files (x86)\Windows Kits\10\bin\x64\signtool.exe",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Where `apo\build.bat` drops the fresh DLL.
pub fn dll_build_path() -> PathBuf {
    // tronteq-cli.exe typically runs via `cargo run -p tronteq-cli` from the
    // workspace root; fall back to CWD-relative.
    let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    here.join("apo").join("build").join("TrontEqApo.dll")
}

pub fn dev_cert_present() -> bool {
    std::path::Path::new(DEV_CERT_PFX).exists()
}

#[derive(Debug)]
pub enum TestSigningState {
    On,
    Off,
    Unknown(String),
}

/// Parse `bcdedit /enum {current}` output. Returns On if the `testsigning`
/// line contains `Yes`.
pub fn test_signing_state() -> Result<TestSigningState> {
    let out = Command::new(crate::setup::system32("bcdedit.exe"))
        .args(["/enum", "{current}"])
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return Ok(TestSigningState::Unknown(format!("{e}"))),
    };
    if !out.status.success() {
        return Ok(TestSigningState::Unknown(format!(
            "bcdedit exit {:?}",
            out.status.code()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("testsigning") {
            if lower.contains("yes") {
                return Ok(TestSigningState::On);
            } else if lower.contains("no") {
                return Ok(TestSigningState::Off);
            }
        }
    }
    // No testsigning line: bcdedit's default is Off.
    Ok(TestSigningState::Off)
}

/// Sign `dll_path` in place with the dev PFX.
pub fn sign_dll(dll_path: &str) -> Result<()> {
    let tool = signtool_path().context("signtool.exe not found in Windows SDK")?;
    let status = Command::new(&tool)
        .args([
            "sign",
            "/v",
            "/fd",
            "SHA256",
            "/f",
            DEV_CERT_PFX,
            "/p",
            DEV_CERT_PASS,
            dll_path,
        ])
        .status()
        .with_context(|| format!("run {}", tool.display()))?;
    if !status.success() {
        anyhow::bail!("signtool failed: {:?}", status);
    }
    Ok(())
}
