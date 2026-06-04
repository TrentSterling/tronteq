$out='C:\trontstack\tronteq\check.log'
"== check @ $(Get-Date) ==" | Out-File $out -Encoding utf8
$pd='C:\ProgramData\TrontEq\TrontEqApo.dll'; $bd='C:\trontstack\tronteq\apo\build\TrontEqApo.dll'
"deployed: $((Get-Item $pd).LastWriteTime) $((Get-Item $pd).Length)b" | Add-Content $out
"built   : $((Get-Item $bd).LastWriteTime) $((Get-Item $bd).Length)b" | Add-Content $out
"hash match (diagnostic DLL deployed?): $((Get-FileHash $pd).Hash -eq (Get-FileHash $bd).Hash)" | Add-Content $out
try { $fs=[IO.File]::Open($pd,'Open','ReadWrite','None'); $fs.Close(); "DLL LOCK: FREE -> NOT loaded by any process" | Add-Content $out }
catch [System.IO.IOException] { "DLL LOCK: LOCKED -> loaded as image (audiodg has it)" | Add-Content $out }
catch { "DLL LOCK: $($_.Exception.GetType().Name)" | Add-Content $out }
Set-Location 'C:\trontstack\tronteq'
"--- devices ---" | Add-Content $out; (& .\target\release\tronteq-cli.exe list-devices | Out-String) | Add-Content $out
$ep='{76af72a1-a1af-42f8-88ea-7f5023c6e269}'
$fx="HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$ep\FxProperties"
"--- Nuvelon FxProperties ($ep) ---" | Add-Content $out
if(Test-Path $fx){ (Get-ItemProperty $fx).PSObject.Properties | Where-Object {$_.Name -notlike 'PS*'} | ForEach-Object { "  $($_.Name) = $($_.Value)" | Add-Content $out } } else { "  (none)" | Add-Content $out }
