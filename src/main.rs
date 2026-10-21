//! Encaixe de janelas para monitor ultrawide: metades (com A centrada na divisa) ou, com Alt, terços.
//!
//! Ao arrastar qualquer janela pela barra de título, as zonas aparecem por cima da tela.
//! Entre zonas sobrepostas vence a de centro mais próximo do cursor.
//! Ctrl ao soltar (terços): ocupa as duas colunas vizinhas. Shift: arrasta sem encaixar.
//! Notch no topo: tela cheia.
//! Fica na bandeja do sistema; o menu do ícone pausa ou fecha.

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Column,
    Bridge,
}

struct Zone {
    label: &'static str,
    kind: Kind,
    x0: f64,
    x1: f64,
}

/// Padrão: metades, como o Windows; A (centrada na divisa) só com 1 e 2 ocupadas.
const HALVES: &[Zone] = &[
    Zone { label: "1", kind: Kind::Column, x0: 0.0, x1: 0.5 },
    Zone { label: "2", kind: Kind::Column, x0: 0.5, x1: 1.0 },
    Zone { label: "A", kind: Kind::Bridge, x0: 0.25, x1: 0.75 },
];

/// Com Alt: terços.
const T: f64 = 1.0 / 3.0;
const THIRDS: &[Zone] = &[
    Zone { label: "1", kind: Kind::Column, x0: 0.0, x1: T },
    Zone { label: "2", kind: Kind::Column, x0: T, x1: 2.0 * T },
    Zone { label: "3", kind: Kind::Column, x0: 2.0 * T, x1: 1.0 },
];

fn layout(alt: bool) -> &'static [Zone] {
    if alt { THIRDS } else { HALVES }
}

const TIMER_ID: usize = 1;
const WM_TRAY: u32 = WM_APP + 1;
const MENU_PAUSE: usize = 1;
const MENU_QUIT: usize = 2;
const KEY: COLORREF = COLORREF(0x00FF00FF); // magenta = transparente
const BLUE: COLORREF = rgb(110, 165, 255);
const ORANGE: COLORREF = rgb(255, 175, 70);
const GREEN: COLORREF = rgb(90, 215, 140);

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

#[derive(Default)]
struct Drag {
    hwnd: HWND,
    start: RECT,
    moved: bool,
    resizing: bool,
}

#[derive(Default)]
struct State {
    overlay: HWND,
    tray: HWND,
    icon: HICON,
    paused: bool,
    drag: Option<Drag>,
    work: RECT,
    picked: Vec<usize>,
    alt: bool, // layout de terços
    avail: Vec<bool>,
    full: bool, // cursor no notch do topo: soltar maximiza
}

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn key_down(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
}

fn center(z: &Zone) -> f64 {
    (z.x0 + z.x1) / 2.0
}

/// Zonas escolhidas para a posição horizontal `fx` (0..1 da área de trabalho).
fn pick(fx: f64, ctrl: bool, zones: &[Zone], avail: &[bool]) -> Vec<usize> {
    let dist = |i: &usize| (center(&zones[*i]) - fx).abs();
    let mut cols: Vec<usize> = (0..zones.len()).filter(|&i| zones[i].kind == Kind::Column).collect();
    if ctrl && cols.len() > 2 {
        cols.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
        cols.truncate(2);
        cols.sort();
        return cols;
    }
    (0..zones.len())
        .filter(|&i| avail[i] && fx >= zones[i].x0 && fx <= zones[i].x1)
        .min_by(|a, b| dist(a).total_cmp(&dist(b)))
        .into_iter()
        .collect()
}

fn zone_rect(work: &RECT, x0: f64, x1: f64) -> RECT {
    let w = (work.right - work.left) as f64;
    RECT {
        left: work.left + (x0 * w).round() as i32,
        right: work.left + (x1 * w).round() as i32,
        top: work.top,
        bottom: work.bottom,
    }
}

fn window_rect(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe { _ = GetWindowRect(hwnd, &mut r) };
    r
}

