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

/// Resolve both the portable layout and the source build independently of CWD.
/// An elevated GUI launch commonly starts in System32.
pub fn dll_build_path() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
    let portable = dir.join("TrontEqApo.dll");
    if portable.is_file() {
        return portable;
    }
    // target/release or target/debug (and a deps subdirectory for tests).
    for parent in dir.ancestors() {
        if parent.file_name().is_some_and(|name| name == "target") {
            if let Some(root) = parent.parent() {
                return root.join("apo/build/TrontEqApo.dll");
            }
        }
    }
    portable
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
