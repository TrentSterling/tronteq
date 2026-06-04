$out='C:\trontstack\tronteq\apostore.log'
"== AudioProcessingObjects registration schema @ $(Get-Date) ==" | Out-File $out -Encoding utf8
function Dump($clsid,$label){
  "### $label  $clsid" | Add-Content $out
  $p="Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\AudioEngine\AudioProcessingObjects\$clsid"
  if(Test-Path $p){
    $k=Get-Item $p
    foreach($vn in $k.GetValueNames()){ "    '$vn' [$($k.GetValueKind($vn))] = $($k.GetValue($vn))" | Add-Content $out }
    Get-ChildItem $p -Recurse -EA SilentlyContinue | ForEach-Object {
      $s=Get-Item $_.PSPath; "    [SUB $($_.PSChildName)]" | Add-Content $out
      foreach($vn in $s.GetValueNames()){ "        '$vn' [$($s.GetValueKind($vn))] = $($s.GetValue($vn))" | Add-Content $out }
    }
  } else { "    (MISSING)" | Add-Content $out }
}
Dump '{13AB3EBD-137E-4903-9D89-60BE8277FD17}' 'EFX (WMALFXGFXDSP)'
Dump '{C9453E73-8C5C-4463-9984-AF8BAB2F5447}' 'MFX (WMALFXGFXDSP)'
"### our CLSID present?" | Add-Content $out
"  $(Test-Path 'Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Classes\AudioEngine\AudioProcessingObjects\{CA64E60A-A3C4-43B8-970F-0360055172F2}')" | Add-Content $out