fn eligible(hwnd: HWND, overlay: HWND) -> bool {
    unsafe {
        if hwnd.is_invalid() || hwnd == overlay || !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        if GetAncestor(hwnd, GA_ROOT) != hwnd {
            return false;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        style & WS_THICKFRAME.0 != 0
    }
}

/// Retângulo visível da janela (sem a borda invisível).
fn frame_rect(hwnd: HWND) -> RECT {
    let mut frame = RECT::default();
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut _,
            size_of::<RECT>() as u32,
        )
    };
    if ok.is_ok() { frame } else { window_rect(hwnd) }
}

struct Scan {
    skip: [HWND; 3],
    frames: Vec<RECT>,
}

unsafe extern "system" fn collect_window(hwnd: HWND, lp: LPARAM) -> BOOL {
    let scan = &mut *(lp.0 as *mut Scan);
    if scan.skip.contains(&hwnd)
        || !IsWindowVisible(hwnd).as_bool()
        || IsIconic(hwnd).as_bool()
        || GetAncestor(hwnd, GA_ROOT) != hwnd
    {
        return TRUE;
    }
    // janela de outra área de trabalho virtual fica "cloaked"
    let mut cloaked = 0u32;
    _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
    if cloaked == 0 {
        scan.frames.push(frame_rect(hwnd));
    }
    TRUE
}

/// Quais zonas podem receber janela agora, conforme as que já estão ocupadas no monitor.
/// Colunas sempre; zona de divisa só quando todas as colunas sob ela estão ocupadas.
fn available(work: &RECT, skip: [HWND; 3], zones: &[Zone]) -> Vec<bool> {
    let mut scan = Scan { skip, frames: Vec::new() };
    unsafe { _ = EnumWindows(Some(collect_window), LPARAM(&mut scan as *mut _ as isize)) };

    const TOL: i32 = 10;
    let close = |a: &RECT, b: &RECT| {
        (a.left - b.left).abs() <= TOL
            && (a.right - b.right).abs() <= TOL
            && (a.top - b.top).abs() <= TOL
            && (a.bottom - b.bottom).abs() <= TOL
    };
    let cols: Vec<usize> = (0..zones.len()).filter(|&i| zones[i].kind == Kind::Column).collect();
    let mut used = vec![false; zones.len()];
    for f in &scan.frames {
        for (i, z) in zones.iter().enumerate() {
            if close(f, &zone_rect(work, z.x0, z.x1)) {
                used[i] = true;
            }
        }
        // janela ocupando duas colunas vizinhas (Ctrl) preenche as duas
        for pair in cols.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if close(f, &zone_rect(work, zones[a].x0, zones[b].x1)) {
                used[a] = true;
                used[b] = true;
            }
        }
    }
    zones
        .iter()
        .map(|z| match z.kind {
            Kind::Column => true,
            Kind::Bridge => cols
                .iter()
                .filter(|&&c| zones[c].x0 < z.x1 && z.x0 < zones[c].x1)
                .all(|&c| used[c]),
        })
        .collect()
}

/// Notch no alto da tela, centralizado: soltar a janela nele maximiza.
fn notch_rect(work: &RECT) -> RECT {
    let (w, h) = (work.right - work.left, work.bottom - work.top);
    let (nw, nh) = (w / 5, (h / 12).max(56));
    let left = work.left + (w - nw) / 2;
    RECT { left, top: work.top, right: left + nw, bottom: work.top + nh }
}

fn monitor_work(pt: POINT) -> RECT {
    unsafe {
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcWork
    }
}

