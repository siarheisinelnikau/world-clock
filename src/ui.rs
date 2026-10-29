use crate::{
    appearance::{CardStyle, TextElement},
    settings::{Clock, Settings},
    taskbar::{self, Location},
    time::{self, Zone, wide},
};
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

pub const WIDTH: i32 = 218;
pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
pub fn colorref(rgb: u32) -> u32 {
    ((rgb >> 16) & 255) | (rgb & 0xff00) | ((rgb & 255) << 16)
}
pub fn px(v: i32, dpi: u32) -> i32 {
    ((v as i64 * dpi as i64 + 48) / 96) as i32
}
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let r = r.min(w / 2.0).min(h / 2.0);
    let qx = (x - w / 2.0).abs() - (w / 2.0 - r);
    let qy = (y - h / 2.0).abs() - (h / 2.0 - r);
    (0.5 - qx.max(0.0).hypot(qy.max(0.0)) - qx.max(qy).min(0.0) + r).clamp(0.0, 1.0)
}
pub unsafe fn font(size: i32, weight: i32, face: &str) -> HFONT {
    unsafe {
        CreateFontW(
            -size,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            ANTIALIASED_QUALITY as u32,
            0,
            wide(face).as_ptr(),
        )
    }
}
pub unsafe fn fill(dc: HDC, r: &RECT, color: u32) {
    unsafe {
        let b = CreateSolidBrush(color);
        FillRect(dc, r, b);
        DeleteObject(b);
    }
}
#[allow(clippy::too_many_arguments)]
pub unsafe fn text(dc: HDC, value: &str, r: RECT, size: i32, weight: i32, color: u32, flags: u32) {
    unsafe {
        text_face(dc, value, r, size, weight, color, flags, "Segoe UI");
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn text_face(
    dc: HDC,
    value: &str,
    mut r: RECT,
    size: i32,
    weight: i32,
    color: u32,
    flags: u32,
    face: &str,
) {
    unsafe {
        let f = font(size, weight, face);
        let old = SelectObject(dc, f);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        let value = wide(value);
        DrawTextW(
            dc,
            value.as_ptr(),
            (value.len() - 1) as i32,
            &mut r,
            flags | DT_NOPREFIX,
        );
        SelectObject(dc, old);
        DeleteObject(f);
    }
}

struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    w: i32,
    h: i32,
}
impl Surface {
    unsafe fn new(w: i32, h: i32) -> Option<Self> {
        unsafe {
            if w <= 0 || h <= 0 || w as i64 * h as i64 > 32_000_000 {
                return None;
            }
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return None;
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w;
            info.bmiHeader.biHeight = -h;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, bitmap);
            let s = Self {
                dc,
                bitmap,
                old,
                bits: bits.cast(),
                w,
                h,
            };
            std::slice::from_raw_parts_mut(s.bits, (w * h) as usize).fill(0);
            Some(s)
        }
    }
    unsafe fn pixels(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize) }
    }
    unsafe fn mask(&self) -> Vec<u8> {
        unsafe {
            GdiFlush();
            std::slice::from_raw_parts(self.bits, (self.w * self.h) as usize)
                .iter()
                .map(|v| ((v & 255).max((v >> 8) & 255).max((v >> 16) & 255)) as u8)
                .collect()
        }
    }
    unsafe fn upload(&self, hwnd: HWND) {
        unsafe {
            let mut r: RECT = zeroed();
            GetWindowRect(hwnd, &mut r);
            self.upload_at(
                hwnd,
                POINT {
                    x: r.left,
                    y: r.top,
                },
            );
        }
    }
    unsafe fn upload_at(&self, hwnd: HWND, pos: POINT) -> bool {
        unsafe {
            let source = POINT { x: 0, y: 0 };
            let size = SIZE {
                cx: self.w,
                cy: self.h,
            };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                hwnd,
                null_mut(),
                &pos,
                &size,
                self.dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            ) != 0
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

#[derive(Debug)]
pub struct Placement {
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
pub fn desktop_layout(s: &Settings, dpi: u32, available: i32) -> (i32, i32, Vec<Placement>) {
    let limit = (available * 96 / dpi as i32 - 24).max(24);
    let mut items = Vec::new();
    let mut x = 0;
    let mut y = 0;
    let mut column = 0;
    let mut width = 0;
    let mut height = 0;
    for (index, c) in s.clocks.iter().enumerate().filter(|(_, c)| c.desktop) {
        let w = c.style.width as i32;
        let h = c.style.height();
        let gap = c.style.gap as i32;
        if y > 0 && y + h > limit {
            x += column + gap;
            y = 0;
            column = 0;
        }
        items.push(Placement {
            index,
            x: px(x, dpi),
            y: px(y, dpi),
            w: px(w, dpi),
            h: px(h, dpi),
        });
        width = width.max(x + w);
        height = height.max(y + h);
        column = column.max(w);
        y += h + gap;
    }
    (px(width.max(1), dpi), px(height + 24, dpi), items)
}
pub fn desktop_size(s: &Settings, dpi: u32, available: i32) -> (i32, i32, usize) {
    let (w, h, items) = desktop_layout(s, dpi, available);
    (w, h, items.len())
}

fn expanded(
    template: &str,
    c: &Clock,
    z: &Zone,
    utc: &SYSTEMTIME,
    twelve: bool,
    seconds: bool,
) -> String {
    let Some(t) = time::convert(z, utc) else {
        return "Time unavailable".into();
    };
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find('}') else {
            out.push_str(tail);
            return out;
        };
        let token = &tail[..=end];
        let value = match token {
            "{name}" => c.label.clone(),
            "{time}" => time::clock(&t, twelve, seconds),
            "{date}" => time::date(&t),
            "{offset}" => time::offset(&t, utc),
            "{day}" => time::day_relation(&t, &time::local_now()),
            _ => token.into(),
        };
        out.push_str(&value);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn template_parts(row: &str) -> Vec<String> {
    let mut starts = vec![0];
    let mut offset = 0;
    let mut seen = false;
    while let Some(start) = row[offset..].find('{') {
        let start = offset + start;
        let Some(end) = row[start..].find('}') else {
            break;
        };
        if seen && starts.len() < 16 {
            starts.push(start);
        }
        seen = true;
        offset = start + end + 1;
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, start)| row[*start..starts.get(i + 1).copied().unwrap_or(row.len())].to_owned())
        .collect()
}

unsafe fn measure_element(dc: HDC, style: &CardStyle, text: &str, large: bool) -> SIZE {
    unsafe {
        let size = if large {
            style.time_size
        } else {
            style.detail_size
        } as i32;
        let font = font(
            size,
            if large && style.bold { 600 } else { 400 },
            &style.font,
        );
        let old = SelectObject(dc, font);
        let value = wide(text);
        let mut result: SIZE = zeroed();
        GetTextExtentPoint32W(dc, value.as_ptr(), (value.len() - 1) as i32, &mut result);
        SelectObject(dc, old);
        DeleteObject(font);
        result.cy = size + if large { 8 } else { 5 };
        result
    }
}

pub unsafe fn text_elements(
    c: &Clock,
    style: &CardStyle,
    z: &Zone,
    twelve: bool,
    seconds: bool,
) -> Vec<TextElement> {
    unsafe {
        if !style.elements.is_empty() {
            return style.elements.clone();
        }
        let dc = CreateCompatibleDC(null_mut());
        let utc = time::utc_now();
        let mut y = style.padding as i32;
        let mut result = Vec::new();
        for row in style.template.lines() {
            let large = row.contains("{time}");
            let parts = template_parts(row);
            let widths: Vec<_> = parts
                .iter()
                .map(|part| {
                    measure_element(
                        dc,
                        style,
                        &expanded(part, c, z, &utc, twelve, seconds),
                        large,
                    )
                    .cx
                })
                .collect();
            let free =
                (style.width as i32 - style.padding as i32 * 2 - widths.iter().sum::<i32>()).max(0);
            let mut x = style.padding as i32
                + match style.alignment {
                    1 => free / 2,
                    2 => free,
                    _ => 0,
                };
            for (text, width) in parts.into_iter().zip(widths) {
                result.push(TextElement {
                    text,
                    x: x.clamp(0, 2000) as i16,
                    y: y.clamp(0, 2000) as i16,
                    large,
                });
                x += width;
            }
            y += if large {
                style.time_size as i32 + 8
            } else {
                style.detail_size as i32 + 5
            };
        }
        DeleteDC(dc);
        result
    }
}

#[derive(Clone, Copy)]
pub struct ElementGeometry {
    pub index: usize,
    pub logical: RECT,
    pub preview: RECT,
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn preview_elements(
    c: &Clock,
    style: &CardStyle,
    z: &Zone,
    twelve: bool,
    seconds: bool,
    geometry: PreviewGeometry,
) -> Vec<ElementGeometry> {
    unsafe {
        let elements = text_elements(c, style, z, twelve, seconds);
        let dc = CreateCompatibleDC(null_mut());
        let utc = time::utc_now();
        let mut result = Vec::new();
        for (index, e) in elements.iter().enumerate() {
            let value = expanded(&e.text, c, z, &utc, twelve, seconds);
            let size = measure_element(dc, style, &value, e.large);
            let x = e.x as i32;
            let y = e.y as i32;
            let right = (x + size.cx).min(style.width as i32 - style.padding as i32);
            let bottom = (y + size.cy).min(style.height());
            if value.is_empty() || right <= x || bottom <= y {
                continue;
            }
            let scale = geometry.pixels_per_unit;
            let r = geometry.card;
            let point = |value: i32| (value as f64 * scale).round() as i32;
            result.push(ElementGeometry {
                index,
                logical: RECT {
                    left: x,
                    top: y,
                    right,
                    bottom,
                },
                preview: RECT {
                    left: r.left + point(x),
                    top: r.top + point(y),
                    right: r.left + point(right),
                    bottom: r.top + point(bottom),
                },
            });
        }
        DeleteDC(dc);
        result
    }
}

pub unsafe fn paint_element_selection(dc: HDC, mut bounds: RECT) {
    unsafe {
        bounds.left -= 2;
        bounds.top -= 1;
        bounds.right += 2;
        bounds.bottom += 1;
        let pen = CreatePen(PS_DOT, 1, rgb(230, 137, 30));
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        Rectangle(dc, bounds.left, bounds.top, bounds.right, bounds.bottom);
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(pen);
    }
}

fn dilated(mask: &[u8], w: usize, h: usize, r: usize) -> Vec<u8> {
    if r == 0 {
        return mask.to_vec();
    }
    let mut horizontal = vec![0; mask.len()];
    let mut result = vec![0; mask.len()];
    for y in 0..h {
        for x in 0..w {
            horizontal[y * w + x] = (x.saturating_sub(r)..=(x + r).min(w - 1))
                .map(|xx| mask[y * w + xx])
                .max()
                .unwrap_or(0);
        }
    }
    for y in 0..h {
        for x in 0..w {
            result[y * w + x] = (y.saturating_sub(r)..=(y + r).min(h - 1))
                .map(|yy| horizontal[yy * w + x])
                .max()
                .unwrap_or(0);
        }
    }
    result
}

fn over(dst: u32, color: u32, alpha: u32) -> u32 {
    let a = alpha.min(255);
    let inv = 255 - a;
    let channel = |shift: u32| {
        (((color >> shift) & 255u32) * a + 127) / 255
            + (((dst >> shift) & 255u32) * inv + 127) / 255
    };
    ((a + ((dst >> 24) * inv + 127) / 255) << 24)
        | (channel(16) << 16)
        | (channel(8) << 8)
        | channel(0)
}

unsafe fn card_surface(
    c: &Clock,
    style: &CardStyle,
    z: &Zone,
    twelve: bool,
    seconds: bool,
    dpi: u32,
) -> Option<Surface> {
    unsafe {
        let w = px(style.width as i32, dpi);
        let h = px(style.height(), dpi);
        let mut surface = Surface::new(w, h)?;
        let p = |v| px(v, dpi);
        let padding = p(style.padding as i32);
        let utc = time::utc_now();
        if !style.elements.is_empty() {
            for e in &style.elements {
                let size = if e.large {
                    style.time_size
                } else {
                    style.detail_size
                } as i32;
                let height = size + if e.large { 8 } else { 5 };
                let x = p(e.x as i32);
                let y = p(e.y as i32);
                if x >= w - padding || y >= h {
                    continue;
                }
                text_face(
                    surface.dc,
                    &expanded(&e.text, c, z, &utc, twelve, seconds),
                    rect(x, y, (w - padding - x).max(1), p(e.y as i32 + height) - y),
                    p(size),
                    if e.large && style.bold { 600 } else { 400 },
                    0xffffff,
                    DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                    &style.font,
                );
            }
        } else {
            let mut y = style.padding as i32;
            for row in style.template.lines() {
                let time = row.contains("{time}");
                let size = if time {
                    style.time_size
                } else {
                    style.detail_size
                } as i32;
                let height = size + if time { 8 } else { 5 };
                let alignment = match style.alignment {
                    1 => DT_CENTER,
                    2 => DT_RIGHT,
                    _ => DT_LEFT,
                };
                text_face(
                    surface.dc,
                    &expanded(row, c, z, &utc, twelve, seconds),
                    rect(
                        padding,
                        p(y),
                        (w - padding * 2).max(1),
                        p(y + height) - p(y),
                    ),
                    p(size),
                    if time && style.bold { 600 } else { 400 },
                    0xffffff,
                    DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | alignment,
                    &style.font,
                );
                y += height;
            }
        }
        let mask = surface.mask();
        let outline = dilated(
            &mask,
            w as usize,
            h as usize,
            p(style.outline_width as i32) as usize,
        );
        let pixels = surface.pixels();
        let radius = p(style.radius as i32) as f32;
        let border = p(style.border_width as i32) as f32;
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let outer = rounded(x as f32, y as f32, w as f32, h as f32, radius);
                let inner = if border > 0.0 {
                    rounded(
                        x as f32 - border,
                        y as f32 - border,
                        w as f32 - 2.0 * border,
                        h as f32 - 2.0 * border,
                        (radius - border).max(0.0),
                    )
                } else {
                    outer
                };
                let mut pixel = over(
                    0,
                    style.background_color,
                    (outer * style.background_opacity as f32 * 2.55).round() as u32,
                );
                pixel = over(
                    pixel,
                    style.border_color,
                    ((outer - inner).max(0.0) * style.border_opacity as f32 * 2.55).round() as u32,
                );
                pixel = over(
                    pixel,
                    style.outline_color,
                    outline[i].saturating_sub(mask[i]) as u32 * style.outline_opacity as u32 / 100,
                );
                pixels[i] = over(
                    pixel,
                    style.text_color,
                    mask[i] as u32 * style.text_opacity as u32 / 100,
                );
            }
        }
        Some(surface)
    }
}

pub unsafe fn render_desktop(hwnd: HWND, s: &Settings, zones: &[Zone], moving: bool) {
    unsafe {
        let mut r: RECT = zeroed();
        GetWindowRect(hwnd, &mut r);
        let w = (r.right - r.left).max(1);
        let h = (r.bottom - r.top).max(1);
        let dpi = GetDpiForWindow(hwnd).max(96);
        let Some(mut canvas) = Surface::new(w, h) else {
            return;
        };
        let (_, _, items) = desktop_layout(s, dpi, h);
        for item in items {
            let c = &s.clocks[item.index];
            let z = zones.iter().find(|z| z.key == c.zone).unwrap_or(&zones[0]);
            if let Some(mut card) = card_surface(c, &c.style, z, s.twelve, s.seconds, dpi) {
                let src = card.pixels();
                let dst = canvas.pixels();
                for y in 0..item.h.min(h - item.y) {
                    let start = ((item.y + y) * w + item.x) as usize;
                    let len = item.w.min(w - item.x).max(0) as usize;
                    if len > 0 {
                        dst[start..start + len].copy_from_slice(
                            &src[(y * item.w) as usize..(y * item.w) as usize + len],
                        );
                    }
                }
            }
        }
        if moving {
            let pixels = canvas.pixels();
            for pixel in pixels.iter_mut() {
                if *pixel == 0 {
                    *pixel = 0x01000000;
                }
            }
            for x in 0..w {
                pixels[x as usize] = 0xffffffff;
                pixels[((h - 1) * w + x) as usize] = 0xffffffff;
            }
            for y in 0..h {
                pixels[(y * w) as usize] = 0xffffffff;
                pixels[(y * w + w - 1) as usize] = 0xffffffff;
            }
        }
        canvas.upload(hwnd);
    }
}

#[derive(Clone, Copy)]
pub struct PreviewGeometry {
    pub card: RECT,
    pub pixels_per_unit: f64,
}

pub fn preview_geometry(bounds: RECT, style: &CardStyle, dpi: u32) -> PreviewGeometry {
    let width = (bounds.right - bounds.left - px(24, dpi)).max(1);
    let height = (bounds.bottom - bounds.top - px(24, dpi)).max(1);
    let zoom = (width as f64 / style.width as f64)
        .min(height as f64 / style.height() as f64)
        .min(dpi as f64 / 96.0);
    let w = (style.width as f64 * zoom).round().max(1.0) as i32;
    let h = (style.height() as f64 * zoom).round().max(1.0) as i32;
    PreviewGeometry {
        card: rect(
            bounds.left + (bounds.right - bounds.left - w) / 2,
            bounds.top + (bounds.bottom - bounds.top - h) / 2,
            w,
            h,
        ),
        pixels_per_unit: zoom,
    }
}

pub unsafe fn paint_resize_handles(dc: HDC, geometry: PreviewGeometry, dpi: u32) {
    unsafe {
        let r = geometry.card;
        let color = GetSysColor(COLOR_HIGHLIGHT);
        let brush = CreateSolidBrush(color);
        FrameRect(dc, &r, brush);
        let half = px(4, dpi).max(3);
        for (x, y) in [
            (r.left, r.top),
            ((r.left + r.right) / 2, r.top),
            (r.right, r.top),
            (r.left, (r.top + r.bottom) / 2),
            (r.right, (r.top + r.bottom) / 2),
            (r.left, r.bottom),
            ((r.left + r.right) / 2, r.bottom),
            (r.right, r.bottom),
        ] {
            let handle = rect(x - half, y - half, half * 2 + 1, half * 2 + 1);
            fill(dc, &handle, rgb(255, 255, 255));
            FrameRect(dc, &handle, brush);
        }
        DeleteObject(brush);
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn paint_card_preview(
    dc: HDC,
    bounds: RECT,
    c: &Clock,
    style: &CardStyle,
    z: &Zone,
    twelve: bool,
    seconds: bool,
    dpi: u32,
    geometry: PreviewGeometry,
) {
    unsafe {
        let w = bounds.right - bounds.left;
        let h = bounds.bottom - bounds.top;
        let tile = px(12, dpi);
        for y in (0..h).step_by(tile as usize) {
            for x in (0..w).step_by(tile as usize) {
                fill(
                    dc,
                    &rect(
                        bounds.left + x,
                        bounds.top + y,
                        tile.min(w - x),
                        tile.min(h - y),
                    ),
                    if (x / tile + y / tile) % 2 == 0 {
                        rgb(215, 222, 230)
                    } else {
                        rgb(238, 242, 246)
                    },
                );
            }
        }
        if let Some(card) = card_surface(c, style, z, twelve, seconds, dpi) {
            let r = geometry.card;
            let tw = r.right - r.left;
            let th = r.bottom - r.top;
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            GdiAlphaBlend(
                dc, r.left, r.top, tw, th, card.dc, 0, 0, card.w, card.h, blend,
            );
        }
        paint_resize_handles(dc, geometry, dpi);
    }
}

pub unsafe fn render_taskbar(
    hwnd: HWND,
    s: &Settings,
    zones: &[Zone],
    location: Location,
    page: usize,
    dpi: u32,
) -> bool {
    unsafe {
        let Some((surface, pos)) = taskbar_surface(s, zones, location, page, dpi) else {
            return false;
        };
        surface.upload_at(hwnd, pos)
    }
}

unsafe fn fit_taskbar_time(dc: HDC, value: &str, preferred: i32, width: i32, dpi: u32) -> i32 {
    unsafe {
        let value = wide(value);
        let minimum = px(8, dpi);
        for size in (minimum..=preferred).rev() {
            let face = font(size, 600, "Segoe UI");
            let old = SelectObject(dc, face);
            let mut extent: SIZE = zeroed();
            let measured =
                GetTextExtentPoint32W(dc, value.as_ptr(), (value.len() - 1) as i32, &mut extent);
            SelectObject(dc, old);
            DeleteObject(face);
            if measured != 0 && extent.cx <= width {
                return size;
            }
        }
        minimum
    }
}

unsafe fn taskbar_surface(
    s: &Settings,
    zones: &[Zone],
    location: Location,
    page: usize,
    dpi: u32,
) -> Option<(Surface, POINT)> {
    unsafe {
        let clocks: Vec<_> = s.clocks.iter().filter(|c| c.tray).collect();
        let (r, slots) = taskbar::lane(location, clocks.len(), dpi)?;
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        let p = |v| px(v, dpi);
        let mut surface = Surface::new(w, h)?;
        let pages = clocks.len().div_ceil(slots).max(1);
        let start = (page % pages) * slots;
        let utc = time::utc_now();
        let fg = location.foreground;
        for (i, c) in clocks.iter().skip(start).take(slots).enumerate() {
            let z = zones.iter().find(|z| z.key == c.zone).unwrap_or(&zones[0]);
            if let Some(t) = time::convert(z, &utc) {
                let vertical = location.vertical();
                let cell_w = if vertical { w } else { w / slots as i32 };
                let cell_h = if vertical { h / slots as i32 } else { h };
                let x = if vertical { 0 } else { i as i32 * cell_w };
                let top = if vertical { i as i32 * cell_h } else { 0 } + (cell_h - p(34)) / 2;
                let value = time::clock(&t, s.twelve, false);
                let time_size = fit_taskbar_time(
                    surface.dc,
                    &value,
                    p(if s.twelve || vertical { 13 } else { 15 }),
                    cell_w - p(4),
                    dpi,
                );
                text(
                    surface.dc,
                    &c.label,
                    rect(x + p(3), top, cell_w - p(6), p(15)),
                    p(10),
                    400,
                    0xffffff,
                    DT_CENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                text(
                    surface.dc,
                    &value,
                    rect(x + p(2), top + p(14), cell_w - p(4), p(21)),
                    time_size,
                    600,
                    0xffffff,
                    DT_CENTER | DT_SINGLELINE,
                );
            }
        }
        let mask = surface.mask();
        let outline = dilated(&mask, w as usize, h as usize, 1);
        let pixels = surface.pixels();
        for (i, pixel) in pixels.iter_mut().enumerate() {
            let bg = if fg == 0x181818 { 0xffffff } else { 0x000000 };
            *pixel = over(
                over(0, bg, outline[i].saturating_sub(mask[i]) as u32 / 3),
                fg,
                mask[i] as u32,
            );
        }
        Some((
            surface,
            POINT {
                x: r.left,
                y: r.top,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertical_taskbar(dpi: u32) -> Location {
        Location {
            bar: rect(0, 0, px(48, dpi), px(1080, dpi)),
            tray: rect(0, px(880, dpi), px(48, dpi), px(200, dpi)),
            buttons_end: px(320, dpi),
            foreground: 0xffffff,
        }
    }

    #[test]
    fn narrow_vertical_bar_renders_each_clock_in_both_time_formats() {
        unsafe {
            let zones = time::zones();
            let mut settings = Settings::default();
            for dpi in [96, 120, 144, 192] {
                for twelve in [false, true] {
                    settings.twelve = twelve;
                    let (mut surface, _) =
                        taskbar_surface(&settings, &zones, vertical_taskbar(dpi), 0, dpi).unwrap();
                    let width = surface.w as usize;
                    let row_height = surface.h as usize / 3;
                    let pixels = surface.pixels();
                    for row in 0..3 {
                        // Both the label and the time must remain inside their own row.
                        for (top, bottom) in [(7, 18), (20, 36)] {
                            let first = row * row_height + px(top, dpi) as usize;
                            let last = row * row_height + px(bottom, dpi) as usize;
                            assert!(
                                pixels[first * width..last * width]
                                    .iter()
                                    .filter(|v| **v >> 24 != 0)
                                    .count()
                                    > 10
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn complete_layered_frames_can_change_orientation_without_a_separate_resize() {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                windows_sys::core::w!("STATIC"),
                windows_sys::core::w!("World Clock frame test"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                null_mut(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null());
            let settings = Settings::default();
            let zones = time::zones();
            for (location, dpi) in [
                (vertical_taskbar(96), 96),
                (
                    Location {
                        bar: rect(0, 1032, 1920, 48),
                        tray: rect(1740, 1032, 180, 48),
                        buttons_end: 980,
                        foreground: 0xffffff,
                    },
                    96,
                ),
                (vertical_taskbar(144), 144),
            ] {
                let (expected, _) = taskbar::lane(location, 3, dpi).unwrap();
                let uploaded = render_taskbar(hwnd, &settings, &zones, location, 0, dpi);
                let mut actual: RECT = zeroed();
                GetWindowRect(hwnd, &mut actual);
                assert!(uploaded);
                assert!(taskbar::same_rect(actual, expected));
                assert_eq!(IsWindowVisible(hwnd), 0);
            }
            DestroyWindow(hwnd);
        }
    }

    #[test]
    fn layer_opacity_is_independent_and_premultiplied() {
        assert_eq!(over(0, 0x12abef, 0), 0);
        assert_eq!(over(0, 0x12abef, 255), 0xff12abef);
        let bg = over(0, 0xff0000, 100);
        assert_eq!(over(bg, 0xffffff, 0), bg);
        let pixel = over(bg, 0x00ff00, 80);
        let a = pixel >> 24;
        assert!(((pixel >> 16) & 255) <= a && ((pixel >> 8) & 255) <= a && (pixel & 255) <= a);
    }
    #[test]
    fn compact_variable_cards_fit_without_overlap() {
        let mut s = Settings::default();
        s.clocks[1].style.width = 150;
        s.clocks[2].style.template = "{time}".into();
        for dpi in [96, 120, 144, 192] {
            let (w, h, items) = desktop_layout(&s, dpi, 700);
            assert!(h <= 700);
            for a in &items {
                assert!(a.x + a.w <= w && a.y + a.h <= h);
                for b in &items {
                    if a.index != b.index {
                        assert!(
                            a.x + a.w <= b.x
                                || b.x + b.w <= a.x
                                || a.y + a.h <= b.y
                                || b.y + b.h <= a.y
                        );
                    }
                }
            }
            let (_, _, again) = desktop_layout(&s, dpi, h);
            assert_eq!(
                items.iter().map(|v| (v.x, v.y)).collect::<Vec<_>>(),
                again.iter().map(|v| (v.x, v.y)).collect::<Vec<_>>()
            );
        }
    }
    #[test]
    fn card_corners_stay_clear() {
        assert_eq!(rounded(0.0, 0.0, 218.0, 90.0, 11.0), 0.0);
        assert_eq!(rounded(100.0, 40.0, 218.0, 90.0, 11.0), 1.0);
    }
    #[test]
    fn automatic_content_exposes_independent_time_name_and_metadata() {
        unsafe {
            let c = Settings::default().clocks.remove(0);
            let zone = crate::time::zones().remove(0);
            let elements = text_elements(&c, &c.style, &zone, false, false);
            assert_eq!(elements.len(), 5);
            for token in ["{name}", "{time}", "{date}", "{offset}", "{day}"] {
                assert_eq!(
                    elements.iter().filter(|e| e.text.contains(token)).count(),
                    1
                );
            }
            let geometry = preview_geometry(rect(0, 0, 368, 276), &c.style, 96);
            assert_eq!(
                preview_elements(&c, &c.style, &zone, false, false, geometry).len(),
                5
            );
            let manual = CardStyle {
                elements,
                height: 0,
                ..c.style.clone()
            };
            assert_eq!(manual.height(), c.style.height());
        }
    }
    #[test]
    fn card_layers_render_in_their_own_colors() {
        unsafe {
            let c = Settings::default().clocks[0].clone();
            let z = crate::time::zones().remove(0);
            let mut style = CardStyle {
                template: "{time}".into(),
                width: 140,
                time_size: 20,
                text_opacity: 0,
                outline_opacity: 0,
                background_opacity: 0,
                border_opacity: 0,
                ..CardStyle::default()
            };
            let mut empty = card_surface(&c, &style, &z, false, false, 96).unwrap();
            assert!(empty.pixels().iter().all(|v| *v == 0));
            style.background_color = 0xE04020;
            style.background_opacity = 100;
            let mut background = card_surface(&c, &style, &z, false, false, 96).unwrap();
            assert!(background.pixels().contains(&0xffe04020));
            style.background_opacity = 0;
            style.text_color = 0x00ff00;
            style.text_opacity = 100;
            style.outline_color = 0x0000ff;
            style.outline_opacity = 100;
            style.outline_width = 2;
            let mut colored = card_surface(&c, &style, &z, false, false, 96).unwrap();
            let pixels = colored.pixels();
            assert!(pixels.iter().any(|p| p & 0x00ffffff == 0x0000ff00));
            assert!(pixels.iter().any(|p| p & 0x00ffffff == 0x000000ff));
        }
    }
}
