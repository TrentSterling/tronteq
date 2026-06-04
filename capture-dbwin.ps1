# Capture session-0 OutputDebugString (DBWIN) with a NULL DACL so audiodg's
# restricted token can write to the shared buffer. Run elevated.
$ErrorActionPreference='Continue'
$src=@'
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
public class DbWin {
  [StructLayout(LayoutKind.Sequential)] struct SA { public int nLength; public IntPtr sd; public int inherit; }
  [DllImport("advapi32.dll",SetLastError=true)] static extern bool InitializeSecurityDescriptor(byte[] sd, uint rev);
  [DllImport("advapi32.dll",SetLastError=true)] static extern bool SetSecurityDescriptorDacl(byte[] sd, bool present, IntPtr dacl, bool def);
  [DllImport("kernel32.dll",SetLastError=true,CharSet=CharSet.Unicode)] static extern IntPtr CreateFileMapping(IntPtr h, ref SA sa, uint prot, uint hi, uint lo, string name);
  [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr MapViewOfFile(IntPtr h, uint acc, uint hi, uint lo, UIntPtr size);
  [DllImport("kernel32.dll",SetLastError=true,CharSet=CharSet.Unicode)] static extern IntPtr CreateEvent(ref SA sa, bool manual, bool init, string name);
  [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr h, uint ms);
  [DllImport("kernel32.dll")] static extern bool SetEvent(IntPtr h);
  public static List<string> Capture(int seconds){
    var o=new List<string>();
    byte[] sd=new byte[20];
    InitializeSecurityDescriptor(sd,1);
    SetSecurityDescriptorDacl(sd,true,IntPtr.Zero,false); // NULL DACL -> everyone
    var gc=GCHandle.Alloc(sd,GCHandleType.Pinned);
    var sa=new SA(); sa.nLength=Marshal.SizeOf(typeof(SA)); sa.sd=gc.AddrOfPinnedObject(); sa.inherit=0;
    IntPtr bufReady=CreateEvent(ref sa,false,false,"Global\\DBWIN_BUFFER_READY");
    IntPtr dataReady=CreateEvent(ref sa,false,false,"Global\\DBWIN_DATA_READY");
    IntPtr map=CreateFileMapping(new IntPtr(-1),ref sa,0x04,0,4096,"Global\\DBWIN_BUFFER");
    IntPtr view=MapViewOfFile(map,0x04,0,0,UIntPtr.Zero);
    if(view==IntPtr.Zero){ o.Add("ERR MapViewOfFile gle="+Marshal.GetLastWin32Error()); gc.Free(); return o; }
    var end=DateTime.UtcNow.AddSeconds(seconds);
    SetEvent(bufReady);
    while(DateTime.UtcNow<end){
      if(WaitForSingleObject(dataReady,400)==0){
        int pid=Marshal.ReadInt32(view);
        string s=Marshal.PtrToStringAnsi(new IntPtr(view.ToInt64()+4));
        if(s!=null) o.Add(pid+": "+s.TrimEnd());
        SetEvent(bufReady);
      }
    }
    gc.Free(); return o;
  }
}
'@
Add-Type $src

# Force a fresh APO load mid-capture: at +1s restart audio, at +2s play a tone.
Start-Job { Start-Sleep 1; Restart-Service audiosrv -Force; Start-Sleep 1; (New-Object System.Media.SoundPlayer 'C:\Windows\Media\chord.wav').PlaySync() } | Out-Null

$logs=[DbWin]::Capture(10)
$logs | Out-File 'C:\trontstack\tronteq\dbwin.log' -Encoding utf8
"captured $($logs.Count) lines; TrontEqApo lines:" | Out-File 'C:\trontstack\tronteq\dbwin.log' -Append -Encoding utf8
($logs | Where-Object { $_ -match 'TrontEqApo' }) | Out-File 'C:\trontstack\tronteq\dbwin.log' -Append -Encoding utf8
