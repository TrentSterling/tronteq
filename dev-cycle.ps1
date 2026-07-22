# Dev cycle: kill running GUI -> rebuild release -> relaunch -> verify. ZERO UAC.
#
# The trick: TrontEQ is launched by an onlogon /rl HIGHEST scheduled task
# ("TrontEQ") that runs elevated. Driving that task from the Task Scheduler ends
# and relaunches the elevated GUI with NO consent prompt (managing your own task
# needs no elevation). `schtasks /run` launches the freshly-built exe elevated.
#
# This replaced a self-elevating variant that LOOPED: its admin re-check kept
# re-spawning UAC prompts and never ran, so every "yes" appeared to do nothing.
# Do NOT reintroduce Start-Process -Verb RunAs here.
#
# Results -> dev-cycle.log (append, timestamped).

$ErrorActionPreference = 'Continue'
$root = $PSScriptRoot
$rel  = "$root\target\release"
$log  = "$root\dev-cycle.log"
Set-Location $root

"`n== dev-cycle @ $(Get-Date) ==" | Out-File $log -Append -Encoding utf8

# 1) End the running GUI via the scheduler (elevated terminate, no UAC).
schtasks /end /tn TrontEQ *>> $log
Start-Sleep -Seconds 2
if (Get-Process tronteq -ErrorAction SilentlyContinue) {
    "warn: still alive after /end (self-heal?) - retrying" | Out-File $log -Append -Encoding utf8
    schtasks /end /tn TrontEQ *>> $log
    Start-Sleep -Seconds 2
}

# 2) Rebuild (exe now unlocked). cmd /c so native stderr doesn't wrap in ErrorRecords.
"--- cargo build --release -p tronteq ---" | Out-File $log -Append -Encoding utf8
cmd /c "cargo build --release -p tronteq 2>&1" | Out-File $log -Append -Encoding utf8
$build = $LASTEXITCODE
"build exit: $build" | Out-File $log -Append -Encoding utf8
if ($build -ne 0) {
    "RESULT: BUILD FAILED - old instance stays dead, fix + rerun" | Out-File $log -Append -Encoding utf8
    exit 1
}

# 3) Relaunch the new build via the scheduler (elevated, no UAC).
schtasks /run /tn TrontEQ *>> $log
Start-Sleep -Seconds 10

# 4) Verify: process alive + crash.log tail.
$p = Get-Process tronteq -ErrorAction SilentlyContinue
if ($p) {
    "RESULT: RUNNING pid=$($p.Id) privMB=$([math]::Round($p.PrivateMemorySize64/1MB,0))" | Out-File $log -Append -Encoding utf8
} else {
    "RESULT: NOT RUNNING after launch (check crash.log below)" | Out-File $log -Append -Encoding utf8
}
"--- crash.log tail ---" | Out-File $log -Append -Encoding utf8
Get-Content 'C:\ProgramData\TrontEq\crash.log' -Tail 5 -ErrorAction SilentlyContinue | Out-File $log -Append -Encoding utf8
"== done ==" | Out-File $log -Append -Encoding utf8
