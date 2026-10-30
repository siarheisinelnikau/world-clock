use crate::{
    appearance::{self, CardStyle},
    desktop,
    render::{Cadence, RefreshGate},
    settings::{self, Clock, MAX_CLOCKS, Settings},
    taskbar::{self, Location, Tracker},
    time::{self, Zone, wide},
    ui, update,
};
use std::{
    cell::{Cell, RefCell},
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
        UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::w,
};

const CLASS: windows_sys::core::PCWSTR = w!("WorldClock.Desktop.v1");
const EDIT_CLASS: windows_sys::core::PCWSTR = w!("WorldClock.Settings.v1");
const TASKBAR_CLASS: windows_sys::core::PCWSTR = w!("WorldClock.Taskbar.v2");
const POSITION: i32 = 232;
const APPEARANCE: i32 = 233;
const MOVE: u32 = 104;
const TRAY_MSG: u32 = WM_APP + 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const REOPEN_MSG: u32 = WM_APP + 2;
const SHOW: u32 = 100;
const SETTINGS: u32 = 101;
const TOPMOST: u32 = 102;
const QUIT: u32 = 103;
const UPDATE: i32 = 234;
const AUTO_UPDATE: i32 = 235;
const UPDATE_STATUS: i32 = 236;
const LIST: i32 = 200;
const ADD: i32 = 201;
const REMOVE: i32 = 202;
const UP: i32 = 203;
const DOWN: i32 = 204;
const ZONE: i32 = 210;
const LABEL: i32 = 211;
const DESKTOP: i32 = 212;
const TRAY: i32 = 213;
const ZONE_SEARCH: i32 = 214;
const CLEAR_SEARCH: i32 = 215;
const TWELVE: i32 = 220;
const SECONDS: i32 = 221;
const ONTOP: i32 = 222;
const STARTUP: i32 = 223;
const SAVE: i32 = 230;
const CANCEL: i32 = 231;

thread_local! { static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) }; }
fn app() -> Rc<App> {
    APP.with(|a| a.borrow().as_ref().expect("app initialized").clone())
}

struct Editor {
    draft: RefCell<Settings>,
    selected: Cell<usize>,
    loading: Cell<bool>,
    font: Cell<HFONT>,
    heading_font: Cell<HFONT>,
    heading: HWND,
    controls: Vec<(i32, HWND)>,
    layout: Vec<(HWND, RECT)>,
    startup: bool,
}
impl Editor {
    fn control(&self, id: i32) -> HWND {
        self.controls
            .iter()
            .find(|(i, _)| *i == id)
            .map_or(null_mut(), |(_, h)| *h)
    }
}

struct App {
    settings: RefCell<Settings>,
    zones: Vec<Zone>,
    hwnd: Cell<HWND>,
    editor_hwnd: Cell<HWND>,
    editor: RefCell<Option<Rc<Editor>>>,
    tray_ids: RefCell<Vec<u32>>,
    minute: Cell<i64>,
    moving: Cell<bool>,
    bar_hwnd: Cell<HWND>,
    stacking_guard: RefCell<Option<taskbar::StackingGuard>>,
    tracker: Tracker,
    location: Cell<Option<Location>>,
    ticks: Cell<u32>,
    desktop_refresh: RefreshGate,
    sweep_timer: Cell<bool>,
    bar_page: Cell<usize>,
    taskbar_created: u32,
    updater: RefCell<update::Updater>,
}

fn error(hwnd: HWND, message: &str) {
    unsafe {
        MessageBoxW(
            hwnd,
            wide(message).as_ptr(),
            w!("World Clock"),
            MB_OK | MB_ICONERROR,
        );
    }
}

unsafe fn populate_zone_choices(combo: HWND, zones: &[Zone], query: &str, current: &str) {
    unsafe {
        SendMessageW(combo, WM_SETREDRAW, 0, 0);
        SendMessageW(combo, CB_RESETCONTENT, 0, 0);
        let matches: Vec<_> = zones
            .iter()
            .enumerate()
            .filter(|(_, zone)| zone.matches(query))
            .collect();
        let mut selection = 0;
        if !matches.iter().any(|(_, zone)| zone.key == current) {
            let prompt = if matches.is_empty() {
                "No matching time zones"
            } else {
                "Select a result to change the time zone"
            };
            let row = SendMessageW(combo, CB_ADDSTRING, 0, wide(prompt).as_ptr() as isize);
            if row >= 0 {
                SendMessageW(combo, CB_SETITEMDATA, row as usize, -1);
            }
        }
        for (index, zone) in &matches {
            let row = SendMessageW(
                combo,
                CB_ADDSTRING,
                0,
                wide(&zone.display).as_ptr() as isize,
            );
            if row >= 0 {
                // Filtered row numbers are different from the original Windows zone indexes.
                SendMessageW(combo, CB_SETITEMDATA, row as usize, *index as isize);
                if zone.key == current {
                    selection = row as usize;
                }
            }
        }
        SendMessageW(combo, CB_SETCURSEL, selection, 0);
        EnableWindow(combo, !matches.is_empty() as i32);
        SendMessageW(combo, WM_SETREDRAW, 1, 0);
        InvalidateRect(combo, null(), 1);
    }
}

unsafe fn selected_zone_index(combo: HWND) -> Option<usize> {
    unsafe {
        let row = SendMessageW(combo, CB_GETCURSEL, 0, 0);
        if row < 0 {
            return None;
        }
        let index = SendMessageW(combo, CB_GETITEMDATA, row as usize, 0);
        (index >= 0).then_some(index as usize)
    }
}

