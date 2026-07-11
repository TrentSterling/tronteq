# Dev cycle: kill running GUI -> rebuild release -> relaunch -> verify alive + crash.log.
# Self-elevating (one UAC per run). The exe is LOCKED while running, so the kill must
# happen before the link step — that's why the build lives inside this script.
# Results -> dev-cycle.log (append, timestamped).
param([switch]$Elevated)

$me = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $me.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Start-Process powershell -Verb RunAs -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -Elevated"
    exit
}

$ErrorActionPreference = 'Continue'
$root = 'C:\trontstack\tronteq'
$rel  = "$root\target\release"
$log  = "$root\dev-cycle.log"
Set-Location $root

"`n== dev-cycle @ $(Get-Date) ==" | Out-File $log -Append -Encoding utf8

# 1) Kill the running GUI (elevated, so no Access Denied).
$p = Get-Process tronteq -ErrorAction SilentlyContinue
if ($p) {
    "kill: pid=$($p.Id)" | Out-File $log -Append -Encoding utf8
    Stop-Process -Name tronteq -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
} else {
    "kill: not running" | Out-File $log -Append -Encoding utf8
}

# 2) Rebuild (exe now unlocked). cmd /c so stderr doesn't wrap in ErrorRecords.
"--- cargo build --release -p tronteq ---" | Out-File $log -Append -Encoding utf8
cmd /c "cargo build --release -p tronteq 2>&1" | Out-File $log -Append -Encoding utf8
$build = $LASTEXITCODE
"build exit: $build" | Out-File $log -Append -Encoding utf8
if ($build -ne 0) {
    "RESULT: BUILD FAILED — not launching" | Out-File $log -Append -Encoding utf8
    exit 1
}

# 3) Relaunch (elevated parent -> inherited token, no second UAC).
Start-Process -FilePath "$rel\tronteq.exe"
Start-Sleep -Seconds 10

# 4) Verify: process alive after 10s + crash.log tail.
$p = Get-Process tronteq -ErrorAction SilentlyContinue
if ($p) {
    "RESULT: RUNNING pid=$($p.Id) privMB=$([math]::Round($p.PrivateMemorySize64/1MB,0))" | Out-File $log -Append -Encoding utf8
} else {
    "RESULT: NOT RUNNING after launch (check crash.log below)" | Out-File $log -Append -Encoding utf8
}
"--- crash.log tail ---" | Out-File $log -Append -Encoding utf8
Get-Content 'C:\ProgramData\TrontEq\crash.log' -Tail 5 -ErrorAction SilentlyContinue | Out-File $log -Append -Encoding utf8
"== done ==" | Out-File $log -Append -Encoding utf8
