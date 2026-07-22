# TrontEQ bootstrap: elevated, $PSScriptRoot-relative, idempotent, two-phase
# around the one unavoidable test-signing reboot.
#
# Works from EITHER layout, auto-detected from what's next to this script:
#   prebuilt   dist\TrontEQ\   -- TrontEqApo.dll, tronteq.exe, tronteq-cli.exe,
#                                  dev-cert.cer, bootstrap.ps1, SETUP.md flat
#   source     repo root       -- apo\build\TrontEqApo.dll +
#                                  target\release\{tronteq,tronteq-cli}.exe
#                                  (after `cargo build --release --workspace`
#                                  and `apo\build.bat`)
#
# Phase 1 (needs admin, may need one reboot):
#   - enable test-signing (bcdedit /set testsigning on)
#   - ensure a code-signing dev cert exists at C:\ProgramData\TrontEq\dev-cert.pfx
#     (generated fresh per-machine if missing -- see note below) and is trusted
#     in LocalMachine\Root + TrustedPublisher; also trust the shipped
#     dev-cert.cer on the prebuilt path
#   - create C:\ProgramData\TrontEq with the APPLICATION-PACKAGES ACL
# Phase 2 (post-reboot, or same run if no reboot was needed):
#   - verify test-signing is actually active
#   - `tronteq-cli install` to the default output device
#   - register autostart
#   - launch the GUI
#
# NOTE on the dev cert: `tronteq-cli install` (cli/src/signing.rs) ALWAYS
# (re)signs the DLL using C:\ProgramData\TrontEq\dev-cert.pfx, on every
# machine, prebuilt or source-built -- that's baked into the CLI, not this
# script. So Phase 1 generates a machine-local cert whenever no .pfx is
# present yet, even on the prebuilt path (which only ships the public
# dev-cert.cer, per SETUP.md/make-dist.ps1 -- the private .pfx never leaves
# the main PC). The shipped .cer is trusted too, so the DLL's original
# signature from the main PC verifies right up until Phase 2 re-signs it
# with the new machine-local cert.
#
# Every step is gated behind a state check, so re-running this on an
# already-set-up machine (e.g. the main PC) is a no-op that reports
# "already set up" -- it does not redo bcdedit, does not regenerate an
# existing cert, and does not re-run install/autostart/launch if they're
# already in place.

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root       = $PSScriptRoot
$install    = 'C:\ProgramData\TrontEq'
$rebootFlag = Join-Path $install 'reboot.flag'
$clsid      = '{CA64E60A-A3C4-43B8-970F-0360055172F2}'
$taskName   = 'TrontEQ'
$resumeTask = 'TrontEQ-Resume'  # one-shot onlogon task that finishes Phase 2 after the reboot

function Log($m) {
    $stamp = (Get-Date).ToString('HH:mm:ss')
    $line = "$stamp  $m"
    Write-Host $line
    if (Test-Path $install) {
        $line | Add-Content -Path (Join-Path $install 'bootstrap.log') -Encoding utf8
    }
}

