#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod appearance;
mod settings;
mod taskbar;
mod time;
mod ui;

fn main() {
    if std::env::args().any(|s| s == "--watch-taskbar") {
        taskbar::watch_overlay();
        return;
    }
    if std::env::args().any(|s| s == "--preview-style") {
        unsafe {
            use windows_sys::Win32::UI::Controls::*;
            InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES,
            });
            let clock = settings::Settings::default().clocks.remove(0);
            let zone = time::zones().remove(0);
            appearance::edit(std::ptr::null_mut(), &clock, &zone, false, false);
        }
        return;
    }
    if std::env::args().any(|s| s == "--inspect-taskbar") {
        taskbar::inspect();
        return;
    }
    app::run();
}
