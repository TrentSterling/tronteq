# Capture the LIVE TrontEQ window to a PNG. Finds the window by PROCESS (title
# matching proved fragile), tries PrintWindow(PW_RENDERFULLCONTENT), and falls
# back to a screen copy of the window rect (UIPI can block PrintWindow against
# the elevated app; the fallback needs the window frontmost, which it is right
# after a fresh launch in the verify cycle). SetProcessDPIAware so rects are
# physical pixels. Usage: livecap.ps1 -Out x.png
param([string]$Out = "livecap.png")

Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public struct WRECT { public int L; public int T; public int R; public int B; }
public class WCap2 {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextLengthW(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out WRECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    public static IntPtr FindMain(uint targetPid) {
        // Largest visible titled window of the process (tray helpers are tiny).
        IntPtr best = IntPtr.Zero;
        long bestArea = 0;
        EnumWindows((h, l) => {
            uint pid; GetWindowThreadProcessId(h, out pid);
            if (pid == targetPid && IsWindowVisible(h) && GetWindowTextLengthW(h) > 0) {
                WRECT r;
                if (GetWindowRect(h, out r)) {
                    long area = (long)(r.R - r.L) * (long)(r.B - r.T);
                    if (area > bestArea) { bestArea = area; best = h; }
                }
            }
            return true;
        }, IntPtr.Zero);
        return best;
    }
}
'@
Add-Type -AssemblyName System.Drawing
[WCap2]::SetProcessDPIAware() | Out-Null

$p = Get-Process tronteq -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { Write-Output "NOPROCESS"; exit 1 }
$h = [WCap2]::FindMain([uint32]$p.Id)
if ($h -eq [IntPtr]::Zero) { Write-Output "NOWINDOW"; exit 1 }
if ([WCap2]::IsIconic($h)) {
    [WCap2]::ShowWindow($h, 9) | Out-Null   # SW_RESTORE
    [WCap2]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 700
}
$r = New-Object WRECT
[WCap2]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.R - $r.L
$ht = $r.B - $r.T
if ($w -le 0 -or $ht -le 0) { Write-Output "BADRECT"; exit 1 }

function Test-NonBlack([System.Drawing.Bitmap]$b) {
    # Sample a sparse grid; any pixel with channel energy = content.
    for ($y = 10; $y -lt $b.Height; $y += [Math]::Max(40, [int]($b.Height / 12))) {
        for ($x = 10; $x -lt $b.Width; $x += [Math]::Max(40, [int]($b.Width / 12))) {
            $c = $b.GetPixel($x, $y)
            if (($c.R + $c.G + $c.B) -gt 24) { return $true }
        }
    }
    return $false
}

$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc()
$ok = [WCap2]::PrintWindow($h, $dc, 2)   # 2 = PW_RENDERFULLCONTENT
$g.ReleaseHdc($dc)
$g.Dispose()

$method = "printwindow"
if (-not $ok -or -not (Test-NonBlack $bmp)) {
    # UIPI or a black swapchain grab: copy the screen region instead.
    $bmp.Dispose()
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object System.Drawing.Size($w, $ht)))
    $g.Dispose()
    $method = "screencopy"
}
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "SAVED $Out via=$method ${w}x${ht}"
