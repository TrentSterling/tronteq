$log='C:\trontstack\tronteq\deploy2.log'
"== deploy DSP batch @ $(Get-Date) ==" | Out-File $log -Encoding utf8
Set-Location 'C:\trontstack\tronteq'
(& '.\target\release\tronteq-cli.exe' install 2>&1 | Out-String) | Add-Content $log
