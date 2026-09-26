//! Locate the taskbar and notification area independently of Windows' clock visibility.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::Duration,
};
use windows::Win32::{Foundation::HWND as WinHwnd, System::Com::*, UI::Accessibility::*};
use windows_sys::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::w,
};

#[derive(Clone, Copy)]
pub struct Location {
    pub bar: RECT,
    pub tray: RECT,
    pub buttons_right: i32,
    pub foreground: u32,
}

pub struct Tracker {
    pub rx: Receiver<Option<Location>>,
    stop: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
struct Geometry {
    bar: RECT,
    tray: Option<RECT>,
    buttons_right: i32,
}

impl Geometry {
    fn new(bar: RECT) -> Self {
        Self {
            bar,
            tray: None,
            buttons_right: bar.left,
        }
    }

    fn include_tray(&mut self, r: RECT) {
        let clipped = RECT {
            left: r.left.max(self.bar.left),
            top: r.top.max(self.bar.top),
            right: r.right.min(self.bar.right),
            bottom: r.bottom.min(self.bar.bottom),
        };
        if clipped.right <= clipped.left || clipped.bottom <= clipped.top {
            return;
        }
        self.tray = Some(self.tray.map_or(clipped, |old| RECT {
            left: old.left.min(clipped.left),
            top: old.top.min(clipped.top),
            right: old.right.max(clipped.right),
            bottom: old.bottom.max(clipped.bottom),
        }));
    }

    fn observe(&mut self, id: &str, class: &str, r: RECT) {
        if r.right <= r.left
            || r.bottom <= r.top
            || r.bottom <= self.bar.top
            || r.top >= self.bar.bottom
        {
            return;
        }
        if class.contains("TaskListButton")
            || matches!(
                id,
                "startbutton" | "searchbutton" | "taskviewbutton" | "widgetsbutton"
            )
        {
            self.buttons_right = self.buttons_right.max(r.right.min(self.bar.right));
        }
        if class == "TrayNotifyWnd"
            || class.starts_with("SystemTray.")
            || id.starts_with("systemtray")
        {
            self.include_tray(r);
        }
    }

    fn finish(mut self, native_tray: RECT, foreground: u32) -> Location {
        self.include_tray(native_tray);
        let tray = self.tray.unwrap_or(RECT {
            left: self.bar.right,
            top: self.bar.top,
            right: self.bar.right,
            bottom: self.bar.bottom,
        });
        Location {
            bar: self.bar,
            tray,
            buttons_right: self.buttons_right,
            foreground,
        }
    }
}
impl Tracker {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        thread::spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: Option<IUIAutomation> =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
            while !stopping.load(Ordering::Relaxed) {
                let location = locate(automation.as_ref());
                if tx.send(location).is_err() {
                    break;
                }
                for _ in 0..10 {
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
            drop(automation);
            CoUninitialize();
        });
        Self { rx, stop }
    }
}
impl Drop for Tracker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

unsafe fn locate(automation: Option<&IUIAutomation>) -> Option<Location> {
    unsafe {
        let bar = FindWindowW(w!("Shell_TrayWnd"), std::ptr::null());
        if bar.is_null() || IsWindowVisible(bar) == 0 {
            return None;
        }
        let mut bounds: RECT = std::mem::zeroed();
        GetWindowRect(bar, &mut bounds);
        if bounds.right - bounds.left <= bounds.bottom - bounds.top {
            return None;
        }
        let tray_window = FindWindowExW(
            bar,
            std::ptr::null_mut(),
            w!("TrayNotifyWnd"),
            std::ptr::null(),
        );
        let mut tray: RECT = std::mem::zeroed();
        if !tray_window.is_null() && IsWindowVisible(tray_window) != 0 {
            GetWindowRect(tray_window, &mut tray);
        }
        let mut geometry = Geometry::new(bounds);
        let automation = automation?;
        let root = automation.ElementFromHandle(WinHwnd(bar)).ok()?;
        let elements = root
            .FindAll(
                TreeScope_Descendants,
                &automation.CreateTrueCondition().ok()?,
            )
            .ok()?;
        for i in 0..elements.Length().ok()?.min(512) {
            let Ok(e) = elements.GetElement(i) else {
                continue;
            };
            let id = e
                .CurrentAutomationId()
                .map(|s| s.to_string())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let class = e
                .CurrentClassName()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let Ok(r) = e.CurrentBoundingRectangle() else {
                continue;
            };
            geometry.observe(
                &id,
                &class,
                RECT {
                    left: r.left,
                    top: r.top,
                    right: r.right,
                    bottom: r.bottom,
                },
            );
        }
        Some(geometry.finish(tray, if system_light() { 0x181818 } else { 0xf5f5f5 }))
    }
}

pub fn lane(location: Location, count: usize, dpi: u32) -> Option<(RECT, usize)> {
    let scale = |v: i32| ((v as i64 * dpi as i64 + 48) / 96) as i32;
    let right = location.tray.left - scale(8);
    let free = (right - location.buttons_right - scale(12)).max(0);
    let minimum = count.min(3);
    if minimum == 0 || free < scale(58) * minimum as i32 {
        return None;
    }
    let slot = scale(84).min(free / minimum as i32);
    let slots = count.min((free / slot) as usize);
    Some((
        RECT {
            left: right - slot * slots as i32,
            top: location.bar.top,
            right,
            bottom: location.bar.bottom,
        },
        slots,
    ))
}

