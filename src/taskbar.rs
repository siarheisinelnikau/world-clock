//! Locate the taskbar and notification area independently of Windows' clock visibility.
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::HWND as WinHwnd,
    System::Com::*,
    UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Descendants},
};
use windows_sys::{
    Win32::{
        Foundation::*,
        Graphics::Dwm::*,
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

#[derive(Clone, Copy)]
pub struct Location {
    pub bar: RECT,
    pub tray: RECT,
    pub buttons_end: i32,
    pub foreground: u32,
}

impl Location {
    pub fn vertical(self) -> bool {
        self.bar.bottom - self.bar.top > self.bar.right - self.bar.left
    }
}

pub fn same_rect(a: RECT, b: RECT) -> bool {
    (a.left, a.top, a.right, a.bottom) == (b.left, b.top, b.right, b.bottom)
}

pub fn same_location(a: Option<Location>, b: Option<Location>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            same_rect(a.bar, b.bar)
                && same_rect(a.tray, b.tray)
                && a.buttons_end == b.buttons_end
                && a.foreground == b.foreground
        }
        (None, None) => true,
        _ => false,
    }
}

#[derive(Default)]
struct LocationCache {
    last: Option<Location>,
    misses: u8,
}

impl LocationCache {
    fn observe(&mut self, location: Option<Location>) -> Option<Location> {
        if location.is_some() {
            self.last = location;
            self.misses = 0;
        } else {
            // Explorer can briefly rebuild its accessibility tree during activation.
            self.misses = self.misses.saturating_add(1);
            if self.misses >= 3 {
                self.last = None;
            }
        }
        self.last
    }
}

