Add-Type -AssemblyName System.Drawing
$src = [System.Drawing.Image]::FromFile('C:\trontstack\tronteq\gui\assets\icon.png')
$sizes = 256,48,32,16
$datas = @()
foreach ($s in $sizes) {
  $bmp = New-Object System.Drawing.Bitmap $s, $s
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.DrawImage($src, 0, 0, $s, $s); $g.Dispose()
  $ms = New-Object System.IO.MemoryStream
  $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
  $datas += ,($ms.ToArray()); $bmp.Dispose(); $ms.Dispose()
}
$src.Dispose()
$out = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter $out
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i=0; $i -lt $sizes.Count; $i++) {
  $s=$sizes[$i]; $d=$datas[$i]; $w = if($s -ge 256){0}else{$s}
  $bw.Write([byte]$w); $bw.Write([byte]$w); $bw.Write([byte]0); $bw.Write([byte]0)
  $bw.Write([uint16]1); $bw.Write([uint16]32); $bw.Write([uint32]$d.Length); $bw.Write([uint32]$offset)
  $offset += $d.Length
}
foreach ($d in $datas) { $bw.Write($d) }
$bw.Flush()
[System.IO.File]::WriteAllBytes('C:\trontstack\tronteq\gui\assets\icon.ico', $out.ToArray()); $out.Dispose()
"wrote icon.ico: $((Get-Item 'C:\trontstack\tronteq\gui\assets\icon.ico').Length) bytes"