pub fn run() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        });
        let mutex = CreateMutexW(null(), 0, w!("Local\\WorldClock.Portable.v1"));
        if mutex.is_null() {
            error(null_mut(), "World Clock could not start.");
            return;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let existing = FindWindowW(CLASS, null());
            if !existing.is_null() {
                PostMessageW(existing, REOPEN_MSG, 0, 0);
            }
            CloseHandle(mutex);
            return;
        }
        let instance = GetModuleHandleW(null());
        let icon = LoadIconW(instance, 1usize as _);
        for (class, proc, background) in [
            (
                TASKBAR_CLASS,
                Some(taskbar_proc as unsafe extern "system" fn(_, _, _, _) -> _),
                null_mut(),
            ),
            (
                CLASS,
                Some(desktop_proc as unsafe extern "system" fn(_, _, _, _) -> _),
                null_mut(),
            ),
            (
                EDIT_CLASS,
                Some(editor_proc as unsafe extern "system" fn(_, _, _, _) -> _),
                (COLOR_WINDOW + 1) as HBRUSH,
            ),
        ] {
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_DBLCLKS,
                lpfnWndProc: proc,
                hInstance: instance,
                hIcon: icon,
                hIconSm: icon,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: background,
                lpszClassName: class,
                ..zeroed()
            };
            if RegisterClassExW(&wc) == 0 {
                error(null_mut(), "Could not create the application window.");
                CloseHandle(mutex);
                return;
            }
        }
        let zones = time::zones();
        let mut config = Settings::load();
        config
            .clocks
            .retain(|c| zones.iter().any(|z| z.key == c.zone));
        if config.clocks.is_empty() {
            config = Settings::default();
            config
                .clocks
                .retain(|c| zones.iter().any(|z| z.key == c.zone));
        }
        let state = Rc::new(App {
            settings: RefCell::new(config),
            zones,
            hwnd: Cell::new(null_mut()),
            editor_hwnd: Cell::new(null_mut()),
            editor: RefCell::new(None),
            tray_ids: RefCell::new(Vec::new()),
            minute: Cell::new(-1),
            moving: Cell::new(false),
            bar_hwnd: Cell::new(null_mut()),
            stacking_guard: RefCell::new(None),
            tracker: Tracker::start(),
            location: Cell::new(None),
            ticks: Cell::new(0),
            desktop_refresh: RefreshGate::default(),
            sweep_timer: Cell::new(false),
            bar_page: Cell::new(usize::MAX),
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            updater: RefCell::new(update::Updater::new()),
        });
        APP.with(|a| *a.borrow_mut() = Some(state.clone()));
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            CLASS,
            w!("World Clock"),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            ui::WIDTH,
            1,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            error(null_mut(), "Could not create the clock panel.");
            CloseHandle(mutex);
            return;
        }
        state.hwnd.set(hwnd);
        let _desktop_guard = desktop::DesktopGuard::start(hwnd);
        let excluded: i32 = 1;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_EXCLUDED_FROM_PEEK as u32,
            &excluded as *const _ as _,
            size_of::<i32>() as u32,
        );
        let bar = state.taskbar_window();
        if bar.is_null() {
            error(hwnd, "Could not create the taskbar clock.");
            DestroyWindow(hwnd);
            CloseHandle(mutex);
            return;
        }
        state.position_desktop(true);
        state.update_tray(true);
        if state.settings.borrow().visible {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        if std::env::args().any(|s| s == "--settings") {
            state.open_editor();
        }
        SetTimer(hwnd, 1, 250, None);
        if state.settings.borrow().check_updates {
            state.updater.borrow_mut().check(false);
            state.refresh_updates();
        }
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            let editor = state.editor_hwnd.get();
            if !editor.is_null() && IsDialogMessageW(editor, &msg) != 0 {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        state.clear_tray();
        CloseHandle(mutex);
        APP.with(|a| *a.borrow_mut() = None);
    }
}

impl App {
    unsafe fn sync_animation_timer(&self) {
        unsafe {
            let config = self.settings.borrow();
            let active = IsWindowVisible(self.hwnd.get()) != 0
                && config.clocks.iter().any(|clock| {
                    clock.desktop
                        && Cadence::for_style(&clock.style, config.seconds) == Cadence::Sweep
                });
            if self.sweep_timer.replace(active) != active {
                if active {
                    SetTimer(self.hwnd.get(), 2, 50, None);
                } else {
                    KillTimer(self.hwnd.get(), 2);
                }
            }
        }
    }

    unsafe fn paint_desktop(&self, force: bool) {
        unsafe {
            self.sync_animation_timer();
            let config = self.settings.borrow();
            let cadence = if force || IsWindowVisible(self.hwnd.get()) != 0 {
                config
                    .clocks
                    .iter()
                    .filter(|c| c.desktop)
                    .map(|c| Cadence::for_style(&c.style, config.seconds))
                    .max()
            } else {
                None
            };
            let due = self.desktop_refresh.due(cadence, &time::utc_now());
            if (force || due)
                && !ui::render_desktop(self.hwnd.get(), &config, &self.zones, self.moving.get())
            {
                self.desktop_refresh.invalidate();
            }
        }
    }

    fn update_owner(&self) -> HWND {
        let editor = self.editor_hwnd.get();
        if editor.is_null() {
            self.hwnd.get()
        } else {
            editor
        }
    }

    unsafe fn refresh_updates(&self) {
        unsafe {
            if let Some(editor) = self.editor() {
                let updater = self.updater.borrow();
                SetWindowTextW(
                    editor.control(UPDATE),
                    wide(&updater.state.action()).as_ptr(),
                );
                EnableWindow(editor.control(UPDATE), !updater.state.busy() as i32);
                SetWindowTextW(
                    editor.control(UPDATE_STATUS),
                    wide(&updater.state.description()).as_ptr(),
                );
            }
        }
    }

    unsafe fn update_action(&self) {
        unsafe {
            let release = {
                let updater = self.updater.borrow();
                if updater.state.busy() {
                    return;
                }
                if matches!(updater.state, update::State::Ready(_)) {
                    drop(updater);
                    self.install_update();
                    return;
                }
                match &updater.state {
                    update::State::Available(release) => Some(release.clone()),
                    _ => None,
                }
            };
            if let Some(release) = release {
                let message = format!(
                    "World Clock {} is available (current: {}).\n\nDownload the update from GitHub and restart? Your settings will be saved.",
                    release.tag,
                    update::VERSION
                );
                if MessageBoxW(
                    self.update_owner(),
                    wide(&message).as_ptr(),
                    w!("World Clock — Update"),
                    MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
                ) != IDYES
                {
                    return;
                }
                self.updater.borrow_mut().download(release);
            } else {
                self.updater.borrow_mut().check(true);
            }
            self.refresh_updates();
        }
    }

    unsafe fn install_update(&self) {
        unsafe {
            // Use the normal Save path, including validation and startup settings.
            if !self.editor_hwnd.get().is_null() {
                self.editor_command(SAVE, 0);
                if !self.editor_hwnd.get().is_null() {
                    self.refresh_updates();
                    return;
                }
            }
            if let Err(message) = self.settings.borrow().save() {
                error(self.update_owner(), &message);
                return;
            }
            self.updater.borrow_mut().install();
            self.refresh_updates();
        }
    }

