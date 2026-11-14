param([string]$Swf, [string]$OutPng, [int]$WaitSec = 5)
Add-Type -AssemblyName System.Drawing
$exe = "C:\Users\admin\Downloads\flashplayer_32_sa_debug.exe"
$p = Start-Process $exe -ArgumentList "`"$Swf`"" -PassThru
Start-Sleep -Seconds $WaitSec
if ($p.HasExited) { Write-Output "EXITED code=$($p.ExitCode)"; exit 2 }
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class FP32 {
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
$hwnd = $p.MainWindowHandle
if ($hwnd -eq 0) { Start-Sleep -Seconds 2; $hwnd = $p.MainWindowHandle }
if ($hwnd -eq 0) {
  # find window by process id
  $script:found=[IntPtr]::Zero
  $cb = [FP32+EnumWindowsProc]{ param($h,$l)
    $procId=0; [void][Runtime.InteropServices.Marshal]::GetLastWin32Error()
    Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);' -Name U32 -Namespace W -ErrorAction SilentlyContinue
    [W.U32]::GetWindowThreadProcessId($h,[ref]$procId) | Out-Null
    if ($procId -eq $p.Id) { $script:found=$h; return $false }
    return $true }
  [FP32]::EnumWindows($cb,[IntPtr]::Zero) | Out-Null
  $hwnd=$script:found
}
if ($hwnd -eq 0) { Write-Output "NO_WINDOW"; Stop-Process -Id $p.Id -Force; exit 3 }
[FP32]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 500
$r = New-Object FP32+RECT
[FP32]::GetWindowRect($hwnd, [ref]$r) | Out-Null
$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [FP32]::PrintWindow($hwnd, $hdc, 2)
$g.ReleaseHdc($hdc); $g.Dispose()
$bmp.Save($OutPng, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Stop-Process -Id $p.Id -Force
Write-Output "CAPTURED $OutPng printwindow=$ok size=${w}x${h}"
