param([int]$GrabX = 1500, [int]$GrabY = 12)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class T {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct MI { public int dx, dy; public uint data, flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public MI mi; }
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] i, int size);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int hh, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  public static void Btn(uint f) {
    var i = new INPUT[1]; i[0].type = 0; i[0].mi.flags = f;
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }
  // movimento relativo em passos pequenos, como mão de verdade
  public static void Move(int dx, int dy) {
    var i = new INPUT[1]; i[0].type = 0; i[0].mi.flags = 1; i[0].mi.dx = dx; i[0].mi.dy = dy;
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }
}
"@

function State($h, $tag) {
  $r = New-Object T+RECT; [void][T]::GetWindowRect($h, [ref]$r)
  "{0,-14} zoomed={1} rect=({2},{3})-({4},{5}) size={6}x{7}" -f $tag, [T]::IsZoomed($h), $r.L, $r.T, $r.R, $r.B, ($r.R - $r.L), ($r.B - $r.T)
}

$p = Start-Process pwsh -ArgumentList "-NoProfile","-STA","-File","$PSScriptRoot/form.ps1" -WindowStyle Hidden -PassThru
$h = 0
for ($k = 0; $k -lt 30 -and $h -eq 0; $k++) {
  Start-Sleep -Milliseconds 300
  $h = (Get-Process -Id $p.Id -ErrorAction SilentlyContinue).MainWindowHandle
}
if (-not $h -or $h -eq 0) { "janela de teste não encontrada; abortando sem mexer no mouse"; Stop-Process -Id $p.Id -ErrorAction SilentlyContinue; exit 1 }
Start-Sleep -Milliseconds 600
State $h "maximizada"

[void][T]::SetCursorPos($GrabX, $GrabY)
Start-Sleep -Milliseconds 200
[T]::Btn(2)  # LEFTDOWN
Start-Sleep -Milliseconds 150
for ($k = 0; $k -lt 6; $k++) { [T]::Move(14, 18); Start-Sleep -Milliseconds 8 }
Start-Sleep -Milliseconds 400
State $h "apos 1o arraste"
for ($k = 0; $k -lt 30; $k++) { [T]::Move(-6, 8); Start-Sleep -Milliseconds 15 }
Start-Sleep -Milliseconds 300
State $h "apos 2o arraste"
[T]::Btn(4)  # LEFTUP
Start-Sleep -Milliseconds 400
State $h "soltou"
for ($k = 0; $k -lt 10; $k++) { [T]::Move(20, 0); Start-Sleep -Milliseconds 10 }
Start-Sleep -Milliseconds 300
State $h "mexeu depois"
Stop-Process -Id $p.Id -ErrorAction SilentlyContinue
