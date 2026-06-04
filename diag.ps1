$dir='C:\ProgramData\TrontEq'
Remove-Item "$dir\apo.log" -ErrorAction SilentlyContinue
Set-Location 'C:\trontstack\tronteq'
& '.\target\release\tronteq-cli.exe' uninstall | Out-Null
& '.\target\release\tronteq-cli.exe' install --device 2 | Out-Null
# grant WRITE (modify) so audiodg's token can create apo.log
icacls $dir /grant '*S-1-15-2-1:(OI)(CI)(M)' /grant '*S-1-15-2-2:(OI)(CI)(M)' /grant '*S-1-5-19:(OI)(CI)(M)' /T /C | Out-Null
Restart-Service audiosrv -Force
Start-Sleep 3
'diag-ready' | Out-File "$dir\diag-ready.txt"
