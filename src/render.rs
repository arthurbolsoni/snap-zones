//! Desenho do overlay e do ícone com Direct2D, em superfície com transparência por pixel.

use std::ffi::c_void;

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Style {
    /// coluna (terço ou metade)
    Column,
    /// zona sobreposta às colunas (A)
    Overlay,
}

pub struct ZoneDraw {
    pub x0: f32,
    pub x1: f32,
    pub label: &'static str,
    pub caption: &'static str,
    pub style: Style,
    pub active: bool,
}

pub struct Scene {
    pub w: i32,
    pub h: i32,
    pub scale: f32,
    /// Alt: escurece a tela e mostra rótulos
    pub detailed: bool,
    pub zones: Vec<ZoneDraw>,
    /// notch de tela cheia (Some(ativo)) — só no modo Alt
    pub notch: Option<(RECT, bool)>,
    /// borda de cima: prévia da tela cheia
    pub full_preview: bool,
    /// 0..1, progresso da entrada da zona ativa
    pub t: f32,
}

/// Retângulo do notch para uma área de `w` px de largura, em coordenadas da área.
pub fn notch_rect(w: i32, scale: f32) -> RECT {
    let nw = (232.0 * scale) as i32;
    let nh = (44.0 * scale) as i32;
    let left = (w - nw) / 2;
    RECT { left, top: 0, right: left + nw, bottom: nh }
}

fn rgba(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

fn white(a: f32) -> D2D1_COLOR_F {
    rgba(1.0, 1.0, 1.0, a)
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn reg_dword(key: PCWSTR, value: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut size = 4u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut _ as *mut c_void),
            Some(&mut size),
        )
    };
    ok.is_ok().then_some(data)
}

/// Cor de destaque do Windows; se for escura demais para ler sobre o overlay, branco.
fn accent() -> D2D1_COLOR_F {
    let abgr = reg_dword(
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent"),
        w!("AccentColorMenu"),
    );
    if let Some(v) = abgr {
        let r = (v & 0xFF) as f32 / 255.0;
        let g = ((v >> 8) & 0xFF) as f32 / 255.0;
        let b = ((v >> 16) & 0xFF) as f32 / 255.0;
        if 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.35 {
            return rgba(r, g, b, 1.0);
        }
    }
    white(1.0)
}

pub fn light_taskbar() -> bool {
    reg_dword(
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
        w!("SystemUsesLightTheme"),
    ) == Some(1)
}

/// Superfície de memória 32 bpp ligada a um alvo Direct2D.
struct Surface {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut c_void,
    w: i32,
    h: i32,
}

impl Surface {
    fn new(w: i32, h: i32) -> Result<Self> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
            let old = SelectObject(dc, bmp.into());
            Ok(Self { dc, bmp, old, bits, w, h })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            _ = DeleteObject(self.bmp.into());
            _ = DeleteDC(self.dc);
        }
    }
}

pub struct Gfx {
    dw: IDWriteFactory,
    rt: ID2D1DCRenderTarget,
    dash: ID2D1StrokeStyle,
    surface: Option<Surface>,
    accent: D2D1_COLOR_F,
}