/// Move a janela para `target`, compensando a borda invisível que o Windows 10/11 põe em volta.
fn place(hwnd: HWND, target: RECT) {
    unsafe {
        if IsZoomed(hwnd).as_bool() {
            _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let outer = window_rect(hwnd);
        let mut frame = RECT::default();
        let (mut l, mut t, mut r, mut b) = (0, 0, 0, 0);
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut _,
            size_of::<RECT>() as u32,
        )
        .is_ok()
        {
            l = frame.left - outer.left;
            t = frame.top - outer.top;
            r = outer.right - frame.right;
            b = outer.bottom - frame.bottom;
        }
        _ = SetWindowPos(
            hwnd,
            None,
            target.left - l,
            target.top - t,
            target.right - target.left + l + r,
            target.bottom - target.top + t + b,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn show_overlay(st: &State, show: bool) {
    unsafe {
        if show {
            let w = st.work;
            _ = SetWindowPos(
                st.overlay,
                Some(HWND_TOPMOST),
                w.left,
                w.top,
                w.right - w.left,
                w.bottom - w.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            _ = InvalidateRect(Some(st.overlay), None, true);
        } else {
            _ = ShowWindow(st.overlay, SW_HIDE);
        }
    }
}

/// Chamado a cada ~15 ms durante o arraste.
fn tick() {
    STATE.with_borrow_mut(|st| {
        let Some(drag) = st.drag.as_mut() else { return };
        if drag.resizing {
            return;
        }
        let now = window_rect(drag.hwnd);
        let (sw, sh) = (drag.start.right - drag.start.left, drag.start.bottom - drag.start.top);
        if !drag.moved {
            if now == drag.start {
                return; // só clicou na barra
            }
            if now.right - now.left != sw || now.bottom - now.top != sh {
                drag.resizing = true; // redimensionando pela borda, não é arraste
                return;
            }
            drag.moved = true;
        }

        let mut pt = POINT::default();
        unsafe { _ = GetCursorPos(&mut pt) };
        let work = monitor_work(pt);
        let skip = key_down(VK_SHIFT.0 as i32);
        let fx = ((pt.x - work.left) as f64 / (work.right - work.left) as f64).clamp(0.0, 1.0);
        let alt = key_down(VK_MENU.0 as i32);
        let relayout = work != st.work || alt != st.alt;
        if relayout {
            st.avail = available(&work, [drag.hwnd, st.overlay, st.tray], layout(alt));
        }
        let full = !skip && unsafe { PtInRect(&notch_rect(&work), pt).as_bool() };
        let picked = if skip || full {
            vec![]
        } else {
            pick(fx, key_down(VK_CONTROL.0 as i32), layout(alt), &st.avail)
        };
        st.work = work;
        st.alt = alt;
        if skip {
            show_overlay(st, false);
        } else if relayout
            || picked != st.picked
            || full != st.full
            || unsafe { !IsWindowVisible(st.overlay).as_bool() }
        {
            st.picked = picked;
            st.full = full;
            show_overlay(st, true);
            return;
        }
        st.picked = picked;
        st.full = full;
    });
}

unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != OBJID_WINDOW.0 {
        return;
    }
    match event {
        EVENT_SYSTEM_MOVESIZESTART => STATE.with_borrow_mut(|st| {
            if st.paused || !eligible(hwnd, st.overlay) {
                return;
            }
            st.drag = Some(Drag { hwnd, start: window_rect(hwnd), ..Default::default() });
            st.picked.clear();
            st.full = false;
            st.alt = false;
            st.work = RECT::default();
            unsafe { SetTimer(Some(st.overlay), TIMER_ID, 15, None) };
        }),
        EVENT_SYSTEM_MOVESIZEEND => {
            let snap = STATE.with_borrow_mut(|st| {
                let drag = st.drag.take()?;
                unsafe { _ = KillTimer(Some(st.overlay), TIMER_ID) };
                show_overlay(st, false);
                let picked = std::mem::take(&mut st.picked);
                let full = std::mem::take(&mut st.full);
                if drag.hwnd != hwnd || !drag.moved || drag.resizing {
                    return None;
                }
                if full {
                    return Some(None);
                }
                if picked.is_empty() {
                    return None;
                }
                let zones = layout(st.alt);
                let x0 = picked.iter().map(|&i| zones[i].x0).fold(f64::MAX, f64::min);
                let x1 = picked.iter().map(|&i| zones[i].x1).fold(f64::MIN, f64::max);
                Some(Some(zone_rect(&st.work, x0, x1)))
            });
            match snap {
                Some(Some(target)) => place(hwnd, target),
                Some(None) => unsafe { _ = ShowWindow(hwnd, SW_MAXIMIZE) },
                None => {}
            }
        }
        _ => {}
    }
}

fn paint(hwnd: HWND) {
    STATE.with_borrow(|st| unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut client = RECT::default();
        _ = GetClientRect(hwnd, &mut client);
        let origin = RECT { left: 0, top: 0, right: client.right, bottom: client.bottom };

        let key = CreateSolidBrush(KEY);
        FillRect(hdc, &origin, key);
        _ = DeleteObject(key.into());

        let font = CreateFontW(
            client.bottom / 6, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            NONANTIALIASED_QUALITY, (FF_SWISS.0 | VARIABLE_PITCH.0) as u32, w!("Segoe UI"),
        );
        let old_font = SelectObject(hdc, font.into());
        SetBkMode(hdc, TRANSPARENT);
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));

        // Colunas primeiro, zonas de borda por cima, a ativa por último.
        let zones = layout(st.alt);
        let mut order: Vec<usize> =
            (0..zones.len()).filter(|&i| st.avail.get(i) == Some(&true) && !st.picked.contains(&i)).collect();
        order.extend(st.picked.iter().copied());

        for i in order {
            let z = &zones[i];
            let active = st.picked.contains(&i);
            let color = if z.kind == Kind::Column { BLUE } else { ORANGE };
            let mut r = zone_rect(&origin, z.x0, z.x1);
            // zonas de borda um pouco menores na vertical para se distinguirem das colunas
            let inset = if z.kind == Kind::Bridge { client.bottom / 30 } else { 4 };
            r.left += 4;
            r.right -= 4;
            r.top += inset;
            r.bottom -= inset;

            if active {
                let fill = CreateSolidBrush(color);
                FillRect(hdc, &r, fill);
                _ = DeleteObject(fill.into());
            }
            let pen = CreatePen(PS_SOLID, if active { 6 } else { 3 }, color);
            let old_pen = SelectObject(hdc, pen.into());
            _ = Rectangle(hdc, r.left, r.top, r.right, r.bottom);
            SelectObject(hdc, old_pen);
            _ = DeleteObject(pen.into());

            SetTextColor(hdc, if active { rgb(255, 255, 255) } else { color });
            let mut text: Vec<u16> = z.label.encode_utf16().collect();
            let mut tr = r;
            if z.kind == Kind::Bridge {
                tr.bottom = r.top + client.bottom / 4; // rótulo da borda no alto, para não cobrir o da coluna
            }
            DrawTextW(hdc, &mut text, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }

        // notch do topo, por cima de tudo
        let n = notch_rect(&origin);
        let radius = (n.bottom - n.top) / 2;
        let fill = CreateSolidBrush(if st.full { GREEN } else { rgb(28, 30, 36) });
        let pen = CreatePen(PS_SOLID, 3, GREEN);
        let prev_brush = SelectObject(hdc, fill.into());
        let prev_pen = SelectObject(hdc, pen.into());
        // cantos de baixo arredondados: estende para cima para esconder os de cima
        _ = RoundRect(hdc, n.left, n.top - radius, n.right, n.bottom, radius, radius);
        SelectObject(hdc, prev_brush);
        SelectObject(hdc, prev_pen);
        _ = DeleteObject(fill.into());
        _ = DeleteObject(pen.into());

        let small = CreateFontW(
            (n.bottom - n.top) * 2 / 5, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            NONANTIALIASED_QUALITY, (FF_SWISS.0 | VARIABLE_PITCH.0) as u32, w!("Segoe UI"),
        );
        SelectObject(hdc, small.into());
        SetTextColor(hdc, if st.full { rgb(20, 20, 20) } else { GREEN });
        let mut text: Vec<u16> = "Tela cheia".encode_utf16().collect();
        let mut tr = n;
        DrawTextW(hdc, &mut text, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_font);
        _ = DeleteObject(font.into());
        _ = DeleteObject(small.into());
        _ = EndPaint(hwnd, &ps);
    });
}

/// Ícone desenhado em memória: três colunas azuis e uma faixa laranja sobre a primeira divisa.
fn make_icon() -> windows::core::Result<HICON> {
    unsafe {
        let s = GetSystemMetrics(SM_CXSMICON).max(16);
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: s,
                biHeight: -s, // de cima para baixo
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (s * s) as usize);
        let (fs, third) = (s as f64, s as f64 / 3.0);
        for y in 0..s {
            for x in 0..s {
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                let in_gap = (fx - third).abs() < 0.8 || (fx - 2.0 * third).abs() < 0.8;
                let in_band = fy > fs * 0.36 && fy < fs * 0.64 && fx > third / 2.0 && fx < third * 1.5;
                let edge = y < 1 || y >= s - 1 || x < 1 || x >= s - 1;
                px[(y * s + x) as usize] = if in_band {
                    0xFFFF_AF46 // laranja
                } else if in_gap || edge {
                    0x0000_0000
                } else {
                    0xFF6E_A5FF // azul
                };
            }
        }
        let mask_bits = vec![0u8; (s * s / 8) as usize + 2 * s as usize];
        let mask = CreateBitmap(s, s, 1, 1, Some(mask_bits.as_ptr() as *const _));
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: TRUE,
            hbmMask: mask,
            hbmColor: color,
            ..Default::default()
        });
        _ = DeleteObject(color.into());
        _ = DeleteObject(mask.into());
        icon
    }
}

