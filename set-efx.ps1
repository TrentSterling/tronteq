# Point an endpoint's EFX (endpoint-effect) slot at the TrontEQ APO. Run elevated.
# Usage: set-efx.ps1 [-ep '{endpoint-guid}']   (defaults to Smokin' Buds)
param([string]$ep='{15399fc7-8fee-4569-8f7c-e54ba74a4065}')
$ErrorActionPreference='Continue'
$log='C:\trontstack\tronteq\set-efx.log'
"== set-efx $ep @ $(Get-Date) ==" | Out-File $log -Encoding utf8
function W($m){ $m | Out-File $log -Append -Encoding utf8 }

$def=@'
using System; using System.Runtime.InteropServices;
public class P {
 [DllImport("advapi32.dll",SetLastError=true)] public static extern bool OpenProcessToken(IntPtr h,uint a,out IntPtr t);
 [DllImport("advapi32.dll",SetLastError=true)] public static extern bool LookupPrivilegeValue(string s,string n,out long l);
 [DllImport("advapi32.dll",SetLastError=true)] public static extern bool AdjustTokenPrivileges(IntPtr t,bool d,ref TP n,int l,IntPtr p,IntPtr r);
 [DllImport("kernel32.dll")] public static extern IntPtr GetCurrentProcess();
 [StructLayout(LayoutKind.Sequential,Pack=1)] public struct TP { public int C; public long L; public int A; }
 public static void E(string p){ IntPtr t; OpenProcessToken(GetCurrentProcess(),0x28,out t); long l; LookupPrivilegeValue(null,p,out l); var tp=new TP(); tp.C=1; tp.L=l; tp.A=2; AdjustTokenPrivileges(t,false,ref tp,0,IntPtr.Zero,IntPtr.Zero); }
}
'@
Add-Type $def
[P]::E('SeTakeOwnershipPrivilege'); [P]::E('SeRestorePrivilege')

$rel="SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\$ep\FxProperties"
$admins=New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-544')
$RR=[System.Security.AccessControl.RegistryRights]; $PC=[Microsoft.Win32.RegistryKeyPermissionCheck]
if(-not [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($rel)){ W "FxProperties MISSING for $ep"; exit 1 }
try {
  $k=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($rel,$PC::ReadWriteSubTree,$RR::TakeOwnership)
  $a=$k.GetAccessControl([System.Security.AccessControl.AccessControlSections]::None); $a.SetOwner($admins); $k.SetAccessControl($a); $k.Close()
  $k=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($rel,$PC::ReadWriteSubTree,$RR::ChangePermissions)
  $a=$k.GetAccessControl(); $a.AddAccessRule((New-Object System.Security.AccessControl.RegistryAccessRule($admins,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))); $k.SetAccessControl($a); $k.Close()
} catch { W "ownership: $($_.Exception.Message)" }

$fx="HKLM:\$rel"
$our='{CA64E60A-A3C4-43B8-970F-0360055172F2}'
try {
  New-ItemProperty $fx -Name '{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6' -Value $our -PropertyType String -Force -EA Stop | Out-Null
  Remove-ItemProperty $fx -Name '{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5' -EA SilentlyContinue
  W "EFX(,6) set to TrontEQ on $ep"
} catch { W "write FAILED: $($_.Exception.Message)" }
W "readback ,6 = $((Get-ItemProperty $fx -Name '{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6' -EA SilentlyContinue).'{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6')"

Restart-Service audiosrv -Force
Start-Sleep 3
(New-Object System.Media.SoundPlayer "$env:WINDIR\Media\chord.wav").PlaySync()
Start-Sleep 1
$p='C:\ProgramData\TrontEq\TrontEqApo.dll'
try { $fs=[IO.File]::Open($p,'Open','ReadWrite','None'); $fs.Close(); W "DLL LOCK: FREE -> not loaded (toggle the device)" }
catch [System.IO.IOException] { W "DLL LOCK: LOCKED -> APO loaded on this endpoint!" }
catch { W "DLL LOCK: $($_.Exception.GetType().Name)" }
