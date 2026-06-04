$out='C:\trontstack\tronteq\reg.log'
"== reg investigate @ $(Get-Date) ==" | Out-File $out -Encoding utf8
function Dump($path,$label){
  "### $label" | Add-Content $out
  "    $path" | Add-Content $out
  if(Test-Path $path){
    $root=Get-Item $path
    foreach($vn in $root.GetValueNames()){ "    (val) '$vn' [$($root.GetValueKind($vn))] = $($root.GetValue($vn))" | Add-Content $out }
    Get-ChildItem $path -Recurse -EA SilentlyContinue | ForEach-Object {
      $k=Get-Item $_.PSPath
      "    [SUBKEY] $($_.PSChildName)" | Add-Content $out
      foreach($vn in $k.GetValueNames()){ "        '$vn' [$($k.GetValueKind($vn))] = $($k.GetValue($vn))" | Add-Content $out }
    }
  } else { "    (MISSING)" | Add-Content $out }
}
Dump 'Registry::HKEY_CLASSES_ROOT\CLSID\{5860E1C5-F95C-4a7a-8EC8-8AEF24F379A1}' 'WORKING Microsoft SFX APO'
Dump 'Registry::HKEY_CLASSES_ROOT\CLSID\{CA64E60A-A3C4-43B8-970F-0360055172F2}' 'OUR APO'
