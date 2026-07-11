# One-shot: stop old instance -> rebuild (with viz device-swap fix) -> relaunch.
# Runs elevated (single UAC). Same user, so target files stay owned by Trent.
$ErrorActionPreference = 'Continue'
$log = 'C:\trontstack\tronteq\vizfix-deploy.log'
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
"== vizfix deploy @ $(Get-Date) ==" | Out-File $log -Encoding utf8

"stopping old instance(s)..." | Out-File $log -Append -Encoding utf8
Stop-Process -Name tronteq -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

Set-Location 'C:\trontstack\tronteq'
"building release (cargo: $cargo)..." | Out-File $log -Append -Encoding utf8
& $cargo build --release -p tronteq *>> $log
$code = $LASTEXITCODE
"cargo exit: $code" | Out-File $log -Append -Encoding utf8

if ($code -eq 0) {
    "launching new build..." | Out-File $log -Append -Encoding utf8
    Start-Process 'C:\trontstack\tronteq\target\release\tronteq.exe'
    Start-Sleep -Seconds 6
    $p = Get-Process tronteq -ErrorAction SilentlyContinue
    if ($p) {
        "RUNNING pid=$($p.Id) privMB=$([math]::Round($p.PrivateMemorySize64/1MB,0))" | Out-File $log -Append -Encoding utf8
    } else {
        "NOT RUNNING after launch" | Out-File $log -Append -Encoding utf8
    }
} else {
    "BUILD FAILED - not launching" | Out-File $log -Append -Encoding utf8
}
"== done @ $(Get-Date) ==" | Out-File $log -Append -Encoding utf8
