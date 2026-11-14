param([string]$Swf, [string]$OutPng, [int]$WaitSec = 4)
Add-Type -AssemblyName System.Drawing
$exe = "C:\Program Files\Ruffle\bin\ruffle.exe"
$p = Start-Process $exe -ArgumentList "`"$Swf`"" -PassThru
Start-Sleep -Seconds $WaitSec
if ($p.HasExited) { Write-Output "EXITED code=$($p.ExitCode)"; exit 2 }
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
$hwnd = $p.MainWindowHandle
if ($hwnd -eq 0) { Start-Sleep -Seconds 2; $hwnd = $p.MainWindowHandle }
if ($hwnd -eq 0) { Write-Output "NO_WINDOW"; Stop-Process -Id $p.Id -Force; exit 3 }
[Win32]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 500
$r = New-Object Win32+RECT
[Win32]::GetWindowRect($hwnd, [ref]$r) | Out-Null
$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [Win32]::PrintWindow($hwnd, $hdc, 2)
$g.ReleaseHdc($hdc); $g.Dispose()
$bmp.Save($OutPng, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Stop-Process -Id $p.Id -Force
Write-Output "CAPTURED $OutPng printwindow=$ok size=${w}x${h}"
