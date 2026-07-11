# Capture the LIVE TrontEQ window to a PNG via PrintWindow(PW_RENDERFULLCONTENT).
# Works even if the window is behind others (DWM-composited content, GL included).
# The kittest harness can't render the glow GL stage (it runs wgpu), so this is
# the visual-verification path for shader modes. Usage: livecap.ps1 -Out x.png
param([string]$Out = "livecap.png")

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public struct WRECT { public int L; public int T; public int R; public int B; }
public class WCap {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern IntPtr FindWindowW(string cls, string title);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr h, out WRECT r);
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
}
'@
Add-Type -AssemblyName System.Drawing

$h = [WCap]::FindWindowW($null, "TrontEQ")
if ($h -eq [IntPtr]::Zero) { Write-Output "NOWINDOW"; exit 1 }
$r = New-Object WRECT
[WCap]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.R - $r.L
$ht = $r.B - $r.T
if ($w -le 0 -or $ht -le 0) { Write-Output "BADRECT"; exit 1 }

$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc()
$ok = [WCap]::PrintWindow($h, $dc, 2)   # 2 = PW_RENDERFULLCONTENT
$g.ReleaseHdc($dc)
$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "SAVED $Out ok=$ok ${w}x${ht}"