# ---- must be elevated (no self-elevation -- see dev-cycle.ps1's note on why
# self-elevating loops are a trap here; just fail clearly instead) ----------
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host "ERROR: run this from an ELEVATED PowerShell (Run as administrator)." -ForegroundColor Red
    Write-Host "  e.g.: Start-Process powershell -Verb RunAs -ArgumentList '-NoExit','-File',`"$($MyInvocation.MyCommand.Path)`"" -ForegroundColor Yellow
    exit 1
}

New-Item -ItemType Directory -Force -Path $install | Out-Null
Log "== TrontEQ bootstrap @ $(Get-Date) running as $(whoami) =="

# ---- detect layout -----------------------------------------------------------
$prebuiltDll = Join-Path $root 'TrontEqApo.dll'
$sourceDll   = Join-Path $root 'apo\build\TrontEqApo.dll'

if (Test-Path $prebuiltDll) {
    $layout  = 'prebuilt'
    $dllSrc  = $prebuiltDll
    $exeDir  = $root
    $shipCer = Join-Path $root 'dev-cert.cer'
} elseif (Test-Path $sourceDll) {
    $layout  = 'source'
    $dllSrc  = $sourceDll
    $exeDir  = Join-Path $root 'target\release'
    $shipCer = $null
} else {
    Log "ERROR: found neither a prebuilt DLL (TrontEqApo.dll next to this script)"
    Log "  nor a source build (apo\build\TrontEqApo.dll) under $root."
    Log "  Prebuilt: keep bootstrap.ps1 inside the extracted dist\TrontEQ folder."
    Log "  Source:   run 'cargo build --release --workspace' then 'apo\build.bat' first."
    exit 1
}
$cliExe = Join-Path $exeDir 'tronteq-cli.exe'
$guiExe = Join-Path $exeDir 'tronteq.exe'
foreach ($p in @($cliExe, $guiExe)) {
    if (-not (Test-Path $p)) { Log "ERROR: missing $p"; exit 1 }
}
Log "layout: $layout  (exe dir: $exeDir)"

# =============================== PHASE 1 =======================================
$bcd = bcdedit /enum "{current}" 2>&1 | Out-String
$testSigningOn = $bcd -match '(?im)^\s*testsigning\s+Yes'

$pfx = Join-Path $install 'dev-cert.pfx'
$cer = Join-Path $install 'dev-cert.cer'
$certReady = Test-Path $pfx
$needReboot = $false

if ($testSigningOn) {
    Log "[phase1] test-signing: already ON"
} else {
    Log "[phase1] test-signing: OFF -- enabling now (requires reboot)"
    $out = bcdedit /set testsigning on 2>&1 | Out-String
    Log "  bcdedit: $($out.Trim())"
    $needReboot = $true
}

if ($certReady) {
    Log "[phase1] dev cert: already present at $pfx"
} else {
    Log "[phase1] dev cert: generating a machine-local self-signed code-signing cert"
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=TrontEQ Dev' `
        -KeyUsage DigitalSignature -FriendlyName 'TrontEQ Dev' `
        -CertStoreLocation 'Cert:\CurrentUser\My'
    $pwd = ConvertTo-SecureString 'tronteq' -Force -AsPlainText
    Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $pwd | Out-Null
    Export-Certificate -Cert $cert -FilePath $cer | Out-Null
    Log "  exported $pfx / $cer"
}

foreach ($c in @($cer, $shipCer) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique) {
    try {
        Import-Certificate -FilePath $c -CertStoreLocation 'Cert:\LocalMachine\Root' | Out-Null
        Import-Certificate -FilePath $c -CertStoreLocation 'Cert:\LocalMachine\TrustedPublisher' | Out-Null
        Log "[phase1] trusted $c"
    } catch {
        Log "  warn: import $c failed: $_"
    }
}

Log "[phase1] settings dir ACL (ALL APPLICATION PACKAGES read/modify) on $install"
$icacls = "$env:WINDIR\System32\icacls.exe"
& $icacls $install /grant "*S-1-15-2-1:(OI)(CI)(M)" /grant "*S-1-15-2-2:(OI)(CI)(M)" /grant "*S-1-5-19:(OI)(CI)(M)" /T /C 2>&1 |
    ForEach-Object { Log "  $_" }
if (Test-Path $pfx) {
    # Keep the private key out of the broad app-package grant above.
    & $icacls $pfx /inheritance:r /grant "*S-1-5-32-544:(F)" /grant "*S-1-5-18:(F)" 2>&1 | Out-Null
}

if ($needReboot) {
    '__REBOOT_REQUIRED__' | Out-File -FilePath $rebootFlag -Encoding utf8
    # Auto-resume Phase 2 after the reboot so a non-dev user only has to run this
    # once. A one-shot onlogon HIGHEST task (elevated, no UAC) re-runs THIS script;
    # after the reboot test-signing is ON, so it sails through Phase 2 and then
    # deletes itself (see the Unregister call at the top of Phase 2). If this fails,
    # nothing breaks: re-running bootstrap.ps1 by hand after the reboot still works.
    try {
        $self = $MyInvocation.MyCommand.Path
        $usr  = "$env:USERDOMAIN\$env:USERNAME"
        $act  = New-ScheduledTaskAction -Execute 'powershell.exe' `
            -Argument "-NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$self`""
        $trg  = New-ScheduledTaskTrigger -AtLogOn -User $usr
        $prn  = New-ScheduledTaskPrincipal -UserId $usr -LogonType Interactive -RunLevel Highest
        Register-ScheduledTask -TaskName $resumeTask -Action $act -Trigger $trg -Principal $prn -Force | Out-Null
        Log "[phase1] auto-resume task '$resumeTask' registered (finishes Phase 2 once after reboot)"
    } catch {
        Log "  warn: could not register auto-resume ($_) -- re-run bootstrap.ps1 by hand after the reboot"
    }
    Log ""
    Log "RESULT: Phase 1 done. REBOOT REQUIRED for test-signing to take effect."
    Log "        Just reboot -- TrontEQ finishes installing and launches automatically"
    Log "        on the next login. (If it doesn't, run bootstrap.ps1 again, elevated.)"
    exit 0
}
Remove-Item -Force $rebootFlag -ErrorAction SilentlyContinue
Log "[phase1] satisfied, no reboot needed -- continuing to Phase 2"

