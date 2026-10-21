//! Encaixe de janelas para monitor ultrawide.
//!
//! Arrastando uma janela: borda esquerda = metade esquerda, borda direita = metade direita,
//! borda de cima = tela cheia. Com Alt aparecem todas as zonas (terços, metades, A centrada
//! quando as duas metades estão ocupadas, notch de tela cheia); vence a de centro mais próximo.
//! Ctrl (com Alt): dois terços. Shift: arrasta sem encaixar.
//! Fica na bandeja do sistema; o menu do ícone pausa ou fecha.

#![windows_subsystem = "windows"]

mod render;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::time::{Duration, Instant};

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
    GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEINPUT, MOUSE_EVENT_FLAGS, VK_CONTROL, VK_LBUTTON, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Third,
    Half,
    Bridge,
}

struct Zone {
    label: &'static str,
    kind: Kind,
    x0: f64,
    x1: f64,
}

const T: f64 = 1.0 / 3.0;
const ZONES: &[Zone] = &[
    Zone { label: "1", kind: Kind::Third, x0: 0.0, x1: T },
    Zone { label: "2", kind: Kind::Third, x0: T, x1: 2.0 * T },
    Zone { label: "3", kind: Kind::Third, x0: 2.0 * T, x1: 1.0 },
    Zone { label: "1", kind: Kind::Half, x0: 0.0, x1: 0.5 },
    Zone { label: "2", kind: Kind::Half, x0: 0.5, x1: 1.0 },
    Zone { label: "A", kind: Kind::Bridge, x0: T / 2.0, x1: T / 2.0 + T },
    Zone { label: "B", kind: Kind::Bridge, x0: T + T / 2.0, x1: T + T / 2.0 + T },
];
const HALF_LEFT: usize = 3;
const HALF_RIGHT: usize = 4;

/// Como as zonas aparecem durante o arraste.
#[derive(Clone, Copy, PartialEq, Default)]
enum Mode {
    /// nada na tela
    #[default]
    Hidden,
    /// cursor encostado numa borda: só a zona daquela borda
    Edge,
    /// Alt: todas as zonas
    All,
}

/// Distância do cursor até a borda do monitor para valer como "encostado".
const EDGE: i32 = 2;

const TIMER_ID: usize = 1;
const WM_TRAY: u32 = WM_APP + 1;
const MENU_PAUSE: usize = 1;
const MENU_QUIT: usize = 2;
#[derive(Default)]
struct Drag {
    hwnd: HWND,
    start: RECT,
    start_frame: RECT,
    moved: bool,
    resizing: bool,
    /// Ao redimensionar: janelas que dividiam a borda esquerda/direita e acompanham a mudança.
    left_neighbors: Vec<(HWND, RECT)>,
    right_neighbors: Vec<(HWND, RECT)>,
    last_frame: RECT,
}

/// Janelas cuja borda direita (ou esquerda) encosta na borda esquerda (ou direita) de `frame`.
fn neighbors(frame: &RECT, skip: [HWND; 3]) -> (Vec<(HWND, RECT)>, Vec<(HWND, RECT)>) {
    const TOL: i32 = 10;
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (hwnd, f) in visible_windows(skip) {
        let overlap = f.bottom.min(frame.bottom) - f.top.max(frame.top);
        if overlap < (frame.bottom - frame.top) / 2 {
            continue; // precisa dividir boa parte da altura
        }
        if (f.right - frame.left).abs() <= TOL {
            left.push((hwnd, f));
        } else if (f.left - frame.right).abs() <= TOL {
            right.push((hwnd, f));
        }
    }
    (left, right)
}