    unsafe fn poll_updates(&self) {
        unsafe {
            loop {
                let notice = self.updater.borrow_mut().poll();
                let Some(notice) = notice else { return };
                self.refresh_updates();
                match notice {
                    update::Notice::Changed => {}
                    update::Notice::Downloaded => self.install_update(),
                    update::Notice::Exit => {
                        DestroyWindow(self.hwnd.get());
                        return;
                    }
                    update::Notice::Checked => {
                        let updater = self.updater.borrow();
                        let manual = updater.manual;
                        let (message, failed) = match &updater.state {
                            update::State::Available(release) => (
                                format!(
                                    "World Clock {} is available. Choose Download and install from Settings or the tray menu.",
                                    release.tag
                                ),
                                false,
                            ),
                            update::State::Current if manual => (
                                format!("World Clock {} is up to date.", update::VERSION),
                                false,
                            ),
                            update::State::Failed(message) if manual => (message.clone(), true),
                            _ => return,
                        };
                        drop(updater);
                        if manual {
                            MessageBoxW(
                                self.update_owner(),
                                wide(&message).as_ptr(),
                                w!("World Clock — Updates"),
                                MB_OK
                                    | if failed {
                                        MB_ICONERROR
                                    } else {
                                        MB_ICONINFORMATION
                                    },
                            );
                        } else {
                            let mut data: NOTIFYICONDATAW = zeroed();
                            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
                            data.hWnd = self.hwnd.get();
                            data.uID = 1;
                            data.uFlags = NIF_INFO;
                            data.dwInfoFlags = NIIF_INFO;
                            let text = wide(&message);
                            let len = (text.len() - 1).min(data.szInfo.len() - 1);
                            data.szInfo[..len].copy_from_slice(&text[..len]);
                            let title = wide("World Clock update available");
                            data.szInfoTitle[..title.len()].copy_from_slice(&title);
                            Shell_NotifyIconW(NIM_MODIFY, &data);
                        }
                    }
                }
            }
        }
    }

