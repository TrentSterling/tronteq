$log = 'C:\trontstack\tronteq\fix-acl.log'
"== TrontEQ ACL fix @ $(Get-Date) ==" | Out-File $log -Encoding utf8
$dir = 'C:\ProgramData\TrontEq'
# Grant READ to: ALL APPLICATION PACKAGES, ALL RESTRICTED APPLICATION PACKAGES,
# LOCAL SERVICE, Everyone. (OI)(CI) so files created later inherit it; /T fixes existing.
$out = icacls $dir /grant '*S-1-15-2-1:(OI)(CI)(RX)' /grant '*S-1-15-2-2:(OI)(CI)(RX)' /grant '*S-1-5-19:(OI)(CI)(RX)' /grant '*S-1-1-0:(OI)(CI)(RX)' /T /C 2>&1 | Out-String
$out | Add-Content $log -Encoding utf8
"--- restart audiosrv ---" | Add-Content $log -Encoding utf8
Restart-Service audiosrv -Force
Start-Sleep 2
"done" | Add-Content $log -Encoding utf8
