# PoC: register TrontEQ APO as the SFX on the Nuvelon ONE endpoint via the
# registry FxProperties (the Equalizer-APO method). Run elevated.
$ErrorActionPreference='Continue'
$log='C:\trontstack\tronteq\fxinstall.log'
"== FxProperties install @ $(Get-Date) ==" | Out-File $log -Encoding utf8
function W($m){ $m | Out-File $log -Append -Encoding utf8 }

$def=@'
using System;
using System.Runtime.InteropServices;
public class Priv {
  [DllImport("advapi32.dll", SetLastError=true)] public static extern bool OpenProcessToken(IntPtr h, uint acc, out IntPtr tok);
  [DllImport("advapi32.dll", SetLastError=true)] public static extern bool LookupPrivilegeValue(string host, string name, out long luid);
  [DllImport("advapi32.dll", SetLastError=true)] public static extern bool AdjustTokenPrivileges(IntPtr tok, bool dis, ref TOKEN_PRIVILEGES n, int len, IntPtr p, IntPtr r);
  [DllImport("kernel32.dll")] public static extern IntPtr GetCurrentProcess();
  [DllImport("kernel32.dll")] public static extern uint GetLastError();
  [StructLayout(LayoutKind.Sequential, Pack=1)] public struct TOKEN_PRIVILEGES { public int Count; public long Luid; public int Attr; }
  public static string Enable(string priv){
    IntPtr tok; if(!OpenProcessToken(GetCurrentProcess(), 0x28, out tok)) return "OpenProcessToken err "+GetLastError();
    long luid; if(!LookupPrivilegeValue(null, priv, out luid)) return "LookupPrivilegeValue err "+GetLastError();
    var tp=new TOKEN_PRIVILEGES(); tp.Count=1; tp.Luid=luid; tp.Attr=0x2;
    bool ok=AdjustTokenPrivileges(tok, false, ref tp, 0, IntPtr.Zero, IntPtr.Zero);
    return priv+": ok="+ok+" gle="+GetLastError();
  }
}
'@
Add-Type $def
W ("priv: " + [Priv]::Enable('SeTakeOwnershipPrivilege'))
W ("priv: " + [Priv]::Enable('SeRestorePrivilege'))
W ("priv: " + [Priv]::Enable('SeBackupPrivilege'))

$admins=New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-544')
$RR=[System.Security.AccessControl.RegistryRights]
$PC=[Microsoft.Win32.RegistryKeyPermissionCheck]
function TakeOwn($rel){
  try {
    $k=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($rel,$PC::ReadWriteSubTree,$RR::TakeOwnership)
    $a=$k.GetAccessControl([System.Security.AccessControl.AccessControlSections]::None); $a.SetOwner($admins); $k.SetAccessControl($a); $k.Close()
    $k=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($rel,$PC::ReadWriteSubTree,$RR::ChangePermissions)
    $a=$k.GetAccessControl()
    $a.AddAccessRule((New-Object System.Security.AccessControl.RegistryAccessRule($admins,'FullControl','ContainerInherit,ObjectInherit','None','Allow')))
    $k.SetAccessControl($a); $k.Close()
    W "  took ownership + FC: $rel"
  } catch { W "  TakeOwn FAILED on ${rel}: $($_.Exception.Message)" }
}

$ep='{76af72a1-a1af-42f8-88ea-7f5023c6e269}'   # Headphones (Nuvelon ONE)
$rel="SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$ep"
if(-not [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($rel)){ W "ENDPOINT KEY MISSING: $rel"; exit 1 }

TakeOwn $rel
TakeOwn "$rel\FxProperties"

$fx="HKLM:\$rel\FxProperties"
if(-not (Test-Path $fx)){ New-Item -Path $fx -Force | Out-Null; W "created FxProperties" }
$sfx='{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},3'
$dis='{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5'
try { New-ItemProperty -Path $fx -Name $sfx -Value '{CA64E60A-A3C4-43B8-970F-0360055172F2}' -PropertyType String -Force -EA Stop | Out-Null; W "wrote SFX clsid OK" }
catch { W "SFX write FAILED: $($_.Exception.Message)" }
try { New-ItemProperty -Path $fx -Name $dis -Value 0 -PropertyType DWord -Force -EA Stop | Out-Null; W "wrote Disable_SysFx=0 OK" }
catch { W "Disable_SysFx write FAILED: $($_.Exception.Message)" }

W "readback:"; (Get-ItemProperty $fx).PSObject.Properties | Where-Object {$_.Name -notlike 'PS*'} | ForEach-Object { W ("  {0} = {1}" -f $_.Name,$_.Value) }

Remove-Item 'C:\ProgramData\TrontEq\apo.log' -ErrorAction SilentlyContinue
Restart-Service audiosrv -Force
Start-Sleep 3
W "audiosrv restarted"
