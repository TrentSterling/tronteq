$out='C:\trontstack\tronteq\loadcheck.log'
"== loadcheck @ $(Get-Date) ==" | Out-File $out -Encoding utf8
$p='C:\ProgramData\TrontEq\TrontEqApo.dll'
try { $fs=[IO.File]::Open($p,'Open','ReadWrite','None'); $fs.Close(); "DLL LOCK: FREE -> not loaded" | Add-Content $out }
catch [System.IO.IOException] { "DLL LOCK: LOCKED -> APO IS LOADED INTO A PROCESS!" | Add-Content $out }
catch { "DLL LOCK: $($_.Exception.GetType().Name)" | Add-Content $out }
$a=Get-Process audiodg -EA SilentlyContinue
"audiodg: $(if($a){"PID $($a.Id), up since $($a.StartTime)"}else{'NOT running'})" | Add-Content $out
$ep='{76af72a1-a1af-42f8-88ea-7f5023c6e269}'
$fx="HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$ep\FxProperties"
"EFX(,6) = $((Get-ItemProperty $fx -Name '{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6' -EA SilentlyContinue).'{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6')" | Add-Content $out
