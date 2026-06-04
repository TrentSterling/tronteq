$dir='C:\ProgramData\TrontEq'
Remove-Item "$dir\apo.log" -ErrorAction SilentlyContinue
# Everyone: modify (so whatever restricted token audiodg uses can write the log)
icacls $dir /grant '*S-1-1-0:(OI)(CI)(M)' /T /C | Out-Null
Restart-Service audiosrv -Force
Start-Sleep 3
