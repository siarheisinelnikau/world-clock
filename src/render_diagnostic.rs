//! Finite, app-owned renderer diagnostics. No input injection or desktop capture.
use serde::Serialize;
use std::{
    cell::Cell,
    path::PathBuf,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{FILETIME, HWND},
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{GetCurrentProcess, GetProcessTimes},
    },
    UI::WindowsAndMessaging::*,
};

#[derive(Clone, Copy, Default, Serialize)]
pub struct Counts {
    pub paints: u64,
    pub rasters: u64,
    pub presents: u64,
    pub native_controls: u64,
    pub data_changes: u64,
}
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts { paints:0, rasters:0, presents:0, native_controls:0, data_changes:0 }) };
}
pub fn count(kind: &str) {
    if !ACTIVE.get() {
        return;
    }
    COUNTS.with(|counts| {
        let mut value = counts.get();
        match kind {
            "paint" => value.paints += 1,
            "raster" => value.rasters += 1,
            "present" => value.presents += 1,
            "native" => value.native_controls += 1,
            "data" => value.data_changes += 1,
            _ => {}
        }
        counts.set(value);
    });
}
fn counts() -> Counts {
    COUNTS.get()
}
fn cpu_seconds() -> f64 {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: current-process pseudo-handle is valid and every output is a live FILETIME.
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return f64::NAN;
    }
    let ticks = |time: FILETIME| ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
    (ticks(kernel) + ticks(user)) as f64 / 10_000_000.0
}
#[derive(Serialize)]
struct Sample {
    state: &'static str,
    seconds: f64,
    cpu_one_core_percent: f64,
    cpu_machine_percent: f64,
    visible: bool,
    minimized: bool,
    frames: Counts,
}
pub struct Diagnostic {
    pub taskbar: bool,
    pub tray_activation_checks: Vec<serde_json::Value>,
    path: PathBuf,
    phase: usize,
    started: Instant,
    baseline: Option<(Instant, f64, Counts)>,
    samples: Vec<Sample>,
}
impl Diagnostic {
    pub fn phase(&self) -> usize {
        self.phase
    }
    pub fn begin(&mut self) {
        self.started = Instant::now();
    }
    pub fn from_args() -> Result<Option<Self>, String> {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if !args
            .iter()
            .any(|arg| arg == "--diagnose-render" || arg == "--diagnose-taskbar")
        {
            return Ok(None);
        }
        if args.len() != 2 || (args[0] != "--diagnose-render" && args[0] != "--diagnose-taskbar") {
            return Err("Usage: --diagnose-render|--diagnose-taskbar <output.json>".into());
        }
        ACTIVE.set(true);
        Ok(Some(Self {
            taskbar: args[0] == "--diagnose-taskbar",
            tray_activation_checks: Vec::new(),
            path: (&args[1]).into(),
            phase: 0,
            started: Instant::now(),
            baseline: None,
            samples: Vec::new(),
        }))
    }
    /// Called by the host's existing timer; transitions affect only its own window.
    pub fn tick(&mut self, window: HWND) -> bool {
        if self.baseline.is_none() {
            if self.started.elapsed() >= Duration::from_secs(1) {
                self.baseline = Some((Instant::now(), cpu_seconds(), counts()));
            }
            return false;
        }
        let (started, cpu, before) = self.baseline.as_ref().unwrap();
        if started.elapsed() < Duration::from_secs(5) {
            return false;
        }
        let seconds = started.elapsed().as_secs_f64();
        let cpu = 100.0 * (cpu_seconds() - cpu) / seconds;
        let after = counts();
        // SAFETY: the host supplies its own live HWND on its UI thread.
        let (visible, minimized) = unsafe { (IsWindowVisible(window) != 0, IsIconic(window) != 0) };
        let processors = std::thread::available_parallelism().map_or(1, |count| count.get());
        self.samples.push(Sample {
            state: [
                "visible_idle",
                "hidden_idle",
                "minimized_idle",
                "restored_idle",
            ][self.phase],
            seconds,
            cpu_one_core_percent: cpu,
            cpu_machine_percent: cpu / processors as f64,
            visible,
            minimized,
            frames: Counts {
                paints: after.paints - before.paints,
                rasters: after.rasters - before.rasters,
                presents: after.presents - before.presents,
                native_controls: after.native_controls - before.native_controls,
                data_changes: after.data_changes - before.data_changes,
            },
        });
        self.phase += 1;
        let overloaded = cpu > 5.0 || after.rasters - before.rasters > 20;
        if self.phase == 4 || overloaded {
            let graphics_modules: Vec<_> =
                ["opengl32.dll", "d3d11.dll", "d3d12.dll", "vulkan-1.dll"]
                    .into_iter()
                    .filter(|name| {
                        let wide: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
                        // SAFETY: read-only lookup of a live, terminated module name; does not load it.
                        unsafe { !GetModuleHandleW(wide.as_ptr()).is_null() }
                    })
                    .collect();
            let tray_failed = self
                .tray_activation_checks
                .iter()
                .any(|check| check["passed"] != true);
            let report = serde_json::json!({"executable":std::env::current_exe().ok(), "profile":if cfg!(debug_assertions){"debug"}else{"release"}, "gate_failed":overloaded || tray_failed,
                "surface":if self.taskbar {"taskbar fixture: crowded, hiding disabled; backdrop toggled twice before settling"} else {"desktop clocks"},
                "renderer":"CPU rasterization / Windows GDI", "logical_processors_available":processors,
                "graphics_modules_loaded":graphics_modules, "total_frames":after, "samples":self.samples,
                "tray_activation_checks":self.tray_activation_checks,
                "isolation":"Fixture settings; no tray, hotkeys, hooks, correction worker, network, autostart changes, or system taskbar attachment. No injected input.",
                "scope":"Real production window/painter and message loop, finite idle states; background services excluded. No GPU timing or input-latency measurement."});
            let result = (|| {
                if let Some(parent) = self
                    .path
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                std::fs::write(
                    &self.path,
                    serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())
            })();
            if let Err(error) = result {
                eprintln!("Renderer diagnostic failed: {error}");
            }
            return true;
        }
        // SAFETY: these finite diagnostic transitions touch only the app-owned window;
        // no user input, foreground activation, or other application's state is changed.
        unsafe {
            ShowWindow(
                window,
                match self.phase {
                    1 => SW_HIDE,
                    2 => SW_SHOWMINNOACTIVE,
                    _ => SW_SHOWNOACTIVATE,
                },
            );
        }
        self.started = Instant::now();
        self.baseline = None;
        false
    }
}