fn tray_data(st: &State) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: st.tray,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: st.icon,
        ..Default::default()
    };
    let tip = if st.paused { "snap-zones (pausado)" } else { "snap-zones" };
    for (dst, src) in nid.szTip.iter_mut().zip(tip.encode_utf16()) {
        *dst = src;
    }
    nid
}

fn tray_menu(hwnd: HWND) {
    let paused = STATE.with_borrow(|st| st.paused);
    let cmd = unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let pause_flags = if paused { MF_STRING | MF_CHECKED } else { MF_STRING };
        _ = AppendMenuW(menu, pause_flags, MENU_PAUSE, w!("Pausar"));
        _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Sair"));
        let mut pt = POINT::default();
        _ = GetCursorPos(&mut pt);
        _ = SetForegroundWindow(hwnd); // sem isso o menu não fecha ao clicar fora
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        _ = DestroyMenu(menu);
        cmd.0 as usize
    };
    match cmd {
        MENU_PAUSE => STATE.with_borrow_mut(|st| {
            st.paused = !st.paused;
            unsafe { _ = Shell_NotifyIconW(NIM_MODIFY, &tray_data(st)) };
        }),
        MENU_QUIT => STATE.with_borrow(|st| unsafe {
            _ = Shell_NotifyIconW(NIM_DELETE, &tray_data(st));
            PostQuitMessage(0);
        }),
        _ => {}
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Relaxed) {
        // Explorer reiniciou: o ícone some e precisa ser adicionado de novo
        STATE.with_borrow(|st| _ = Shell_NotifyIconW(NIM_ADD, &tray_data(st)));
        return LRESULT(0);
    }
    match msg {
        WM_TRAY => {
            let event = (lp.0 & 0xFFFF) as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                tray_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_TIMER if wp.0 == TIMER_ID => {
            tick();
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() -> windows::core::Result<()> {
    unsafe {
        // uma instância só
        let _mutex = CreateMutexW(None, true, w!("Local\\snap-zones"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
        _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(PCWSTR::null())?;
        let class = w!("SnapZonesOverlay");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let overlay = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("snap-zones"),
            WS_POPUP,
            0, 0, 0, 0,
            None, None, Some(instance.into()), None,
        )?;
        SetLayeredWindowAttributes(overlay, KEY, 150, LWA_COLORKEY | LWA_ALPHA)?;
        // janela oculta que recebe os eventos do ícone da bandeja
        let tray = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("snap-zones-tray"),
            WS_OVERLAPPED,
            0, 0, 0, 0,
            None, None, Some(instance.into()), None,
        )?;
        let icon = make_icon()?;
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        STATE.with_borrow_mut(|st| {
            st.overlay = overlay;
            st.tray = tray;
            st.icon = icon;
            _ = Shell_NotifyIconW(NIM_ADD, &tray_data(st));
        });

        let hook = SetWinEventHook(
            EVENT_SYSTEM_MOVESIZESTART,
            EVENT_SYSTEM_MOVESIZEEND,
            None,
            Some(win_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if hook.is_invalid() {
            return Err(windows::core::Error::from_win32());
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