/// Durante o redimensionamento, leva a borda das vizinhas junto com a da janela.
fn follow_resize(drag: &mut Drag) {
    let now = frame_rect(drag.hwnd);
    if now == drag.last_frame {
        return;
    }
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
    if now.left != drag.last_frame.left {
        for (hwnd, f) in &drag.left_neighbors {
            if now.left - f.left > 50 {
                place_with(*hwnd, RECT { right: now.left, ..*f }, flags);
            }
        }
    }
    if now.right != drag.last_frame.right {
        for (hwnd, f) in &drag.right_neighbors {
            if f.right - now.right > 50 {
                place_with(*hwnd, RECT { left: now.right, ..*f }, flags);
            }
        }
    }
    drag.last_frame = now;
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
    mode: Mode,
    avail: Vec<bool>,
    full: bool, // soltar ocupa a área toda (borda de cima, ou notch no modo Alt)
    /// Tamanho (largura, altura) de cada janela antes de ser encaixada, para restaurar ao puxar.
    originals: HashMap<isize, (i32, i32)>,
    /// Janela cujo arraste foi encerrado para voltar ao tamanho original e recomeçar.
    restart: Option<(HWND, (i32, i32))>,
    gfx: Option<render::Gfx>,
    /// início da animação de entrada da zona ativa
    anim_start: Option<Instant>,
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

/// Modo Alt: zona escolhida para a posição horizontal `fx` (0..1 da área de trabalho).
/// Vence a de centro mais próximo; empate (A e terço 2) fica com a A.
fn pick(fx: f64, ctrl: bool, avail: &[bool]) -> Vec<usize> {
    let dist = |i: &usize| (center(&ZONES[*i]) - fx).abs();
    if ctrl {
        let mut cols: Vec<usize> = (0..ZONES.len()).filter(|&i| ZONES[i].kind == Kind::Third).collect();
        cols.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
        cols.truncate(2);
        cols.sort();
        return cols;
    }
    let key = |i: &usize| (dist(i), ZONES[*i].kind != Kind::Bridge);
    (0..ZONES.len())
        .filter(|&i| avail[i] && fx >= ZONES[i].x0 && fx <= ZONES[i].x1)
        .min_by(|a, b| {
            let (da, ba) = key(a);
            let (db, bb) = key(b);
            if (da - db).abs() < 1e-9 { ba.cmp(&bb) } else { da.total_cmp(&db) }
        })
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
    frames: Vec<(HWND, RECT)>,
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
        scan.frames.push((hwnd, frame_rect(hwnd)));
    }
    TRUE
}

/// Janelas visíveis de nível superior na área de trabalho virtual atual, com o retângulo visível.
fn visible_windows(skip: [HWND; 3]) -> Vec<(HWND, RECT)> {
    let mut scan = Scan { skip, frames: Vec::new() };
    unsafe { _ = EnumWindows(Some(collect_window), LPARAM(&mut scan as *mut _ as isize)) };
    scan.frames
}

/// Quais zonas podem receber janela agora, conforme as que já estão ocupadas no monitor.
/// No Alt: terços sempre; A e B (sobre as divisas) só com os dois terços de baixo ocupados.
/// Metades ficam só nas bordas.
fn available(work: &RECT, skip: [HWND; 3]) -> Vec<bool> {
    let zones = ZONES;
    let scan = Scan { skip, frames: visible_windows(skip) };

    const TOL: i32 = 10;
    let close = |a: &RECT, b: &RECT| {
        (a.left - b.left).abs() <= TOL
            && (a.right - b.right).abs() <= TOL
            && (a.top - b.top).abs() <= TOL
            && (a.bottom - b.bottom).abs() <= TOL
    };
    let cols: Vec<usize> = (0..zones.len()).filter(|&i| zones[i].kind == Kind::Third).collect();
    let mut used = vec![false; zones.len()];
    for (_, f) in &scan.frames {
        for (i, z) in zones.iter().enumerate() {
            if close(f, &zone_rect(work, z.x0, z.x1)) {
                used[i] = true;
            }
        }
        // janela ocupando dois terços vizinhos (Ctrl) preenche os dois
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
            Kind::Third => true,
            Kind::Half => false, // metades só pelas bordas, não aparecem no Alt
            // A (divisa 1|2) com 1 e 2 ocupados; B (divisa 2|3) com 2 e 3 ocupados
            Kind::Bridge => cols
                .iter()
                .filter(|&&c| zones[c].x0 < z.x1 && z.x0 < zones[c].x1)
                .all(|&c| used[c]),
        })
        .collect()
}

