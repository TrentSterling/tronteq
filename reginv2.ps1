$out='C:\trontstack\tronteq\reg2.log'
"== real APO registration @ $(Get-Date) ==" | Out-File $out -Encoding utf8
function Dump($clsid,$label){
  "### $label  $clsid" | Add-Content $out
  $p="Registry::HKEY_CLASSES_ROOT\CLSID\$clsid"
  if(Test-Path $p){
    Get-ChildItem $p -Recurse -EA SilentlyContinue | ForEach-Object {
      $k=Get-Item $_.PSPath; "  [$($_.PSChildName)]" | Add-Content $out
      foreach($vn in $k.GetValueNames()){ "      '$vn' [$($k.GetValueKind($vn))] = $($k.GetValue($vn))" | Add-Content $out }
    }
  } else { "  (MISSING in HKCR\CLSID)" | Add-Content $out }
}
Dump '{13AB3EBD-137E-4903-9D89-60BE8277FD17}' 'EFX (real endpoint-effect APO)'
Dump '{C9453E73-8C5C-4463-9984-AF8BAB2F5447}' 'MFX (real mode-effect APO)'
# Where are APOs registered with the engine? check the known APO store
"### AudioProcessingObjects store" | Add-Content $out
$apo='Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\...'
foreach($cand in @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Audio',
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio',
  'Registry::HKEY_CLASSES_ROOT\AudioEngine\AudioProcessingObjects',
  'HKLM:\SOFTWARE\Classes\AudioEngine\AudioProcessingObjects')){
  "  exists? $cand : $(Test-Path $cand)" | Add-Content $out
}
# does the EFX APO appear under any AudioProcessingObjects key?
$found = reg query HKLM\SOFTWARE /f "13AB3EBD-137E-4903-9D89-60BE8277FD17" /s 2>$null
"### reg search HKLM for EFX clsid:" | Add-Content $out
$found | Add-Content $out
