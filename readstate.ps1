function Read-State {
  $fs=[IO.File]::Open('C:\ProgramData\TrontEq\state.bin','Open','Read','ReadWrite')
  $b=New-Object byte[] 144; [void]$fs.Read($b,0,144); $fs.Close()
  "ver={0} b0={1} b3={2} byp={3}" -f ([BitConverter]::ToUInt64($b,0)),([BitConverter]::ToSingle($b,12)),([BitConverter]::ToSingle($b,12+3*16)),($b[140])
}
$p=Get-Process tronteq -EA SilentlyContinue
"GUI count: $(($p|Measure-Object).Count)  Responding: $($p.Responding)  MainWindowTitle: '$($p.MainWindowTitle)'"
"DRAG A SLIDER up and down continuously for the next ~12 seconds..."
for ($i=0; $i -lt 14; $i++) { "  $i : $(Read-State)"; Start-Sleep -Milliseconds 850 }