impl Gfx {
    pub fn new() -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let rt = d2d.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            })?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let dash = d2d.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 10.0,
                    dashStyle: D2D1_DASH_STYLE_CUSTOM,
                    dashOffset: 0.0,
                },
                Some(&[3.0, 3.0]),
            )?;
            Ok(Self { dw, rt, dash, surface: None, accent: accent() })
        }
    }

    pub fn refresh_accent(&mut self) {
        self.accent = accent();
    }

    fn bind(&mut self, w: i32, h: i32) -> Result<HDC> {
        if !matches!(&self.surface, Some(s) if s.w == w && s.h == h) {
            self.surface = None;
            self.surface = Some(Surface::new(w, h)?);
        }
        let dc = self.surface.as_ref().unwrap().dc;
        unsafe { self.rt.BindDC(dc, &RECT { left: 0, top: 0, right: w, bottom: h })? };
        Ok(dc)
    }

    fn brush(&self, c: D2D1_COLOR_F) -> Result<ID2D1SolidColorBrush> {
        unsafe { self.rt.CreateSolidColorBrush(&c, None) }
    }

    fn format(&self, size: f32, weight: DWRITE_FONT_WEIGHT) -> Result<IDWriteTextFormat> {
        unsafe {
            let f = self.dw.CreateTextFormat(
                w!("Segoe UI Variable Display"),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("pt-br"),
            )?;
            f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            Ok(f)
        }
    }

    fn text(&self, s: &str, f: &IDWriteTextFormat, r: D2D_RECT_F, c: D2D1_COLOR_F) -> Result<()> {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        let b = self.brush(c)?;
        unsafe {
            self.rt.DrawText(&utf16, f, &r, &b, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
        }
        Ok(())
    }

    fn text_width(&self, s: &str, f: &IDWriteTextFormat) -> Result<f32> {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            let layout = self.dw.CreateTextLayout(&utf16, f, 4000.0, 400.0)?;
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            Ok(m.width)
        }
    }

    /// Desenha a cena e a coloca na janela em camadas `hwnd`, na posição `pos` da tela.
    pub fn present(&mut self, hwnd: HWND, pos: POINT, scene: &Scene) -> Result<()> {
        let dc = self.bind(scene.w, scene.h)?;
        unsafe {
            self.rt.BeginDraw();
            self.rt.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
        }
        let drawn = self.draw(scene);
        unsafe { self.rt.EndDraw(None, None)? };
        drawn?;
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            UpdateLayeredWindow(
                hwnd,
                None,
                Some(&pos),
                Some(&SIZE { cx: scene.w, cy: scene.h }),
                Some(dc),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
    }

    fn draw(&self, sc: &Scene) -> Result<()> {
        let s = sc.scale;
        let (w, h) = (sc.w as f32, sc.h as f32);
        let gap = 10.0 * s;
        let radius = 10.0 * s;
        let e = ease_out(sc.t);
        let acc = self.accent;
        let rt = &self.rt;

        if sc.detailed {
            // véu leve para as zonas lerem sobre qualquer papel de parede
            let veil = self.brush(rgba(0.0, 0.0, 0.0, 0.32))?;
            unsafe { rt.FillRectangle(&D2D_RECT_F { left: 0.0, top: 0.0, right: w, bottom: h }, &veil) };
        }

        let label_f = self.format(44.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
        let caption_f = self.format(15.0 * s, DWRITE_FONT_WEIGHT_NORMAL)?;

        for z in &sc.zones {
            let mut r = D2D_RECT_F {
                left: z.x0 + if z.x0 < 1.0 { gap } else { gap / 2.0 },
                right: z.x1 - if z.x1 > w - 1.0 { gap } else { gap / 2.0 },
                top: gap,
                bottom: h - gap,
            };
            if z.style == Style::Overlay && sc.detailed {
                // fica "por cima" das colunas: mais baixa, centrada
                let inset = h * 0.14;
                r.top += inset;
                r.bottom -= inset;
            }
            let (fill, stroke, width) = if z.active {
                let grow = (1.0 - e) * 10.0 * s;
                r.left += grow;
                r.right -= grow;
                r.top += grow;
                r.bottom -= grow;
                (
                    rgba(acc.r, acc.g, acc.b, 0.36 * e),
                    rgba(acc.r, acc.g, acc.b, 0.95 * e),
                    2.5 * s,
                )
            } else if z.style == Style::Overlay {
                (rgba(0.0, 0.0, 0.0, 0.38), white(0.62), 1.5 * s)
            } else {
                (white(0.045), white(0.24), 1.0 * s)
            };
            let rr = D2D1_ROUNDED_RECT { rect: r, radiusX: radius, radiusY: radius };
            let fb = self.brush(fill)?;
            let sb = self.brush(stroke)?;
            // contorno meio pixel para dentro, para a linha ficar nítida
            let half = width / 2.0;
            let inner = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: r.left + half,
                    top: r.top + half,
                    right: r.right - half,
                    bottom: r.bottom - half,
                },
                radiusX: radius - half,
                radiusY: radius - half,
            };
            unsafe {
                rt.FillRoundedRectangle(&rr, &fb);
                if z.style == Style::Overlay && !z.active {
                    rt.DrawRoundedRectangle(&inner, &sb, width, &self.dash);
                } else {
                    rt.DrawRoundedRectangle(&inner, &sb, width, None);
                }
            }

            if sc.detailed {
                // A e B: rótulo no alto da zona; colunas: na faixa de baixo, livre de A e B
                let mid = match z.style {
                    Style::Overlay => r.top + 70.0 * s,
                    Style::Column => r.bottom - h * 0.065,
                };
                let a = if z.active { 1.0 } else { 0.7 };
                self.text(
                    z.label,
                    &label_f,
                    D2D_RECT_F { left: r.left, right: r.right, top: mid - 40.0 * s, bottom: mid + 12.0 * s },
                    white(a),
                )?;
                self.text(
                    z.caption,
                    &caption_f,
                    D2D_RECT_F { left: r.left, right: r.right, top: mid + 14.0 * s, bottom: mid + 36.0 * s },
                    white(a * 0.72),
                )?;
            }
        }

        if sc.full_preview {
            let g = gap + (1.0 - e) * 10.0 * s;
            let r = D2D_RECT_F { left: g, top: g, right: w - g, bottom: h - g };
            let rr = D2D1_ROUNDED_RECT { rect: r, radiusX: radius, radiusY: radius };
            unsafe {
                rt.FillRoundedRectangle(&rr, &self.brush(rgba(acc.r, acc.g, acc.b, 0.30 * e))?);
                rt.DrawRoundedRectangle(&rr, &self.brush(rgba(acc.r, acc.g, acc.b, 0.95 * e))?, 2.0 * s, None);
            }
        }

        if let Some((n, active)) = sc.notch {
            self.draw_notch(n, active, s)?;
        }
        Ok(())
    }

    /// Pílula presa ao topo: ícone de tela cheia + "Tela cheia".
    fn draw_notch(&self, n: RECT, active: bool, s: f32) -> Result<()> {
        let rt = &self.rt;
        let acc = self.accent;
        let radius = (n.bottom - n.top) as f32 / 2.0;
        let r = D2D_RECT_F {
            left: n.left as f32,
            top: n.top as f32 - radius, // estende para cima: só os cantos de baixo ficam redondos
            right: n.right as f32,
            bottom: n.bottom as f32,
        };
        let rr = D2D1_ROUNDED_RECT { rect: r, radiusX: radius, radiusY: radius };
        let (fill, ink) = if active {
            // destaque branco pede tinta escura; destaque colorido, tinta branca
            let dark_ink = acc.r + acc.g + acc.b > 2.4;
            (rgba(acc.r, acc.g, acc.b, 0.96), if dark_ink { rgba(0.08, 0.08, 0.09, 1.0) } else { white(1.0) })
        } else {
            (rgba(0.125, 0.125, 0.135, 0.94), white(0.92))
        };
        unsafe {
            rt.FillRoundedRectangle(&rr, &self.brush(fill)?);
            rt.DrawRoundedRectangle(&rr, &self.brush(white(if active { 0.0 } else { 0.14 }))?, 1.0 * s, None);
        }

        let f = self.format(14.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
        let label = "Tela cheia";
        let tw = self.text_width(label, &f)?;
        let icon = 14.0 * s;
        let space = 9.0 * s;
        let total = icon + space + tw;
        let cx = (n.left + n.right) as f32 / 2.0;
        let cy = (n.top + n.bottom) as f32 / 2.0;
        let x = cx - total / 2.0;

        // ícone: quatro cantos de enquadramento
        let ink_b = self.brush(ink)?;
        let (l, t, rr_, b) = (x, cy - icon / 2.0, x + icon, cy + icon / 2.0);
        let arm = icon * 0.38;
        let sw = 1.6 * s;
        let p = |x: f32, y: f32| windows_numerics::Vector2 { X: x, Y: y };
        unsafe {
            for (corner, dx, dy) in [((l, t), 1.0, 1.0), ((rr_, t), -1.0, 1.0), ((l, b), 1.0, -1.0), ((rr_, b), -1.0, -1.0)] {
                rt.DrawLine(p(corner.0, corner.1), p(corner.0 + dx * arm, corner.1), &ink_b, sw, None);
                rt.DrawLine(p(corner.0, corner.1), p(corner.0, corner.1 + dy * arm), &ink_b, sw, None);
            }
        }
        self.text(
            label,
            &f,
            D2D_RECT_F { left: x + icon + space, right: x + total + 2.0, top: n.top as f32, bottom: n.bottom as f32 },
            ink,
        )
    }

    /// Pixels da última superfície desenhada (BGRA pré-multiplicado).
    pub fn pixels(&self) -> Vec<u32> {
        let Some(s) = self.surface.as_ref() else { return vec![] };
        unsafe { std::slice::from_raw_parts(s.bits as *const u32, (s.w * s.h) as usize) }.to_vec()
    }

    /// Desenha a cena e devolve os pixels BGRA pré-multiplicados (para o modo --preview).
    pub fn snapshot(&mut self, scene: &Scene) -> Result<Vec<u32>> {
        self.bind(scene.w, scene.h)?;
        unsafe {
            self.rt.BeginDraw();
            self.rt.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
        }
        let drawn = self.draw(scene);
        unsafe { self.rt.EndDraw(None, None)? };
        drawn?;
        let s = self.surface.as_ref().unwrap();
        let px = unsafe { std::slice::from_raw_parts(s.bits as *const u32, (s.w * s.h) as usize) };
        Ok(px.to_vec())
    }

    /// Ícone da bandeja: monitor largo com três colunas, a do meio preenchida.
    pub fn icon(&mut self, size: i32, light_taskbar: bool) -> Result<HICON> {
        let dc = self.bind(size, size)?;
        let s = size as f32;
        let ink = if light_taskbar { rgba(0.1, 0.1, 0.12, 1.0) } else { white(1.0) };
        unsafe {
            self.rt.BeginDraw();
            self.rt.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
            let b = self.brush(ink)?;
            let stroke = (s / 16.0).max(1.0);
            let top = s * 0.22;
            let bottom = s * 0.78;
            let frame = D2D_RECT_F {
                left: stroke / 2.0 + 0.5,
                top: top,
                right: s - stroke / 2.0 - 0.5,
                bottom,
            };
            let rad = s * 0.12;
            self.rt.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT { rect: frame, radiusX: rad, radiusY: rad },
                &b,
                stroke,
                None,
            );
            let pad = stroke * 2.0;
            let inner_w = frame.right - frame.left - pad * 2.0;
            let col = (inner_w - pad * 2.0) / 3.0;
            for i in 0..3 {
                let left = frame.left + pad + i as f32 * (col + pad);
                let r = D2D_RECT_F { left, top: frame.top + pad, right: left + col, bottom: frame.bottom - pad };
                let cr = s * 0.04;
                let rr = D2D1_ROUNDED_RECT { rect: r, radiusX: cr, radiusY: cr };
                if i == 1 {
                    self.rt.FillRoundedRectangle(&rr, &b);
                } else {
                    let faint = self.brush(D2D1_COLOR_F { a: 0.45, ..ink })?;
                    self.rt.FillRoundedRectangle(&rr, &faint);
                }
            }
            self.rt.EndDraw(None, None)?;
        }

        // DIB pré-multiplicado -> ícone com alfa direto
        let surf = self.surface.as_ref().unwrap();
        let px = unsafe { std::slice::from_raw_parts_mut(surf.bits as *mut u32, (size * size) as usize) };
        let mut straight: Vec<u32> = px
            .iter()
            .map(|&p| {
                let a = p >> 24;
                if a == 0 {
                    return 0;
                }
                let un = |c: u32| ((c * 255 + a / 2) / a).min(255);
                (a << 24) | (un((p >> 16) & 0xFF) << 16) | (un((p >> 8) & 0xFF) << 8) | un(p & 0xFF)
            })
            .collect();
        let _ = dc;
        unsafe {
            let color = CreateBitmap(size, size, 1, 32, Some(straight.as_mut_ptr() as *const c_void));
            let mask_bits = vec![0u8; ((size + 15) / 16 * 2 * size) as usize];
            let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr() as *const c_void));
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
}
