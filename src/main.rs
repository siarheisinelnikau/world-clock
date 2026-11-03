#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod appearance;
mod capture;
mod desktop;
mod i18n;
mod render;
mod render_diagnostic;
mod settings;
mod taskbar;
mod taskbar_effects;
mod theme;
mod time;
mod ui;
// Keep the pre-shared-updater helper for installations staged by 0.4.x.
#[allow(dead_code)]
mod update;
mod updates;

fn main() {
    match updates::command_mode() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("Update failed: {error}");
            std::process::exit(1);
        }
    }
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
