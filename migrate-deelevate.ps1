# One-shot elevated migration for TrontEQ v0.9.0 de-elevation (2026-07-23).
# Run ONCE elevated. Batches every admin step so there's exactly one UAC:
#   1. kill the old elevated GUI (Medium can't)
#   2. grant BUILTIN\Users Modify on C:\ProgramData\TrontEq (Medium GUI must
#      write state.bin / settings.json / profiles; dev-cert.pfx keeps its own
#      broken-inheritance Admin/SYSTEM-only ACL, untouched by this grant)
#   3. recreate the TrontEQ logon task at LIMITED run level
# The build + relaunch happen OUTSIDE this script, non-elevated.
$ErrorActionPreference = 'Continue'
$log = "$PSScriptRoot\migrate-deelevate.log"
"START $(Get-Date -Format o)" | Out-File $log -Encoding utf8

taskkill /im tronteq.exe /f | Add-Content $log

icacls "C:\ProgramData\TrontEq" /grant "*S-1-5-32-545:(OI)(CI)(M)" /T /C | Add-Content $log

schtasks /create /tn TrontEQ /tr "C:\trontstack\tronteq\target\release\tronteq.exe" /sc onlogon /rl LIMITED /f | Add-Content $log

"DONE $(Get-Date -Format o)" | Add-Content $log
