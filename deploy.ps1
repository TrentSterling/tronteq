$log='C:\trontstack\tronteq\deploy.log'
"== deploy buffer-fix @ $(Get-Date) ==" | Out-File $log -Encoding utf8
Set-Location 'C:\trontstack\tronteq'
Stop-Service audiosrv -Force
Start-Sleep 1
(& '.\target\release\tronteq-cli.exe' install --device 0 2>&1 | Out-String) | Add-Content $log
Start-Service audiosrv
Start-Sleep 3
(New-Object System.Media.SoundPlayer "$env:WINDIR\Media\chord.wav").PlaySync()
Start-Sleep 1
try { $fs=[IO.File]::Open('C:\ProgramData\TrontEq\TrontEqApo.dll','Open','ReadWrite','None'); $fs.Close(); "DLL LOCK: FREE (not reloaded - toggle device)" | Add-Content $log }
catch [System.IO.IOException] { "DLL LOCK: LOCKED -> reloaded" | Add-Content $log }
catch { "DLL LOCK: other" | Add-Content $log }
