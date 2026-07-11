# Deploy the in/out-telemetry APO + relaunch the GUI. Runs elevated (one UAC).
$ErrorActionPreference = 'Continue'
$log = 'C:\trontstack\tronteq\deploy-vumeter.log'
"== deploy in/out VU @ $(Get-Date) ==" | Out-File $log -Encoding utf8

$install = 'C:\ProgramData\TrontEq'
$src     = 'C:\trontstack\tronteq\apo\build\TrontEqApo.dll'
$dest    = "$install\TrontEqApo.dll"
$state   = "$install\state.bin"
$cert    = "$install\dev-cert.pfx"
$gui     = 'C:\trontstack\tronteq\target\release\tronteq.exe'

# 1. Extend state.bin to the new 216-byte layout (preserve the first 192 bytes =
#    the user's committed EQ/dynamics). The appended 24 bytes are the telemetry block.
if (Test-Path $state) {
    $fs = [IO.File]::Open($state, 'Open', 'ReadWrite', 'ReadWrite')
    if ($fs.Length -lt 216) { $fs.SetLength(216) }
    $fs.Close()
    "state.bin length: $((Get-Item $state).Length)" | Add-Content $log
} else {
    "state.bin absent (GUI will create at 216)" | Add-Content $log
}

# 2. Grant the audiodg app-container token Modify so the APO can write telemetry.
$icacls = "$env:WINDIR\System32\icacls.exe"
& $icacls $install /grant "*S-1-15-2-1:(OI)(CI)(M)" /grant "*S-1-15-2-2:(OI)(CI)(M)" /grant "*S-1-5-19:(OI)(CI)(M)" /T /C 2>&1 | Add-Content $log
if (Test-Path $cert) {
    # keep the dev signing cert private (folder grant above would otherwise expose it)
    & $icacls $cert /inheritance:r /grant "*S-1-5-32-544:(F)" /grant "*S-1-5-18:(F)" 2>&1 | Add-Content $log
}

# 3. Stop audio so audiodg releases the loaded DLL.
Stop-Service audiosrv -Force 2>&1 | Add-Content $log
Start-Sleep 1

# 4. Swap the DLL (behind the existing CLSID/EFX registration — device unchanged).
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
