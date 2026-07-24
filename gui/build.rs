// Embed an asInvoker manifest + the .exe icon (Trent's face). The GUI runs at
// MEDIUM integrity on purpose: an elevated GUI window blocks every Medium-process
// global hotkey without modifiers (bare PrtSc in TrontSnap/ShareX dies whenever
// TrontEQ has focus) and UIPI-breaks drag/drop + SetForegroundWindow from normal
// apps. The only privileged work (APO install / device retarget) lives in
// tronteq-cli, which devices.rs launches elevated via ShellExecuteW "runas".
fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set_manifest(MANIFEST);
        res.compile().expect("embed icon + manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
}

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>"#;
