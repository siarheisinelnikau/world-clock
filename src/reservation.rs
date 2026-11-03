//! Embedded, opt-in primary-taskbar layout integration. No external installer.
use crate::{taskbar, time::wide};
use std::{
    path::PathBuf,
    ptr::{null, null_mut},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::*},
    UI::WindowsAndMessaging::*,
};

const MODULE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/WorldClock.Taskbar.dll"));
const CLASS: windows_sys::core::PCWSTR = windows_sys::core::w!("WorldClock.TaskbarReservation.v6");
const QUERY: u32 = WM_APP + 42;
#[repr(C)]
struct Command {
    size: u32,
    owner: u32,
    pixels: i32,
    available: i32,
    dpi: u32,
}

fn module_path() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let root = executable
        .parent()
        .ok_or("The executable folder is unavailable.")?;
    let hash = MODULE.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    let directory = root.join(".WorldClock-taskbar");
    std::fs::create_dir_all(&directory)
        .map_err(|_| "The application folder must be writable to reserve taskbar space.")?;
    let path = directory.join(format!("WorldClock.Taskbar-{hash:016x}.dll"));
    if !path.exists() {
        std::fs::write(&path, MODULE)
            .map_err(|_| "The application folder must be writable to reserve taskbar space.")?;
    }
    if std::fs::read(&path).map_err(|error| error.to_string())? != MODULE {
        return Err("The embedded taskbar module could not be verified.".into());
    }
    Ok(path)
}

