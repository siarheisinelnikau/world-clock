//! One isolated native Settings render, saved to PNG, with no user settings or background services.
use crate::{
    i18n::{LANGUAGES, Language},
    theme::Theme,
    time::wide,
};
use std::{
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Foundation::HWND,
        Graphics::{
            Gdi::*,
            GdiPlus::{
                GdipCreateBitmapFromHBITMAP, GdipDisposeImage, GdipSaveImageToFile,
                GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, Ok as GDIP_OK,
            },
        },
        UI::WindowsAndMessaging::*,
    },
    core::GUID,
};

pub struct Capture {
    pub path: PathBuf,
    pub taskbar: bool,
    pub language: Language,
    pub theme: Theme,
}
impl Capture {
    pub fn from_args() -> Result<Option<Self>, String> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if !args
            .iter()
            .any(|arg| arg == "--capture-settings" || arg == "--capture-taskbar")
        {
            return Ok(None);
        }
        if args.len() != 6
            || (args[0] != "--capture-settings" && args[0] != "--capture-taskbar")
            || args[2] != "--language"
            || args[4] != "--theme"
        {
            return Err("Usage: --capture-settings|--capture-taskbar <absolute-output.png> --language <en|ru|fr|de|es|it> --theme <light|dark>".into());
        }
        let path = PathBuf::from(&args[1]);
        if !path.is_absolute() || path.extension().is_none_or(|value| value != "png") {
            return Err("Capture requires an absolute PNG output path.".into());
        }
        let index = ["en", "ru", "fr", "de", "es", "it"]
            .iter()
            .position(|code| args[3] == *code)
            .ok_or("Unsupported capture language.")?;
        let theme = if args[5] == "light" {
            Theme::Light
        } else if args[5] == "dark" {
            Theme::Dark
        } else {
            return Err("Unsupported capture theme.".into());
        };
        Ok(Some(Self {
            path,
            taskbar: args[0] == "--capture-taskbar",
            language: LANGUAGES[index],
            theme,
        }))
    }
    pub unsafe fn save(&self, hwnd: HWND) -> Result<(), String> {
        unsafe {
            if hwnd.is_null() {
                return Err("Settings capture window was not created.".into());
            }
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut bounds = std::mem::zeroed();
            GetClientRect(hwnd, &mut bounds);
            let dc = GetDC(hwnd);
            let memory = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, bounds.right, bounds.bottom);
            if dc.is_null() || memory.is_null() || bitmap.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                if !memory.is_null() {
                    DeleteDC(memory);
                }
                if !dc.is_null() {
                    ReleaseDC(hwnd, dc);
                }
                return Err("Could not allocate a native Settings capture bitmap.".into());
            }
            let previous = SelectObject(memory, bitmap);
            crate::theme::erase(hwnd, memory);
            // DefWindowProc prints child coordinates relative to the outer window.
            // Move that origin back to the client so native borders never clip the bottom row.
            let mut client_origin = std::mem::zeroed();
            ClientToScreen(hwnd, &mut client_origin);
            let mut window_bounds = std::mem::zeroed();
            GetWindowRect(hwnd, &mut window_bounds);
            SetViewportOrgEx(
                memory,
                window_bounds.left - client_origin.x,
                window_bounds.top - client_origin.y,
                null_mut(),
            );
            // Native controls use their normal WM_PRINT client painting and parent colors.
            SendMessageW(
                hwnd,
                WM_PRINT,
                memory as usize,
                (PRF_CLIENT | PRF_CHILDREN | PRF_ERASEBKGND) as isize,
            );
            SelectObject(memory, previous);
            let result = save_bitmap(bitmap, &self.path);
            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(hwnd, dc);
            result?;
            println!(
                "Captured {} × {} native Settings client pixels to {}",
                bounds.right,
                bounds.bottom,
                self.path.display()
            );
            std::result::Result::Ok(())
        }
    }
}

/// Save an application-owned bitmap through the native PNG encoder.
pub unsafe fn save_bitmap(bitmap: HBITMAP, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    unsafe {
        let mut token = 0;
        let startup = GdiplusStartupInput {
            GdiplusVersion: 1,
            SuppressExternalCodecs: 1,
            ..Default::default()
        };
        let mut status = GdiplusStartup(&mut token, &startup, null_mut());
        if status == GDIP_OK {
            let mut image = null_mut();
            status = GdipCreateBitmapFromHBITMAP(bitmap, null_mut(), &mut image);
            if status == GDIP_OK {
                let encoder = GUID {
                    data1: 0x557cf406,
                    data2: 0x1a04,
                    data3: 0x11d3,
                    data4: [0x9a, 0x73, 0, 0, 0xf8, 0x1e, 0xf3, 0x2e],
                };
                let temporary = path.with_extension("pending.png");
                status = GdipSaveImageToFile(
                    image.cast(),
                    wide(&temporary.to_string_lossy()).as_ptr(),
                    &encoder,
                    null(),
                );
                GdipDisposeImage(image.cast());
            }
            GdiplusShutdown(token);
        }

        if status != GDIP_OK {
            return Err(format!("Native PNG capture failed (GDI+ status {status})."));
        }
        std::fs::rename(path.with_extension("pending.png"), path)
            .map_err(|error| error.to_string())?;
        std::result::Result::Ok(())
    }
}
