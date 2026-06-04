Set-Location 'C:\trontstack\tronteq'
# copy+sign the freshly built DLL into place (FxProperties already points at our CLSID)
& '.\target\release\tronteq-cli.exe' install --device 2 | Out-Null
Restart-Service audiosrv -Force
Start-Sleep 2
'refreshed'
