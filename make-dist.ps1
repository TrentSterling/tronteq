# Assemble dist\TrontEQ\ -- everything a second machine needs to run TrontEQ
# with NO Rust / VS / Windows SDK installed. $PSScriptRoot-relative so this
# works no matter where the repo is cloned.
#
# Read-only against the live install: only READS C:\ProgramData\TrontEq\dev-cert.cer
# (the public cert). Never touches state.bin, the installed DLL, or any service.
#
# Output: dist\TrontEQ\
#   TrontEqApo.dll   tronteq.exe   tronteq-cli.exe   dev-cert.cer
#   bootstrap.ps1    SETUP.md
#
# Run after building:
#   cargo build --release --workspace
#   cmd /c apo\build.bat

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$dist = Join-Path $root 'dist\TrontEQ'

$sources = [ordered]@{
    'TrontEqApo.dll'      = Join-Path $root 'apo\build\TrontEqApo.dll'
    'tronteq.exe'         = Join-Path $root 'target\release\tronteq.exe'
    'tronteq-cli.exe'     = Join-Path $root 'target\release\tronteq-cli.exe'
    'dev-cert.cer'        = 'C:\ProgramData\TrontEq\dev-cert.cer'
    'bootstrap.ps1'       = Join-Path $root 'bootstrap.ps1'
    'SETUP.md'            = Join-Path $root 'SETUP.md'
    'Install TrontEQ.cmd' = Join-Path $root 'packaging\Install TrontEQ.cmd'
    'Launch TrontEQ.cmd'  = Join-Path $root 'packaging\Launch TrontEQ.cmd'
}

$missing = $sources.GetEnumerator() | Where-Object { -not (Test-Path $_.Value) }
if ($missing) {
    Write-Host "ERROR: missing source file(s) -- build first:" -ForegroundColor Red
    foreach ($m in $missing) { Write-Host "  $($m.Key)  <-  $($m.Value)" -ForegroundColor Red }
    Write-Host ""
    Write-Host "  cargo build --release --workspace"
    Write-Host "  cmd /c apo\build.bat"
    Write-Host "  (and make sure C:\ProgramData\TrontEq\dev-cert.cer exists -- see SETUP.md)"
    exit 1
}

if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force -Path $dist | Out-Null

foreach ($kv in $sources.GetEnumerator()) {
    Copy-Item -Path $kv.Value -Destination (Join-Path $dist $kv.Key) -Force
}

Write-Host ""
Write-Host "== dist\TrontEQ\ assembled ==" -ForegroundColor Green
Get-ChildItem $dist | ForEach-Object {
    Write-Host ("  {0,-20} {1,8:N1} KB" -f $_.Name, ($_.Length / 1KB))
}

# The DLL that ships must carry a valid Authenticode signature -- an unsigned
# (or unsigned-since-last-build) DLL will silently fail to load in audiodg on
# the target machine, and that failure is hard to diagnose remotely.
$dllPath = Join-Path $dist 'TrontEqApo.dll'
$sig = Get-AuthenticodeSignature $dllPath
Write-Host ""
Write-Host "TrontEqApo.dll signature: $($sig.Status)" -NoNewline
if ($sig.SignerCertificate) { Write-Host "  ($($sig.SignerCertificate.Subject))" } else { Write-Host "" }
if ($sig.Status -ne 'Valid') {
    Write-Host "WARNING: DLL is not validly signed on THIS machine. Sign it before shipping:" -ForegroundColor Yellow
    Write-Host '  cargo run -p tronteq-cli -- install    (re-signs as part of install)' -ForegroundColor Yellow
    Write-Host '  or run one of the existing deploy-*.ps1 scripts, then re-run make-dist.ps1.' -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Next: zip dist\TrontEQ\, copy it to the target machine, extract, and"
Write-Host "double-click 'Install TrontEQ.cmd' (it self-elevates and runs bootstrap)."
Write-Host "See SETUP.md for the manual / source-build paths."
