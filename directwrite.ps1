# Write an obvious "kill the bass" curve directly into state.bin via the seqlock,
# bypassing the GUI, to test whether the APO applies live updates.
$fs=[IO.File]::Open('C:\ProgramData\TrontEq\state.bin','Open','ReadWrite','ReadWrite')
$br=New-Object IO.BinaryReader $fs
$bw=New-Object IO.BinaryWriter $fs
$fs.Position=0; $cur=$br.ReadUInt64()

# version -> odd (writer in progress)
$fs.Position=0; $bw.Write([uint64]($cur+1)); $bw.Flush()

$freqs=@(31.25,62.5,125.0,250.0,500.0,1000.0,2000.0,4000.0)
$gains=@(-24.0,-24.0,-24.0,-18.0,0.0,0.0,0.0,0.0)   # gut the low end
for($i=0;$i -lt 8;$i++){
  $fs.Position=8+$i*16
  $bw.Write([float]$freqs[$i]); $bw.Write([float]$gains[$i]); $bw.Write([float]1.0); $bw.Write([uint32]0)
}
$fs.Position=140; $bw.Write([byte]0)   # bypass = off
$bw.Flush()

# version -> even (committed)
$fs.Position=0; $bw.Write([uint64]($cur+2)); $bw.Flush()
$fs.Close()
"wrote bass-kill curve; version $cur -> $($cur+2)"
