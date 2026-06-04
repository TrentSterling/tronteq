# TrontEQ admin setup. Run this elevated. Does everything that needs admin
# in one shot so Trent only has to click UAC once.
#
# Writes a log to C:\ProgramData\TrontEq\setup.log so the non-admin CLI can
# read what happened.

$ErrorActionPreference = 'Continue'
$log = 'C:\ProgramData\TrontEq\setup.log'

function Log($m) {
    $stamp = (Get-Date).ToString('HH:mm:ss')
    "$stamp  $m" | Tee-Object -FilePath $log -Append | Out-Host
}

New-Item -ItemType Directory -Force -Path 'C:\ProgramData\TrontEq' | Out-Null
Remove-Item -Force $log -ErrorAction SilentlyContinue
Log "TrontEQ setup running as $(whoami)"

# 1) Check / enable test signing
$bcd = bcdedit /enum "{current}" 2>&1 | Out-String
if ($bcd -match '(?im)^\s*testsigning\s+Yes') {
    Log 'testsigning: ALREADY ON'
    $needReboot = $false
} elseif ($bcd -match '(?im)^\s*testsigning\s+No') {
    Log 'testsigning: currently OFF -- enabling now'
    $out = bcdedit /set testsigning on 2>&1 | Out-String
    Log "bcdedit output: $out"
    $needReboot = $true
} else {
    Log 'testsigning: not present -- enabling now'
    $out = bcdedit /set testsigning on 2>&1 | Out-String
    Log "bcdedit output: $out"
    $needReboot = $true
}

# 2) Cert
$pfx = 'C:\ProgramData\TrontEq\dev-cert.pfx'
$cer = 'C:\ProgramData\TrontEq\dev-cert.cer'
if (Test-Path $pfx) {
    Log "cert already at $pfx -- skipping generation"
} else {
    Log 'creating self-signed code-signing cert'
    $cert = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject 'CN=TrontEQ Dev' `
        -KeyUsage DigitalSignature `
        -FriendlyName 'TrontEQ Dev' `
        -CertStoreLocation 'Cert:\CurrentUser\My'
    $pwd = ConvertTo-SecureString 'tronteq' -Force -AsPlainText
    Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $pwd | Out-Null
    Export-Certificate  -Cert $cert -FilePath $cer | Out-Null
    Log "exported $pfx and $cer"
}

# 3) Import into trusted stores so audiodg trusts us
try {
    Import-Certificate -FilePath $cer -CertStoreLocation 'Cert:\LocalMachine\Root' | Out-Null
    Log 'imported into LocalMachine\Root'
} catch { Log "Root import error: $_" }
try {
    Import-Certificate -FilePath $cer -CertStoreLocation 'Cert:\LocalMachine\TrustedPublisher' | Out-Null
    Log 'imported into LocalMachine\TrustedPublisher'
} catch { Log "TrustedPublisher import error: $_" }

# 4) Final state
if ($needReboot) {
    Log 'RESULT: reboot required for testsigning to take effect'
    '__REBOOT_REQUIRED__' | Out-File -FilePath 'C:\ProgramData\TrontEq\reboot.flag'
} else {
    Log 'RESULT: all set. No reboot needed.'
    Remove-Item -Force 'C:\ProgramData\TrontEq\reboot.flag' -ErrorAction SilentlyContinue
}

Log 'Done.'