# =============================== PHASE 2 =======================================
Log ""
$bcd2 = bcdedit /enum "{current}" 2>&1 | Out-String
if ($bcd2 -notmatch '(?im)^\s*testsigning\s+Yes') {
    Log "ERROR: test-signing still not reporting ON."
    Log "       If you just rebooted, check 'bcdedit /enum {current}' -- Secure Boot"
    Log "       or another boot entry may be blocking it."
    exit 1
}
Log "[phase2] test-signing: confirmed ON"

# The one-shot auto-resume task (if any) has done its job now that we've reached
# Phase 2 -- remove it so it never fires again on future logons.
Unregister-ScheduledTask -TaskName $resumeTask -Confirm:$false -ErrorAction SilentlyContinue
if ($?) { Log "[phase2] removed auto-resume task '$resumeTask'" }

# Stage apo\build\TrontEqApo.dll relative to this script's own directory: the
# CLI's dll_build_path() (cli/src/signing.rs) resolves it off the process's
# CWD, so tronteq-cli.exe is always run with -WorkingDirectory $root below.
# No-op on the source layout (it's already there).
$stageDir = Join-Path $root 'apo\build'
New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
Copy-Item $dllSrc (Join-Path $stageDir 'TrontEqApo.dll') -Force

$clsidPresent = Test-Path "Registry::HKEY_CLASSES_ROOT\CLSID\$clsid"
$dllInstalled = Test-Path (Join-Path $install 'TrontEqApo.dll')
if ($clsidPresent -and $dllInstalled) {
    Log "[phase2] APO already installed (CLSID registered, DLL present) -- skipping install"
    Log "         (run '$cliExe install' by hand if you've changed the default output device)"
} else {
    Log "[phase2] tronteq-cli install (default output device)..."
    $outLog = Join-Path $install 'bootstrap-install.out.log'
    $errLog = Join-Path $install 'bootstrap-install.err.log'
    $p = Start-Process -FilePath $cliExe -ArgumentList 'install' -WorkingDirectory $root `
        -NoNewWindow -PassThru -Wait -RedirectStandardOutput $outLog -RedirectStandardError $errLog
    Get-Content $outLog -ErrorAction SilentlyContinue | ForEach-Object { Log "  $_" }
    Get-Content $errLog -ErrorAction SilentlyContinue | ForEach-Object { Log "  ! $_" }
    if ($p.ExitCode -ne 0) {
        Log "ERROR: tronteq-cli install failed (exit $($p.ExitCode)). See bootstrap-install.err.log."
        exit 1
    }
    Log "[phase2] install: OK"
}

$taskQuery = schtasks /query /tn $taskName 2>&1 | Out-String
$taskExists = $taskQuery -notmatch 'ERROR'
if ($taskExists) {
    Log "[phase2] autostart task '$taskName' already registered -- skipping"
} else {
    Log "[phase2] registering autostart..."
    & $cliExe register-autostart 2>&1 | ForEach-Object { Log "  $_" }
}

$proc = Get-Process tronteq -ErrorAction SilentlyContinue
if ($proc) {
    Log "[phase2] GUI already running (pid=$($proc.Id)) -- skipping launch"
} else {
    Log "[phase2] launching GUI via the autostart scheduled task (elevated, no UAC)..."
    schtasks /run /tn $taskName 2>&1 | ForEach-Object { Log "  $_" }
    Start-Sleep -Seconds 3
    $proc = Get-Process tronteq -ErrorAction SilentlyContinue
    if ($proc) {
        Log "RESULT: RUNNING pid=$($proc.Id)"
    } else {
        Log "RESULT: launch triggered but process not detected yet -- check Task"
        Log "        Scheduler '$taskName' and $install\crash.log"
    }
}

Log ""
if ($clsidPresent -and $dllInstalled -and $taskExists -and $proc) {
    Log "RESULT: already set up. Nothing to do."
} else {
    Log "RESULT: bootstrap complete. Open the GUI, confirm the curve responds,"
    Log "        and the A/V SYNC knob is visible at the top of the CHAIN tab."
}