pub fn connect() -> Result<isize, String> {
    unsafe {
        let _apartment = Apartment::new();
        let (bar, _) = taskbar::native_bar().ok_or("The Windows taskbar is unavailable.")?;
        let mut pid = 0;
        GetWindowThreadProcessId(bar, &mut pid);
        let find = || {
            let window = FindWindowExW(HWND_MESSAGE, null_mut(), CLASS, null());
            let mut owner = 0;
            GetWindowThreadProcessId(window, &mut owner);
            if owner == pid { window } else { null_mut() }
        };
        let existing = find();
        if !existing.is_null() {
            return Ok(existing as isize);
        }
        let path = module_path()?;
        let module = LoadLibraryW(wide(&path.to_string_lossy()).as_ptr());
        if module.is_null() {
            return Err(format!("Module load failed: {}", GetLastError()));
        }
        let export = GetProcAddress(module, c"WorldClockConnect".as_ptr().cast())
            .ok_or("Missing taskbar connector entry point.")?;
        type Connect = unsafe extern "system" fn(u32, *const u16, *const u16) -> i32;
        let connect: Connect = std::mem::transmute(export);
        let result = connect(pid, null(), wide(&path.to_string_lossy()).as_ptr());
        if result < 0 {
            return Err(format!(
                "Could not connect to the taskbar (0x{result:08X})."
            ));
        }
        // The connection may finish loading its client asynchronously.
        // Keep the diagnostics runtime loaded until this process exits.
        for _ in 0..400 {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let window = find();
            if !window.is_null() {
                return Ok(window as isize);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Err("The taskbar layout could not be accessed. Choose another placement mode.".into())
    }
}

pub fn apply(window: isize, pixels: i32, available: i32, dpi: u32) -> bool {
    let command = Command {
        size: std::mem::size_of::<Command>() as u32,
        owner: std::process::id(),
        pixels,
        available,
        dpi,
    };
    let data = COPYDATASTRUCT {
        dwData: 0x57435231,
        cbData: command.size,
        lpData: (&command as *const Command).cast_mut().cast(),
    };
    let mut result = 0;
    unsafe {
        SendMessageTimeoutW(
            window as HWND,
            WM_COPYDATA,
            0,
            &data as *const _ as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        ) != 0
            && result != 0
    }
}

pub fn query(window: isize, metric: usize) -> usize {
    let mut result = 0;
    unsafe {
        SendMessageTimeoutW(
            window as HWND,
            QUERY,
            metric,
            0,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        );
    }
    result
}

struct Apartment(bool);
impl Apartment {
    fn new() -> Self {
        Self(unsafe {
            windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            )
            .is_ok()
        })
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub bar: usize,
    pub pixels: i32,
    pub available: i32,
    pub dpi: u32,
}
#[derive(Debug, Default)]
pub enum Status {
    #[default]
    Inactive,
    Connecting,
    Active,
    Failed(String),
}
#[derive(Default)]
pub struct Controller {
    sender: Option<std::sync::mpsc::Sender<Option<Request>>>,
    receiver: Option<std::sync::mpsc::Receiver<(Request, Result<(), String>)>>,
    requested: Option<Request>,
    pub status: Status,
}
impl Controller {
    pub fn fail(&mut self, message: &str) {
        self.request(None);
        self.status = Status::Failed(message.into());
    }
    pub fn request(&mut self, request: Option<Request>) {
        if request == self.requested {
            if request.is_none() {
                self.status = Status::Inactive;
            }
            return;
        }
        self.requested = request;
        if self.sender.is_none() && request.is_some() {
            let (send, receive) = std::sync::mpsc::channel::<Option<Request>>();
            let (done, results) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _apartment = Apartment::new();
                struct Connection(isize);
                impl Drop for Connection {
                    fn drop(&mut self) {
                        if self.0 != 0 {
                            apply(self.0, 0, 0, 96);
                        }
                    }
                }
                let mut connection = Connection(0);
                while let Ok(mut request) = receive.recv() {
                    while let Ok(newer) = receive.try_recv() {
                        request = newer;
                    }
                    let Some(request) = request else {
                        connection = Connection(0);
                        continue;
                    };
                    let result = (|| {
                        let mut bar_pid = 0;
                        let mut connector_pid = 0;
                        unsafe {
                            GetWindowThreadProcessId(request.bar as HWND, &mut bar_pid);
                            GetWindowThreadProcessId(connection.0 as HWND, &mut connector_pid);
                        }
                        if connection.0 == 0 || bar_pid == 0 || bar_pid != connector_pid {
                            connection = Connection(connect()?);
                        }
                        if !apply(connection.0, request.pixels, request.available, request.dpi) {
                            return Err("The Windows taskbar could not reserve space. Choose another placement mode.".into());
                        }
                        Ok(())
                    })();
                    if done.send((request, result)).is_err() {
                        break;
                    }
                }
            });
            self.sender = Some(send);
            self.receiver = Some(results);
        }
        self.status = if request.is_some() {
            Status::Connecting
        } else {
            Status::Inactive
        };
        if let Some(sender) = &self.sender {
            let _ = sender.send(request);
        }
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(receiver) = &self.receiver {
            while let Ok((request, result)) = receiver.try_recv() {
                if self.requested == Some(request) {
                    self.status = match result {
                        Ok(()) => Status::Active,
                        Err(error) => Status::Failed(error),
                    };
                    changed = true;
                }
            }
        }
        changed
    }
    pub fn ready(&self) -> bool {
        matches!(self.status, Status::Active)
    }
    pub fn message(&self) -> &str {
        match &self.status {
            Status::Inactive => "Choose how clocks share taskbar space with app buttons.",
            Status::Connecting => "Reserving taskbar space…",
            Status::Active => "Space is reserved for the taskbar clocks.",
            Status::Failed(error) => error,
        }
    }
}