    unsafe fn taskbar_window(&self) -> HWND {
        unsafe {
            let existing = self.bar_hwnd.get();
            if !existing.is_null() && IsWindow(existing) != 0 {
                return existing;
            }
            let owner = taskbar::native_bar().map_or(null_mut(), |(hwnd, _)| hwnd);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
                TASKBAR_CLASS,
                w!("World Clock — Taskbar"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                owner,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            self.bar_hwnd.set(hwnd);
            if !hwnd.is_null() {
                self.stacking_guard.borrow_mut().take();
                *self.stacking_guard.borrow_mut() = Some(taskbar::StackingGuard::start(hwnd));
            }
            hwnd
        }
    }

    fn desktop_config(&self) -> Settings {
        self.settings.borrow().clone()
    }
    fn persist(&self) {
        if let Err(e) = self.settings.borrow().save() {
            error(self.hwnd.get(), &e);
        }
    }

    unsafe fn position_desktop(&self, restore: bool) {
        unsafe {
            let hwnd = self.hwnd.get();
            let config = self.desktop_config();
            if hwnd.is_null() {
                return;
            }
            let monitor = if restore && config.x != i32::MIN {
                MonitorFromPoint(
                    POINT {
                        x: config.x,
                        y: config.y,
                    },
                    MONITOR_DEFAULTTONEAREST,
                )
            } else {
                MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
            };
            let mut mi: MONITORINFO = zeroed();
            mi.cbSize = size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(monitor, &mut mi);
            let work = mi.rcWork;
            if restore {
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    work.left + 16,
                    work.top + 16,
                    0,
                    0,
                    SWP_NOZORDER | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            let dpi = GetDpiForWindow(hwnd).max(96);
            let (width, height, _) =
                ui::desktop_size(&config, dpi, work.bottom - work.top - ui::px(24, dpi));
            let mut current: RECT = zeroed();
            GetWindowRect(hwnd, &mut current);
            let (x, y) = if restore {
                if config.x == i32::MIN {
                    (
                        work.right - width - ui::px(24, dpi),
                        work.top + ui::px(32, dpi),
                    )
                } else {
                    (config.x, config.y)
                }
            } else {
                (current.left, current.top)
            };
            let x = x.clamp(work.left, (work.right - width).max(work.left));
            let y = y.clamp(work.top, (work.bottom - height).max(work.top));
            SetWindowPos(
                hwnd,
                if self.moving.get() || config.topmost {
                    HWND_TOPMOST
                } else {
                    HWND_BOTTOM
                },
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE,
            );
            self.paint_desktop(true);
            ShowWindow(
                hwnd,
                if config.visible && config.clocks.iter().any(|c| c.desktop) {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_HIDE
                },
            );
            self.sync_animation_timer();
            desktop::keep_above_desktop(hwnd);
        }
    }

    unsafe fn move_desktop(&self) {
        unsafe {
            let moving = !self.moving.get();
            self.moving.set(moving);
            let hwnd = self.hwnd.get();
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                if moving {
                    style & !WS_EX_TRANSPARENT
                } else {
                    style | WS_EX_TRANSPARENT
                } as isize,
            );
            if moving {
                self.settings.borrow_mut().visible = true;
            }
            let mut bounds: RECT = zeroed();
            GetWindowRect(hwnd, &mut bounds);
            SetWindowPos(
                hwnd,
                if moving || self.settings.borrow().topmost {
                    HWND_TOPMOST
                } else {
                    HWND_BOTTOM
                },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            self.paint_desktop(true);
            if moving {
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            self.sync_animation_timer();
            desktop::keep_above_desktop(hwnd);
            if !moving {
                {
                    let mut s = self.settings.borrow_mut();
                    s.x = bounds.left;
                    s.y = bounds.top;
                }
                self.persist();
            }
        }
    }

    unsafe fn update_taskbar(&self, redraw: bool) {
        unsafe {
            let mut changed = false;
            while let Ok(location) = self.tracker.rx.try_recv() {
                changed |= !taskbar::same_location(self.location.get(), location);
                self.location.set(location);
            }
            let hwnd = self.taskbar_window();
            if hwnd.is_null() {
                return;
            }
            let config = self.settings.borrow().clone();
            let Some(location) = self.location.get() else {
                ShowWindow(hwnd, SW_HIDE);
                return;
            };
            let Some((native_bar, native_bounds)) = taskbar::native_bar() else {
                ShowWindow(hwnd, SW_HIDE);
                return;
            };
            taskbar::attach_overlay_owner(hwnd, native_bar);
            // Cached accessibility data is safe only while the real bar stays put.
            if !taskbar::same_rect(location.bar, native_bounds) {
                ShowWindow(hwnd, SW_HIDE);
                return;
            }
            let r = location.bar;
            let mut mi: MONITORINFO = zeroed();
            mi.cbSize = size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST), &mut mi);
            let screen = mi.rcMonitor;
            let foreground = GetForegroundWindow();
            let mut front: RECT = zeroed();
            GetWindowRect(foreground, &mut front);
            DwmGetWindowAttribute(
                foreground,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut front as *mut RECT).cast(),
                size_of::<RECT>() as u32,
            );
            let mut cloaked = 0u32;
            DwmGetWindowAttribute(
                foreground,
                DWMWA_CLOAKED as u32,
                (&mut cloaked as *mut u32).cast(),
                size_of::<u32>() as u32,
            );
            let mut class = [0u16; 128];
            GetClassNameW(foreground, class.as_mut_ptr(), 128);
            let class = time::unw(&class);
            let mut foreground_pid = 0;
            let mut shell_pid = 0;
            GetWindowThreadProcessId(foreground, &mut foreground_pid);
            GetWindowThreadProcessId(native_bar, &mut shell_pid);
            let shell = (shell_pid != 0 && foreground_pid == shell_pid)
                || matches!(
                    class.as_str(),
                    "Progman"
                        | "WorkerW"
                        | "Shell_TrayWnd"
                        | "Shell_SecondaryTrayWnd"
                        | "MultitaskingViewFrame"
                        | "TaskSwitcherWnd"
                );
            let fullscreen = !foreground.is_null()
                && foreground != self.editor_hwnd.get()
                && foreground != self.hwnd.get()
                && cloaked == 0
                && taskbar::fullscreen_bounds(
                    front,
                    screen,
                    IsZoomed(foreground) != 0
                        && GetWindowLongPtrW(foreground, GWL_STYLE) as u32 & WS_CAPTION != 0,
                    shell,
                );
            let dpi = GetDpiForWindow(native_bar).max(96);
            if !config.clocks.iter().any(|c| c.tray)
                || taskbar::visible_thickness(r, screen) < ui::px(12, dpi)
                || fullscreen
            {
                ShowWindow(hwnd, SW_HIDE);
                return;
            }
            let Some((r, slots)) = crate::taskbar::lane(
                location,
                config.clocks.iter().filter(|c| c.tray).count(),
                dpi,
            ) else {
                ShowWindow(hwnd, SW_HIDE);
                return;
            };
            let visible = IsWindowVisible(hwnd) != 0;
            let count = config.clocks.iter().filter(|c| c.tray).count();
            let page = (self.ticks.get() as usize / 24) % count.div_ceil(slots);
            let mut previous: RECT = zeroed();
            GetWindowRect(hwnd, &mut previous);
            if (changed
                || redraw
                || self.bar_page.get() != page
                || !visible
                || !taskbar::same_rect(previous, r))
                && !ui::render_taskbar(hwnd, &config, &self.zones, location, page, dpi)
            {
                return;
            }
            self.bar_page.set(page);
            // Pixel content, position and size were uploaded together; only raise/show here.
            if !visible || !taskbar::above(hwnd, native_bar) {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE
                        | SWP_NOMOVE
                        | SWP_NOSIZE
                        | SWP_NOOWNERZORDER
                        | if visible { 0 } else { SWP_SHOWWINDOW },
                );
            }
        }
    }

    unsafe fn verify_overlays(&self) {
        unsafe {
            let desktop = self.hwnd.get();
            let bar = self.bar_hwnd.get();
            let check = |hwnd: HWND| {
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                let mut r: RECT = zeroed();
                GetWindowRect(hwnd, &mut r);
                let mut hits_self = false;
                for y in (r.top..r.bottom).step_by(3) {
                    for x in (r.left..r.right).step_by(3) {
                        if WindowFromPoint(POINT { x, y }) == hwnd {
                            hits_self = true;
                        }
                    }
                }
                (
                    style & WS_EX_TRANSPARENT != 0,
                    style & WS_EX_NOACTIVATE != 0,
                    hits_self,
                )
            };
            SetWindowPos(
                desktop,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let desktop_result = check(desktop);
            let bar_result = check(bar);
            let mut before: RECT = zeroed();
            GetWindowRect(desktop, &mut before);
            self.move_desktop();
            SendMessageW(desktop, WM_NCLBUTTONDBLCLK, HTCAPTION as usize, 0);
            let mut after: RECT = zeroed();
            GetWindowRect(desktop, &mut after);
            let stable = (before.left, before.top, before.right, before.bottom)
                == (after.left, after.top, after.right, after.bottom);
            if !self.settings.borrow().topmost && !self.moving.get() {
                SetWindowPos(
                    desktop,
                    HWND_BOTTOM,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            let mut report = format!(
                "desktop (transparent, noactivate, intercepts input): {desktop_result:?}\ntaskbar (transparent, noactivate, intercepts input): {bar_result:?}\nnotification icons: {}\n",
                self.tray_ids.borrow().len()
            );
            if let Some(l) = self.location.get() {
                let dpi = GetDpiForWindow(bar).max(96);
                if let Some((lane, slots)) = crate::taskbar::lane(
                    l,
                    self.settings
                        .borrow()
                        .clocks
                        .iter()
                        .filter(|c| c.tray)
                        .count(),
                    dpi,
                ) {
                    let clear = if l.vertical() {
                        lane.top > l.buttons_end && lane.bottom < l.tray.top
                    } else {
                        lane.left > l.buttons_end && lane.right < l.tray.left
                    };
                    report.push_str(&format!("taskbar visible slots: {slots}; avoids native tray and app buttons: {clear}\n"));
                }
                report.push_str(&format!(
                    "taskbar region: ({},{})-({},{}); tray starts at {}\n",
                    l.bar.left, l.bar.top, l.bar.right, l.bar.bottom, l.tray.left
                ));
            }
            report.push_str(&format!(
                "position stable after double-click lock: {stable}\n"
            ));
            let _ = std::fs::write("overlay-verification.log", report);
        }
    }

    unsafe fn clear_tray(&self) {
        unsafe {
            let ids = std::mem::take(&mut *self.tray_ids.borrow_mut());
            for id in ids {
                let data = NOTIFYICONDATAW {
                    cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                    hWnd: self.hwnd.get(),
                    uID: id,
                    ..zeroed()
                };
                Shell_NotifyIconW(NIM_DELETE, &data);
            }
        }
    }

    unsafe fn update_tray(&self, rebuild: bool) {
        unsafe {
            if rebuild {
                self.clear_tray();
            }
            let utc = time::utc_now();
            let stamp = ((utc.wDay as i64 * 24 + utc.wHour as i64) * 60) + utc.wMinute as i64;
            if !rebuild && stamp == self.minute.get() {
                return;
            }
            self.minute.set(stamp);
            let mut tooltip = format!("World Clock — Settings\n{}", time::date(&time::local_now()));
            let config = self.settings.borrow().clone();
            for c in config.clocks.iter().filter(|c| c.tray) {
                if let Some(t) = self
                    .zones
                    .iter()
                    .find(|z| z.key == c.zone)
                    .and_then(|z| time::convert(z, &utc))
                {
                    tooltip.push_str(&format!(
                        "\n{}  {}",
                        c.label,
                        time::clock(&t, config.twelve, false)
                    ));
                }
            }
            let mut data: NOTIFYICONDATAW = zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.hwnd.get();
            data.uID = 1;
            data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
            data.uCallbackMessage = TRAY_MSG;
            data.hIcon = LoadIconW(GetModuleHandleW(null()), 1usize as _);
            let tip = wide(&tooltip);
            let len = (tip.len() - 1).min(127);
            data.szTip[..len].copy_from_slice(&tip[..len]);
            let exists = self.tray_ids.borrow().contains(&1);
            let mut ok = Shell_NotifyIconW(if exists { NIM_MODIFY } else { NIM_ADD }, &data);
            if ok == 0 && exists {
                ok = Shell_NotifyIconW(NIM_ADD, &data);
            }
            if ok != 0 {
                if !exists {
                    self.tray_ids.borrow_mut().push(1);
                }
                data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                Shell_NotifyIconW(NIM_SETVERSION, &data);
            }
        }
    }

    unsafe fn show_desktop(&self, toggle: bool) {
        unsafe {
            let visible = !toggle || !self.settings.borrow().visible;
            self.settings.borrow_mut().visible = visible;
            if visible {
                self.position_desktop(false);
                ShowWindow(self.hwnd.get(), SW_SHOWNOACTIVATE);
            } else {
                ShowWindow(self.hwnd.get(), SW_HIDE);
            }
            self.sync_animation_timer();
            self.persist();
        }
    }

    unsafe fn menu(&self) {
        unsafe {
            let hwnd = self.hwnd.get();
            let menu = CreatePopupMenu();
            let config = self.settings.borrow().clone();
            let utc = time::utc_now();
            for c in &config.clocks {
                if let Some(t) = self
                    .zones
                    .iter()
                    .find(|z| z.key == c.zone)
                    .and_then(|z| time::convert(z, &utc))
                {
                    let label = format!(
                        "{}    {} · {} · {}",
                        c.label.replace('&', "&&"),
                        time::clock(&t, config.twelve, false),
                        time::day_relation(&t, &time::local_now()),
                        time::offset(&t, &utc)
                    );
                    AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, wide(&label).as_ptr());
                }
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(
                menu,
                MF_STRING,
                SHOW as usize,
                if config.visible {
                    w!("Hide desktop clocks")
                } else {
                    w!("Show desktop clocks")
                },
            );
            AppendMenuW(menu, MF_STRING, SETTINGS as usize, w!("Settings…"));
            AppendMenuW(
                menu,
                MF_STRING,
                MOVE as usize,
                if self.moving.get() {
                    w!("Lock desktop clocks")
                } else {
                    w!("Position desktop clocks…")
                },
            );
            AppendMenuW(
                menu,
                MF_STRING | if config.topmost { MF_CHECKED } else { 0 },
                TOPMOST as usize,
                w!("Always on top"),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            {
                let updater = self.updater.borrow();
                AppendMenuW(
                    menu,
                    MF_STRING | if updater.state.busy() { MF_GRAYED } else { 0 },
                    UPDATE as usize,
                    wide(&updater.state.action()).as_ptr(),
                );
            }
            AppendMenuW(menu, MF_STRING, QUIT as usize, w!("Quit World Clock"));
            let mut pt: POINT = zeroed();
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                0,
                hwnd,
                null(),
            );
            DestroyMenu(menu);
            PostMessageW(hwnd, WM_NULL, 0, 0);
            match command as u32 {
                command if command == UPDATE as u32 => self.update_action(),
                SHOW => self.show_desktop(true),
                SETTINGS => self.open_editor(),
                MOVE => self.move_desktop(),
                TOPMOST => {
                    let on = self.settings.borrow().topmost;
                    self.settings.borrow_mut().topmost = !on;
                    self.position_desktop(false);
                    self.persist();
                }
                QUIT => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
        }
    }

    fn editor(&self) -> Option<Rc<Editor>> {
        self.editor.borrow().clone()
    }

    unsafe fn open_editor(&self) {
        unsafe {
            if matches!(self.updater.borrow().state, update::State::Installing) {
                return;
            }
            let existing = self.editor_hwnd.get();
            if !existing.is_null() {
                ShowWindow(existing, SW_RESTORE);
                SetForegroundWindow(existing);
                return;
            }
            let dpi = GetDpiForWindow(self.hwnd.get()).max(96);
            let mut r = ui::rect(0, 0, ui::px(710, dpi), ui::px(604, dpi));
            AdjustWindowRectExForDpi(&mut r, WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU, 0, 0, dpi);
            let mut desktop: RECT = zeroed();
            GetWindowRect(self.hwnd.get(), &mut desktop);
            let monitor = MonitorFromWindow(self.hwnd.get(), MONITOR_DEFAULTTONEAREST);
            let mut mi: MONITORINFO = zeroed();
            mi.cbSize = size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(monitor, &mut mi);
            let width = r.right - r.left;
            let height = r.bottom - r.top;
            let hwnd = CreateWindowExW(
                WS_EX_CONTROLPARENT | WS_EX_APPWINDOW,
                EDIT_CLASS,
                w!("World Clock — Settings"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                mi.rcWork.left + (mi.rcWork.right - mi.rcWork.left - width) / 2,
                mi.rcWork.top + (mi.rcWork.bottom - mi.rcWork.top - height) / 2,
                width,
                height,
                self.hwnd.get(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            if hwnd.is_null() {
                error(self.hwnd.get(), "Could not open Settings.");
                return;
            }
            self.editor_hwnd.set(hwnd);
            self.build_editor(hwnd);
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            if let Some(e) = self.editor() {
                SetFocus(e.control(LIST));
            }
        }
    }

    unsafe fn build_editor(&self, hwnd: HWND) {
        unsafe {
            let dpi = GetDpiForWindow(hwnd).max(96);
            let p = |v| ui::px(v, dpi);
            let font = ui::font(p(13), 400, "Segoe UI");
            let mut controls = Vec::new();
            let mut layout = Vec::new();
            let mut make = |class: &str,
                            title: &str,
                            id: i32,
                            x: i32,
                            y: i32,
                            width: i32,
                            height: i32,
                            style: u32,
                            ex: u32| {
                let h = CreateWindowExW(
                    ex,
                    wide(class).as_ptr(),
                    wide(title).as_ptr(),
                    WS_CHILD | WS_VISIBLE | style,
                    p(x),
                    p(y),
                    p(width),
                    p(height),
                    hwnd,
                    id as usize as HMENU,
                    GetModuleHandleW(null()),
                    null(),
                );
                SendMessageW(h, WM_SETFONT, font as usize, 1);
                if id != 0 {
                    controls.push((id, h));
                }
                layout.push((h, ui::rect(x, y, width, height)));
                h
            };
            let heading = make("STATIC", "Your clocks", 0, 24, 14, 400, 30, 0, 0);
            make(
                "STATIC",
                "Choose a time zone, give it a name, and pick where it appears.",
                0,
                24,
                47,
                665,
                22,
                0,
                0,
            );
            make("STATIC", "CLOCKS", 0, 24, 86, 190, 20, 0, 0);
            make(
                "LISTBOX",
                "",
                LIST,
                24,
                110,
                228,
                229,
                LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
            );
            make("BUTTON", "+ Add", ADD, 24, 351, 70, 30, WS_TABSTOP, 0);
            make("BUTTON", "Remove", REMOVE, 102, 351, 70, 30, WS_TABSTOP, 0);
            make("BUTTON", "↑", UP, 180, 351, 32, 30, WS_TABSTOP, 0);
            make("BUTTON", "↓", DOWN, 220, 351, 32, 30, WS_TABSTOP, 0);
            make("STATIC", "TIME ZONE", 0, 278, 86, 400, 20, 0, 0);
            let search = make(
                "EDIT",
                "",
                ZONE_SEARCH,
                278,
                110,
                310,
                28,
                ES_AUTOHSCROLL as u32 | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
            );
            SendMessageW(search, EM_SETLIMITTEXT, 128, 0);
            SendMessageW(
                search,
                EM_SETCUEBANNER,
                1,
                w!("Search city, zone or UTC offset…") as isize,
            );
            make(
                "BUTTON",
                "Clear",
                CLEAR_SEARCH,
                598,
                110,
                88,
                28,
                WS_TABSTOP,
                0,
            );
            make(
                "COMBOBOX",
                "",
                ZONE,
                278,
                146,
                408,
                310,
                CBS_DROPDOWNLIST as u32 | CBS_HASSTRINGS as u32 | WS_VSCROLL | WS_TABSTOP,
                0,
            );
            make("STATIC", "DISPLAY NAME", 0, 278, 188, 390, 20, 0, 0);
            let label = make(
                "EDIT",
                "",
                LABEL,
                278,
                212,
                408,
                30,
                ES_AUTOHSCROLL as u32 | WS_TABSTOP,
                WS_EX_CLIENTEDGE,
            );
            SendMessageW(label, EM_SETLIMITTEXT, 36, 0);
            make("STATIC", "SHOW THIS CLOCK", 0, 278, 254, 390, 20, 0, 0);
            make(
                "BUTTON",
                "On desktop",
                DESKTOP,
                278,
                278,
                190,
                27,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            make(
                "BUTTON",
                "In taskbar",
                TRAY,
                478,
                278,
                208,
                27,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            make(
                "STATIC",
                "Taskbar clocks sit beside the tray.\r\nDesktop cards pass all mouse input through.",
                0,
                278,
                315,
                408,
                48,
                0,
                0,
            );
            make("STATIC", "", 0, 24, 399, 662, 1, 0x10, 0);
            make(
                "BUTTON",
                "12-hour time",
                TWELVE,
                24,
                412,
                155,
                26,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            make(
                "BUTTON",
                "Show seconds",
                SECONDS,
                195,
                412,
                155,
                26,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            make(
                "BUTTON",
                "Always on top",
                ONTOP,
                366,
                412,
                155,
                26,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            make(
                "BUTTON",
                "Start with Windows",
                STARTUP,
                537,
                412,
                157,
                26,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            make(
                "BUTTON",
                "Check for updates when World Clock starts",
                AUTO_UPDATE,
                24,
                450,
                430,
                26,
                BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
                0,
            );
            make("STATIC", "", UPDATE_STATUS, 24, 490, 442, 42, 0, 0);
            make(
                "BUTTON",
                "Check for updates…",
                UPDATE,
                478,
                486,
                208,
                32,
                WS_TABSTOP,
                0,
            );
            make(
                "BUTTON",
                "Save && position clocks…",
                POSITION,
                24,
                552,
                228,
                32,
                WS_TABSTOP,
                0,
            );
            make(
                "BUTTON",
                "Appearance…",
                APPEARANCE,
                278,
                358,
                208,
                30,
                WS_TABSTOP,
                0,
            );
            make("BUTTON", "Cancel", CANCEL, 486, 552, 94, 32, WS_TABSTOP, 0);
            make(
                "BUTTON",
                "Save",
                SAVE,
                592,
                552,
                94,
                32,
                BS_DEFPUSHBUTTON as u32 | WS_TABSTOP,
                0,
            );
            let heading_font = ui::font(p(22), 600, "Segoe UI");
            SendMessageW(heading, WM_SETFONT, heading_font as usize, 1);
            let e = Rc::new(Editor {
                draft: RefCell::new(self.settings.borrow().clone()),
                selected: Cell::new(0),
                loading: Cell::new(true),
                font: Cell::new(font),
                heading_font: Cell::new(heading_font),
                heading,
                controls,
                layout,
                startup: settings::startup_enabled(),
            });
            let s = e.draft.borrow();
            for (id, checked) in [
                (TWELVE, s.twelve),
                (SECONDS, s.seconds),
                (ONTOP, s.topmost),
                (STARTUP, e.startup),
                (AUTO_UPDATE, s.check_updates),
            ] {
                SendMessageW(
                    e.control(id),
                    BM_SETCHECK,
                    if checked { BST_CHECKED } else { BST_UNCHECKED } as usize,
                    0,
                );
            }
            drop(s);
            *self.editor.borrow_mut() = Some(e.clone());
            self.refresh_list(&e);
            self.select_clock(&e, 0);
            e.loading.set(false);
            self.refresh_updates();
        }
    }

    unsafe fn refresh_list(&self, e: &Editor) {
        unsafe {
            let was = e.loading.replace(true);
            let list = e.control(LIST);
            SendMessageW(list, WM_SETREDRAW, 0, 0);
            SendMessageW(list, LB_RESETCONTENT, 0, 0);
            for c in &e.draft.borrow().clocks {
                SendMessageW(list, LB_ADDSTRING, 0, wide(&c.label).as_ptr() as isize);
            }
            SendMessageW(list, LB_SETCURSEL, e.selected.get(), 0);
            SendMessageW(list, WM_SETREDRAW, 1, 0);
            InvalidateRect(list, null(), 1);
            e.loading.set(was);
            let len = e.draft.borrow().clocks.len();
            EnableWindow(
                e.control(REMOVE),
                e.draft.borrow().can_remove_clock(e.selected.get()) as i32,
            );
            EnableWindow(e.control(ADD), (len < MAX_CLOCKS) as i32);
            EnableWindow(e.control(UP), (e.selected.get() > 0) as i32);
            EnableWindow(e.control(DOWN), (e.selected.get() + 1 < len) as i32);
        }
    }

    unsafe fn select_clock(&self, e: &Editor, index: usize) {
        unsafe {
            let c = e.draft.borrow().clocks.get(index).cloned();
            let Some(c) = c else { return };
            let was = e.loading.replace(true);
            e.selected.set(index);
            SetWindowTextW(e.control(LABEL), wide(&c.label).as_ptr());
            SetWindowTextW(e.control(ZONE_SEARCH), w!(""));
            self.filter_zones(e);
            for (id, checked) in [(DESKTOP, c.desktop), (TRAY, c.tray)] {
                SendMessageW(
                    e.control(id),
                    BM_SETCHECK,
                    if checked { BST_CHECKED } else { BST_UNCHECKED } as usize,
                    0,
                );
            }
            self.refresh_list(e);
            e.loading.set(was);
        }
    }

    unsafe fn filter_zones(&self, e: &Editor) {
        unsafe {
            let was = e.loading.replace(true);
            let mut query = [0u16; 129];
            GetWindowTextW(
                e.control(ZONE_SEARCH),
                query.as_mut_ptr(),
                query.len() as i32,
            );
            let query = time::unw(&query);
            let current = e.draft.borrow().clocks[e.selected.get()].zone.clone();
            populate_zone_choices(e.control(ZONE), &self.zones, &query, &current);
            EnableWindow(e.control(CLEAR_SEARCH), !query.is_empty() as i32);
            e.loading.set(was);
        }
    }

    unsafe fn editor_command(&self, id: i32, notification: u32) {
        unsafe {
            let Some(e) = self.editor() else { return };
            if e.loading.get() {
                return;
            }
            let hwnd = self.editor_hwnd.get();
            let index = e.selected.get();
            match id {
                UPDATE => self.update_action(),
                LIST if notification == LBN_SELCHANGE => {
                    let selected = SendMessageW(e.control(LIST), LB_GETCURSEL, 0, 0);
                    if selected >= 0 {
                        self.select_clock(&e, selected as usize);
                    }
                }
                LABEL if notification == EN_CHANGE => {
                    let mut buf = [0u16; 128];
                    GetWindowTextW(e.control(LABEL), buf.as_mut_ptr(), buf.len() as i32);
                    if let Some(c) = e.draft.borrow_mut().clocks.get_mut(index) {
                        c.label = time::unw(&buf);
                    }
                    self.refresh_list(&e);
                }
                ZONE_SEARCH if notification == EN_CHANGE => self.filter_zones(&e),
                CLEAR_SEARCH => {
                    SetFocus(e.control(ZONE_SEARCH));
                    SetWindowTextW(e.control(ZONE_SEARCH), w!(""));
                }
                ZONE if notification == CBN_SELCHANGE => {
                    if let Some(z) =
                        selected_zone_index(e.control(ZONE)).and_then(|index| self.zones.get(index))
                    {
                        e.draft.borrow_mut().clocks[index].zone = z.key.clone();
                    }
                    self.refresh_list(&e);
                }
                DESKTOP | TRAY => {
                    let checked =
                        SendMessageW(e.control(id), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                    let mut s = e.draft.borrow_mut();
                    let c = &mut s.clocks[index];
                    if id == DESKTOP {
                        c.desktop = checked;
                    } else {
                        c.tray = checked;
                    }
                }
                ADD => {
                    if e.draft.borrow().clocks.len() >= MAX_CLOCKS {
                        return;
                    }
                    e.draft.borrow_mut().clocks.push(Clock {
                        zone: self
                            .zones
                            .iter()
                            .find(|z| z.key == "UTC")
                            .or_else(|| self.zones.iter().find(|z| z.key != "@local"))
                            .map_or_else(|| "@local".into(), |z| z.key.clone()),
                        label: "New clock".into(),
                        desktop: true,
                        tray: false,
                        style: CardStyle::default(),
                    });
                    let index = e.draft.borrow().clocks.len() - 1;
                    self.select_clock(&e, index);
                    SetFocus(e.control(LABEL));
                    SendMessageW(e.control(LABEL), EM_SETSEL, 0, -1);
                }
                REMOVE => {
                    if !e.draft.borrow_mut().remove_clock(index) {
                        return;
                    }
                    self.select_clock(&e, index.min(e.draft.borrow().clocks.len() - 1));
                }
                UP | DOWN => {
                    let len = e.draft.borrow().clocks.len();
                    let other = if id == UP {
                        index.saturating_sub(1)
                    } else {
                        (index + 1).min(len - 1)
                    };
                    e.draft.borrow_mut().clocks.swap(index, other);
                    self.select_clock(&e, other);
                }
                SAVE | IDOK => {
                    let mut draft = e.draft.borrow().clone();
                    for (i, c) in draft.clocks.iter_mut().enumerate() {
                        c.label = c.label.trim().to_owned();
                        if c.label.is_empty() || !c.has_valid_destination() {
                            self.select_clock(&e, i);
                            MessageBoxW(
                                hwnd,
                                if c.label.is_empty() {
                                    w!("Give this clock a display name.")
                                } else {
                                    w!("Choose Desktop, Taskbar, or both for this clock.")
                                },
                                w!("World Clock"),
                                MB_OK | MB_ICONINFORMATION,
                            );
                            SetFocus(e.control(if c.label.is_empty() { LABEL } else { DESKTOP }));
                            return;
                        }
                    }
                    let check =
                        |id| SendMessageW(e.control(id), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                    draft.twelve = check(TWELVE);
                    draft.seconds = check(SECONDS);
                    draft.topmost = check(ONTOP);
                    draft.check_updates = check(AUTO_UPDATE);
                    let startup = check(STARTUP);
                    if startup != e.startup
                        && let Err(err) = settings::set_startup(startup)
                    {
                        error(hwnd, &err);
                        return;
                    }
                    if let Err(err) = draft.save() {
                        if startup != e.startup {
                            let _ = settings::set_startup(e.startup);
                        }
                        error(hwnd, &err);
                        return;
                    }
                    *self.settings.borrow_mut() = draft;
                    self.position_desktop(false);
                    self.update_tray(true);
                    self.update_taskbar(true);
                    DestroyWindow(hwnd);
                }
                APPEARANCE => {
                    let clock = e.draft.borrow().clocks[index].clone();
                    let zone = self
                        .zones
                        .iter()
                        .find(|z| z.key == clock.zone)
                        .unwrap_or(&self.zones[0]);
                    let twelve =
                        SendMessageW(e.control(TWELVE), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                    let seconds =
                        SendMessageW(e.control(SECONDS), BM_GETCHECK, 0, 0) == BST_CHECKED as isize;
                    if let Some((style, all)) =
                        appearance::edit(hwnd, &clock, zone, twelve, seconds)
                    {
                        let mut draft = e.draft.borrow_mut();
                        if all {
                            for c in &mut draft.clocks {
                                c.style = style.clone();
                            }
                        } else {
                            draft.clocks[index].style = style;
                        }
                    }
                }
                POSITION => {
                    self.editor_command(SAVE, 0);
                    if self.editor_hwnd.get().is_null() && !self.moving.get() {
                        self.move_desktop();
                    }
                }
                CANCEL | IDCANCEL => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
        }
    }
}

unsafe extern "system" fn desktop_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let a = app();
        if msg == a.taskbar_created && msg != 0 {
            a.tray_ids.borrow_mut().clear();
            a.update_tray(true);
            a.location.set(None);
            return 0;
        }
        match msg {
            WM_PAINT => {
                let mut ps = zeroed();
                BeginPaint(hwnd, &mut ps);
                EndPaint(hwnd, &ps);
                // The layered window retains its complete frame across expose events.
                0
            }
            WM_ERASEBKGND => 1,
            WM_TIMER if wparam == 2 => {
                a.paint_desktop(false);
                0
            }
            WM_TIMER => {
                a.poll_updates();
                let tick = a.ticks.get().wrapping_add(1);
                a.ticks.set(tick);
                let previous = a.minute.get();
                a.update_tray(false);
                let changed = previous != a.minute.get();
                a.paint_desktop(false);
                desktop::keep_above_desktop(hwnd);
                a.update_taskbar(changed);
                if tick == 8 && std::env::args().any(|s| s == "--verify-overlays") {
                    a.verify_overlays();
                }
                0
            }
            WM_TIMECHANGE | WM_SETTINGCHANGE | WM_FONTCHANGE | WM_THEMECHANGED => {
                ui::invalidate_caches();
                a.minute.set(-1);
                a.update_tray(false);
                a.position_desktop(false);
                a.update_taskbar(true);
                0
            }
            WM_DPICHANGED => {
                if !a.hwnd.get().is_null() {
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
                    a.position_desktop(false);
                }
                0
            }
            WM_DISPLAYCHANGE => {
                a.position_desktop(false);
                a.update_taskbar(true);
                0
            }
            WM_EXITSIZEMOVE => {
                let mut r = zeroed();
                GetWindowRect(hwnd, &mut r);
                {
                    let mut s = a.settings.borrow_mut();
                    s.x = r.left;
                    s.y = r.top;
                }
                a.persist();
                0
            }
            WM_NCLBUTTONDBLCLK | WM_LBUTTONDBLCLK if a.moving.get() => {
                a.move_desktop();
                0
            }
            WM_NCHITTEST => {
                if a.moving.get() {
                    HTCAPTION as isize
                } else {
                    HTTRANSPARENT as isize
                }
            }
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            TRAY_MSG => {
                match lparam as u32 & 0xffff {
                    NIN_SELECT | NIN_KEYSELECT | NIN_BALLOONUSERCLICK => a.open_editor(),
                    WM_CONTEXTMENU => a.menu(),
                    _ => {}
                }
                0
            }
            REOPEN_MSG => {
                a.open_editor();
                0
            }
            WM_CLOSE => {
                if a.moving.get() {
                    a.move_desktop();
                }
                0
            }
            WM_QUERYENDSESSION => {
                a.persist();
                1
            }
            WM_DESTROY => {
                let e = a.editor_hwnd.get();
                if !e.is_null() {
                    DestroyWindow(e);
                }
                let bar = a.bar_hwnd.get();
                if !bar.is_null() {
                    DestroyWindow(bar);
                }
                a.clear_tray();
                KillTimer(hwnd, 1);
                KillTimer(hwnd, 2);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe extern "system" fn taskbar_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                // UpdateLayeredWindow retains the last complete frame across activation.
                let mut ps = zeroed();
                BeginPaint(hwnd, &mut ps);
                EndPaint(hwnd, &ps);
                0
            }
            WM_ERASEBKGND => 1,
            WM_NCHITTEST => HTTRANSPARENT as isize,
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_DESTROY => {
                // Explorer destroys owned popups along with a recreated taskbar.
                // The next update creates and attaches a fresh overlay.
                let a = app();
                if a.bar_hwnd.get() == hwnd {
                    a.bar_hwnd.set(null_mut());
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe extern "system" fn editor_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                SetTextColor(wparam as HDC, ui::rgb(30, 38, 48));
                SetBkColor(wparam as HDC, GetSysColor(COLOR_WINDOW));
                GetSysColorBrush(COLOR_WINDOW) as isize
            }
            WM_COMMAND => {
                app().editor_command((wparam & 0xffff) as i32, ((wparam >> 16) & 0xffff) as u32);
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
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
                // Keep native controls aligned on displays with different scaling.
                if let Some(e) = app().editor() {
                    let dpi = GetDpiForWindow(hwnd).max(96);
                    let new_font = ui::font(ui::px(13, dpi), 400, "Segoe UI");
                    for (control, r) in &e.layout {
                        SetWindowPos(
                            *control,
                            null_mut(),
                            ui::px(r.left, dpi),
                            ui::px(r.top, dpi),
                            ui::px(r.right - r.left, dpi),
                            ui::px(r.bottom - r.top, dpi),
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                        SendMessageW(*control, WM_SETFONT, new_font as usize, 1);
                    }
                    let heading_font = ui::font(ui::px(22, dpi), 600, "Segoe UI");
                    SendMessageW(e.heading, WM_SETFONT, heading_font as usize, 1);
                    DeleteObject(e.heading_font.replace(heading_font));
                    DeleteObject(e.font.replace(new_font));
                }
                0
            }
            WM_DESTROY => {
                let a = app();
                a.editor_hwnd.set(null_mut());
                let e = a.editor.borrow_mut().take();
                if let Some(e) = e {
                    DeleteObject(e.font.get());
                    DeleteObject(e.heading_font.get());
                }
                a.paint_desktop(true);
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
    fn filtered_zone_picker_preserves_identity_and_requires_an_explicit_choice() {
        unsafe {
            struct TestWindow(HWND);
            impl Drop for TestWindow {
                fn drop(&mut self) {
                    unsafe { DestroyWindow(self.0) };
                }
            }
            let parent = TestWindow(CreateWindowExW(
                0,
                w!("STATIC"),
                null(),
                WS_POPUP,
                0,
                0,
                400,
                200,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            ));
            assert!(!parent.0.is_null());
            let combo = CreateWindowExW(
                0,
                w!("COMBOBOX"),
                null(),
                WS_CHILD | CBS_DROPDOWNLIST as u32 | CBS_HASSTRINGS as u32,
                0,
                0,
                400,
                200,
                parent.0,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!combo.is_null());
            let zones: Vec<_> = [
                ("@local", "Local"),
                ("GMT Standard Time", "London"),
                ("Tokyo Standard Time", "Tokyo"),
            ]
            .into_iter()
            .map(|(key, display)| Zone {
                key: key.into(),
                display: display.into(),
                info: None,
            })
            .collect();
            populate_zone_choices(combo, &zones, "", "GMT Standard Time");
            assert_eq!(SendMessageW(combo, CB_GETCOUNT, 0, 0), 3);
            assert_eq!(selected_zone_index(combo), Some(1));

            populate_zone_choices(combo, &zones, "tokyo", "GMT Standard Time");
            assert_eq!(SendMessageW(combo, CB_GETCOUNT, 0, 0), 2);
            assert_eq!(selected_zone_index(combo), None);
            SendMessageW(combo, CB_SETCURSEL, 1, 0);
            assert_eq!(selected_zone_index(combo), Some(2));

            populate_zone_choices(combo, &zones, "no such zone", "Tokyo Standard Time");
            assert_eq!(IsWindowEnabled(combo), 0);
            assert_eq!(selected_zone_index(combo), None);

            populate_zone_choices(combo, &zones, "", "Tokyo Standard Time");
            assert_ne!(IsWindowEnabled(combo), 0);
            assert_eq!(selected_zone_index(combo), Some(2));
            populate_zone_choices(combo, &zones, "tokyo", "Tokyo Standard Time");
            assert_eq!(SendMessageW(combo, CB_GETCOUNT, 0, 0), 1);
            assert_eq!(selected_zone_index(combo), Some(2));
        }
    }
}
