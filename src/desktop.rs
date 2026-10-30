use std::{cell::Cell, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::*,
    },
};

thread_local! {
    static PANEL: Cell<HWND> = const { Cell::new(null_mut()) };
}

// Show Desktop raises Explorer's desktop windows above ordinary popups. Keep the
// panel immediately above that desktop, preserving applications above the panel.
pub unsafe fn keep_above_desktop(panel: HWND) {
    unsafe {
        if IsWindowVisible(panel) == 0
            || GetWindowLongPtrW(panel, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0
        {
            return;
        }
        let shell = GetShellWindow();
        if shell.is_null() {
            return;
        }
        let mut shell_pid = 0;
        GetWindowThreadProcessId(shell, &mut shell_pid);
        let mut anchor = null_mut();
        let mut window = GetWindow(panel, GW_HWNDPREV);
        while !window.is_null() {
            let mut pid = 0;
            GetWindowThreadProcessId(window, &mut pid);
            if pid == shell_pid && IsWindowVisible(window) != 0 {
                let mut class = [0u16; 64];
                let len = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
                let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
                if window == shell || class == "WorkerW" || class == "Progman" {
                    anchor = window;
                }
            }
            window = GetWindow(window, GW_HWNDPREV);
        }
        if !anchor.is_null() {
            lift_above(panel, anchor);
        }
    }
}

unsafe fn lift_above(panel: HWND, anchor: HWND) {
    unsafe {
        let previous = GetWindow(anchor, GW_HWNDPREV);
        SetWindowPos(
            panel,
            if previous.is_null() {
                HWND_TOP
            } else {
                previous
            },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

pub struct DesktopGuard {
    hooks: [HWINEVENTHOOK; 2],
}

impl DesktopGuard {
    pub unsafe fn start(panel: HWND) -> Self {
        unsafe {
            PANEL.with(|target| target.set(panel));
            let hooks = [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_REORDER].map(|event| {
                SetWinEventHook(
                    event,
                    event,
                    null_mut(),
                    Some(desktop_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            });
            Self { hooks }
        }
    }
}

impl Drop for DesktopGuard {
    fn drop(&mut self) {
        unsafe {
            for hook in self.hooks {
                if !hook.is_null() {
                    UnhookWinEvent(hook);
                }
            }
        }
        PANEL.with(|target| target.set(null_mut()));
    }
}

unsafe extern "system" fn desktop_event(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    PANEL.with(|target| unsafe { keep_above_desktop(target.get()) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::core::w;

    #[test]
    fn desktop_repair_preserves_app_order_position_and_input_styles() {
        unsafe {
            struct Window(HWND);
            impl Drop for Window {
                fn drop(&mut self) {
                    unsafe {
                        DestroyWindow(self.0);
                    }
                }
            }
            let create = || {
                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                    w!("STATIC"),
                    w!("World Clock desktop stacking test"),
                    WS_POPUP,
                    -30000,
                    -30000,
                    1,
                    1,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    std::ptr::null(),
                );
                assert!(!hwnd.is_null());
                SetLayeredWindowAttributes(hwnd, 0, 1, LWA_ALPHA);
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                Window(hwnd)
            };
            let panel = create();
            let desktop = create();
            let application = create();
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
            SetWindowPos(panel.0, HWND_TOP, 0, 0, 0, 0, flags);
            SetWindowPos(desktop.0, HWND_TOP, 0, 0, 0, 0, flags);
            SetWindowPos(application.0, HWND_TOP, 0, 0, 0, 0, flags);
            let foreground = GetForegroundWindow();
            let style = GetWindowLongPtrW(panel.0, GWL_EXSTYLE);
            lift_above(panel.0, desktop.0);
            assert_eq!(GetWindow(panel.0, GW_HWNDNEXT), desktop.0);
            assert_eq!(GetWindow(panel.0, GW_HWNDPREV), application.0);
            assert_eq!(GetWindowLongPtrW(panel.0, GWL_EXSTYLE), style);
            assert_eq!(GetForegroundWindow(), foreground);
            let mut bounds = std::mem::zeroed();
            GetWindowRect(panel.0, &mut bounds);
            assert_eq!((bounds.left, bounds.top), (-30000, -30000));
            assert_eq!(IsIconic(panel.0), 0);

            // Exercise real Explorer discovery too when a desktop is available
            // (headless CI may not have one), moving only our offscreen panel.
            let shell = GetShellWindow();
            if !shell.is_null() && IsWindowVisible(shell) != 0 {
                SetWindowPos(panel.0, HWND_BOTTOM, 0, 0, 0, 0, flags);
                keep_above_desktop(panel.0);
                let mut window = GetWindow(panel.0, GW_HWNDNEXT);
                while !window.is_null() && window != shell {
                    window = GetWindow(window, GW_HWNDNEXT);
                }
                assert_eq!(
                    window, shell,
                    "clock panel must be above Explorer's desktop"
                );
                ShowWindow(panel.0, SW_HIDE);
                keep_above_desktop(panel.0);
                assert_eq!(
                    IsWindowVisible(panel.0),
                    0,
                    "hidden clocks must stay hidden"
                );
            }
        }
    }
}
