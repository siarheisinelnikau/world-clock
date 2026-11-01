//! Native control colors. Rendering remains event driven and uses Windows GDI.
use serde::{Deserialize, Serialize};
use std::{
    mem::size_of,
    ptr::null,
    sync::atomic::{AtomicU8, Ordering},
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        UI::{Controls::*, Input::KeyboardAndMouse::GetFocus, WindowsAndMessaging::*},
    },
    core::w,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Theme {
    Dark = 1,
    #[default]
    #[serde(other)]
    Light = 0,
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
unsafe extern "system" fn apply_child(hwnd: HWND, _: LPARAM) -> i32 {
    unsafe {
        apply_control(hwnd);
    }
    1
}
pub unsafe fn apply_control(hwnd: HWND) {
    unsafe {
        let mut class = [0u16; 64];
        let length = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        if String::from_utf16_lossy(&class[..length.max(0) as usize])
            .eq_ignore_ascii_case("ComboBox")
            && GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & 3 == CBS_DROPDOWNLIST as u32
        {
            SetWindowSubclass(hwnd, Some(combo_proc), 1, 0);
        }
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

unsafe fn paint_combo(hwnd: HWND, dc: HDC) {
    unsafe {
        let mut bounds = std::mem::zeroed();
        GetClientRect(hwnd, &mut bounds);
        SetDCBrushColor(dc, surface());
        FillRect(dc, &bounds, GetStockObject(DC_BRUSH) as HBRUSH);
        let focused = GetFocus() == hwnd;
        SetDCPenColor(
            dc,
            if focused {
                GetSysColor(COLOR_HIGHLIGHT)
            } else if current() == Theme::Dark {
                0x6A625A
            } else {
                0xB4ACA4
            },
        );
        let old_pen = SelectObject(dc, GetStockObject(DC_PEN));
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        Rectangle(dc, 0, 0, bounds.right, bounds.bottom);
        SelectObject(dc, old_brush);
        let mut content = bounds;
        content.left += 3;
        content.right -= 24;
        content.top += 1;
        content.bottom -= 1;
        let selected = SendMessageW(hwnd, CB_GETCURSEL, 0, 0);
        let item = DRAWITEMSTRUCT {
            CtlType: ODT_COMBOBOX,
            CtlID: GetDlgCtrlID(hwnd) as u32,
            itemID: if selected < 0 {
                u32::MAX
            } else {
                selected as u32
            },
            itemState: ODS_COMBOBOXEDIT,
            hwndItem: hwnd,
            hDC: dc,
            rcItem: content,
            ..std::mem::zeroed()
        };
        draw_combo(&item);
        SetDCPenColor(dc, foreground());
        let x = bounds.right - 13;
        let y = bounds.bottom / 2;
        MoveToEx(dc, x - 4, y - 2, std::ptr::null_mut());
        LineTo(dc, x, y + 2);
        LineTo(dc, x + 4, y - 2);
        SelectObject(dc, old_pen);
    }
}

unsafe extern "system" fn combo_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut paint = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut paint);
                paint_combo(hwnd, dc);
                EndPaint(hwnd, &paint);
                0
            }
            WM_PRINT | WM_PRINTCLIENT => {
                paint_combo(hwnd, wparam as HDC);
                0
            }
            WM_ERASEBKGND => 1,
            WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE => {
                let result = DefSubclassProc(hwnd, msg, wparam, lparam);
                InvalidateRect(hwnd, null(), 0);
                result
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(combo_proc), 1);
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
            _ => DefSubclassProc(hwnd, msg, wparam, lparam),
        }
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

/// Native string-backed combos retain keyboard navigation and accessibility while
/// their selected value and dropdown rows share the application palette.
pub unsafe fn draw_combo(item: &DRAWITEMSTRUCT) -> bool {
    unsafe {
        if item.CtlType != ODT_COMBOBOX {
            return false;
        }
        let selected = item.itemState & ODS_SELECTED != 0 && item.itemState & ODS_COMBOBOXEDIT == 0;
        let background = if selected {
            GetSysColor(COLOR_HIGHLIGHT)
        } else {
            surface()
        };
        let foreground = if selected {
            GetSysColor(COLOR_HIGHLIGHTTEXT)
        } else {
            foreground()
        };
        SetDCBrushColor(item.hDC, background);
        FillRect(item.hDC, &item.rcItem, GetStockObject(DC_BRUSH) as HBRUSH);
        if item.itemID != u32::MAX {
            let length = SendMessageW(item.hwndItem, CB_GETLBTEXTLEN, item.itemID as usize, 0);
            if length >= 0 {
                let mut text = vec![0u16; length as usize + 1];
                SendMessageW(
                    item.hwndItem,
                    CB_GETLBTEXT,
                    item.itemID as usize,
                    text.as_mut_ptr() as isize,
                );
                SetTextColor(item.hDC, foreground);
                SetBkColor(item.hDC, background);
                let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
                let old_font = if font.is_null() {
                    std::ptr::null_mut()
                } else {
                    SelectObject(item.hDC, font)
                };
                let mut bounds = item.rcItem;
                bounds.left += 5;
                bounds.right -= 2;
                DrawTextW(
                    item.hDC,
                    text.as_ptr(),
                    length as i32,
                    &mut bounds,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
                if !old_font.is_null() {
                    SelectObject(item.hDC, old_font);
                }
            }
        }
        if item.itemState & ODS_FOCUS != 0 {
            DrawFocusRect(item.hDC, &item.rcItem);
        }
        true
    }
}
