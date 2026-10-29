//! World Clock identity; the reusable update engine is a sibling workspace.
use rust_auto_updator::{Config, Controller, State};
pub const WAKE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 3;

pub fn config() -> Result<Config, String> {
    let mut config = Config::new(
        "siarheisinelnikau",
        "world-clock",
        env!("CARGO_PKG_VERSION"),
        "WorldClock",
    )?;
    config.install.stage_prefix = ".WorldClock-update-".into();
    config.expected_subsystem = Some(2);
    config.install.restart_args = vec!["--settings".into()];
    Ok(config)
}

pub fn command_mode() -> Result<bool, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let config = config()?;
    if args.first().is_some_and(|arg| arg == "--apply-update") {
        if args.len() == 4 {
            // Older installed versions stage their original helper with PID/target/hash arguments.
            return Ok(crate::update::handle_helper_args());
        }
        return rust_auto_updator::apply_update(&config).map(|()| true);
    }
    if let Some(index) = args.iter().position(|arg| arg == "--cleanup-update") {
        let path = args
            .get(index + 1)
            .ok_or("Missing update cleanup directory.")?;
        if !std::path::Path::new(path).join("update.json").is_file() {
            // Retain the bounded, validated cleanup protocol used by installed 0.4.x clients.
            crate::update::handle_helper_args();
            return Ok(false);
        }
        rust_auto_updator::cleanup_after_update(path.into(), config.install);
    }
    Ok(false)
}

pub fn status(controller: &Controller) -> String {
    let status = if let Some(reason) = controller.disabled_reason() {
        reason.to_owned()
    } else {
        match controller.state() {
            State::Idle => "No update check yet.".into(),
            State::Checking => "Checking GitHub for updates…".into(),
            State::Current => "You are up to date.".into(),
            State::Available(release) => format!(
                "Version {} is available. Download and install to verify the update and restart. Your saved clocks and settings are preserved.",
                release.version
            ),
            State::Downloading { received, total } => format!(
                "Downloading and verifying: {}% ({:.1} / {:.1} MB)",
                received.saturating_mul(100) / total.max(&1),
                *received as f64 / 1048576.0,
                *total as f64 / 1048576.0
            ),
            State::Installing => "Preparing installation and restart…".into(),
            State::Error(error) => {
                format!("Update failed: {error}\nYour current installation is unchanged.")
            }
        }
    };
    format!(
        "Installed version: {}\n\n{status}",
        controller.installed_version()
    )
}