/// Escala de DPI do monitor que contém a área (1.0 = 96 dpi).
fn dpi_scale(work: &RECT) -> f32 {
    let pt = POINT { x: (work.left + work.right) / 2, y: (work.top + work.bottom) / 2 };
    let (mut x, mut y) = (96u32, 96u32);
    unsafe {
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut x, &mut y);
    }
    x as f32 / 96.0
}

/// Notch no alto da tela, centralizado (coordenadas de tela): soltar a janela nele = tela cheia.
fn notch_rect(work: &RECT) -> RECT {
    let n = render::notch_rect(work.right - work.left, dpi_scale(work));
    // área de soltar um pouco maior que o desenho, para não exigir precisão
    let grow = (n.bottom - n.top) / 2;
    RECT {
        left: work.left + n.left - grow,
        top: work.top,
        right: work.left + n.right + grow,
        bottom: work.top + n.bottom + grow,
    }
}

/// (retângulo do monitor, área de trabalho sem a barra de tarefas) sob o ponto.
fn monitor_rects(pt: POINT) -> (RECT, RECT) {
    unsafe {
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        _ = GetMonitorInfoW(mon, &mut mi);
        (mi.rcMonitor, mi.rcWork)
    }
}

fn monitor_work(pt: POINT) -> RECT {
    monitor_rects(pt).1
}

/// Move a janela para `target`, compensando a borda invisível que o Windows 10/11 põe em volta.
fn place(hwnd: HWND, target: RECT) {
    place_with(hwnd, target, SWP_NOZORDER | SWP_NOACTIVATE);
}

fn place_with(hwnd: HWND, target: RECT, flags: SET_WINDOW_POS_FLAGS) {
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
            flags,
        );
    }
}

/// Duração da entrada da zona ativa.
const ANIM: Duration = Duration::from_millis(170);

fn caption(kind: Kind) -> &'static str {
    match kind {
        Kind::Third => "33%",
        Kind::Half => "50%",
        Kind::Bridge => "33% · divisa",
    }
}

fn scene(st: &State) -> render::Scene {
    let (w, h) = (st.work.right - st.work.left, st.work.bottom - st.work.top);
    let scale = dpi_scale(&st.work);
    let t = st.anim_start.map_or(1.0, |s| s.elapsed().as_secs_f32() / ANIM.as_secs_f32());
    let detailed = st.mode == Mode::All;
    let draw = |i: usize, active: bool| {
        let z = &ZONES[i];
        render::ZoneDraw {
            x0: (z.x0 * w as f64).round() as f32,
            x1: (z.x1 * w as f64).round() as f32,
            label: z.label,
            caption: caption(z.kind),
            style: if z.kind == Kind::Bridge { render::Style::Overlay } else { render::Style::Column },
            active,
        }
    };
    let mut zones = Vec::new();
    if detailed {
        // colunas, depois a sobreposta, por último as ativas
        for pass in [Kind::Third, Kind::Bridge] {
            for i in 0..ZONES.len() {
                if ZONES[i].kind == pass && st.avail.get(i) == Some(&true) && !st.picked.contains(&i) {
                    zones.push(draw(i, false));
                }
            }
        }
    }
    zones.extend(st.picked.iter().map(|&i| draw(i, true)));
    render::Scene {
        w,
        h,
        scale,
        detailed,
        zones,
        notch: detailed.then(|| (render::notch_rect(w, scale), st.full)),
        full_preview: st.mode == Mode::Edge && st.full,
        t,
    }
}

