$out='C:\trontstack\tronteq\dumpfx.log'
"== FxProperties dump @ $(Get-Date) ==" | Out-File $out -Encoding utf8
$base='HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render'
Get-ChildItem $base -EA SilentlyContinue | ForEach-Object {
  $ep=$_.PSChildName
  $fx=Join-Path $_.PSPath 'FxProperties'
  if (Test-Path $fx) {
    $props = Get-ItemProperty $fx
    $names = $props.PSObject.Properties | Where-Object { $_.Name -notlike 'PS*' }
    if ($names) {
      "--- endpoint $ep ---" | Add-Content $out
      foreach($pr in $names){
        $v=$pr.Value
        if($v -is [byte[]]){ $hex=($v|ForEach-Object{$_.ToString('X2')}) -join ' '; ("  {0}`n     [{1} bytes] {2}" -f $pr.Name,$v.Length,$hex) | Add-Content $out }
        else { ("  {0} = {1}" -f $pr.Name,$v) | Add-Content $out }
      }
    }
  }
}
