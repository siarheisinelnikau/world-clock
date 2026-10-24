use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockFace {
    #[default]
    Digital,
    Mechanical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Numerals {
    None,
    #[default]
    Arabic,
    Roman,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DialStyle {
    pub numerals: Numerals,
    pub ticks: bool,
    pub second_hand: bool,
    pub sweep: bool,
    pub label: bool,
    pub date: bool,
    pub hand_width: u8,
    pub hour_color: u32,
    pub minute_color: u32,
    pub second_color: u32,
}

impl Default for DialStyle {
    fn default() -> Self {
        Self {
            numerals: Numerals::Arabic,
            ticks: true,
            second_hand: true,
            sweep: true,
            label: true,
            date: false,
            hand_width: 4,
            hour_color: 0xF2F5FA,
            minute_color: 0xF2F5FA,
            second_color: 0xF09B67,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextElement {
    pub text: String,
    pub x: i16,
    pub y: i16,
    pub large: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CardStyle {
    pub face: ClockFace,
    pub dial: DialStyle,
    pub width: u16,
    pub height: u16,
    // Retained for styles saved before exact heights were configurable.
    pub min_height: u16,
    pub padding: u8,
    pub gap: u8,
    pub radius: u8,
    pub time_size: u8,
    pub detail_size: u8,
    pub font: String,
    pub alignment: u8,
    pub bold: bool,
    pub template: String,
    pub elements: Vec<TextElement>,
    pub text_color: u32,
    pub text_opacity: u8,
    pub outline_color: u32,
    pub outline_opacity: u8,
    pub outline_width: u8,
    pub background_color: u32,
    pub background_opacity: u8,
    pub border_color: u32,
    pub border_opacity: u8,
    pub border_width: u8,
}

impl Default for CardStyle {
    fn default() -> Self {
        Self {
            face: ClockFace::Digital,
            dial: DialStyle::default(),
            width: 218,
            height: 0,
            min_height: 0,
            padding: 10,
            gap: 8,
            radius: 11,
            time_size: 30,
            detail_size: 11,
            font: "Segoe UI".into(),
            alignment: 0,
            bold: true,
            template: "{name} · {offset}\n{time}\n{date} · {day}".into(),
            elements: Vec::new(),
            text_color: 0xF2F5FA,
            text_opacity: 100,
            outline_color: 0x000000,
            outline_opacity: 65,
            outline_width: 1,
            background_color: 0x1B2330,
            background_opacity: 70,
            border_color: 0x62758D,
            border_opacity: 50,
            border_width: 0,
        }
    }
}

impl CardStyle {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=8).contains(&self.dial.hand_width)
            || [
                self.dial.hour_color,
                self.dial.minute_color,
                self.dial.second_color,
            ]
            .iter()
            .any(|color| *color > 0xffffff)
        {
            return Err("Hand width must be 1–8 and hand colors valid RGB values.".into());
        }
        if !(120..=600).contains(&self.width)
            || (self.height != 0 && !(24..=700).contains(&self.height))
            || self.min_height > 700
            || self.padding > 40
            || self.gap > 40
            || self.radius > 40
        {
            return Err(
                "Width must be 120–600, height 24–700 (or 0 for Auto), and padding, gap and corners 0–40."
                    .into(),
            );
        }
        if !(10..=72).contains(&self.time_size)
            || !(8..=32).contains(&self.detail_size)
            || self.alignment > 2
        {
            return Err("Time size must be 10–72 and detail size 8–32.".into());
        }
        if self.font.trim().is_empty()
            || self.font.chars().count() > 64
            || self.template.len() > 2048
            || self.template.lines().count() > 8
            || self.template.trim().is_empty()
        {
            return Err("Choose a font and enter 1–8 content rows (up to 2048 bytes).".into());
        }
        if self.elements.len() > 128
            || self.elements.iter().any(|e| {
                e.text.len() > 2048 || !(0..=2000).contains(&e.x) || !(0..=2000).contains(&e.y)
            })
        {
            return Err("Text elements must have valid content and positions.".into());
        }
        if [
            self.text_opacity,
            self.outline_opacity,
            self.background_opacity,
            self.border_opacity,
        ]
        .iter()
        .any(|v| *v > 100)
            || self.outline_width > 4
            || self.border_width > 4
            || [
                self.text_color,
                self.outline_color,
                self.background_color,
                self.border_color,
            ]
            .iter()
            .any(|v| *v > 0xffffff)
        {
            return Err(
                "Opacity must be 0–100%, border widths 0–4, and colors valid RGB values.".into(),
            );
        }
        Ok(())
    }

    pub fn height(&self) -> i32 {
        if self.height != 0 {
            return self.height as i32;
        }
        if self.face == ClockFace::Mechanical {
            return (self.width as i32).max(self.min_height as i32);
        }
        if !self.elements.is_empty() {
            return self
                .elements
                .iter()
                .map(|e| {
                    e.y as i32
                        + if e.large {
                            self.time_size as i32 + 8
                        } else {
                            self.detail_size as i32 + 5
                        }
                })
                .max()
                .unwrap_or(0)
                .saturating_add(self.padding as i32)
                .max(self.min_height as i32)
                .max(24);
        }
        let mut height = self.padding as i32 * 2;
        for row in self.template.lines() {
            height += if row.contains("{time}") {
                self.time_size as i32 + 8
            } else {
                self.detail_size as i32 + 5
            };
        }
        height.max(self.min_height as i32).max(24)
    }
}

use crate::{
    render::{Cadence, RefreshGate},
    settings::Clock,
    time::{Zone, unw, wide},
    ui,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{Dialogs::*, *},
            HiDpi::*,
            Input::KeyboardAndMouse::{
                EnableWindow, GetCapture, ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE,
            },
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

const WIDTH: i32 = 300;
const HEIGHT: i32 = 301;
const PADDING: i32 = 302;
const GAP: i32 = 303;
const RADIUS: i32 = 304;
const AUTO_HEIGHT: i32 = 305;
const FACE: i32 = 306;
const TIME: i32 = 310;
const DETAIL: i32 = 311;
const FONT: i32 = 312;
const ALIGN: i32 = 313;
const BOLD: i32 = 314;
const TEMPLATE: i32 = 315;
const PRESET: i32 = 316;
const RESET_POSITIONS: i32 = 317;
const TEXT_COLOR: i32 = 320;
const OUTLINE_COLOR: i32 = 321;
const BG_COLOR: i32 = 322;
const BORDER_COLOR: i32 = 323;
const TEXT_ALPHA: i32 = 330;
const OUTLINE_ALPHA: i32 = 331;
const BG_ALPHA: i32 = 332;
const BORDER_ALPHA: i32 = 333;
const OUTLINE_WIDTH: i32 = 340;
const BORDER_WIDTH: i32 = 341;
const PREVIEW: i32 = 350;
const ALL: i32 = 351;
const RESET: i32 = 352;
const SIZE_LABEL: i32 = 353;
const TAB_LAYOUT: i32 = 360;
const TAB_CONTENT: i32 = 361;
const TAB_COLORS: i32 = 362;
const TAB_DIAL: i32 = 363;
const NUMERALS: i32 = 370;
const TICKS: i32 = 371;
const SECOND_HAND: i32 = 372;
const SWEEP: i32 = 373;
const DIAL_LABEL: i32 = 374;
const DIAL_DATE: i32 = 375;
const HAND_WIDTH: i32 = 376;
const HOUR_COLOR: i32 = 380;
const MINUTE_COLOR: i32 = 381;
const SECOND_COLOR: i32 = 382;
const TBM_GETPOS: u32 = WM_USER;

unsafe extern "system" fn collect_font_family(
    font: *const LOGFONTW,
    _metrics: *const TEXTMETRICW,
    _font_type: u32,
    data: LPARAM,
) -> i32 {
    unsafe {
        if !font.is_null() {
            let name = unw(&(*font).lfFaceName);
            if !name.is_empty() && !name.starts_with('@') {
                let families = &mut *(data as *mut BTreeMap<String, String>);
                families.entry(name.to_lowercase()).or_insert(name);
            }
        }
        1
    }
}

fn installed_fonts() -> Vec<String> {
    unsafe {
        let mut families = BTreeMap::new();
        let dc = CreateCompatibleDC(null_mut());
        if !dc.is_null() {
            let query = LOGFONTW {
                lfCharSet: DEFAULT_CHARSET,
                ..zeroed()
            };
            EnumFontFamiliesExW(
                dc,
                &query,
                Some(collect_font_family),
                (&mut families as *mut BTreeMap<String, String>) as LPARAM,
                0,
            );
            DeleteDC(dc);
        }
        if families.is_empty() {
            families.insert("segoe ui".into(), "Segoe UI".into());
        }
        families.into_values().collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResizeHandle {
    x: i8,
    y: i8,
}

fn resize_hit(card: RECT, point: POINT, tolerance: i32) -> Option<ResizeHandle> {
    if point.x < card.left - tolerance
        || point.x > card.right + tolerance
        || point.y < card.top - tolerance
        || point.y > card.bottom + tolerance
    {
        return None;
    }
    let x = if (point.x - card.left).abs() <= tolerance {
        -1
    } else if (point.x - card.right).abs() <= tolerance {
        1
    } else {
        0
    };
    let y = if (point.y - card.top).abs() <= tolerance {
        -1
    } else if (point.y - card.bottom).abs() <= tolerance {
        1
    } else {
        0
    };
    (x != 0 || y != 0).then_some(ResizeHandle { x, y })
}

#[derive(Clone, Copy)]
struct PreviewDrag {
    handle: ResizeHandle,
    start: POINT,
    geometry: ui::PreviewGeometry,
    width: u16,
    height: u16,
    min_height: u16,
    actual_height: i32,
}

impl PreviewDrag {
    fn new(
        handle: ResizeHandle,
        start: POINT,
        geometry: ui::PreviewGeometry,
        style: &CardStyle,
    ) -> Self {
        Self {
            handle,
            start,
            geometry,
            width: style.width,
            height: style.height,
            min_height: style.min_height,
            actual_height: style.height(),
        }
    }

    fn apply(&self, point: POINT, style: &mut CardStyle) -> bool {
        let before = (style.width, style.height, style.min_height);
        if self.handle.x != 0 {
            let delta =
                ((point.x - self.start.x) as f64 / self.geometry.pixels_per_unit).round() as i32;
            style.width = (self.width as i32 + delta * self.handle.x as i32).clamp(120, 600) as u16;
        }
        if self.handle.y != 0 {
            let delta =
                ((point.y - self.start.y) as f64 / self.geometry.pixels_per_unit).round() as i32;
            style.height =
                (self.actual_height + delta * self.handle.y as i32).clamp(24, 700) as u16;
            style.min_height = 0;
        }
        before != (style.width, style.height, style.min_height)
    }

    fn geometry(&self, style: &CardStyle) -> ui::PreviewGeometry {
        let scale = self.geometry.pixels_per_unit;
        let w = (style.width as f64 * scale).round().max(1.0) as i32;
        let h = (style.height() as f64 * scale).round().max(1.0) as i32;
        let old = self.geometry.card;
        let x = if self.handle.x < 0 {
            old.right - w
        } else {
            old.left
        };
        let y = if self.handle.y < 0 {
            old.bottom - h
        } else {
            old.top
        };
        ui::PreviewGeometry {
            card: ui::rect(x, y, w, h),
            pixels_per_unit: scale,
        }
    }

    fn restore(&self, style: &mut CardStyle) {
        style.width = self.width;
        style.height = self.height;
        style.min_height = self.min_height;
    }
}

struct ElementDrag {
    index: usize,
    start: POINT,
    x: i16,
    y: i16,
    visible_width: i32,
    visible_height: i32,
    card_width: i32,
    card_height: i32,
    geometry: ui::PreviewGeometry,
    previous_elements: Vec<TextElement>,
    previous_height: u16,
    previous_min_height: u16,
    previous_selection: Option<usize>,
}

impl ElementDrag {
    fn apply(&self, point: POINT, style: &mut CardStyle) -> bool {
        let Some(e) = style.elements.get_mut(self.index) else {
            return false;
        };
        let scale = self.geometry.pixels_per_unit;
        let x = (self.x as i32 + ((point.x - self.start.x) as f64 / scale).round() as i32)
            .clamp(0, (self.card_width - self.visible_width).max(0)) as i16;
        let y = (self.y as i32 + ((point.y - self.start.y) as f64 / scale).round() as i32)
            .clamp(0, (self.card_height - self.visible_height).max(0)) as i16;
        let changed = (e.x, e.y) != (x, y);
        e.x = x;
        e.y = y;
        changed
    }

    fn restore(self, style: &mut CardStyle) {
        style.elements = self.previous_elements;
        style.height = self.previous_height;
        style.min_height = self.previous_min_height;
    }
}

struct Control {
    id: i32,
    hwnd: HWND,
    group: u8,
    bounds: RECT,
}
struct Designer {
    hwnd: Cell<HWND>,
    style: RefCell<CardStyle>,
    clock: Clock,
    zone: Zone,
    twelve: bool,
    seconds: bool,
    controls: RefCell<Vec<Control>>,
    font: Cell<HFONT>,
    loading: Cell<bool>,
    done: Cell<bool>,
    accepted: Cell<bool>,
    all: Cell<bool>,
    drag: Cell<Option<PreviewDrag>>,
    element_drag: RefCell<Option<ElementDrag>>,
    selected_element: Cell<Option<usize>>,
    element_boxes: RefCell<Vec<ui::ElementGeometry>>,
    refresh_gate: RefreshGate,
    timer_interval: Cell<u32>,
}
impl Designer {
    unsafe fn paint_preview(&self, dc: HDC, bounds: RECT) {
        unsafe {
            let style = self.style.borrow().clone();
            let dpi = GetDpiForWindow(self.hwnd.get()).max(96);
            let geometry = self.preview_geometry(bounds, &style, dpi);
            let elements = ui::preview_elements(
                &self.clock,
                &style,
                &self.zone,
                self.twelve,
                self.seconds,
                geometry,
            );
            ui::paint_buffered(dc, bounds, |buffer| {
                ui::paint_card_preview(
                    buffer,
                    bounds,
                    &self.clock,
                    &style,
                    &self.zone,
                    self.twelve,
                    self.seconds,
                    dpi,
                    geometry,
                );
                if let Some(selected) = self
                    .selected_element
                    .get()
                    .and_then(|index| elements.iter().find(|e| e.index == index))
                {
                    ui::paint_element_selection(buffer, selected.preview);
                }
            });
            *self.element_boxes.borrow_mut() = elements;
        }
    }

    unsafe fn sync_refresh_timer(&self) {
        unsafe {
            let interval = Cadence::for_style(&self.style.borrow(), self.seconds).timer_ms();
            if self.timer_interval.replace(interval) != interval {
                SetTimer(self.hwnd.get(), 1, interval, None);
            }
        }
    }

    fn control(&self, id: i32) -> HWND {
        self.controls
            .borrow()
            .iter()
            .find(|c| c.id == id)
            .map_or(null_mut(), |c| c.hwnd)
    }

    fn preview_geometry(&self, bounds: RECT, style: &CardStyle, dpi: u32) -> ui::PreviewGeometry {
        if let Some(drag) = self.drag.get() {
            return drag.geometry(style);
        }
        if let Some(drag) = self.element_drag.borrow().as_ref() {
            return drag.geometry;
        }
        ui::preview_geometry(bounds, style, dpi)
    }

    unsafe fn sync_dimensions(&self) {
        unsafe {
            let was_loading = self.loading.replace(true);
            let style = self.style.borrow().clone();
            SetWindowTextW(self.control(WIDTH), wide(&style.width.to_string()).as_ptr());
            SetWindowTextW(
                self.control(HEIGHT),
                wide(&style.height.to_string()).as_ptr(),
            );
            let info = self
                .selected_element
                .get()
                .and_then(|i| style.elements.get(i))
                .map_or_else(
                    || {
                        if style.face == ClockFace::Mechanical {
                            "drag edges to resize".into()
                        } else {
                            "drag text / edges".into()
                        }
                    },
                    |e| format!("text: x {}, y {}", e.x, e.y),
                );
            SetWindowTextW(
                self.control(SIZE_LABEL),
                wide(&format!("{} × {} px · {info}", style.width, style.height())).as_ptr(),
            );
            self.loading.set(was_loading);
            InvalidateRect(self.control(PREVIEW), null(), 0);
        }
    }

    unsafe fn update_drag(&self, point: POINT) {
        unsafe {
            let Some(drag) = self.drag.get() else {
                return;
            };
            let changed = drag.apply(point, &mut self.style.borrow_mut());
            if changed {
                self.sync_dimensions();
            }
        }
    }

    unsafe fn end_drag(&self, cancel: bool) {
        unsafe {
            let Some(drag) = self.drag.take() else {
                return;
            };
            if cancel {
                drag.restore(&mut self.style.borrow_mut());
            }
            self.sync_dimensions();
            if GetCapture() == self.control(PREVIEW) {
                ReleaseCapture();
            }
        }
    }

    unsafe fn preview_handle(&self, point: POINT) -> Option<ResizeHandle> {
        unsafe {
            if self.element_drag.borrow().is_some() {
                return None;
            }
            if let Some(drag) = self.drag.get() {
                return Some(drag.handle);
            }
            let mut bounds: RECT = zeroed();
            GetClientRect(self.control(PREVIEW), &mut bounds);
            let dpi = GetDpiForWindow(self.hwnd.get()).max(96);
            resize_hit(
                self.preview_geometry(bounds, &self.style.borrow(), dpi)
                    .card,
                point,
                ui::px(7, dpi),
            )
        }
    }

    unsafe fn preview_cursor(&self, point: POINT) -> bool {
        unsafe {
            if self.element_drag.borrow().is_some()
                || self.element_at(point).is_some() && self.preview_handle(point).is_none()
            {
                SetCursor(LoadCursorW(null_mut(), IDC_SIZEALL));
                return true;
            }
            let Some(handle) = self.preview_handle(point) else {
                return false;
            };
            let cursor = match (handle.x, handle.y) {
                (0, _) => IDC_SIZENS,
                (_, 0) => IDC_SIZEWE,
                (-1, -1) | (1, 1) => IDC_SIZENWSE,
                _ => IDC_SIZENESW,
            };
            SetCursor(LoadCursorW(null_mut(), cursor));
            true
        }
    }

    fn element_at(&self, point: POINT) -> Option<ui::ElementGeometry> {
        self.element_boxes
            .borrow()
            .iter()
            .rev()
            .find(|e| {
                point.x >= e.preview.left
                    && point.x <= e.preview.right
                    && point.y >= e.preview.top
                    && point.y <= e.preview.bottom
            })
            .copied()
    }

    unsafe fn begin_element_drag(&self, hit: ui::ElementGeometry, start: POINT) {
        unsafe {
            let mut bounds: RECT = zeroed();
            GetClientRect(self.control(PREVIEW), &mut bounds);
            let dpi = GetDpiForWindow(self.hwnd.get()).max(96);
            let mut style = self.style.borrow().clone();
            let geometry = ui::preview_geometry(bounds, &style, dpi);
            let previous_elements = style.elements.clone();
            let previous_height = style.height;
            let previous_min_height = style.min_height;
            let actual_height = style.height();
            let mut elements =
                ui::text_elements(&self.clock, &style, &self.zone, self.twelve, self.seconds);
            if hit.index >= elements.len() {
                return;
            }
            let element = elements.remove(hit.index);
            let x = element.x;
            let y = element.y;
            let index = elements.len();
            elements.push(element);
            let drag = ElementDrag {
                index,
                start,
                x,
                y,
                visible_width: hit.logical.right - hit.logical.left,
                visible_height: hit.logical.bottom - hit.logical.top,
                card_width: style.width as i32,
                card_height: actual_height,
                geometry,
                previous_elements,
                previous_height,
                previous_min_height,
                previous_selection: self.selected_element.get(),
            };
            style.elements = elements;
            style.height = actual_height.clamp(24, 700) as u16;
            style.min_height = 0;
            *self.style.borrow_mut() = style;
            *self.element_drag.borrow_mut() = Some(drag);
            self.selected_element.set(Some(index));
            self.sync_dimensions();
        }
    }

    unsafe fn update_element_drag(&self, point: POINT) {
        unsafe {
            let changed = {
                let drag = self.element_drag.borrow();
                let Some(drag) = drag.as_ref() else {
                    return;
                };
                drag.apply(point, &mut self.style.borrow_mut())
            };
            if changed {
                self.sync_dimensions();
            }
        }
    }

    unsafe fn end_element_drag(&self, cancel: bool) {
        unsafe {
            let Some(drag) = self.element_drag.borrow_mut().take() else {
                return;
            };
            if cancel {
                self.selected_element.set(drag.previous_selection);
                drag.restore(&mut self.style.borrow_mut());
            }
            self.sync_dimensions();
            if GetCapture() == self.control(PREVIEW) {
                ReleaseCapture();
            }
        }
    }

    fn interacting(&self) -> bool {
        self.drag.get().is_some() || self.element_drag.borrow().is_some()
    }

    unsafe fn end_interaction(&self, cancel: bool) {
        unsafe {
            self.end_drag(cancel);
            self.end_element_drag(cancel);
        }
    }
    #[allow(clippy::too_many_arguments)]
    unsafe fn add(
        &self,
        class: &str,
        title: &str,
        id: i32,
        group: u8,
        bounds: RECT,
        style: u32,
        ex: u32,
    ) -> HWND {
        unsafe {
            let dpi = GetDpiForWindow(self.hwnd.get()).max(96);
            let p = |v| ui::px(v, dpi);
            let hwnd = CreateWindowExW(
                ex,
                wide(class).as_ptr(),
                wide(title).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                p(bounds.left),
                p(bounds.top),
                p(bounds.right - bounds.left),
                p(bounds.bottom - bounds.top),
                self.hwnd.get(),
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(hwnd, WM_SETFONT, self.font.get() as usize, 1);
            self.controls.borrow_mut().push(Control {
                id,
                hwnd,
                group,
                bounds,
            });
            hwnd
        }
    }
    unsafe fn build(&self) {
        unsafe {
            let add = |class: &str,
                       title: &str,
                       id: i32,
                       group: u8,
                       x: i32,
                       y: i32,
                       width: i32,
                       height: i32,
                       style: u32,
                       ex: u32| {
                self.add(
                    class,
                    title,
                    id,
                    group,
                    ui::rect(x, y, width, height),
                    style,
                    ex,
                )
            };
            add("STATIC", "Design this clock", 0, 0, 24, 18, 600, 24, 0, 0);
            add(
                "STATIC",
                "Set the size, content, and colors of this clock.",
                0,
                0,
                24,
                48,
                390,
                22,
                0,
                0,
            );
            add("STATIC", "Clock type", 0, 0, 448, 48, 96, 24, 0, 0);
            let face = add(
                "COMBOBOX",
                "",
                FACE,
                0,
                548,
                43,
                268,
                120,
                CBS_DROPDOWNLIST as u32 | WS_TABSTOP,
                0,
            );
            for name in ["Digital", "Mechanical (hands)"] {
                SendMessageW(face, CB_ADDSTRING, 0, wide(name).as_ptr() as isize);
            }
            for (id, label, x) in [
                (TAB_LAYOUT, "Card", 24),
                (TAB_CONTENT, "Content", 120),
                (TAB_COLORS, "Colors", 216),
                (TAB_DIAL, "Dial", 312),
            ] {
                add(
                    "BUTTON",
                    label,
                    id,
                    0,
                    x,
                    84,
                    88,
                    30,
                    BS_AUTOCHECKBOX as u32 | BS_PUSHLIKE as u32 | WS_TABSTOP,
                    0,
                );
            }
            for (id, label, y, value) in [
                (WIDTH, "Width (120–600)", 140, 218),
                (HEIGHT, "Height (0 = auto)", 192, 0),
                (PADDING, "Inner padding (0–40)", 244, 10),
                (GAP, "Space between cards (0–40)", 296, 8),
                (RADIUS, "Corner radius (0–40)", 348, 11),
            ] {
                add("STATIC", label, 0, 1, 24, y + 4, 218, 24, 0, 0);
                add(
                    "EDIT",
                    &value.to_string(),
                    id,
                    1,
                    254,
                    y,
                    if id == HEIGHT { 88 } else { 140 },
                    30,
                    WS_TABSTOP | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32,
                    WS_EX_CLIENTEDGE,
                );
            }
            add(
                "BUTTON",
                "Auto",
                AUTO_HEIGHT,
                1,
                352,
                192,
                42,
                30,
                WS_TABSTOP,
                0,
            );
            add(
                "STATIC",
                "Drag text to move it, or blue handles to resize.\r\nChoose Auto to fit the height to the content.",
                0,
                1,
                24,
                412,
                370,
                48,
                0,
                0,
            );
            add("STATIC", "Preset", 0, 2, 24, 137, 80, 24, 0, 0);
            let preset = add(
                "COMBOBOX",
                "",
                PRESET,
                2,
                114,
                132,
                280,
                220,
                CBS_DROPDOWNLIST as u32 | WS_TABSTOP,
                0,
            );
            for name in [
                "Compact",
                "Time only",
                "One line",
                "Date and offset",
                "Custom",
            ] {
                SendMessageW(preset, CB_ADDSTRING, 0, wide(name).as_ptr() as isize);
            }
            add(
                "EDIT",
                "",
                TEMPLATE,
                2,
                24,
                176,
                370,
                108,
                WS_TABSTOP
                    | ES_MULTILINE as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_WANTRETURN as u32
                    | WS_VSCROLL,
                WS_EX_CLIENTEDGE,
            );
            SendMessageW(self.control(TEMPLATE), EM_SETLIMITTEXT, 2048, 0);
            add(
                "STATIC",
                "Use {name}, {time}, {date}, {offset}, {day}.\r\nDrag the text in Preview to place it.",
                0,
                2,
                24,
                292,
                370,
                40,
                0,
                0,
            );
            let fonts = installed_fonts();
            add(
                "STATIC",
                &format!("Font ({} installed)", fonts.len()),
                0,
                2,
                24,
                337,
                220,
                22,
                0,
                0,
            );
            let font = add(
                "COMBOBOX",
                "",
                FONT,
                2,
                24,
                361,
                210,
                190,
                CBS_DROPDOWN as u32 | CBS_AUTOHSCROLL as u32 | WS_TABSTOP | WS_VSCROLL,
                0,
            );
            for name in fonts {
                SendMessageW(font, CB_ADDSTRING, 0, wide(&name).as_ptr() as isize);
            }
            SendMessageW(
                font,
                CB_SETDROPPEDWIDTH,
                ui::px(325, GetDpiForWindow(self.hwnd.get()).max(96)) as usize,
                0,
            );
            add("STATIC", "Alignment", 0, 2, 252, 337, 140, 22, 0, 0);
            let align = add(
                "COMBOBOX",
                "",
                ALIGN,
                2,
                252,
                361,
                142,
                160,
                CBS_DROPDOWNLIST as u32 | WS_TABSTOP,
                0,
            );
            for name in ["Left", "Center", "Right"] {
                SendMessageW(align, CB_ADDSTRING, 0, wide(name).as_ptr() as isize);
            }
            add("STATIC", "Time size (10–72)", 0, 2, 24, 410, 130, 21, 0, 0);
            add(
                "STATIC",
                "Detail size (8–32)",
                0,
                2,
                166,
                410,
                130,
                21,
                0,
                0,
            );
            add(
                "EDIT",
                "30",
                TIME,
                2,
                24,
                435,
                104,
                28,
                ES_NUMBER as u32 | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
            );
            add(
                "EDIT",
                "11",
                DETAIL,
                2,
                166,
                435,
                104,
                28,
                ES_NUMBER as u32 | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
            );
            add(
                "BUTTON",
                "Bold time",
                BOLD,
                2,
                292,
                435,
                104,
                28,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            add(
                "BUTTON",
                "Reset text positions",
                RESET_POSITIONS,
                2,
                24,
                474,
                180,
                28,
                WS_TABSTOP,
                0,
            );
            add(
                "STATIC",
                "Editing rows or alignment resets their positions.",
                0,
                2,
                24,
                507,
                370,
                18,
                0,
                0,
            );
            for (name, color, alpha, y) in [
                ("Text", TEXT_COLOR, TEXT_ALPHA, 126),
                ("Text outline", OUTLINE_COLOR, OUTLINE_ALPHA, 204),
                ("Background", BG_COLOR, BG_ALPHA, 296),
                ("Card border", BORDER_COLOR, BORDER_ALPHA, 374),
            ] {
                add("STATIC", name, 0, 3, 24, y, 365, 20, 0, 0);
                add(
                    "BUTTON",
                    &format!("{name} color"),
                    color,
                    3,
                    24,
                    y + 24,
                    126,
                    30,
                    BS_OWNERDRAW as u32 | WS_TABSTOP,
                    0,
                );
                let slider = add(
                    "msctls_trackbar32",
                    "",
                    alpha,
                    3,
                    160,
                    y + 20,
                    164,
                    38,
                    TBS_AUTOTICKS | WS_TABSTOP,
                    0,
                );
                SendMessageW(slider, TBM_SETRANGE, 1, 100 << 16);
                SendMessageW(slider, TBM_SETTICFREQ, 25, 0);
                add("STATIC", "", alpha + 100, 3, 332, y + 28, 64, 22, 0, 0);
            }
            add("STATIC", "Width (0–4)", 0, 3, 24, 264, 105, 22, 0, 0);
            add(
                "EDIT",
                "1",
                OUTLINE_WIDTH,
                3,
                144,
                258,
                66,
                28,
                ES_NUMBER as u32 | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
            );
            add("STATIC", "Width (0–4)", 0, 3, 24, 434, 105, 22, 0, 0);
            add(
                "EDIT",
                "0",
                BORDER_WIDTH,
                3,
                144,
                428,
                66,
                28,
                ES_NUMBER as u32 | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
            );
            add("STATIC", "Numerals", 0, 4, 24, 140, 130, 24, 0, 0);
            let numerals = add(
                "COMBOBOX",
                "",
                NUMERALS,
                4,
                166,
                134,
                228,
                160,
                CBS_DROPDOWNLIST as u32 | WS_TABSTOP,
                0,
            );
            for name in ["None", "Arabic · 1, 2, 3", "Roman · I, II, III"] {
                SendMessageW(numerals, CB_ADDSTRING, 0, wide(name).as_ptr() as isize);
            }
            for (id, label, x, y) in [
                (TICKS, "Minute marks", 24, 184),
                (SECOND_HAND, "Second hand", 214, 184),
                (SWEEP, "Smooth sweep", 214, 222),
                (DIAL_LABEL, "Clock name", 24, 222),
                (DIAL_DATE, "Date", 24, 260),
            ] {
                add(
                    "BUTTON",
                    label,
                    id,
                    4,
                    x,
                    y,
                    180,
                    28,
                    BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                    0,
                );
            }
            add("STATIC", "Hand width (1–8)", 0, 4, 24, 306, 200, 24, 0, 0);
            add(
                "EDIT",
                "4",
                HAND_WIDTH,
                4,
                254,
                300,
                140,
                30,
                ES_NUMBER as u32 | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
            );
            for (id, title, y) in [
                (HOUR_COLOR, "Hour hand", 348),
                (MINUTE_COLOR, "Minute hand", 392),
                (SECOND_COLOR, "Second hand", 436),
            ] {
                add("STATIC", title, 0, 4, 24, y + 4, 210, 24, 0, 0);
                add(
                    "BUTTON",
                    title,
                    id,
                    4,
                    254,
                    y,
                    140,
                    30,
                    BS_OWNERDRAW as u32 | WS_TABSTOP,
                    0,
                );
            }
            add(
                "STATIC",
                "Colors sets the dial, rim, marks, and opacity.\r\nContent sets the font. Auto makes a round clock.",
                0,
                4,
                24,
                482,
                370,
                42,
                0,
                0,
            );
            add(
                "STATIC",
                "PREVIEW · SCALES TO FIT",
                0,
                0,
                448,
                90,
                368,
                24,
                0,
                0,
            );
            let preview = add(
                "STATIC",
                "Card preview. Drag the edges or corners to resize.",
                PREVIEW,
                0,
                448,
                124,
                368,
                276,
                0x100 | WS_TABSTOP, // SS_NOTIFY; painting is handled by preview_proc.
                0,
            );
            SetWindowSubclass(
                preview,
                Some(preview_proc),
                1,
                self as *const Designer as usize,
            );
            add("STATIC", "", SIZE_LABEL, 0, 448, 404, 368, 18, 0, 0);
            add(
                "BUTTON",
                "Apply this style to all clocks",
                ALL,
                0,
                448,
                424,
                368,
                28,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            add(
                "STATIC",
                "Use style returns to Settings.\r\nSave in Settings keeps your changes.",
                0,
                0,
                448,
                465,
                368,
                42,
                0,
                0,
            );
            add(
                "BUTTON",
                "Reset to compact",
                RESET,
                0,
                24,
                530,
                172,
                32,
                WS_TABSTOP,
                0,
            );
            add(
                "BUTTON", "Cancel", IDCANCEL, 0, 594, 530, 100, 32, WS_TABSTOP, 0,
            );
            add(
                "BUTTON",
                "Use style",
                IDOK,
                0,
                706,
                530,
                110,
                32,
                BS_DEFPUSHBUTTON as u32 | WS_TABSTOP,
                0,
            );
            self.sync();
            self.tab(1);
        }
    }
    unsafe fn tab(&self, group: u8) {
        unsafe {
            let controls = self.controls.borrow();
            for c in controls.iter() {
                if c.group != 0 {
                    ShowWindow(c.hwnd, if c.group == group { SW_SHOW } else { SW_HIDE });
                }
            }
            for (id, g) in [
                (TAB_LAYOUT, 1),
                (TAB_CONTENT, 2),
                (TAB_COLORS, 3),
                (TAB_DIAL, 4),
            ] {
                SendMessageW(
                    self.control(id),
                    BM_SETCHECK,
                    if group == g {
                        BST_CHECKED
                    } else {
                        BST_UNCHECKED
                    } as usize,
                    0,
                );
            }
        }
    }
    unsafe fn sync(&self) {
        unsafe {
            self.loading.set(true);
            let s = self.style.borrow().clone();
            for (id, value) in [
                (WIDTH, s.width as i32),
                (HEIGHT, s.height as i32),
                (PADDING, s.padding as i32),
                (GAP, s.gap as i32),
                (RADIUS, s.radius as i32),
                (TIME, s.time_size as i32),
                (DETAIL, s.detail_size as i32),
                (OUTLINE_WIDTH, s.outline_width as i32),
                (BORDER_WIDTH, s.border_width as i32),
                (HAND_WIDTH, s.dial.hand_width as i32),
            ] {
                SetWindowTextW(self.control(id), wide(&value.to_string()).as_ptr());
            }
            SetWindowTextW(
                self.control(TEMPLATE),
                wide(&s.template.replace('\n', "\r\n")).as_ptr(),
            );
            SetWindowTextW(self.control(FONT), wide(&s.font).as_ptr());
            SendMessageW(self.control(FACE), CB_SETCURSEL, s.face as usize, 0);
            SendMessageW(
                self.control(NUMERALS),
                CB_SETCURSEL,
                s.dial.numerals as usize,
                0,
            );
            for (id, checked) in [
                (TICKS, s.dial.ticks),
                (SECOND_HAND, s.dial.second_hand),
                (SWEEP, s.dial.sweep),
                (DIAL_LABEL, s.dial.label),
                (DIAL_DATE, s.dial.date),
            ] {
                SendMessageW(
                    self.control(id),
                    BM_SETCHECK,
                    if checked { BST_CHECKED } else { BST_UNCHECKED } as usize,
                    0,
                );
            }
            SendMessageW(self.control(ALIGN), CB_SETCURSEL, s.alignment as usize, 0);
            SendMessageW(
                self.control(BOLD),
                BM_SETCHECK,
                if s.bold { BST_CHECKED } else { BST_UNCHECKED } as usize,
                0,
            );
            SendMessageW(
                self.control(PRESET),
                CB_SETCURSEL,
                if s.template == CardStyle::default().template {
                    0
                } else if s.template == "{time}" {
                    1
                } else if s.template == "{name}  {time}" {
                    2
                } else if s.template == "{name}\n{time}\n{date} · {offset}" {
                    3
                } else {
                    4
                },
                0,
            );
            for (id, value) in [
                (TEXT_ALPHA, s.text_opacity),
                (OUTLINE_ALPHA, s.outline_opacity),
                (BG_ALPHA, s.background_opacity),
                (BORDER_ALPHA, s.border_opacity),
            ] {
                SendMessageW(self.control(id), TBM_SETPOS, 1, value as isize);
                SetWindowTextW(self.control(id + 100), wide(&format!("{value}%")).as_ptr());
            }
            for id in [
                TEXT_COLOR,
                OUTLINE_COLOR,
                BG_COLOR,
                BORDER_COLOR,
                HOUR_COLOR,
                MINUTE_COLOR,
                SECOND_COLOR,
                PREVIEW,
            ] {
                InvalidateRect(self.control(id), null(), 0);
            }
            self.sync_dimensions();
            self.sync_enabled();
            self.loading.set(false);
        }
    }
    unsafe fn read(&self) -> Result<CardStyle, String> {
        unsafe {
            let value = |id| {
                let hwnd = self.control(id);
                let len = GetWindowTextLengthW(hwnd);
                let mut text = vec![0u16; (len + 1) as usize];
                GetWindowTextW(hwnd, text.as_mut_ptr(), len + 1);
                unw(&text)
            };
            let number = |id| {
                value(id)
                    .parse::<u16>()
                    .map_err(|_| "Enter a whole number in each size field.".to_owned())
            };
            let small = |id| {
                number(id)
                    .and_then(|v| u8::try_from(v).map_err(|_| "Size is too large.".to_owned()))
            };
            let mut s = self.style.borrow().clone();
            s.width = number(WIDTH)?;
            s.height = number(HEIGHT)?;
            s.min_height = 0;
            s.padding = small(PADDING)?;
            s.gap = small(GAP)?;
            s.radius = small(RADIUS)?;
            s.time_size = small(TIME)?;
            s.detail_size = small(DETAIL)?;
            s.outline_width = small(OUTLINE_WIDTH)?;
            s.border_width = small(BORDER_WIDTH)?;
            s.face = if SendMessageW(self.control(FACE), CB_GETCURSEL, 0, 0) == 1 {
                ClockFace::Mechanical
            } else {
                ClockFace::Digital
            };
            s.dial.numerals = match SendMessageW(self.control(NUMERALS), CB_GETCURSEL, 0, 0) {
                1 => Numerals::Arabic,
                2 => Numerals::Roman,
                _ => Numerals::None,
            };
            let checked =
                |id| SendMessageW(self.control(id), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
            s.dial.ticks = checked(TICKS);
            s.dial.second_hand = checked(SECOND_HAND);
            s.dial.sweep = checked(SWEEP);
            s.dial.label = checked(DIAL_LABEL);
            s.dial.date = checked(DIAL_DATE);
            s.dial.hand_width = small(HAND_WIDTH)?;
            s.font = value(FONT);
            let template = value(TEMPLATE).replace("\r\n", "\n");
            let alignment = SendMessageW(self.control(ALIGN), CB_GETCURSEL, 0, 0).max(0) as u8;
            if s.template != template || s.alignment != alignment {
                s.elements.clear();
            }
            s.template = template;
            s.alignment = alignment;
            s.bold = SendMessageW(self.control(BOLD), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
            s.text_opacity = SendMessageW(self.control(TEXT_ALPHA), TBM_GETPOS, 0, 0) as u8;
            s.outline_opacity = SendMessageW(self.control(OUTLINE_ALPHA), TBM_GETPOS, 0, 0) as u8;
            s.background_opacity = SendMessageW(self.control(BG_ALPHA), TBM_GETPOS, 0, 0) as u8;
            s.border_opacity = SendMessageW(self.control(BORDER_ALPHA), TBM_GETPOS, 0, 0) as u8;
            s.validate()?;
            Ok(s)
        }
    }
    unsafe fn refresh(&self) {
        unsafe {
            if let Ok(s) = self.read() {
                if s.elements.is_empty() {
                    self.selected_element.set(None);
                }
                *self.style.borrow_mut() = s;
                for id in [TEXT_ALPHA, OUTLINE_ALPHA, BG_ALPHA, BORDER_ALPHA] {
                    let v = SendMessageW(self.control(id), TBM_GETPOS, 0, 0);
                    SetWindowTextW(self.control(id + 100), wide(&format!("{v}%")).as_ptr());
                }
                self.sync_dimensions();
                self.sync_enabled();
            }
        }
    }
    unsafe fn sync_enabled(&self) {
        unsafe {
            let style = self.style.borrow();
            let mechanical = style.face == ClockFace::Mechanical;
            for id in [PRESET, TEMPLATE, TIME, ALIGN, RESET_POSITIONS] {
                EnableWindow(self.control(id), !mechanical as i32);
            }
            for id in [
                NUMERALS,
                TICKS,
                SECOND_HAND,
                DIAL_LABEL,
                DIAL_DATE,
                HAND_WIDTH,
                HOUR_COLOR,
                MINUTE_COLOR,
            ] {
                EnableWindow(self.control(id), mechanical as i32);
            }
            for id in [SWEEP, SECOND_COLOR] {
                EnableWindow(
                    self.control(id),
                    (mechanical && style.dial.second_hand) as i32,
                );
            }
        }
        unsafe { self.sync_refresh_timer() };
    }
    unsafe fn choose_color(&self, id: i32) {
        unsafe {
            let style = self.style.borrow().clone();
            let current = match id {
                TEXT_COLOR => style.text_color,
                OUTLINE_COLOR => style.outline_color,
                BG_COLOR => style.background_color,
                HOUR_COLOR => style.dial.hour_color,
                MINUTE_COLOR => style.dial.minute_color,
                SECOND_COLOR => style.dial.second_color,
                _ => style.border_color,
            };
            let mut custom = [0u32; 16];
            let mut chooser: CHOOSECOLORW = zeroed();
            chooser.lStructSize = size_of::<CHOOSECOLORW>() as u32;
            chooser.hwndOwner = self.hwnd.get();
            chooser.rgbResult = ui::colorref(current);
            chooser.lpCustColors = custom.as_mut_ptr();
            chooser.Flags = CC_FULLOPEN | CC_RGBINIT;
            if ChooseColorW(&mut chooser) != 0 {
                let color = ui::colorref(chooser.rgbResult);
                {
                    let mut s = self.style.borrow_mut();
                    match id {
                        TEXT_COLOR => s.text_color = color,
                        OUTLINE_COLOR => s.outline_color = color,
                        BG_COLOR => s.background_color = color,
                        HOUR_COLOR => s.dial.hour_color = color,
                        MINUTE_COLOR => s.dial.minute_color = color,
                        SECOND_COLOR => s.dial.second_color = color,
                        _ => s.border_color = color,
                    }
                }
                InvalidateRect(self.control(id), null(), 1);
                InvalidateRect(self.control(PREVIEW), null(), 0);
            }
        }
    }
}

unsafe extern "system" fn preview_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let d = &*(data as *const Designer);
        let point = || POINT {
            x: lparam as i16 as i32,
            y: (lparam >> 16) as i16 as i32,
        };
        let screen = || {
            let mut p: POINT = zeroed();
            GetCursorPos(&mut p);
            p
        };
        let event_screen = || {
            let mut p = point();
            ClientToScreen(hwnd, &mut p);
            p
        };
        match msg {
            // The complete background is part of the buffered frame. Bypass the
            // STATIC control's paint path so it cannot erase between frames.
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut paint: PAINTSTRUCT = zeroed();
                let dc = BeginPaint(hwnd, &mut paint);
                let mut bounds: RECT = zeroed();
                GetClientRect(hwnd, &mut bounds);
                d.paint_preview(dc, bounds);
                EndPaint(hwnd, &paint);
                0
            }
            WM_PRINTCLIENT => {
                let mut bounds: RECT = zeroed();
                GetClientRect(hwnd, &mut bounds);
                d.paint_preview(wparam as HDC, bounds);
                0
            }
            WM_NCHITTEST => HTCLIENT as isize,
            WM_SETCURSOR => {
                let mut p = screen();
                ScreenToClient(hwnd, &mut p);
                if d.preview_cursor(p) {
                    1
                } else {
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
            }
            WM_LBUTTONDOWN => {
                let p = point();
                SetFocus(hwnd);
                if let Some(handle) = d.preview_handle(p) {
                    let mut bounds: RECT = zeroed();
                    GetClientRect(hwnd, &mut bounds);
                    let dpi = GetDpiForWindow(hwnd).max(96);
                    let style = d.style.borrow().clone();
                    d.drag.set(Some(PreviewDrag::new(
                        handle,
                        event_screen(),
                        ui::preview_geometry(bounds, &style, dpi),
                        &style,
                    )));
                    SetCapture(hwnd);
                    d.preview_cursor(p);
                    0
                } else if let Some(element) = d.element_at(p) {
                    d.begin_element_drag(element, event_screen());
                    SetCapture(hwnd);
                    d.preview_cursor(p);
                    0
                } else {
                    d.selected_element.set(None);
                    InvalidateRect(hwnd, null(), 0);
                    DefSubclassProc(hwnd, msg, wparam, lparam)
                }
            }
            WM_MOUSEMOVE => {
                if GetCapture() == hwnd {
                    let p = event_screen();
                    d.update_drag(p);
                    d.update_element_drag(p);
                }
                d.preview_cursor(point());
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            WM_LBUTTONUP if d.interacting() => {
                let p = event_screen();
                d.update_drag(p);
                d.update_element_drag(p);
                d.end_interaction(false);
                0
            }
            WM_CAPTURECHANGED => {
                d.end_interaction(false);
                0
            }
            WM_CANCELMODE => {
                d.end_interaction(true);
                0
            }
            WM_GETDLGCODE if d.interacting() => DLGC_WANTALLKEYS as isize,
            WM_KEYDOWN if wparam == VK_ESCAPE as usize && d.interacting() => {
                d.end_interaction(true);
                0
            }
            WM_NCDESTROY => {
                d.drag.set(None);
                d.element_drag.borrow_mut().take();
                RemoveWindowSubclass(hwnd, Some(preview_proc), id);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
    }
}

pub unsafe fn edit(
    owner: HWND,
    clock: &Clock,
    zone: &Zone,
    twelve: bool,
    seconds: bool,
) -> Option<(CardStyle, bool)> {
    unsafe {
        let instance = GetModuleHandleW(null());
        let class = w!("WorldClock.Appearance.v3");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(designer_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            lpszClassName: class,
            ..zeroed()
        };
        RegisterClassW(&wc);
        let dpi = if owner.is_null() {
            GetDpiForSystem()
        } else {
            GetDpiForWindow(owner)
        }
        .max(96);
        let mut bounds = ui::rect(0, 0, ui::px(840, dpi), ui::px(582, dpi));
        let flags = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        AdjustWindowRectExForDpi(
            &mut bounds,
            flags,
            0,
            WS_EX_DLGMODALFRAME
                | WS_EX_CONTROLPARENT
                | if owner.is_null() { WS_EX_APPWINDOW } else { 0 },
            dpi,
        );
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        let monitor = if owner.is_null() {
            MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)
        } else {
            MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST)
        };
        GetMonitorInfoW(monitor, &mut mi);
        let mut initial_style = clock.style.clone();
        if initial_style.height == 0 && initial_style.min_height > 0 {
            initial_style.height = initial_style.height().clamp(24, 700) as u16;
            initial_style.min_height = 0;
        }
        let mut designer = Box::new(Designer {
            hwnd: Cell::new(null_mut()),
            style: RefCell::new(initial_style),
            clock: clock.clone(),
            zone: zone.clone(),
            twelve,
            seconds,
            controls: RefCell::new(Vec::new()),
            font: Cell::new(ui::font(ui::px(13, dpi), 400, "Segoe UI")),
            loading: Cell::new(true),
            done: Cell::new(false),
            accepted: Cell::new(false),
            all: Cell::new(false),
            drag: Cell::new(None),
            element_drag: RefCell::new(None),
            selected_element: Cell::new(None),
            element_boxes: RefCell::new(Vec::new()),
            refresh_gate: RefreshGate::default(),
            timer_interval: Cell::new(0),
        });
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME
                | WS_EX_CONTROLPARENT
                | if owner.is_null() { WS_EX_APPWINDOW } else { 0 },
            class,
            wide(&format!("Appearance — {}", clock.label)).as_ptr(),
            flags,
            mi.rcWork.left + (mi.rcWork.right - mi.rcWork.left - width) / 2,
            mi.rcWork.top + (mi.rcWork.bottom - mi.rcWork.top - height) / 2,
            width,
            height,
            owner,
            null_mut(),
            instance,
            (&mut *designer as *mut Designer).cast(),
        );
        if hwnd.is_null() {
            DeleteObject(designer.font.get());
            return None;
        }
        designer.hwnd.set(hwnd);
        designer.build();
        EnableWindow(owner, 0);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        ShowWindow(hwnd, SW_SHOW);
        designer.sync_refresh_timer();
        SetForegroundWindow(hwnd);
        let mut msg: MSG = zeroed();
        while !designer.done.get() {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result <= 0 {
                if result == 0 {
                    PostQuitMessage(msg.wParam as i32);
                }
                break;
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if IsWindow(hwnd) != 0 {
            DestroyWindow(hwnd);
        }
        EnableWindow(owner, 1);
        SetForegroundWindow(owner);
        DeleteObject(designer.font.get());
        if designer.accepted.get() {
            Some((designer.style.borrow().clone(), designer.all.get()))
        } else {
            None
        }
    }
}

unsafe extern "system" fn designer_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Designer;
        if pointer.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let d = &*pointer;
        match msg {
            WM_COMMAND if !d.loading.get() => {
                let id = (wparam & 0xffff) as i32;
                let code = ((wparam >> 16) & 0xffff) as u32;
                match id {
                    TAB_LAYOUT => d.tab(1),
                    TAB_CONTENT => d.tab(2),
                    TAB_COLORS => d.tab(3),
                    TAB_DIAL => d.tab(4),
                    FACE if code == CBN_SELCHANGE => {
                        d.end_interaction(false);
                        d.refresh();
                        let mut s = d.style.borrow_mut();
                        s.height = 0;
                        s.min_height = 0;
                        drop(s);
                        d.selected_element.set(None);
                        d.sync();
                        if d.style.borrow().face == ClockFace::Mechanical {
                            d.tab(4);
                        }
                    }
                    AUTO_HEIGHT => {
                        let mut s = d.style.borrow_mut();
                        s.height = 0;
                        s.min_height = 0;
                        drop(s);
                        d.sync_dimensions();
                    }
                    RESET_POSITIONS => {
                        d.style.borrow_mut().elements.clear();
                        d.selected_element.set(None);
                        d.sync_dimensions();
                    }
                    TEXT_COLOR | OUTLINE_COLOR | BG_COLOR | BORDER_COLOR | HOUR_COLOR
                    | MINUTE_COLOR | SECOND_COLOR => d.choose_color(id),
                    RESET => {
                        *d.style.borrow_mut() = CardStyle::default();
                        d.selected_element.set(None);
                        d.sync();
                    }
                    PRESET if code == CBN_SELCHANGE => {
                        let preset = SendMessageW(d.control(PRESET), CB_GETCURSEL, 0, 0);
                        if preset < 4 {
                            let mut s = d.style.borrow_mut();
                            s.template = match preset {
                                0 => CardStyle::default().template,
                                1 => "{time}".into(),
                                2 => "{name}  {time}".into(),
                                _ => "{name}\n{time}\n{date} · {offset}".into(),
                            };
                            s.time_size = if preset == 2 { 18 } else { 30 };
                            s.elements.clear();
                            d.selected_element.set(None);
                            drop(s);
                            d.sync();
                        }
                    }
                    IDOK => match d.read() {
                        Ok(s) => {
                            *d.style.borrow_mut() = s;
                            d.all.set(
                                SendMessageW(d.control(ALL), BM_GETCHECK, 0, 0)
                                    == BST_CHECKED as isize,
                            );
                            d.accepted.set(true);
                            DestroyWindow(hwnd);
                        }
                        Err(message) => {
                            MessageBoxW(
                                hwnd,
                                wide(&message).as_ptr(),
                                w!("Appearance"),
                                MB_OK | MB_ICONINFORMATION,
                            );
                        }
                    },
                    IDCANCEL => {
                        DestroyWindow(hwnd);
                    }
                    _ => {
                        if matches!(
                            code,
                            EN_CHANGE | CBN_SELCHANGE | CBN_EDITCHANGE | BN_CLICKED
                        ) {
                            d.refresh();
                        }
                    }
                }
                0
            }
            WM_HSCROLL if !d.loading.get() => {
                d.refresh();
                0
            }
            WM_DRAWITEM => {
                let item = &*(lparam as *const DRAWITEMSTRUCT);
                let s = d.style.borrow().clone();
                let color = match item.CtlID as i32 {
                    TEXT_COLOR => s.text_color,
                    OUTLINE_COLOR => s.outline_color,
                    BG_COLOR => s.background_color,
                    HOUR_COLOR => s.dial.hour_color,
                    MINUTE_COLOR => s.dial.minute_color,
                    SECOND_COLOR => s.dial.second_color,
                    BORDER_COLOR => s.border_color,
                    _ => return 0,
                };
                ui::fill(item.hDC, &item.rcItem, ui::colorref(color));
                let brightness =
                    ((color >> 16) & 255) * 299 + ((color >> 8) & 255) * 587 + (color & 255) * 114;
                ui::text(
                    item.hDC,
                    &format!("#{color:06X}"),
                    item.rcItem,
                    ui::px(12, GetDpiForWindow(hwnd).max(96)),
                    600,
                    if brightness > 145000 { 0 } else { 0xffffff },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                if item.itemState & ODS_FOCUS != 0 {
                    DrawFocusRect(item.hDC, &item.rcItem);
                }
                1
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                SetBkColor(wparam as HDC, GetSysColor(COLOR_WINDOW));
                SetTextColor(wparam as HDC, ui::rgb(30, 38, 48));
                GetSysColorBrush(COLOR_WINDOW) as isize
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, 1);
                d.done.set(true);
                0
            }
            WM_TIMER => {
                let cadence =
                    if IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 && !d.interacting() {
                        Some(Cadence::for_style(&d.style.borrow(), d.seconds))
                    } else {
                        None
                    };
                if d.refresh_gate.due(cadence, &crate::time::utc_now()) {
                    InvalidateRect(d.control(PREVIEW), null(), 0);
                }
                0
            }
            WM_TIMECHANGE | WM_SETTINGCHANGE | WM_FONTCHANGE | WM_THEMECHANGED => {
                ui::invalidate_caches();
                d.refresh_gate.invalidate();
                InvalidateRect(d.control(PREVIEW), null(), 0);
                0
            }
            WM_DPICHANGED => {
                let r = &*(lparam as *const RECT);
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let dpi = GetDpiForWindow(hwnd).max(96);
                let font = ui::font(ui::px(13, dpi), 400, "Segoe UI");
                for c in d.controls.borrow().iter() {
                    let r = c.bounds;
                    SetWindowPos(
                        c.hwnd,
                        null_mut(),
                        ui::px(r.left, dpi),
                        ui::px(r.top, dpi),
                        ui::px(r.right - r.left, dpi),
                        ui::px(r.bottom - r.top, dpi),
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    SendMessageW(c.hwnd, WM_SETFONT, font as usize, 1);
                }
                DeleteObject(d.font.replace(font));
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_font_list_is_complete_sorted_and_deduplicated() {
        let fonts = installed_fonts();
        println!("Installed font families: {}", fonts.len());
        assert!(fonts.len() > 5);
        assert!(fonts.iter().any(|f| f == "Segoe UI"));
        assert!(
            fonts
                .windows(2)
                .all(|pair| pair[0].to_lowercase() < pair[1].to_lowercase())
        );
        assert!(
            fonts
                .iter()
                .all(|name| !name.starts_with('@') && !name.is_empty())
        );
    }

    #[test]
    fn resize_handles_cover_all_edges_and_corners() {
        let r = ui::rect(20, 30, 218, 90);
        for (x, y, expected) in [
            (20, 30, (-1, -1)),
            (129, 30, (0, -1)),
            (238, 30, (1, -1)),
            (20, 75, (-1, 0)),
            (238, 75, (1, 0)),
            (20, 120, (-1, 1)),
            (129, 120, (0, 1)),
            (238, 120, (1, 1)),
        ] {
            assert_eq!(
                resize_hit(r, POINT { x, y }, 7),
                Some(ResizeHandle {
                    x: expected.0,
                    y: expected.1
                })
            );
        }
        assert_eq!(resize_hit(r, POINT { x: 100, y: 70 }, 7), None);
        assert_eq!(resize_hit(r, POINT { x: 5, y: 30 }, 7), None);
    }
    #[test]
    fn older_styles_keep_their_height_and_gain_auto_layout_defaults() {
        let style: CardStyle =
            serde_json::from_str(r#"{"width":218,"min_height":160,"background_color":2379571}"#)
                .unwrap();
        assert_eq!(style.height, 0);
        assert_eq!(style.height(), 160);
        assert!(style.elements.is_empty());
        assert_eq!(style.background_color, 2379571);
        assert!(style.validate().is_ok());
    }

    #[test]
    fn resize_tracks_pointer_at_preview_scale_and_cancels_to_auto() {
        let original = CardStyle::default();
        let mut style = original.clone();
        let geometry = ui::PreviewGeometry {
            card: ui::rect(50, 60, 109, 45),
            pixels_per_unit: 0.5,
        };
        let drag = PreviewDrag::new(
            ResizeHandle { x: 1, y: 1 },
            POINT { x: 100, y: 100 },
            geometry,
            &style,
        );
        drag.apply(POINT { x: 125, y: 115 }, &mut style);
        assert_eq!((style.width, style.height), (268, 120));
        let r = drag.geometry(&style).card;
        assert_eq!((r.left, r.top, r.right, r.bottom), (50, 60, 184, 120));
        drag.restore(&mut style);
        assert_eq!(style, original);
    }

    #[test]
    fn top_left_resize_keeps_opposite_corner_and_width_keeps_auto_height() {
        let mut style = CardStyle::default();
        let geometry = ui::PreviewGeometry {
            card: ui::rect(100, 100, 436, 180),
            pixels_per_unit: 2.0,
        };
        let drag = PreviewDrag::new(
            ResizeHandle { x: -1, y: -1 },
            POINT { x: 600, y: 500 },
            geometry,
            &style,
        );
        drag.apply(POINT { x: 560, y: 460 }, &mut style);
        assert_eq!((style.width, style.height), (238, 110));
        let r = drag.geometry(&style).card;
        assert_eq!((r.left, r.top, r.right, r.bottom), (60, 60, 536, 280));
        let mut auto = CardStyle::default();
        let horizontal = PreviewDrag::new(
            ResizeHandle { x: 1, y: 0 },
            POINT { x: 0, y: 0 },
            geometry,
            &auto,
        );
        horizontal.apply(POINT { x: 100, y: 1000 }, &mut auto);
        assert_eq!((auto.width, auto.height), (268, 0));
    }

    #[test]
    fn resizing_respects_limits_and_supports_exact_small_heights() {
        let mut style = CardStyle::default();
        let geometry = ui::PreviewGeometry {
            card: ui::rect(0, 0, 218, 90),
            pixels_per_unit: 1.0,
        };
        let drag = PreviewDrag::new(
            ResizeHandle { x: 1, y: 1 },
            POINT { x: 0, y: 0 },
            geometry,
            &style,
        );
        drag.apply(POINT { x: 10000, y: 10000 }, &mut style);
        assert_eq!((style.width, style.height), (600, 700));
        assert!(style.validate().is_ok());
        drag.apply(
            POINT {
                x: -10000,
                y: -10000,
            },
            &mut style,
        );
        assert_eq!((style.width, style.height()), (120, 24));
        assert!(style.validate().is_ok());
    }

    #[test]
    fn text_drag_uses_zoom_bounds_and_can_restore_automatic_layout() {
        let original = CardStyle::default();
        let mut style = CardStyle {
            height: 90,
            elements: vec![TextElement {
                text: "{time}".into(),
                x: 10,
                y: 26,
                large: true,
            }],
            ..original.clone()
        };
        let drag = ElementDrag {
            index: 0,
            start: POINT { x: 0, y: 0 },
            x: 10,
            y: 26,
            visible_width: 100,
            visible_height: 38,
            card_width: 218,
            card_height: 90,
            geometry: ui::PreviewGeometry {
                card: ui::rect(0, 0, 109, 45),
                pixels_per_unit: 0.5,
            },
            previous_elements: Vec::new(),
            previous_height: 0,
            previous_min_height: 0,
            previous_selection: None,
        };
        drag.apply(POINT { x: 10, y: -5 }, &mut style);
        assert_eq!((style.elements[0].x, style.elements[0].y), (30, 16));
        drag.apply(POINT { x: 10000, y: 10000 }, &mut style);
        assert_eq!((style.elements[0].x, style.elements[0].y), (118, 52));
        drag.apply(
            POINT {
                x: -10000,
                y: -10000,
            },
            &mut style,
        );
        assert_eq!((style.elements[0].x, style.elements[0].y), (0, 0));
        drag.restore(&mut style);
        assert_eq!(style, original);
    }
}
