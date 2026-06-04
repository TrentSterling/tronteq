$out='C:\trontstack\tronteq\lockcheck.log'
$p='C:\ProgramData\TrontEq\TrontEqApo.dll'
$res=@()
for($i=0;$i -lt 4;$i++){
  try { $fs=[IO.File]::Open($p,'Open','ReadWrite','None'); $fs.Close(); $res+="check $i: FREE (not loaded)" }
  catch [System.IO.IOException] { $res+="check $i: LOCKED (APO LOADED!)" }
  catch { $res+="check $i: $($_.Exception.GetType().Name)" }
  Start-Sleep -Milliseconds 800
}
$res | Out-File $out -Encoding utf8