pub fn diagnose(path: PathBuf) -> Result<(), String> {
    let window = connect()?;
    if query(window, 4) != 0 {
        return Err("Another World Clock instance already reserves taskbar space.".into());
    }
    struct Reset(isize);
    impl Drop for Reset {
        fn drop(&mut self) {
            apply(self.0, 0, 0, 96);
        }
    }
    let _reset = Reset(window);
    let metrics = || {
        [
            query(window, 0),
            query(window, 1),
            query(window, 2),
            query(window, 3),
        ]
    };
    let geometry = || {
        taskbar::diagnostic_location().map(|location| serde_json::json!({"buttons_end":location.buttons_end,"tray_start":location.tray.left}))
    };
    let before = metrics();
    let before_geometry = geometry();
    let (bar, _) = unsafe { taskbar::native_bar() }.ok_or("The taskbar disappeared.")?;
    let mut explorer_pid = 0;
    unsafe {
        GetWindowThreadProcessId(bar, &mut explorer_pid);
    }
    fn cpu(pid: u32) -> f64 {
        use windows_sys::Win32::System::Threading::*;
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return f64::NAN;
            }
            let (mut creation, mut exit, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            let ok = GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user);
            CloseHandle(process);
            if ok == 0 {
                return f64::NAN;
            }
            let ticks =
                |time: FILETIME| ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
            (ticks(kernel) + ticks(user)) as f64 / 10_000_000.0
        }
    }
    let mut samples = Vec::new();
    let mut sample = |state: &str| {
        let started = std::time::Instant::now();
        let host = cpu(std::process::id());
        let explorer = cpu(explorer_pid);
        let writes = query(window, 3);
        std::thread::sleep(Duration::from_secs(5));
        let seconds = started.elapsed().as_secs_f64();
        samples.push(serde_json::json!({"state":state,"seconds":seconds,"host_cpu_one_core_percent":100.0*(cpu(std::process::id())-host)/seconds,"explorer_cpu_one_core_percent":100.0*(cpu(explorer_pid)-explorer)/seconds,"layout_writes":query(window,3)-writes}));
    };
    sample("before_reservation");
    let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(bar) }.max(96);
    let pixels = crate::ui::px(600, dpi);
    let mut controller = Controller::default();
    controller.request(Some(Request {
        bar: bar as usize,
        pixels,
        available: crate::ui::px(1200, dpi),
        dpi,
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    while !controller.ready() && std::time::Instant::now() < deadline {
        controller.poll();
        if let Status::Failed(error) = &controller.status {
            return Err(error.clone());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if !controller.ready() {
        return Err("The taskbar reservation worker timed out.".into());
    }
    std::thread::sleep(Duration::from_millis(250));
    let during = metrics();
    let during_geometry = geometry();
    if std::env::args_os().any(|arg| arg == "--hold-reservation") {
        std::fs::write(&path,serde_json::to_vec_pretty(&serde_json::json!({"ready":true,"pid":std::process::id(),"before":before,"during":during})).unwrap()).map_err(|error|error.to_string())?;
        std::thread::sleep(Duration::from_secs(20));
        return Ok(());
    }
    sample("reservation_active");
    controller.request(None);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while query(window, 2) != 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    std::thread::sleep(Duration::from_millis(250));
    let after = metrics();
    let after_geometry = geometry();
    sample("after_restore");
    let passed = before[0] == during[0] + 600
        && before[0] == after[0]
        && before[1] == after[1]
        && during[2] == 600
        && after[2] == 0
        && samples.iter().all(|sample| sample["layout_writes"] == 0);
    std::fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"executable":std::env::current_exe().ok(),"passed":passed,"metrics":["frame_width_dip","right_margin_dip","reserved_dip","layout_writes_total"],"before":before,"during":during,"after":after,"geometry_before":before_geometry,"geometry_during":during_geometry,"geometry_after":after_geometry,"samples":samples,"scope":"Real primary Windows taskbar; no input injection, settings changes, or Explorer restart. No GPU timing or input latency measurement."})).unwrap()).map_err(|error|error.to_string())?;
    if !passed {
        return Err("Taskbar reservation/restore verification failed.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_completion_cannot_override_a_newer_placement_request() {
        let old = Request {
            bar: 1,
            pixels: 200,
            available: 1000,
            dpi: 96,
        };
        let current = Request { pixels: 300, ..old };
        let (send, receive) = std::sync::mpsc::channel();
        let mut controller = Controller {
            receiver: Some(receive),
            requested: Some(current),
            status: Status::Connecting,
            ..Controller::default()
        };
        send.send((old, Err("Previous request failed".into())))
            .unwrap();
        assert!(!controller.poll());
        assert!(matches!(controller.status, Status::Connecting));
        send.send((current, Ok(()))).unwrap();
        assert!(controller.poll());
        assert!(controller.ready());
        controller.request(None);
        send.send((current, Ok(()))).unwrap();
        assert!(!controller.poll());
        assert!(!controller.ready());
        controller.fail("Reservation unavailable");
        controller.request(None);
        assert!(matches!(controller.status, Status::Inactive));
    }
}
