$out='C:\trontstack\tronteq\registerapo.log'
"== register APO @ $(Get-Date) ==" | Out-File $out -Encoding utf8
function W($m){ $m | Out-File $out -Append -Encoding utf8 }
$clsid='{CA64E60A-A3C4-43B8-970F-0360055172F2}'
$k="HKLM:\SOFTWARE\Classes\AudioEngine\AudioProcessingObjects\$clsid"
try {
  New-Item -Path $k -Force -EA Stop | Out-Null
  New-ItemProperty $k -Name 'FriendlyName' -Value 'TrontEQ APO' -PropertyType String -Force | Out-Null
  New-ItemProperty $k -Name 'Copyright' -Value 'Trent Sterling' -PropertyType String -Force | Out-Null
  New-ItemProperty $k -Name 'MajorVersion' -Value 1 -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'MinorVersion' -Value 0 -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'Flags' -Value 15 -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'MinInputConnections' -Value 1 -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'MaxInputConnections' -Value 1 -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'MinOutputConnections' -Value 1 -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'MaxOutputConnections' -Value 1 -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'MaxInstances' -Value (-1) -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'NumAPOInterfaces' -Value 1 -PropertyType DWord -Force | Out-Null
  New-ItemProperty $k -Name 'APOInterface0' -Value '{FD7F2B29-24D0-4B5C-B177-592C39F9CA10}' -PropertyType String -Force | Out-Null
  W "registered APO at $k"
} catch { W "REGISTER FAILED: $($_.Exception.Message)" }
W "readback:"; (Get-Item $k -EA SilentlyContinue).GetValueNames() | ForEach-Object { W "  $_ = $((Get-Item $k).GetValue($_))" }
Restart-Service audiosrv -Force
Start-Sleep 2
(New-Object System.Media.SoundPlayer "$env:WINDIR\Media\chord.wav").PlaySync()
Start-Sleep 1
try { $fs=[IO.File]::Open('C:\ProgramData\TrontEq\TrontEqApo.dll','Open','ReadWrite','None'); $fs.Close(); W "DLL LOCK: FREE -> still NOT loaded" }
catch [System.IO.IOException] { W "DLL LOCK: LOCKED -> APO IS LOADED!" }
catch { W "DLL LOCK: $($_.Exception.GetType().Name)" }
