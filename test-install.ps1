$log='C:\trontstack\tronteq\test-install.log'
"== test new CLI install @ $(Get-Date) ==" | Out-File $log -Encoding utf8
Set-Location 'C:\trontstack\tronteq'
(& '.\target\release\tronteq-cli.exe' install 2>&1 | Out-String) | Add-Content $log
"--- verify registry ---" | Add-Content $log
$our='{CA64E60A-A3C4-43B8-970F-0360055172F2}'
"APO store present: $(Test-Path "HKLM:\SOFTWARE\Classes\AudioEngine\AudioProcessingObjects\$our")" | Add-Content $log
$ep='{15399fc7-8fee-4569-8f7c-e54ba74a4065}'
$fx="HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$ep\FxProperties"
"EFX(,6) on Smokin Buds = $((Get-ItemProperty $fx -Name '{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6' -EA SilentlyContinue).'{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6')" | Add-Content $log
Start-Sleep 2
try { $f=[IO.File]::Open('C:\ProgramData\TrontEq\TrontEqApo.dll','Open','ReadWrite','None'); $f.Close(); "DLL: FREE" | Add-Content $log } catch [System.IO.IOException] { "DLL: LOCKED (loaded)" | Add-Content $log } catch { "DLL: ?" | Add-Content $log }
