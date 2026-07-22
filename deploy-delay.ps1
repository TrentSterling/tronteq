# Deploy the A/V-sync delay APO (224-byte state.bin layout) + relaunch the GUI.
# Runs elevated (one UAC). Modeled on deploy-vumeter.ps1 (which did the 192 ->
# 216 growth for telemetry); this one grows 216(or 192) -> 224 and explicitly
# zeroes the NEW bytes 192..224 (delay_ms + _reserved + the relocated
# telemetry block) so stale telemetry bytes from an old build never leak in as
# a phantom delay_ms (see spec 1.3). Bytes 0..192 -- the user's committed
# EQ/preamp/bypass/dynamics -- are never touched.
#
# delay_ms is a session setting, not part of a saved profile (spec 1.5), so
# resetting it to 0 on every deploy is intended, not a bug.
#
# DO NOT RUN THIS as part of an automated task. This restarts audiosrv (a
# ~1-2s audio cut) and is meant to be run once, by hand, when Trent is ready
# for the live cutover.

$ErrorActionPreference = 'Continue'
$root = $PSScriptRoot
$log  = Join-Path $root 'deploy-delay.log'
"== deploy A/V-sync delay @ $(Get-Date) ==" | Out-File $log -Encoding utf8

$install = 'C:\ProgramData\TrontEq'
$src     = Join-Path $root 'apo\build\TrontEqApo.dll'
$dest    = Join-Path $install 'TrontEqApo.dll'
$state   = Join-Path $install 'state.bin'
$cert    = Join-Path $install 'dev-cert.pfx'
$gui     = Join-Path $root 'target\release\tronteq.exe'

if (-not (Test-Path $src)) {
    "ERROR: $src not found -- run 'cmd /c apo\build.bat' first" | Add-Content $log
    Write-Host "ERROR: $src not found -- run 'cmd /c apo\build.bat' first" -ForegroundColor Red
    exit 1
}
if (-not (Test-Path $gui)) {
    "ERROR: $gui not found -- run 'cargo build --release -p tronteq' first" | Add-Content $log
    Write-Host "ERROR: $gui not found -- run 'cargo build --release -p tronteq' first" -ForegroundColor Red
    exit 1
}

# 1. Extend state.bin to the 224-byte layout, and zero bytes 192..224:
#      192..196  delay_ms    (new -- must not inherit stale telemetry bytes)
#      196..200  _reserved   (new)
#      200..224  telemetry   (relocated from the old offset 192 -- an old
#                             216-byte file's telemetry lived exactly where
#                             delay_ms now sits, so it must be wiped, not
#                             just grown)
#    Bytes 0..192 (bands/preamp/bypass/dynamics) are left exactly as-is.
if (Test-Path $state) {
    $fs = [IO.File]::Open($state, 'Open', 'ReadWrite', 'ReadWrite')
    try {
        if ($fs.Length -lt 224) { $fs.SetLength(224) }
        $zero = New-Object byte[] 32   # 224 - 192
        $fs.Seek(192, [IO.SeekOrigin]::Begin) | Out-Null
        $fs.Write($zero, 0, $zero.Length)
    } finally {
        $fs.Close()
    }
    "state.bin length: $((Get-Item $state).Length), zeroed bytes 192..224" | Add-Content $log
} else {
    "state.bin absent (GUI will create fresh at 224 via default_flat)" | Add-Content $log
}

# 2. Grant the audiodg app-container token Modify so the APO can write telemetry.
$icacls = "$env:WINDIR\System32\icacls.exe"
& $icacls $install /grant "*S-1-15-2-1:(OI)(CI)(M)" /grant "*S-1-15-2-2:(OI)(CI)(M)" /grant "*S-1-5-19:(OI)(CI)(M)" /T /C 2>&1 | Add-Content $log
if (Test-Path $cert) {
    # keep the dev signing cert private (the folder grant above would otherwise expose it)
    & $icacls $cert /inheritance:r /grant "*S-1-5-32-544:(F)" /grant "*S-1-5-18:(F)" 2>&1 | Add-Content $log
}

# 3. Stop audio so audiodg releases the loaded DLL.
Stop-Service audiosrv -Force 2>&1 | Add-Content $log
Start-Sleep 1

# 4. Swap the DLL (behind the existing CLSID/EFX registration -- device unchanged).
Copy-Item $src $dest -Force
"copied DLL: $((Get-Item $dest).Length) bytes, modified $((Get-Item $dest).LastWriteTime)" | Add-Content $log

# 5. Sign it (test-signing dev cert).
$signtool = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe'
if (-not (Test-Path $signtool)) { $signtool = 'C:\Program Files (x86)\Windows Kits\10\bin\x64\signtool.exe' }
& $signtool sign /v /fd SHA256 /f $cert /p tronteq $dest 2>&1 | Add-Content $log

# 6. Start audio back up.
Start-Service audiosrv 2>&1 | Add-Content $log
Start-Sleep 2

# 7. Relaunch the GUI (inherits elevation from this script).
Start-Process $gui
"launched GUI" | Add-Content $log
(New-Object System.Media.SoundPlayer "$env:WINDIR\Media\chord.wav").PlaySync()
