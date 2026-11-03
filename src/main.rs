#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod appearance;
mod capture;
mod desktop;
mod i18n;
mod render;
mod render_diagnostic;
mod reservation;
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
    let args: Vec<_> = std::env::args_os().collect();
    if args
        .get(1)
        .is_some_and(|arg| arg == "--diagnose-reservation")
    {
        let result = args
            .get(2)
            .ok_or("Missing diagnostic output path.".into())
            .and_then(|path| reservation::diagnose(path.into()));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
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
