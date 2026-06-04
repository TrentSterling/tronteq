$dir='C:\ProgramData\TrontEq'; $out="$dir\diag2.log"
function W($m){ $m | Out-File $out -Append -Encoding utf8 }
"== evidence @ $(Get-Date) ==" | Out-File $out -Encoding utf8
try { Set-Location 'C:\trontstack\tronteq'; W "=== DEVICES ==="; W ((& '.\target\release\tronteq-cli.exe' list-devices | Out-String)) } catch { W "devices err: $_" }
try {
  $p="$dir\TrontEqApo.dll"
  try { $fs=[IO.File]::Open($p,'Open','ReadWrite','None'); $fs.Close(); $lock='FREE -> NOT loaded by any process' }
  catch [System.IO.IOException] { $lock='LOCKED -> loaded as image (audiodg has it)' }
  catch { $lock="other: $($_.Exception.GetType().Name)" }
  W "=== DLL LOCK ==="; W $lock
} catch { W "lock err: $_" }
try {
  W "=== our CLSID in render FxProperties registry? ==="
  $any=$false
  Get-ChildItem 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render' -EA SilentlyContinue | ForEach-Object {
    $fx=Join-Path $_.PSPath 'FxProperties'
    if (Test-Path $fx) {
      foreach($pr in (Get-ItemProperty $fx).PSObject.Properties){
        $v=$pr.Value
        $s = if($v -is [byte[]]){ ($v | ForEach-Object { $_.ToString('X2') }) -join '' } else { "$v" }
        if($s -match 'CA64E60A' -or $s -match '0AE664CA'){ W ("  endpoint {0} :: {1} = {2}" -f $_.PSChildName,$pr.Name,$s); $any=$true }
      }
    }
  }
  if(-not $any){ W "  NOT FOUND in any render endpoint FxProperties" }
} catch { W "reg err: $_" }
try { (New-Object System.Media.SoundPlayer "$env:WINDIR\Media\chord.wav").PlaySync(); Start-Sleep 1 } catch {}
W "=== apo.log ==="
if(Test-Path "$dir\apo.log"){ Get-Content "$dir\apo.log" | ForEach-Object { W $_ } } else { W "(none)" }
