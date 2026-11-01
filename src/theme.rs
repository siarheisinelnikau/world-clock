//! Native control colors. Rendering remains event driven and uses Windows GDI.
use serde::{Deserialize, Serialize};
use std::{
    mem::size_of,
    ptr::null,
    sync::atomic::{AtomicU8, Ordering},
};
use windows_sys::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        UI::{Controls::SetWindowTheme, WindowsAndMessaging::*},
    },
    core::w,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
static CURRENT: AtomicU8 = AtomicU8::new(0);
pub fn set_theme(theme: Theme) {
    CURRENT.store(theme as u8, Ordering::Relaxed);
}
pub fn current() -> Theme {
    if CURRENT.load(Ordering::Relaxed) == 0 {
        Theme::Light
    } else {
        Theme::Dark
    }
}
pub fn background() -> u32 {
    if current() == Theme::Dark {
        0x211D1A
    } else {
        0xFAF8F6
    }
}
pub fn foreground() -> u32 {
    if current() == Theme::Dark {
        0xF4F0EA
    } else {
        0x30261E
    }
}
pub fn surface() -> u32 {
    if current() == Theme::Dark {
        0x302B26
    } else {
        0xFFFFFF
    }
}

/// Uses stock DC brushes, avoiding a GDI allocation for every paint.
pub unsafe fn control_color(msg: u32, dc: HDC) -> HBRUSH {
    unsafe {
        let color = if matches!(msg, WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX) {
            surface()
        } else {
            background()
        };
        SetTextColor(dc, foreground());
        SetBkColor(dc, color);
        SetDCBrushColor(dc, color);
        GetStockObject(DC_BRUSH) as HBRUSH
    }
}
pub unsafe fn erase(hwnd: HWND, dc: HDC) -> LRESULT {
    unsafe {
        let mut bounds = std::mem::zeroed();
        GetClientRect(hwnd, &mut bounds);
        SetDCBrushColor(dc, background());
        FillRect(dc, &bounds, GetStockObject(DC_BRUSH) as HBRUSH);
        1
    }
}
unsafe extern "system" fn apply_child(hwnd: HWND, _: LPARAM) -> BOOL {
    unsafe {
        apply_control(hwnd);
    }
    1
}
pub unsafe fn apply_control(hwnd: HWND) {
    unsafe {
        SetWindowTheme(
            hwnd,
            if current() == Theme::Dark {
                w!("DarkMode_Explorer")
            } else {
                w!("Explorer")
            },
            null(),
        );
        InvalidateRect(hwnd, null(), 1);
    }
}
pub unsafe fn apply_window(hwnd: HWND) {
    unsafe {
        let dark = i32::from(current() == Theme::Dark);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&dark as *const i32).cast(),
            size_of::<i32>() as u32,
        );
        EnumChildWindows(hwnd, Some(apply_child), 0);
        RedrawWindow(
            hwnd,
            null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_FRAME,
        );
    }
}
