# TrontEQ: reinstall rebuilt APO (now implements IAudioSystemEffects2) to
# Headphones (Nuvelon ONE, device 2) + enable audio diagnostics. Run elevated.
$log = 'C:\trontstack\tronteq\resume-install.log'
"== TrontEQ APO reinstall @ $(Get-Date) ==" | Out-File $log -Encoding utf8
Set-Location 'C:\trontstack\tronteq'

# Enable the audio operational log so APO load failures get recorded (audiodg is
# a protected process, so this is our window into what it does).
"--- enable Microsoft-Windows-Audio/Operational ---" | Add-Content $log -Encoding utf8
(cmd /c 'wevtutil set-log "Microsoft-Windows-Audio/Operational" /enabled:true 2>&1') | Add-Content $log -Encoding utf8

"--- uninstall (clean old DLL) ---" | Add-Content $log -Encoding utf8
(& '.\target\release\tronteq-cli.exe' uninstall 2>&1 | Out-String) | Add-Content $log -Encoding utf8

"--- install --device 2 (rebuilt DLL -> Nuvelon ONE) ---" | Add-Content $log -Encoding utf8
(& '.\target\release\tronteq-cli.exe' install --device 2 2>&1 | Out-String) | Add-Content $log -Encoding utf8
"exit code: $LASTEXITCODE" | Add-Content $log -Encoding utf8