fn system_light() -> bool {
    use windows_sys::Win32::System::Registry::*;
    unsafe {
        let mut value = 0u32;
        let mut size = 4;
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        );
        value != 0
    }
}

pub fn inspect() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).unwrap();
        let bar = FindWindowW(w!("Shell_TrayWnd"), std::ptr::null());
        let mut report = String::new();
        if let Ok(root) = automation.ElementFromHandle(WinHwnd(bar)) {
            let all = root
                .FindAll(
                    TreeScope_Descendants,
                    &automation.CreateTrueCondition().unwrap(),
                )
                .unwrap();
            for i in 0..all.Length().unwrap().min(512) {
                if let Ok(e) = all.GetElement(i) {
                    let id = e
                        .CurrentAutomationId()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let class = e
                        .CurrentClassName()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let r = e.CurrentBoundingRectangle().unwrap_or_default();
                    report.push_str(&format!(
                        "{i}: {id:?} {class:?} ({},{})-({},{})\n",
                        r.left, r.top, r.right, r.bottom
                    ));
                }
            }
        }
        if let Some(l) = locate(Some(&automation)) {
            report.push_str(&format!(
                "Located taskbar: ({},{})-({},{}); tray starts at {}; app buttons end at {}\n",
                l.bar.left, l.bar.top, l.bar.right, l.bar.bottom, l.tray.left, l.buttons_right
            ));
        }
        let _ = std::fs::write("taskbar-diagnostics.log", report);
        drop(automation);
        CoUninitialize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taskbar() -> RECT {
        RECT {
            left: 0,
            top: 1032,
            right: 1920,
            bottom: 1080,
        }
    }
    fn without_clock() -> Geometry {
        let mut geometry = Geometry::new(taskbar());
        geometry.observe(
            "startbutton",
            "ToggleButton",
            RECT {
                left: 0,
                top: 1032,
                right: 55,
                bottom: 1080,
            },
        );
        geometry.observe(
            "window: app",
            "Taskbar.TaskListButtonAutomationPeer",
            RECT {
                left: 55,
                top: 1032,
                right: 1226,
                bottom: 1080,
            },
        );
        geometry.observe(
            "systemtrayicon",
            "SystemTray.NormalButton",
            RECT {
                left: 1740,
                top: 1032,
                right: 1772,
                bottom: 1080,
            },
        );
        geometry.observe(
            "systemtrayicon",
            "SystemTray.OmniButtonRight",
            RECT {
                left: 1848,
                top: 1032,
                right: 1872,
                bottom: 1080,
            },
        );
        geometry
    }

    #[test]
    fn hidden_windows_clock_does_not_hide_selected_clocks() {
        let geometry = without_clock();
        let location = geometry.finish(
            RECT {
                left: 1740,
                top: 1032,
                right: 1920,
                bottom: 1080,
            },
            0xffffff,
        );
        let (r, slots) = lane(location, 4, 96).unwrap();
        assert_eq!(slots, 4);
        assert!(r.left > 1226 && r.right < 1740);
        let mut with_clock = geometry;
        with_clock.observe(
            "systemtrayicon",
            "SystemTray.OmniButton",
            RECT {
                left: 1806,
                top: 1032,
                right: 1876,
                bottom: 1080,
            },
        );
        let enabled = with_clock.finish(location.tray, 0xffffff);
        let (enabled_rect, enabled_slots) = lane(enabled, 4, 96).unwrap();
        assert_eq!(enabled_slots, slots);
        assert_eq!((enabled_rect.left, enabled_rect.right), (r.left, r.right));
    }

    #[test]
    fn tray_icons_are_enough_without_native_clock_or_tray_wrapper() {
        let location = without_clock().finish(unsafe { std::mem::zeroed() }, 0xffffff);
        assert_eq!(location.tray.left, 1740);
        assert_eq!(lane(location, 4, 96).unwrap().1, 4);
    }

    #[test]
    fn hidden_entire_system_area_uses_taskbar_edge() {
        let mut geometry = Geometry::new(taskbar());
        geometry.observe(
            "window: app",
            "Taskbar.TaskListButtonAutomationPeer",
            RECT {
                left: 55,
                top: 1032,
                right: 1226,
                bottom: 1080,
            },
        );
        let location = geometry.finish(unsafe { std::mem::zeroed() }, 0xffffff);
        assert_eq!(location.tray.left, 1920);
        let (r, slots) = lane(location, 4, 96).unwrap();
        assert_eq!(slots, 4);
        assert!(r.left > 1226 && r.right < 1920);
    }
    #[test]
    fn three_clocks_fit_without_covering_tray_or_apps() {
        let l = Location {
            bar: RECT {
                left: 0,
                top: 1032,
                right: 1920,
                bottom: 1080,
            },
            tray: RECT {
                left: 1694,
                top: 1032,
                right: 1920,
                bottom: 1080,
            },
            buttons_right: 980,
            foreground: 0xffffff,
        };
        for dpi in [96, 120, 144, 192] {
            let (r, n) = lane(l, 3, dpi).unwrap();
            assert_eq!(n, 3);
            assert!(r.left > l.buttons_right && r.right < l.tray.left);
            assert_eq!((r.top, r.bottom), (l.bar.top, l.bar.bottom));
        }
        let tight = Location {
            buttons_right: 1450,
            ..l
        };
        let (r, n) = lane(tight, 3, 96).unwrap();
        assert_eq!(n, 3);
        assert!(r.left > tight.buttons_right && r.right < tight.tray.left);
    }
}
