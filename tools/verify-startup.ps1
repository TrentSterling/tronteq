# Exercise the real Windows tray and task. Leaves TrontEQ running in the tray.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class TrontEqTrayProbe {
    public static readonly Guid IconGuid = new Guid("63b19802-492e-4f5f-9cc1-6ba48e46271d");
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] struct Identifier {
        public uint Size; public IntPtr Window; public uint Id; public Guid Guid;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct IconData {
        public uint Size; public IntPtr Window; public uint Id, Flags, Callback;
        public IntPtr Icon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string Tip;
        public uint State, StateMask;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=256)] public string Info;
        public uint Timeout;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=64)] public string InfoTitle;
        public uint InfoFlags; public Guid Guid; public IntPtr Balloon;
    }
    delegate bool EnumProc(IntPtr window, IntPtr param);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr param);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int max);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int max);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern uint RegisterWindowMessage(string name);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("shell32.dll")] static extern int Shell_NotifyIconGetRect(ref Identifier id, out Rect rect);
    [DllImport("shell32.dll", CharSet=CharSet.Unicode)] static extern bool Shell_NotifyIcon(uint message, ref IconData data);
    public static IntPtr Window(uint process, bool tray) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, _) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process) return true;
            var text = new StringBuilder(256);
            if (tray) GetClassName(window, text, 256); else GetWindowText(window, text, 256);
            if (text.ToString() == (tray ? "tray_icon_app" : "TrontEQ")) { found = window; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static Rect IconRect() {
        var id = new Identifier { Size = (uint)Marshal.SizeOf(typeof(Identifier)), Guid = IconGuid };
        Rect rect;
        int result = Shell_NotifyIconGetRect(ref id, out rect);
        if (result != 0) Marshal.ThrowExceptionForHR(result);
        return rect;
    }
    public static bool RemoveIcon() {
        var data = new IconData { Size = (uint)Marshal.SizeOf(typeof(IconData)), Flags=0x20, Guid=IconGuid };
        return Shell_NotifyIcon(2, ref data);
    }
    public static void RecreateIcon(uint process) {
        IntPtr window = Window(process, true);
        if (window == IntPtr.Zero) throw new Exception("Tray message window missing");
        PostMessage(window, RegisterWindowMessage("TaskbarCreated"), IntPtr.Zero, IntPtr.Zero);
    }
}
'@
function Assert-True($condition, $message) { if (!$condition) { throw $message } }
function Read-SoundState {
    $stream = [IO.File]::Open('C:/ProgramData/TrontEq/state.bin', 'Open', 'Read', 'ReadWrite')
    try {
        $bytes = New-Object byte[] 224
        Assert-True ($stream.Read($bytes, 0, 224) -eq 224) 'Short audio-state read'
        return ,$bytes
    } finally { $stream.Dispose() }
}
$task = Get-ScheduledTask -TaskName TrontEQ
Assert-True ($task.State -eq 'Running') 'Logon task is not running'
Assert-True ($task.Actions[0].Arguments -eq '--startup') 'Task must start in the tray'
Assert-True ($task.Settings.ExecutionTimeLimit -eq 'PT0S') 'Task has a runtime limit'
Assert-True (!$task.Settings.DisallowStartIfOnBatteries -and !$task.Settings.StopIfGoingOnBatteries) 'Task battery restrictions remain'
Assert-True ($task.Principal.LogonType -eq 'Interactive' -and $task.Principal.RunLevel -eq 'Limited') 'Task must run as the interactive normal user'
$processes = @(Get-Process -Name tronteq)
Assert-True ($processes.Count -eq 1) 'Expected exactly one TrontEQ process'
$app = $processes[0]
$window = [TrontEqTrayProbe]::Window($app.Id, $false)
Assert-True ($window -ne [IntPtr]::Zero) 'TrontEQ main window missing'
Assert-True ([TrontEqTrayProbe]::IsIconic($window)) 'Startup did not park in the tray'
$before = Read-SoundState

# Promote only TrontEQ, then ask its own tray window to re-register with Explorer.
$keys = @(Get-ChildItem 'HKCU:/Control Panel/NotifyIconSettings' | ForEach-Object {
    $item = Get-ItemProperty $_.PSPath
    if ($item.ExecutablePath -like '*\TrontEQ\tronteq.exe' -or $item.InitialTooltip -eq 'TrontEQ') { $item }
})
Assert-True ($keys.Count -gt 0) 'Explorer has no TrontEQ notification entry'
foreach ($key in $keys) {
    New-ItemProperty -LiteralPath $key.PSPath -Name IsPromoted -Value 1 -PropertyType DWord -Force | Out-Null
}
[TrontEqTrayProbe]::RecreateIcon($app.Id)
Start-Sleep -Seconds 1
$rect = [TrontEqTrayProbe]::IconRect()

