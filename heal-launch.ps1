# One-shot heal: register autostart (persistence) + launch the rebuilt GUI
# elevated + verify. Runs elevated so no per-step UAC. Results -> heal-launch.log.
$ErrorActionPreference = 'Continue'
$log = 'C:\trontstack\tronteq\heal-launch.log'
$rel = 'C:\trontstack\tronteq\target\release'
"== heal @ $(Get-Date) ==" | Out-File $log -Encoding utf8

Set-Location $rel

"--- register-autostart (onlogon, HIGHEST) ---" | Out-File $log -Append -Encoding utf8
& "$rel\tronteq-cli.exe" register-autostart *>> $log
"register-autostart exit: $LASTEXITCODE" | Out-File $log -Append -Encoding utf8

"--- confirm scheduled task ---" | Out-File $log -Append -Encoding utf8
& schtasks /query /tn TrontEQ /fo LIST *>> $log

"--- launch GUI (elevated, inherited) ---" | Out-File $log -Append -Encoding utf8
Start-Process -FilePath "$rel\tronteq.exe"
Start-Sleep -Seconds 5

$p = Get-Process tronteq -ErrorAction SilentlyContinue
if ($p) {
    "GUI RUNNING: pid=$($p.Id) startTime=$($p.StartTime) privMB=$([math]::Round($p.PrivateMemorySize64/1MB,0))" | Out-File $log -Append -Encoding utf8
} else {
    "GUI NOT RUNNING after launch" | Out-File $log -Append -Encoding utf8
}

"--- last 6 lines of crash.log ---" | Out-File $log -Append -Encoding utf8
Get-Content 'C:\ProgramData\TrontEq\crash.log' -Tail 6 *>> $log
"== done ==" | Out-File $log -Append -Encoding utf8
