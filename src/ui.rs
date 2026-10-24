use crate::{
    appearance::{CardStyle, ClockFace, Numerals, TextElement},
    settings::{Clock, Settings},
    taskbar::{self, Location},
    time::{self, Zone, wide},
};
use std::{
    cell::RefCell,
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
    unsafe fn upload(&self, hwnd: HWND) -> bool {
        unsafe {
            let mut r: RECT = zeroed();
            GetWindowRect(hwnd, &mut r);
            self.upload_at(
                hwnd,
                POINT {
                    x: r.left,
                    y: r.top,
                },
            )
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
        if style.face == ClockFace::Mechanical {
            return Vec::new();
        }
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

struct DigitalFrame {
    style: CardStyle,
    content: Vec<String>,
    dpi: u32,
    pixels: Vec<u32>,
}

thread_local! {
    // Bound retained digital pixels to 16 MB independently of the dial-face cache.
    static DIGITAL_FRAMES: RefCell<Vec<DigitalFrame>> = const { RefCell::new(Vec::new()) };
}

pub fn invalidate_caches() {
    DIGITAL_FRAMES.with(|cache| cache.borrow_mut().clear());
    DIAL_FACES.with(|cache| cache.borrow_mut().clear());
}

unsafe fn card_surface(
    c: &Clock,
    style: &CardStyle,
    z: &Zone,
    twelve: bool,
    seconds: bool,
    dpi: u32,
) -> Option<Surface> {
    unsafe { card_surface_at(c, style, z, twelve, seconds, dpi, &time::utc_now()) }
}

unsafe fn card_surface_at(
    c: &Clock,
    style: &CardStyle,
    z: &Zone,
    twelve: bool,
    seconds: bool,
    dpi: u32,
    utc: &SYSTEMTIME,
) -> Option<Surface> {
    unsafe {
        if style.face == ClockFace::Mechanical {
            return mechanical_surface(c, style, z, dpi, utc);
        }
        let w = px(style.width as i32, dpi);
        let h = px(style.height(), dpi);
        let mut surface = Surface::new(w, h)?;
        let p = |v| px(v, dpi);
        let padding = p(style.padding as i32);
        let content: Vec<_> = if style.elements.is_empty() {
            style
                .template
                .lines()
                .map(|row| expanded(row, c, z, utc, twelve, seconds))
                .collect()
        } else {
            style
                .elements
                .iter()
                .map(|element| expanded(&element.text, c, z, utc, twelve, seconds))
                .collect()
        };
        let cached = DIGITAL_FRAMES.with(|cache| {
            let cache = cache.borrow();
            if let Some(frame) = cache
                .iter()
                .find(|frame| frame.style == *style && frame.content == content && frame.dpi == dpi)
            {
                surface.pixels().copy_from_slice(&frame.pixels);
                true
            } else {
                false
            }
        });
        if cached {
            return Some(surface);
        }
        if !style.elements.is_empty() {
            for (e, value) in style.elements.iter().zip(&content) {
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
                    value,
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
            for (row, value) in style.template.lines().zip(&content) {
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
                    value,
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
        let pixels = surface.pixels().to_vec();
        if pixels.len() <= 4_000_000 {
            DIGITAL_FRAMES.with(|cache| {
                let mut cache = cache.borrow_mut();
                while cache.len() >= 16
                    || cache.iter().map(|frame| frame.pixels.len()).sum::<usize>() + pixels.len()
                        > 4_000_000
                {
                    cache.remove(0);
                }
                cache.push(DigitalFrame {
                    style: style.clone(),
                    content,
                    dpi,
                    pixels,
                });
            });
        }
        Some(surface)
    }
}

fn dial_point(center: (f32, f32), radius: f32, turns: f32) -> (f32, f32) {
    let angle = turns * std::f32::consts::TAU;
    (
        center.0 + radius * angle.sin(),
        center.1 - radius * angle.cos(),
    )
}

fn hand_turns(time: &SYSTEMTIME, sweep: bool) -> [f32; 3] {
    let seconds = time.wSecond as f32
        + if sweep {
            time.wMilliseconds as f32 / 1000.0
        } else {
            0.0
        };
    let minutes = time.wMinute as f32 + seconds / 60.0;
    [
        (time.wHour % 12) as f32 / 12.0 + minutes / 720.0,
        minutes / 60.0,
        seconds / 60.0,
    ]
}

// Signed-distance primitives keep thin hands and ticks smooth at every DPI.
fn dial_line(
    surface: &mut Surface,
    start: (f32, f32),
    end: (f32, f32),
    width: f32,
    color: u32,
    opacity: u8,
) {
    let w = surface.w;
    let h = surface.h;
    let half = width / 2.0;
    let left = (start.0.min(end.0) - half - 1.0).floor().max(0.0) as i32;
    let right = (start.0.max(end.0) + half + 1.0).ceil().min(w as f32) as i32;
    let top = (start.1.min(end.1) - half - 1.0).floor().max(0.0) as i32;
    let bottom = (start.1.max(end.1) + half + 1.0).ceil().min(h as f32) as i32;
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length_sq = (dx * dx + dy * dy).max(0.001);
    let pixels = unsafe { surface.pixels() };
    for y in top..bottom {
        for x in left..right {
            let vx = x as f32 + 0.5 - start.0;
            let vy = y as f32 + 0.5 - start.1;
            let t = ((vx * dx + vy * dy) / length_sq).clamp(0.0, 1.0);
            let coverage = (half + 0.5 - (vx - t * dx).hypot(vy - t * dy)).clamp(0.0, 1.0);
            let i = (y * w + x) as usize;
            pixels[i] = over(
                pixels[i],
                color,
                (coverage * opacity as f32 * 2.55).round() as u32,
            );
        }
    }
}

struct DialFace {
    style: CardStyle,
    label: String,
    date: String,
    dpi: u32,
    pixels: Vec<u32>,
}

thread_local! {
    // Cache the stationary dial, so sweeping hands do not re-rasterize its text
    // and ticks on every frame. Bound retained pixel memory to 16 MB.
    static DIAL_FACES: RefCell<Vec<DialFace>> = const { RefCell::new(Vec::new()) };
}

unsafe fn mechanical_surface(
    c: &Clock,
    style: &CardStyle,
    z: &Zone,
    dpi: u32,
    utc: &SYSTEMTIME,
) -> Option<Surface> {
    unsafe {
        let local = time::convert(z, utc)?;
        let w = px(style.width as i32, dpi);
        let h = px(style.height(), dpi);
        let mut surface = Surface::new(w, h)?;
        let center = (w as f32 / 2.0, h as f32 / 2.0);
        let radius = ((w.min(h) as f32 / 2.0) - px(style.padding as i32, dpi) as f32).max(1.0);
        let date = if style.dial.date {
            time::date(&local)
        } else {
            String::new()
        };
        let cached = DIAL_FACES.with(|cache| {
            let cache = cache.borrow();
            if let Some(face) = cache.iter().find(|face| {
                face.style == *style
                    && face.label == c.label
                    && face.date == date
                    && face.dpi == dpi
            }) {
                surface.pixels().copy_from_slice(&face.pixels);
                true
            } else {
                false
            }
        });
        if !cached {
            let numeral_size = (radius
                * if style.dial.numerals == Numerals::Roman {
                    0.17
                } else {
                    0.20
                })
            .round()
            .max(1.0) as i32;
            const ROMAN: [&str; 12] = [
                "XII", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI",
            ];
            if style.dial.numerals != Numerals::None {
                for (index, roman) in ROMAN.iter().enumerate() {
                    let (x, y) = dial_point(center, radius * 0.73, index as f32 / 12.0);
                    let label = if style.dial.numerals == Numerals::Roman {
                        roman.to_string()
                    } else {
                        if index == 0 { 12 } else { index }.to_string()
                    };
                    text_face(
                        surface.dc,
                        &label,
                        rect(
                            (x - radius * 0.23).round() as i32,
                            (y - numeral_size as f32 * 0.75).round() as i32,
                            (radius * 0.46).round().max(1.0) as i32,
                            (numeral_size * 3 / 2).max(1),
                        ),
                        numeral_size,
                        if style.bold { 600 } else { 400 },
                        0xffffff,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                        &style.font,
                    );
                }
            }
            let detail_size =
                px(style.detail_size as i32, dpi).min((radius * 0.17).round().max(1.0) as i32);
            for (show, label, position) in [
                (style.dial.label, c.label.clone(), -0.36),
                (style.dial.date, time::date(&local), 0.36),
            ] {
                if show {
                    text_face(
                        surface.dc,
                        &label,
                        rect(
                            (center.0 - radius * 0.48).round() as i32,
                            (center.1 + radius * position - detail_size as f32).round() as i32,
                            (radius * 0.96).round().max(1.0) as i32,
                            detail_size * 2,
                        ),
                        detail_size,
                        400,
                        0xffffff,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                        &style.font,
                    );
                }
            }
            let mask = surface.mask();
            let outline = dilated(
                &mask,
                w as usize,
                h as usize,
                px(style.outline_width as i32, dpi) as usize,
            );
            let border = px(style.border_width as i32, dpi) as f32;
            let pixels = surface.pixels();
            for y in 0..h {
                for x in 0..w {
                    let distance = (x as f32 + 0.5 - center.0).hypot(y as f32 + 0.5 - center.1);
                    let outer = (radius + 0.5 - distance).clamp(0.0, 1.0);
                    let inner = (radius - border + 0.5 - distance).clamp(0.0, 1.0);
                    let i = (y * w + x) as usize;
                    let mut pixel = over(
                        0,
                        style.background_color,
                        (outer * style.background_opacity as f32 * 2.55).round() as u32,
                    );
                    pixel = over(
                        pixel,
                        style.border_color,
                        ((outer - inner) * style.border_opacity as f32 * 2.55).round() as u32,
                    );
                    pixel = over(
                        pixel,
                        style.outline_color,
                        outline[i].saturating_sub(mask[i]) as u32 * style.outline_opacity as u32
                            / 100,
                    );
                    pixels[i] = over(
                        pixel,
                        style.text_color,
                        mask[i] as u32 * style.text_opacity as u32 / 100,
                    );
                }
            }
            if style.dial.ticks {
                for index in 0..60 {
                    let major = index % 5 == 0;
                    let turns = index as f32 / 60.0;
                    dial_line(
                        &mut surface,
                        dial_point(center, radius * if major { 0.84 } else { 0.90 }, turns),
                        dial_point(center, radius * 0.94, turns),
                        (radius * if major { 0.025 } else { 0.010 }).max(0.7),
                        style.text_color,
                        style.text_opacity,
                    );
                }
            }
            let pixels = surface.pixels().to_vec();
            if pixels.len() <= 4_000_000 {
                DIAL_FACES.with(|cache| {
                    let mut cache = cache.borrow_mut();
                    while cache.len() >= 16
                        || cache.iter().map(|face| face.pixels.len()).sum::<usize>() + pixels.len()
                            > 4_000_000
                    {
                        cache.remove(0);
                    }
                    cache.push(DialFace {
                        style: style.clone(),
                        label: c.label.clone(),
                        date,
                        dpi,
                        pixels,
                    });
                });
            }
        }
        let [hour, minute, second] = hand_turns(&local, style.dial.sweep);
        let width = (style.dial.hand_width as f32 * radius / 90.0).max(0.7);
        for (turns, length, thickness, color) in [
            (hour, 0.48, width * 1.35, style.dial.hour_color),
            (minute, 0.70, width * 0.85, style.dial.minute_color),
        ] {
            dial_line(
                &mut surface,
                dial_point(center, -radius * 0.10, turns),
                dial_point(center, radius * length, turns),
                thickness,
                color,
                style.text_opacity,
            );
        }
        if style.dial.second_hand {
            dial_line(
                &mut surface,
                dial_point(center, -radius * 0.18, second),
                dial_point(center, radius * 0.80, second),
                (radius * 0.014).max(0.8),
                style.dial.second_color,
                style.text_opacity,
            );
        }
        dial_line(
            &mut surface,
            center,
            center,
            (width * 2.4).max(3.0),
            if style.dial.second_hand {
                style.dial.second_color
            } else {
                style.dial.hour_color
            },
            style.text_opacity,
        );
        Some(surface)
    }
}

pub unsafe fn render_desktop(hwnd: HWND, s: &Settings, zones: &[Zone], moving: bool) -> bool {
    unsafe {
        let mut r: RECT = zeroed();
        GetWindowRect(hwnd, &mut r);
        let w = (r.right - r.left).max(1);
        let h = (r.bottom - r.top).max(1);
        let dpi = GetDpiForWindow(hwnd).max(96);
        let Some(mut canvas) = Surface::new(w, h) else {
            return false;
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
        canvas.upload(hwnd)
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

pub unsafe fn paint_buffered(dc: HDC, bounds: RECT, paint: impl FnOnce(HDC)) {
    unsafe {
        let w = bounds.right - bounds.left;
        let h = bounds.bottom - bounds.top;
        let Some(frame) = Surface::new(w, h) else {
            // Keep the previous frame if a complete replacement cannot be allocated.
            return;
        };
        SetViewportOrgEx(frame.dc, -bounds.left, -bounds.top, null_mut());
        paint(frame.dc);
        BitBlt(
            dc,
            bounds.left,
            bounds.top,
            w,
            h,
            frame.dc,
            bounds.left,
            bounds.top,
            SRCCOPY,
        );
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

struct TaskbarTextLayout {
    label_size: i32,
    label_height: i32,
    time_size: i32,
    time_height: i32,
}

unsafe fn font_height(dc: HDC, size: i32, weight: i32) -> i32 {
    unsafe {
        let face = font(size, weight, "Segoe UI");
        let old = SelectObject(dc, face);
        let mut metrics: TEXTMETRICW = zeroed();
        GetTextMetricsW(dc, &mut metrics);
        SelectObject(dc, old);
        DeleteObject(face);
        metrics.tmHeight.max(size)
    }
}

unsafe fn taskbar_text_layout(
    dc: HDC,
    width: i32,
    height: i32,
    twelve: bool,
    vertical: bool,
    dpi: u32,
) -> TaskbarTextLayout {
    unsafe {
        let p = |v| px(v, dpi);
        let preferred = p(if twelve || vertical { 13 } else { 15 });
        // Choose one font for every clock, including one- and two-digit hours.
        let samples: &[&str] = if twelve {
            &["12:59 AM", "12:59 PM"]
        } else {
            &["23:59"]
        };
        let time_size = samples
            .iter()
            .map(|value| fit_taskbar_time(dc, value, preferred, width - p(4), dpi))
            .min()
            .unwrap_or(preferred);
        let mut layout = TaskbarTextLayout {
            label_size: p(10),
            label_height: font_height(dc, p(10), 400),
            time_size,
            time_height: font_height(dc, time_size, 600),
        };
        let available = (height - p(2)).max(1);
        let minimum = p(8);
        while layout.label_height + layout.time_height > available {
            if layout.label_size > minimum {
                layout.label_size -= 1;
                layout.label_height = font_height(dc, layout.label_size, 400);
            } else if layout.time_size > minimum {
                layout.time_size -= 1;
                layout.time_height = font_height(dc, layout.time_size, 600);
            } else {
                // Extremely short custom panels can show only the time legibly.
                layout.label_height = 0;
                while layout.time_height > available && layout.time_size > 1 {
                    layout.time_size -= 1;
                    layout.time_height = font_height(dc, layout.time_size, 600);
                }
                break;
            }
        }
        layout
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
        let vertical = location.vertical();
        let cell_w = if vertical { w } else { w / slots as i32 };
        let cell_h = if vertical { h / slots as i32 } else { h };
        let layout = taskbar_text_layout(surface.dc, cell_w, cell_h, s.twelve, vertical, dpi);
        let pages = clocks.len().div_ceil(slots).max(1);
        // Overlap the final group in saved order instead of leaving empty slots.
        let start = ((page % pages) * slots).min(clocks.len() - slots);
        let utc = time::utc_now();
        let fg = location.foreground;
        for (i, c) in clocks.iter().skip(start).take(slots).enumerate() {
            let z = zones.iter().find(|z| z.key == c.zone).unwrap_or(&zones[0]);
            if let Some(t) = time::convert(z, &utc) {
                let x = if vertical { 0 } else { i as i32 * cell_w };
                let top = if vertical { i as i32 * cell_h } else { 0 }
                    + (cell_h - layout.label_height - layout.time_height) / 2;
                let value = time::clock(&t, s.twelve, false);
                if layout.label_height > 0 {
                    text(
                        surface.dc,
                        &c.label,
                        rect(x + p(3), top, cell_w - p(6), layout.label_height),
                        layout.label_size,
                        400,
                        0xffffff,
                        DT_CENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                    );
                }
                text(
                    surface.dc,
                    &value,
                    rect(
                        x + p(2),
                        top + layout.label_height,
                        cell_w - p(4),
                        layout.time_height,
                    ),
                    layout.time_size,
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

    #[test]
    fn buffered_preview_keeps_previous_frame_until_background_and_clock_are_complete() {
        unsafe {
            let clock = Settings::default().clocks.remove(0);
            let zone = time::zones().remove(0);
            for dpi in [96, 120, 144, 192] {
                let bounds = rect(px(7, dpi), px(9, dpi), px(368, dpi), px(276, dpi));
                let mut target = Surface::new(bounds.right + 5, bounds.bottom + 5).unwrap();
                target.pixels().fill(0x00123456);
                for face in [ClockFace::Mechanical, ClockFace::Digital] {
                    let style = CardStyle {
                        face,
                        ..CardStyle::default()
                    };
                    let previous = target.pixels().to_vec();
                    let geometry = preview_geometry(bounds, &style, dpi);
                    paint_buffered(target.dc, bounds, |buffer| {
                        fill(buffer, &bounds, rgb(215, 222, 230));
                        GdiFlush();
                        assert_eq!(target.pixels(), previous, "background must stay offscreen");
                        paint_card_preview(
                            buffer, bounds, &clock, &style, &zone, false, true, dpi, geometry,
                        );
                        GdiFlush();
                        assert_eq!(
                            target.pixels(),
                            previous,
                            "keep the previous complete frame"
                        );
                    });
                    GdiFlush();
                    let width = target.w as usize;
                    let completed = target.pixels();
                    assert_ne!(completed, previous);
                    assert_eq!(
                        completed[0], 0x00123456,
                        "preserve pixels outside the preview"
                    );
                    assert_eq!(
                        completed[bounds.top as usize * width + bounds.left as usize] & 0xffffff,
                        0xd7dee6,
                        "support nonzero paint origins"
                    );
                }
                let previous = target.pixels().to_vec();
                paint_buffered(target.dc, rect(0, 0, 0, 10), |_| panic!("empty frame"));
                assert_eq!(target.pixels(), previous);
            }
        }
    }

    #[test]
    fn mechanical_hands_follow_hours_minutes_seconds_and_sweep() {
        let time = SYSTEMTIME {
            wHour: 15,
            wMinute: 30,
            wSecond: 15,
            wMilliseconds: 500,
            ..unsafe { zeroed() }
        };
        let stepped = hand_turns(&time, false);
        let smooth = hand_turns(&time, true);
        assert!((stepped[0] - (3.0 / 12.0 + 30.25 / 720.0)).abs() < 0.00001);
        assert!((stepped[1] - 30.25 / 60.0).abs() < 0.00001);
        assert_eq!(stepped[2], 0.25);
        assert!((smooth[2] - 15.5 / 60.0).abs() < 0.00001);
        let point = dial_point((100.0, 100.0), 80.0, stepped[2]);
        assert!((point.0 - 180.0).abs() < 0.001 && (point.1 - 100.0).abs() < 0.001);
    }

    #[test]
    fn digital_cache_reuses_unchanged_pixels_and_invalidates_content_style_and_dpi() {
        unsafe {
            DIGITAL_FRAMES.with(|cache| cache.borrow_mut().clear());
            let mut clock = Settings::default().clocks.remove(0);
            let zone = time::zones().remove(0);
            let utc = SYSTEMTIME {
                wYear: 2026,
                wMonth: 10,
                wDay: 30,
                wHour: 12,
                wMinute: 10,
                wSecond: 5,
                ..zeroed()
            };
            let mut first =
                card_surface_at(&clock, &clock.style, &zone, false, false, 96, &utc).unwrap();
            let expected = first.pixels().to_vec();
            let later = SYSTEMTIME { wSecond: 55, ..utc };
            let mut repeated =
                card_surface_at(&clock, &clock.style, &zone, false, false, 96, &later).unwrap();
            assert_eq!(expected, repeated.pixels());
            DIGITAL_FRAMES.with(|cache| assert_eq!(cache.borrow().len(), 1));
            let minute = SYSTEMTIME { wMinute: 11, ..utc };
            let mut advanced =
                card_surface_at(&clock, &clock.style, &zone, false, false, 96, &minute).unwrap();
            assert_ne!(expected, advanced.pixels());
            clock.label = "Renamed clock".into();
            let mut renamed =
                card_surface_at(&clock, &clock.style, &zone, false, false, 96, &utc).unwrap();
            assert_ne!(expected, renamed.pixels());
            clock.style.text_color = 0x00ff00;
            let mut recolored =
                card_surface_at(&clock, &clock.style, &zone, false, false, 96, &utc).unwrap();
            assert_ne!(renamed.pixels(), recolored.pixels());
            let scaled =
                card_surface_at(&clock, &clock.style, &zone, false, false, 144, &utc).unwrap();
            assert_eq!(scaled.w, px(clock.style.width as i32, 144));
            DIGITAL_FRAMES.with(|cache| assert_eq!(cache.borrow().len(), 5));
        }
    }

    #[test]
    fn mechanical_surface_is_round_transparent_and_customizable_at_multiple_dpis() {
        unsafe {
            let clock = Settings::default().clocks.remove(0);
            let zone = time::zones().remove(0);
            let utc = SYSTEMTIME {
                wYear: 2026,
                wMonth: 10,
                wDay: 30,
                wHour: 10,
                wMinute: 9,
                wSecond: 30,
                wMilliseconds: 500,
                ..zeroed()
            };
            for dpi in [96, 144, 192] {
                let mut style = CardStyle {
                    face: ClockFace::Mechanical,
                    background_opacity: 100,
                    ..CardStyle::default()
                };
                assert_eq!(style.height(), style.width as i32);
                assert!(text_elements(&clock, &style, &zone, false, false).is_empty());
                let mut surface = mechanical_surface(&clock, &style, &zone, dpi, &utc).unwrap();
                let pixels = surface.pixels().to_vec();
                assert_eq!(pixels[0], 0);
                assert!(pixels.contains(&(0xff000000 | style.background_color)));
                let mut repeated = mechanical_surface(&clock, &style, &zone, dpi, &utc).unwrap();
                assert_eq!(pixels, repeated.pixels());
                let later = SYSTEMTIME { wSecond: 45, ..utc };
                let mut moved = mechanical_surface(&clock, &style, &zone, dpi, &later).unwrap();
                assert_ne!(pixels, moved.pixels(), "cached dials must still animate");
                style.dial.numerals = Numerals::Roman;
                style.dial.hour_color = 0xff0000;
                style.dial.minute_color = 0x00ff00;
                style.dial.second_color = 0x0000ff;
                let mut colored = mechanical_surface(&clock, &style, &zone, dpi, &utc).unwrap();
                assert_ne!(pixels, colored.pixels());
                for color in [0xffff0000, 0xff00ff00, 0xff0000ff] {
                    assert!(colored.pixels().contains(&color), "{dpi}: {color:x}");
                }
                style.background_opacity = 0;
                style.border_opacity = 0;
                style.text_opacity = 0;
                style.outline_opacity = 0;
                let mut transparent = mechanical_surface(&clock, &style, &zone, dpi, &utc).unwrap();
                assert!(transparent.pixels().iter().all(|p| *p == 0));
            }
        }
    }

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
    fn short_taskbars_do_not_clip_labels_or_time_at_the_surface_edges() {
        unsafe {
            let zones = time::zones();
            let mut settings = Settings::default();
            settings.clocks.truncate(1);
            settings.clocks[0].label = "Ágj".into();
            for dpi in [96, 120, 144, 192] {
                for height in [24, 30, 48] {
                    for twelve in [false, true] {
                        settings.twelve = twelve;
                        let location = Location {
                            bar: rect(0, 0, px(1920, dpi), px(height, dpi)),
                            tray: rect(px(1740, dpi), 0, px(180, dpi), px(height, dpi)),
                            buttons_end: px(1740, dpi) - px(20, dpi) - px(58, dpi),
                            foreground: 0xffffff,
                        };
                        let (mut surface, _) =
                            taskbar_surface(&settings, &zones, location, 0, dpi).unwrap();
                        let width = surface.w as usize;
                        let height_px = surface.h as usize;
                        let pixels = surface.pixels();
                        for row in [0, height_px - 1] {
                            assert!(
                                pixels[row * width..(row + 1) * width]
                                    .iter()
                                    .all(|pixel| pixel & 0xffffff == 0),
                                "clipped text: {dpi} DPI, height={height}, twelve={twelve}, row={row}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn shared_taskbar_font_fits_every_hour_in_both_time_formats() {
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            assert!(!dc.is_null());
            for dpi in [96, 120, 144, 192] {
                for (width, height, vertical) in [
                    (58, 24, false),
                    (58, 30, false),
                    (84, 48, false),
                    (48, 48, true),
                ] {
                    for twelve in [false, true] {
                        let layout = taskbar_text_layout(
                            dc,
                            px(width, dpi),
                            px(height, dpi),
                            twelve,
                            vertical,
                            dpi,
                        );
                        let face = font(layout.time_size, 600, "Segoe UI");
                        let old = SelectObject(dc, face);
                        for hour in 0..24 {
                            for minute in 0..60 {
                                let time = SYSTEMTIME {
                                    wHour: hour,
                                    wMinute: minute,
                                    ..zeroed()
                                };
                                let value = wide(&time::clock(&time, twelve, false));
                                let mut extent: SIZE = zeroed();
                                assert_ne!(
                                    GetTextExtentPoint32W(
                                        dc,
                                        value.as_ptr(),
                                        (value.len() - 1) as i32,
                                        &mut extent,
                                    ),
                                    0
                                );
                                assert!(extent.cx <= px(width - 4, dpi));
                            }
                        }
                        SelectObject(dc, old);
                        DeleteObject(face);
                    }
                }
            }
            DeleteDC(dc);
        }
    }

    #[test]
    fn taskbar_last_page_keeps_every_available_slot_filled() {
        unsafe {
            let zones = time::zones();
            for dpi in [96, 120, 144, 192] {
                let p = |v| px(v, dpi);
                for vertical in [false, true] {
                    let location = if vertical {
                        Location {
                            buttons_end: p(880) - p(20) - p(48) * 3,
                            ..vertical_taskbar(dpi)
                        }
                    } else {
                        Location {
                            bar: rect(0, p(1032), p(1920), p(48)),
                            tray: rect(p(1740), p(1032), p(180), p(48)),
                            buttons_end: p(1740) - p(20) - p(64) * 3,
                            foreground: 0xffffff,
                        }
                    };
                    for count in [4, 5, 7, 8, 10, 11] {
                        let mut settings = Settings::default();
                        settings.clocks = (0..count)
                            .map(|i| Clock {
                                label: format!("Clock {}", i + 1),
                                ..settings.clocks[0].clone()
                            })
                            .collect();
                        let slots = taskbar::lane(location, count, dpi).unwrap().1;
                        assert_eq!(slots, 3);
                        for twelve in [false, true] {
                            settings.twelve = twelve;
                            let page = count.div_ceil(slots) - 1;
                            let (mut surface, _) =
                                taskbar_surface(&settings, &zones, location, page, dpi).unwrap();
                            let cell_w = if vertical {
                                surface.w
                            } else {
                                surface.w / slots as i32
                            };
                            let cell_h = if vertical {
                                surface.h / slots as i32
                            } else {
                                surface.h
                            };
                            let width = surface.w as usize;
                            let pixels = surface.pixels();
                            for slot in 0..slots {
                                let x = if vertical { 0 } else { slot as i32 * cell_w };
                                let y = if vertical { slot as i32 * cell_h } else { 0 };
                                let painted = (y..y + cell_h)
                                    .flat_map(|row| {
                                        let start = row as usize * width + x as usize;
                                        pixels[start..start + cell_w as usize].iter()
                                    })
                                    .filter(|pixel| **pixel >> 24 != 0)
                                    .count();
                                assert!(
                                    painted > 20,
                                    "empty slot {slot}: {count} clocks, {dpi} DPI, vertical={vertical}"
                                );
                            }
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