fn show_overlay(st: &mut State, show: bool) {
    if !show {
        unsafe { _ = ShowWindow(st.overlay, SW_HIDE) };
        return;
    }
    let sc = scene(st);
    let pos = POINT { x: st.work.left, y: st.work.top };
    if let Some(gfx) = st.gfx.as_mut() {
        _ = gfx.present(st.overlay, pos, &sc);
    }
    unsafe {
        if !IsWindowVisible(st.overlay).as_bool() {
            _ = SetWindowPos(
                st.overlay,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

fn drag_cursor() -> POINT {
    let mut pt = POINT::default();
    unsafe { _ = GetCursorPos(&mut pt) };
    pt
}

/// Retângulo com o tamanho `size`, mantendo o cursor na mesma proporção horizontal da barra.
fn restored_rect(hwnd: HWND, (w, h): (i32, i32), work: &RECT) -> RECT {
    let f = frame_rect(hwnd);
    let pt = drag_cursor();
    let rel = ((pt.x - f.left) as f64 / (f.right - f.left).max(1) as f64).clamp(0.0, 1.0);
    let left = pt.x - (rel * w as f64).round() as i32;
    let top = f.top.max(work.top);
    RECT { left, top, right: left + w, bottom: top + h }
}

/// Chamado a cada ~15 ms durante o arraste.
fn tick() {
    let release = STATE.with_borrow_mut(|st| {
        let skip = [Default::default(), st.overlay, st.tray];
        let Some(drag) = st.drag.as_mut() else { return false };
        if drag.resizing {
            follow_resize(drag);
            return false;
        }
        let now = window_rect(drag.hwnd);
        let (sw, sh) = (drag.start.right - drag.start.left, drag.start.bottom - drag.start.top);
        if !drag.moved {
            if now == drag.start {
                return false; // só clicou na barra
            }
            if now.right - now.left != sw || now.bottom - now.top != sh {
                // redimensionando pela borda, não é arraste: vizinhas acompanham
                drag.resizing = true;
                let (l, r) = neighbors(&drag.start_frame, [drag.hwnd, skip[1], skip[2]]);
                drag.left_neighbors = l;
                drag.right_neighbors = r;
                drag.last_frame = drag.start_frame;
                follow_resize(drag);
                return false;
            }
            drag.moved = true;
            // Janela encaixada sendo puxada: precisa voltar ao tamanho original já, mas o
            // arraste do Windows mantém o tamanho do início. Solta o botão (encerra esse
            // arraste), redimensiona no fim do arraste e aperta de novo (recomeça).
            if let Some(&size) = st.originals.get(&(drag.hwnd.0 as isize)) {
                let f = drag.start_frame;
                let differs = ((f.right - f.left) - size.0).abs() > 10 || ((f.bottom - f.top) - size.1).abs() > 10;
                if differs {
                    st.restart = Some((drag.hwnd, size));
                    st.drag = None;
                    unsafe { _ = KillTimer(Some(st.overlay), TIMER_ID) };
                    return true;
                }
            }
        }

        let pt = drag_cursor();
        let (mon, work) = monitor_rects(pt);
        let new_monitor = work != st.work;
        if new_monitor {
            st.avail = available(&work, [drag.hwnd, st.overlay, st.tray]);
        }

        let (mode, picked, full) = if key_down(VK_SHIFT.0 as i32) {
            (Mode::Hidden, vec![], false)
        } else if key_down(VK_MENU.0 as i32) {
            // Alt: todas as zonas, escolhe pela posição
            if unsafe { PtInRect(&notch_rect(&work), pt).as_bool() } {
                (Mode::All, vec![], true)
            } else {
                let fx = ((pt.x - work.left) as f64 / (work.right - work.left) as f64).clamp(0.0, 1.0);
                (Mode::All, pick(fx, key_down(VK_CONTROL.0 as i32), &st.avail), false)
            }
        } else if pt.x <= mon.left + EDGE {
            (Mode::Edge, vec![HALF_LEFT], false)
        } else if pt.x >= mon.right - 1 - EDGE {
            (Mode::Edge, vec![HALF_RIGHT], false)
        } else if pt.y <= mon.top + EDGE {
            (Mode::Edge, vec![], true)
        } else {
            (Mode::Hidden, vec![], false)
        };

        let target_changed = picked != st.picked || full != st.full;
        let changed = new_monitor || mode != st.mode || target_changed;
        st.work = work;
        st.mode = mode;
        st.picked = picked;
        st.full = full;
        if target_changed {
            st.anim_start = Some(Instant::now());
        }
        let animating = st.anim_start.is_some_and(|s| s.elapsed() < ANIM + Duration::from_millis(20));
        if changed || (animating && mode != Mode::Hidden) {
            show_overlay(st, mode != Mode::Hidden);
        }
        false
    });
    if release {
        mouse(MOUSEEVENTF_LEFTUP);
    }
}

fn mouse(flags: MOUSE_EVENT_FLAGS) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dwFlags: flags, ..Default::default() } },
    };
    unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
}

