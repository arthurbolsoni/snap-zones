Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class T {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct PT { public int x, y; }
  [StructLayout(LayoutKind.Sequential)] public struct MONINFO { public int cb; public RECT mon; public RECT work; public uint flags; }
  [StructLayout(LayoutKind.Sequential)] public struct MI { public int dx, dy; public uint data, flags, time; public IntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public MI mi; }
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] i, int size);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out PT p);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int hh, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromPoint(PT p, uint f);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr m, ref MONINFO mi);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, IntPtr extra);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, string t);
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

# Monitor principal: .mon e .work (sem a barra de tarefas e sem AppBars como a barra no topo).
function Monitor {
  $mi = New-Object T+MONINFO; $mi.cb = [Runtime.InteropServices.Marshal]::SizeOf($mi)
  [void][T]::GetMonitorInfoW([T]::MonitorFromPoint((New-Object T+PT), 1), [ref]$mi)
  $mi
}

# Abre a janela de teste (form.ps1) e devolve o processo e o HWND; HWND 0 se não abriu.
function OpenForm([switch]$Normal) {
  $a = @("-NoProfile", "-STA", "-File", "$PSScriptRoot/form.ps1")
  if ($Normal) { $a += "-Normal" }
  $p = Start-Process pwsh -ArgumentList $a -WindowStyle Minimized -PassThru
  $h = 0
  for ($k = 0; $k -lt 30 -and $h -eq 0; $k++) {
    Start-Sleep -Milliseconds 300
    $h = [T]::FindWindowW([NullString]::Value, "snaptest-janela")
  }
  $p, $h
}