pub struct Tracker {
    pub rx: Receiver<Option<Location>>,
    stop: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
struct Geometry {
    bar: RECT,
    tray: Option<RECT>,
    buttons_end: i32,
    saw_buttons: bool,
}

impl Geometry {
    fn new(bar: RECT) -> Self {
        Self {
            bar,
            tray: None,
            buttons_end: if bar.bottom - bar.top > bar.right - bar.left {
                bar.top
            } else {
                bar.left
            },
            saw_buttons: false,
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
            || r.right <= self.bar.left
            || r.left >= self.bar.right
        {
            return;
        }
        if class.contains("TaskListButton")
            || matches!(
                id,
                "startbutton" | "searchbutton" | "taskviewbutton" | "widgetsbutton"
            )
        {
            let vertical = self.bar.bottom - self.bar.top > self.bar.right - self.bar.left;
            let end = if vertical {
                r.bottom.min(self.bar.bottom)
            } else {
                r.right.min(self.bar.right)
            };
            self.buttons_end = self.buttons_end.max(end);
            self.saw_buttons = true;
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
        let vertical = self.bar.bottom - self.bar.top > self.bar.right - self.bar.left;
        let tray = self.tray.unwrap_or(RECT {
            left: if vertical {
                self.bar.left
            } else {
                self.bar.right
            },
            top: if vertical {
                self.bar.bottom
            } else {
                self.bar.top
            },
            right: self.bar.right,
            bottom: self.bar.bottom,
        });
        Location {
            bar: self.bar,
            tray,
            buttons_end: self.buttons_end,
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
            let mut cache = LocationCache::default();
            while !stopping.load(Ordering::Relaxed) {
                let location = cache.observe(locate(automation.as_ref()));
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
        let (bar, bounds) = native_bar()?;
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
        if !geometry.saw_buttons || !same_rect(bounds, native_bar()?.1) {
            return None;
        }
        Some(geometry.finish(tray, if system_light() { 0x181818 } else { 0xf5f5f5 }))
    }
}

pub unsafe fn native_bar() -> Option<(HWND, RECT)> {
    unsafe {
        let hwnd = FindWindowW(w!("Shell_TrayWnd"), std::ptr::null());
        let mut bounds: RECT = std::mem::zeroed();
        if hwnd.is_null()
            || IsWindowVisible(hwnd) == 0
            || GetWindowRect(hwnd, &mut bounds) == 0
            || bounds.right <= bounds.left
            || bounds.bottom <= bounds.top
        {
            None
        } else {
            Some((hwnd, bounds))
        }
    }
}

pub unsafe fn attach_overlay_owner(overlay: HWND, owner: HWND) -> bool {
    unsafe {
        if IsWindow(overlay) == 0 || IsWindow(owner) == 0 {
            return false;
        }
        if GetWindow(overlay, GW_OWNER) != owner {
            // A popup owned by the taskbar stays above it even when Explorer raises it.
            SetWindowLongPtrW(overlay, GWLP_HWNDPARENT, owner as isize);
        }
        GetWindow(overlay, GW_OWNER) == owner
    }
}

pub unsafe fn above(upper: HWND, lower: HWND) -> bool {
    unsafe {
        if upper.is_null() || lower.is_null() || upper == lower {
            return false;
        }
        let mut window = GetWindow(lower, GW_HWNDPREV);
        for _ in 0..2048 {
            if window.is_null() {
                break;
            }
            if window == upper {
                return true;
            }
            window = GetWindow(window, GW_HWNDPREV);
        }
        false
    }
}

thread_local! {
    static GUARDED_OVERLAY: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
}

pub struct StackingGuard {
    overlay: HWND,
    hooks: [HWINEVENTHOOK; 2],
}

impl StackingGuard {
    pub unsafe fn start(overlay: HWND) -> Self {
        unsafe { Self::with_flags(overlay, WINEVENT_SKIPOWNPROCESS) }
    }

    unsafe fn with_flags(overlay: HWND, flags: u32) -> Self {
        unsafe {
            GUARDED_OVERLAY.with(|target| target.set(overlay));
            let hooks = [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_REORDER].map(|event| {
                SetWinEventHook(
                    event,
                    event,
                    std::ptr::null_mut(),
                    Some(stacking_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | flags,
                )
            });
            Self { overlay, hooks }
        }
    }
}

impl Drop for StackingGuard {
    fn drop(&mut self) {
        unsafe {
            for hook in self.hooks {
                if !hook.is_null() {
                    UnhookWinEvent(hook);
                }
            }
        }
        GUARDED_OVERLAY.with(|target| {
            if target.get() == self.overlay {
                target.set(std::ptr::null_mut());
            }
        });
    }
}

unsafe extern "system" fn stacking_event(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    unsafe {
        GUARDED_OVERLAY.with(|target| {
            let overlay = target.get();
            let owner = GetWindow(overlay, GW_OWNER);
            if IsWindowVisible(overlay) != 0 && above(owner, overlay) {
                // No painting, activation, or timer delay: repair only our popup's order.
                SetWindowPos(
                    overlay,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
        });
    }
}

// Read-only diagnostic: records visibility and stacking, without titles or user settings.
pub fn watch_overlay() {
    unsafe {
        use std::io::Write;
        let Ok(mut log) = std::fs::File::create("taskbar-stacking.log") else {
            return;
        };
        let start = Instant::now();
        let mut previous = String::new();
        let mut hidden = 0;
        let mut covered = 0;
        let mut samples = 0;
        while start.elapsed() < Duration::from_secs(20) {
            let overlay = FindWindowW(w!("WorldClock.Taskbar.v2"), std::ptr::null());
            let bar = FindWindowW(w!("Shell_TrayWnd"), std::ptr::null());
            let visible = IsWindowVisible(overlay) != 0;
            let behind = above(bar, overlay);
            let owner_matches = !bar.is_null() && GetWindow(overlay, GW_OWNER) == bar;
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(overlay, &mut r);
            let mut cloaked = 0u32;
            DwmGetWindowAttribute(
                overlay,
                DWMWA_CLOAKED as u32,
                (&mut cloaked as *mut u32).cast(),
                4,
            );
            let snapshot = format!(
                "visible={visible} behind_taskbar={behind} taskbar_owner={owner_matches} cloaked={cloaked} bounds=({},{})-({},{})",
                r.left, r.top, r.right, r.bottom
            );
            if snapshot != previous {
                let _ = writeln!(log, "{}ms {snapshot}", start.elapsed().as_millis());
                let _ = log.flush();
                previous = snapshot;
            }
            samples += 1;
            hidden += usize::from(!visible);
            covered += usize::from(behind && visible);
            thread::sleep(Duration::from_millis(10));
        }
        let _ = writeln!(
            log,
            "samples={samples} hidden={hidden} covered_by_taskbar={covered}"
        );
    }
}

pub fn visible_thickness(bar: RECT, screen: RECT) -> i32 {
    let width = (bar.right.min(screen.right) - bar.left.max(screen.left)).max(0);
    let height = (bar.bottom.min(screen.bottom) - bar.top.max(screen.top)).max(0);
    width.min(height)
}

pub fn fullscreen_bounds(front: RECT, screen: RECT, normal_maximized: bool, shell: bool) -> bool {
    !normal_maximized
        && !shell
        && front.left <= screen.left
        && front.top <= screen.top
        && front.right >= screen.right
        && front.bottom >= screen.bottom
}

pub fn lane(location: Location, count: usize, dpi: u32) -> Option<(RECT, usize)> {
    let scale = |v: i32| ((v as i64 * dpi as i64 + 48) / 96) as i32;
    let vertical = location.vertical();
    let end = if vertical {
        location.tray.top
    } else {
        location.tray.left
    } - scale(8);
    let free = (end - location.buttons_end - scale(12)).max(0);
    let minimum = count.min(3);
    let minimum_slot = scale(if vertical { 40 } else { 58 });
    if minimum == 0 || free < minimum_slot * minimum as i32 {
        return None;
    }
    // Fit as many clocks as possible before using the preferred, wider cells.
    let slots = count.min((free / minimum_slot) as usize);
    let slot = scale(if vertical { 48 } else { 84 }).min(free / slots as i32);
    Some((
        if vertical {
            RECT {
                left: location.bar.left,
                top: end - slot * slots as i32,
                right: location.bar.right,
                bottom: end,
            }
        } else {
            RECT {
                left: end - slot * slots as i32,
                top: location.bar.top,
                right: end,
                bottom: location.bar.bottom,
            }
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
        let overlay = FindWindowW(w!("WorldClock.Taskbar.v2"), std::ptr::null());
        report.push_str(&format!(
            "Overlay has taskbar owner: {}; above taskbar: {}\n",
            !bar.is_null() && GetWindow(overlay, GW_OWNER) == bar,
            above(overlay, bar)
        ));
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
                "Located taskbar: ({},{})-({},{}); vertical: {}; tray starts at {}; app buttons end at {}\n",
                l.bar.left, l.bar.top, l.bar.right, l.bar.bottom, l.vertical(),
                if l.vertical() { l.tray.top } else { l.tray.left }, l.buttons_end
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

    #[test]
    fn owning_the_overlay_prevents_taskbar_activation_from_covering_it() {
        unsafe {
            let popup = || {
                CreateWindowExW(
                    WS_EX_TOPMOST
                        | WS_EX_TOOLWINDOW
                        | WS_EX_NOACTIVATE
                        | WS_EX_LAYERED
                        | WS_EX_TRANSPARENT,
                    w!("STATIC"),
                    w!("World Clock stacking test"),
                    WS_POPUP,
                    0,
                    0,
                    40,
                    40,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            let owner = popup();
            let overlay = popup();
            assert!(!owner.is_null() && !overlay.is_null());
            // Exercise visible-window stacking without drawing over the user's desktop.
            for hwnd in [owner, overlay] {
                SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA);
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;

            // Reproduce the old behavior: raising an independent panel covers the clock.
            SetWindowPos(overlay, HWND_TOPMOST, 0, 0, 0, 0, flags);
            SetWindowPos(owner, HWND_TOPMOST, 0, 0, 0, 0, flags);
            assert!(above(owner, overlay));

            assert!(attach_overlay_owner(overlay, owner));
            SetWindowPos(overlay, HWND_TOPMOST, 0, 0, 0, 0, flags);
            let foreground = GetForegroundWindow();
            for _ in 0..32 {
                SetWindowPos(owner, HWND_TOPMOST, 0, 0, 0, 0, flags & !SWP_NOOWNERZORDER);
                assert!(
                    above(overlay, owner),
                    "owner={owner:?} overlay={overlay:?} owner_ref={:?} overlay_prev={:?} owner_prev={:?}",
                    GetWindow(overlay, GW_OWNER),
                    GetWindow(overlay, GW_HWNDPREV),
                    GetWindow(owner, GW_HWNDPREV)
                );
                assert_eq!(GetWindow(overlay, GW_OWNER), owner);
                assert_eq!(GetForegroundWindow(), foreground);
            }
            // Explicit owner reordering bypasses automatic owned-popup ordering.
            // The out-of-context notification repairs it without the 250 ms timer.
            let guard = StackingGuard::with_flags(overlay, 0);
            assert!(guard.hooks.iter().all(|hook| !hook.is_null()));
            for _ in 0..8 {
                SetWindowPos(owner, HWND_TOPMOST, 0, 0, 0, 0, flags);
                assert!(above(owner, overlay));
                let start = Instant::now();
                while above(owner, overlay) && start.elapsed() < Duration::from_millis(100) {
                    let mut msg: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    thread::yield_now();
                }
                assert!(above(overlay, owner));
                assert_eq!(GetForegroundWindow(), foreground);
            }
            drop(guard);
            assert_ne!(
                GetWindowLongPtrW(overlay, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                0
            );
            DestroyWindow(owner);
            assert_eq!(IsWindow(overlay), 0);
        }
    }

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
            buttons_end: 980,
            foreground: 0xffffff,
        };
        for dpi in [96, 120, 144, 192] {
            let (r, n) = lane(l, 3, dpi).unwrap();
            assert_eq!(n, 3);
            assert!(r.left > l.buttons_end && r.right < l.tray.left);
            assert_eq!((r.top, r.bottom), (l.bar.top, l.bar.bottom));
        }
        let tight = Location {
            buttons_end: 1450,
            ..l
        };
        let (r, n) = lane(tight, 3, 96).unwrap();
        assert_eq!(n, 3);
        assert!(r.left > tight.buttons_end && r.right < tight.tray.left);
    }

    #[test]
    fn taskbar_compacts_all_selected_clocks_before_rotating() {
        for dpi in [96, 120, 144, 192] {
            let p = |v: i32| ((v as i64 * dpi as i64 + 48) / 96) as i32;
            for vertical in [false, true] {
                let bar = if vertical {
                    RECT {
                        left: -p(48),
                        top: -p(200),
                        right: 0,
                        bottom: p(880),
                    }
                } else {
                    taskbar()
                };
                let tray = if vertical {
                    RECT { top: p(680), ..bar }
                } else {
                    RECT {
                        left: p(900),
                        ..bar
                    }
                };
                let end = if vertical { tray.top } else { tray.left };
                let minimum_slot = p(if vertical { 40 } else { 58 });
                for count in 4..=12 {
                    let location = Location {
                        bar,
                        tray,
                        buttons_end: end - p(20) - minimum_slot * count as i32,
                        foreground: 0xffffff,
                    };
                    let (r, slots) = lane(location, count, dpi).unwrap();
                    assert_eq!(
                        slots, count,
                        "{count} clocks, {dpi} DPI, vertical={vertical}"
                    );
                    if vertical {
                        assert!(r.top > location.buttons_end && r.bottom < tray.top);
                    } else {
                        assert!(r.left > location.buttons_end && r.right < tray.left);
                    }
                    let tighter = Location {
                        buttons_end: location.buttons_end + 1,
                        ..location
                    };
                    assert_eq!(lane(tighter, count, dpi).unwrap().1, count - 1);
                }
            }
        }
    }

    #[test]
    fn clocks_fit_all_four_edges_at_each_display_scale() {
        for dpi in [96, 120, 144, 192] {
            let p = |v: i32| (v * dpi as i32 + 48) / 96;
            for vertical in [false, true] {
                for far_edge in [false, true] {
                    let bar = if vertical {
                        RECT {
                            left: p(if far_edge { 1872 } else { 0 }),
                            top: 0,
                            right: p(if far_edge { 1920 } else { 48 }),
                            bottom: p(1080),
                        }
                    } else {
                        RECT {
                            left: 0,
                            top: p(if far_edge { 1032 } else { 0 }),
                            right: p(1920),
                            bottom: p(if far_edge { 1080 } else { 48 }),
                        }
                    };
                    let mut geometry = Geometry::new(bar);
                    let button = if vertical {
                        RECT {
                            bottom: p(320),
                            ..bar
                        }
                    } else {
                        RECT {
                            right: p(900),
                            ..bar
                        }
                    };
                    let tray = if vertical {
                        RECT { top: p(850), ..bar }
                    } else {
                        RECT {
                            left: p(1700),
                            ..bar
                        }
                    };
                    geometry.observe(
                        "window: app",
                        "Taskbar.TaskListButtonAutomationPeer",
                        button,
                    );
                    geometry.observe("systemtrayicon", "SystemTray.NormalButton", tray);
                    let location = geometry.finish(tray, 0xffffff);
                    assert_eq!(location.buttons_end, if vertical { p(320) } else { p(900) });
                    let (r, slots) = lane(location, 4, dpi).unwrap();
                    assert_eq!(slots, 4);
                    if vertical {
                        assert_eq!((r.left, r.right), (bar.left, bar.right));
                        assert!(r.top > button.bottom && r.bottom < tray.top);
                    } else {
                        assert_eq!((r.top, r.bottom), (bar.top, bar.bottom));
                        assert!(r.left > button.right && r.right < tray.left);
                    }
                }
            }
        }
    }

    #[test]
    fn transient_accessibility_failures_keep_the_last_frame_but_eventually_expire() {
        let good = without_clock().finish(unsafe { std::mem::zeroed() }, 0xffffff);
        let mut cache = LocationCache::default();
        assert!(cache.observe(None).is_none());
        assert!(same_location(cache.observe(Some(good)), Some(good)));
        assert!(same_location(cache.observe(None), Some(good)));
        assert!(same_location(cache.observe(None), Some(good)));
        assert!(same_location(cache.observe(Some(good)), Some(good)));
        cache.observe(None);
        cache.observe(None);
        assert!(cache.observe(None).is_none());
        assert!(same_location(cache.observe(Some(good)), Some(good)));
    }

    #[test]
    fn maximized_windows_and_shell_switchers_are_not_fullscreen_apps() {
        let screen = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let maximized_frame = RECT {
            left: -8,
            top: -8,
            right: 1928,
            bottom: 1088,
        };
        assert!(!fullscreen_bounds(maximized_frame, screen, true, false));
        assert!(!fullscreen_bounds(screen, screen, false, true));
        assert!(!fullscreen_bounds(
            RECT {
                bottom: 1032,
                ..screen
            },
            screen,
            false,
            false
        ));
        assert!(fullscreen_bounds(screen, screen, false, false));
        assert!(!fullscreen_bounds(
            RECT {
                left: 1920,
                right: 3840,
                ..screen
            },
            screen,
            false,
            false
        ));
    }

    #[test]
    fn auto_hidden_bars_are_hidden_on_both_axes() {
        let screen = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        for bar in [
            RECT {
                left: -46,
                top: 0,
                right: 2,
                bottom: 1080,
            },
            RECT {
                left: 1918,
                top: 0,
                right: 1966,
                bottom: 1080,
            },
            RECT {
                left: 0,
                top: -46,
                right: 1920,
                bottom: 2,
            },
            RECT {
                left: 0,
                top: 1078,
                right: 1920,
                bottom: 1126,
            },
        ] {
            assert_eq!(visible_thickness(bar, screen), 2);
        }
    }

    #[test]
    fn vertical_geometry_ignores_buttons_outside_the_bar_and_handles_negative_coordinates() {
        let bar = RECT {
            left: -1920,
            top: -200,
            right: -1872,
            bottom: 880,
        };
        let mut geometry = Geometry::new(bar);
        geometry.observe(
            "window: app",
            "TaskListButton",
            RECT {
                right: -1800,
                left: -1850,
                ..bar
            },
        );
        assert!(!geometry.saw_buttons);
        geometry.observe("window: app", "TaskListButton", RECT { bottom: 150, ..bar });
        let location = geometry.finish(RECT { top: 680, ..bar }, 0xffffff);
        let (r, slots) = lane(location, 3, 96).unwrap();
        assert_eq!(slots, 3);
        assert_eq!((r.left, r.right), (-1920, -1872));
        assert!(r.top > 150 && r.bottom < 680);
    }
}