/// Janela de bandeja (recebe WM_UNMAXIMIZE) e pausa, lidos pelo gancho de mouse sem tocar no STATE.
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static PAUSED: AtomicBool = AtomicBool::new(false);
const WM_UNMAXIMIZE: u32 = WM_APP + 2;

thread_local! {
    /// botão apertado na barra de título de uma janela maximizada: (janela, ponto)
    static PRESS: Cell<Option<(HWND, POINT)>> = const { Cell::new(None) };
}

/// Janela maximizada cuja barra de título está sob `pt`.
fn maximized_caption(pt: POINT) -> Option<HWND> {
    unsafe {
        let root = GetAncestor(WindowFromPoint(pt), GA_ROOT);
        if root.is_invalid() || !IsZoomed(root).as_bool() {
            return None;
        }
        let mut hit = 0usize;
        let lp = LPARAM(((pt.y as u16 as isize) << 16) | (pt.x as u16 as isize));
        let ok = SendMessageTimeoutW(root, WM_NCHITTEST, WPARAM(0), lp, SMTO_ABORTIFHUNG, 100, Some(&mut hit));
        (ok.0 != 0 && hit as u32 == HTCAPTION).then_some(root)
    }
}

/// Com "Ajustar janelas" desligado o Windows não deixa arrastar janela maximizada. Este gancho
/// percebe o arraste na barra de título e pede para a janela voltar ao tamanho de antes.
unsafe extern "system" fn mouse_ll(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && !PAUSED.load(Ordering::Relaxed) {
        let info = &*(lp.0 as *const MSLLHOOKSTRUCT);
        if info.flags & LLMHF_INJECTED == 0 {
            match wp.0 as u32 {
                WM_LBUTTONDOWN => PRESS.set(maximized_caption(info.pt).map(|h| (h, info.pt))),
                WM_LBUTTONUP => PRESS.set(None),
                WM_MOUSEMOVE => {
                    if let Some((h, p)) = PRESS.get() {
                        let (dx, dy) = (GetSystemMetrics(SM_CXDRAG), GetSystemMetrics(SM_CYDRAG));
                        if (info.pt.x - p.x).abs() > dx || (info.pt.y - p.y).abs() > dy {
                            PRESS.set(None);
                            let tray = HWND(TRAY_HWND.load(Ordering::Relaxed) as *mut _);
                            _ = PostMessageW(Some(tray), WM_UNMAXIMIZE, WPARAM(h.0 as usize), LPARAM(0));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

/// Restaura a janela maximizada sob o cursor, mantendo o ponto da barra, e recomeça o arraste.
fn unmaximize_drag(hwnd: HWND) {
    unsafe {
        if !IsZoomed(hwnd).as_bool() || !key_down(VK_LBUTTON.0 as i32) {
            return;
        }
        let max = frame_rect(hwnd);
        let pt = drag_cursor();
        let rel = ((pt.x - max.left) as f64 / (max.right - max.left).max(1) as f64).clamp(0.0, 1.0);
        let dy = pt.y - max.top;
        mouse(MOUSEEVENTF_LEFTUP);
        _ = ShowWindow(hwnd, SW_RESTORE);
        let f = frame_rect(hwnd);
        let (w, h) = (f.right - f.left, f.bottom - f.top);
        let left = pt.x - (rel * w as f64).round() as i32;
        let top = pt.y - dy;
        place(hwnd, RECT { left, top, right: left + w, bottom: top + h });
        mouse(MOUSEEVENTF_LEFTDOWN);
    }
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
            st.drag = Some(Drag {
                hwnd,
                start: window_rect(hwnd),
                start_frame: frame_rect(hwnd),
                ..Default::default()
            });
            st.picked.clear();
            st.full = false;
            st.mode = Mode::Hidden;
            st.work = RECT::default();
            unsafe { SetTimer(Some(st.overlay), TIMER_ID, 15, None) };
        }),
        EVENT_SYSTEM_MOVESIZEEND => {
            // fim do arraste que o próprio app encerrou: tamanho original sob o cursor e recomeça
            let restart = STATE.with_borrow_mut(|st| match st.restart {
                Some((h, size)) if h == hwnd => {
                    st.restart = None;
                    Some(restored_rect(hwnd, size, &monitor_work(drag_cursor())))
                }
                _ => None,
            });
            if let Some(r) = restart {
                place(hwnd, r);
                mouse(MOUSEEVENTF_LEFTDOWN);
                return;
            }
            let target = STATE.with_borrow_mut(|st| {
                let drag = st.drag.take()?;
                unsafe { _ = KillTimer(Some(st.overlay), TIMER_ID) };
                show_overlay(st, false);
                let picked = std::mem::take(&mut st.picked);
                let full = std::mem::take(&mut st.full);
                st.mode = Mode::Hidden;
                if drag.hwnd != hwnd || !drag.moved || drag.resizing {
                    return None;
                }
                let key = hwnd.0 as isize;
                if full {
                    // maximiza de verdade; o tamanho de antes fica com o próprio Windows
                    // (arrastar a janela maximizada devolve esse tamanho: ver mouse_ll)
                    st.originals.remove(&key);
                    return Some(None);
                }
                let snap = if !picked.is_empty() {
                    let x0 = picked.iter().map(|&i| ZONES[i].x0).fold(f64::MAX, f64::min);
                    let x1 = picked.iter().map(|&i| ZONES[i].x1).fold(f64::MIN, f64::max);
                    Some(zone_rect(&st.work, x0, x1))
                } else {
                    None
                };
                match snap {
                    Some(t) => {
                        let f = drag.start_frame;
                        st.originals.entry(key).or_insert((f.right - f.left, f.bottom - f.top));
                        Some(Some(t))
                    }
                    // solta fora das zonas: volta ao tamanho de antes do encaixe
                    None => st.originals.remove(&key).map(|size| Some(restored_rect(hwnd, size, &st.work))),
                }
            });
            match target {
                Some(Some(t)) => place(hwnd, t),
                Some(None) => unsafe { _ = ShowWindow(hwnd, SW_MAXIMIZE) },
                None => {}
            }
        }
        _ => {}
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
            PAUSED.store(st.paused, Ordering::Relaxed);
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
        WM_SETTINGCHANGE => {
            // tema ou cor de destaque mudou: refaz cor e ícone
            STATE.with_borrow_mut(|st| {
                let Some(gfx) = st.gfx.as_mut() else { return };
                gfx.refresh_accent();
                if let Ok(icon) = gfx.icon(GetSystemMetrics(SM_CXSMICON).max(16), render::light_taskbar()) {
                    st.icon = icon;
                    unsafe { _ = Shell_NotifyIconW(NIM_MODIFY, &tray_data(st)) };
                }
            });
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_TRAY => {
            let event = (lp.0 & 0xFFFF) as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                tray_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_UNMAXIMIZE => {
            unmaximize_drag(HWND(wp.0 as *mut _));
            LRESULT(0)
        }
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
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 2 && args[1] == "--preview" {
            _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            return preview(&args[2]);
        }
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
        // janela oculta que recebe os eventos do ícone da bandeja
        let tray = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("snap-zones-tray"),
            WS_OVERLAPPED,
            0, 0, 0, 0,
            None, None, Some(instance.into()), None,
        )?;
        let mut gfx = render::Gfx::new()?;
        let icon = gfx.icon(GetSystemMetrics(SM_CXSMICON).max(16), render::light_taskbar())?;
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        STATE.with_borrow_mut(|st| {
            st.overlay = overlay;
            st.tray = tray;
            st.icon = icon;
            st.gfx = Some(gfx);
            _ = Shell_NotifyIconW(NIM_ADD, &tray_data(st));
        });

        TRAY_HWND.store(tray.0 as isize, Ordering::Relaxed);
        let _mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_ll), Some(instance.into()), 0)?;

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

/// `snap-zones --preview <pasta>`: desenha os estados do overlay e o ícone em BMP, sobre um
/// fundo de teste, sem abrir nada na tela.
fn preview(dir: &str) -> windows::core::Result<()> {
    let mut gfx = render::Gfx::new()?;
    let work = RECT { left: 0, top: 0, right: 2560, bottom: 1038 };
    let thirds_only = vec![true, true, true, false, false, false, false];
    let with_a = vec![true, true, true, false, false, true, true];
    let cases: [(&str, Mode, Vec<bool>, Vec<usize>, bool); 7] = [
        ("alt-terco", Mode::All, thirds_only.clone(), vec![1], false),
        ("alt-a", Mode::All, with_a.clone(), vec![5], false),
        ("alt-b", Mode::All, with_a.clone(), vec![6], false),
        ("alt-ctrl", Mode::All, with_a, vec![0, 1], false),
        ("alt-notch", Mode::All, thirds_only, vec![], true),
        ("borda-esq", Mode::Edge, vec![], vec![HALF_LEFT], false),
        ("borda-cima", Mode::Edge, vec![], vec![], true),
    ];
    for (name, mode, avail, picked, full) in cases {
        let st = State { work, mode, avail, picked, full, ..Default::default() };
        let px = gfx.snapshot(&scene(&st))?;
        write_bmp(&format!("{dir}/{name}.bmp"), work.right, work.bottom, &px, true);
    }
    for (name, light) in [("icone-escuro", false), ("icone-claro", true)] {
        gfx.icon(128, light)?;
        write_bmp(&format!("{dir}/{name}.bmp"), 128, 128, &gfx.pixels(), !light);
    }
    Ok(())
}

/// Compõe BGRA pré-multiplicado sobre um fundo de teste e grava BMP 24 bits.
fn write_bmp(path: &str, w: i32, h: i32, px: &[u32], dark_bg: bool) {
    let row = ((w * 3 + 3) & !3) as usize;
    let mut data = vec![0u8; row * h as usize];
    for y in 0..h {
        for x in 0..w {
            // fundo: degradê escuro com "janelas" claras, para julgar a transparência nos dois casos
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let mut bg = if dark_bg {
                [40.0 + 60.0 * fx, 30.0 + 40.0 * fy, 70.0 + 50.0 * (1.0 - fx)]
            } else {
                [238.0, 238.0, 240.0]
            };
            let win = (0.05..0.30).contains(&fx) && (0.15..0.70).contains(&fy)
                || (0.58..0.93).contains(&fx) && (0.30..0.88).contains(&fy);
            if dark_bg && w > 200 && win {
                bg = [243.0, 243.0, 243.0];
            }
            let p = px[(y * w + x) as usize];
            let a = (p >> 24) as f32 / 255.0;
            let c = [(p & 0xFF) as f32, ((p >> 8) & 0xFF) as f32, ((p >> 16) & 0xFF) as f32];
            let o = (h - 1 - y) as usize * row + x as usize * 3;
            for k in 0..3 {
                data[o + k] = (c[k] + bg[k] * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    let mut f = Vec::with_capacity(54 + data.len());
    let size = (54 + data.len()) as u32;
    f.extend_from_slice(b"BM");
    f.extend_from_slice(&size.to_le_bytes());
    f.extend_from_slice(&0u32.to_le_bytes());
    f.extend_from_slice(&54u32.to_le_bytes());
    f.extend_from_slice(&40u32.to_le_bytes());
    f.extend_from_slice(&w.to_le_bytes());
    f.extend_from_slice(&h.to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&24u16.to_le_bytes());
    f.extend_from_slice(&[0u8; 24]);
    f.extend_from_slice(&data);
    _ = std::fs::write(path, f);
}