$duplicate = Start-Process -FilePath $app.Path -ArgumentList '--startup' -WindowStyle Hidden -PassThru
Assert-True ($duplicate.WaitForExit(5000)) 'Duplicate startup did not exit'
Assert-True ($duplicate.ExitCode -eq 0) 'Duplicate startup failed'
Assert-True ([TrontEqTrayProbe]::IsIconic($window)) 'Duplicate startup opened the UI'
Assert-True (@(Get-Process -Name tronteq).Count -eq 1) 'Duplicate startup left another process'

# Simulate icon loss and Explorer's recovery message without restarting Explorer.
Assert-True ([TrontEqTrayProbe]::RemoveIcon()) 'Could not remove test icon'
$absent = $false
try { [TrontEqTrayProbe]::IconRect() | Out-Null } catch { $absent = $true }
Assert-True $absent 'Icon did not disappear for recovery test'
[TrontEqTrayProbe]::RecreateIcon($app.Id)
Start-Sleep -Seconds 1
$rect = [TrontEqTrayProbe]::IconRect()

$samples = @()
for ($cycle = 1; $cycle -le 2; $cycle++) {
    $show = Start-Process -FilePath $app.Path -WindowStyle Hidden -PassThru
    Assert-True ($show.WaitForExit(5000)) 'Second launch did not hand off'
    Start-Sleep -Seconds 2
    Assert-True (![TrontEqTrayProbe]::IsIconic($window)) 'Second launch did not restore the window'
    if ($cycle -eq 1) {
        Set-Content -LiteralPath 'C:/ProgramData/TrontEq/shot.req' -Value capture
        $deadline = [DateTime]::UtcNow.AddSeconds(8)
        while ((Test-Path 'C:/ProgramData/TrontEq/shot.req') -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        Assert-True (!(Test-Path 'C:/ProgramData/TrontEq/shot.req')) 'Restored GUI did not render a screenshot'
        Copy-Item 'C:/ProgramData/TrontEq/shot.png' "$root/target/startup-restored.png"
    }
    Assert-True ([TrontEqTrayProbe]::PostMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)) 'Close request failed'
    Start-Sleep -Seconds 3
    Assert-True ([TrontEqTrayProbe]::IsIconic($window)) 'Close did not hide to the tray'
    $rect = [TrontEqTrayProbe]::IconRect()
    $cursor = New-Object TrontEqTrayProbe+Point
    [TrontEqTrayProbe]::GetCursorPos([ref]$cursor) | Out-Null
    try {
        for ($hover = 0; $hover -lt 5; $hover++) {
            [TrontEqTrayProbe]::SetCursorPos(($rect.Left + $rect.Right)/2, ($rect.Top + $rect.Bottom)/2) | Out-Null
            Start-Sleep -Milliseconds 120
            [TrontEqTrayProbe]::SetCursorPos($rect.Left - 12, $rect.Top - 12) | Out-Null
            Start-Sleep -Milliseconds 120
        }
    } finally { [TrontEqTrayProbe]::SetCursorPos($cursor.X, $cursor.Y) | Out-Null }
    $app.Refresh()
    $cpuStart = $app.TotalProcessorTime.TotalMilliseconds
    $timer = [Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds 10
    $app.Refresh()
    $elapsed = $timer.Elapsed.TotalMilliseconds
    $cpuMs = $app.TotalProcessorTime.TotalMilliseconds - $cpuStart
    $corePercent = 100 * $cpuMs / $elapsed
    $samples += [ordered]@{cycle=$cycle; wallMs=[Math]::Round($elapsed); cpuMs=$cpuMs; oneCorePercent=[Math]::Round($corePercent,3)}
    Assert-True ($corePercent -lt 5) 'Tray process is consuming excessive CPU'
}
$after = Read-SoundState
$unchanged = [Convert]::ToBase64String($before[8..199]) -eq [Convert]::ToBase64String($after[8..199])
Assert-True $unchanged 'Audio control settings changed'
$result = [ordered]@{
    result='PASS'; pid=$app.Id; executable=$app.Path; taskRunning=$true; startupInTray=$true;
    duplicateStartupQuiet=$true; shellIconRecovery=$true; restoreCloseCycles=2;
    audioControlBytesUnchanged=$unchanged; bypass=$after[140]; cpuSamples=$samples;
    iconRect=$rect; promotedEntries=$keys.Count
}
$result | ConvertTo-Json -Depth 5 | Tee-Object -FilePath "$root/target/startup-verification.json"
