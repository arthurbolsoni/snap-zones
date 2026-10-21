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
