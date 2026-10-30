//! GitHub Releases updater. Network and staging work never run on the UI thread.
use crate::time::{unw, wide};
use serde::Deserialize;
use std::{
    ffi::c_void,
    fs::{self, File},
    io::Write,
    os::windows::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::{
    Win32::{
        Foundation::*,
        Networking::WinHttp::*,
        Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash},
        Storage::FileSystem::ReplaceFileW,
        System::Threading::*,
        UI::WindowsAndMessaging::*,
    },
    core::w,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "siarheisinelnikau/world-clock";
const STAGE_PREFIX: &str = ".WorldClock-update-";
const MAX_EXE: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version([u64; 3]);

impl Version {
    fn parse(value: &str) -> Option<Self> {
        let value = value.strip_prefix('v').unwrap_or(value);
        let mut parts = value.split('.');
        let mut numbers = [0; 3];
        for number in &mut numbers {
            let part = parts.next()?;
            if part.is_empty()
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return None;
            }
            *number = part.parse().ok()?;
        }
        parts.next().is_none().then_some(Self(numbers))
    }
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    name: String,
    size: usize,
    browser_download_url: String,
}

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    executable: Asset,
    checksums: Asset,
}

fn parse_release(json: &[u8], current: &str) -> Result<Option<Release>, String> {
    let release: GitHubRelease = serde_json::from_slice(json)
        .map_err(|_| "GitHub returned invalid release information.".to_owned())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(&release.tag_name)
        .ok_or("The latest release does not have a supported stable version.")?;
    if version <= Version::parse(current).ok_or("Invalid application version.")? {
        return Ok(None);
    }
    let asset = |name: &str, limit| -> Result<Asset, String> {
        let mut matches = release.assets.iter().filter(|asset| asset.name == name);
        let found = matches.next().ok_or_else(|| {
            format!("The release is missing {name}. Please try again after it finishes publishing.")
        })?;
        let expected = format!(
            "https://github.com/{REPO}/releases/download/{}/{name}",
            release.tag_name
        );
        if matches.next().is_some()
            || found.browser_download_url != expected
            || found.size == 0
            || found.size > limit
        {
            return Err(format!("The release contains an invalid {name} asset."));
        }
        Ok(found.clone())
    };
    // Published releases before 0.4.1 used an unversioned filename.
    let executable_name = if version < Version([0, 4, 1]) {
        "WorldClock.exe".to_owned()
    } else {
        format!(
            "WorldClock-v{}.exe",
            release.tag_name.trim_start_matches('v')
        )
    };
    Ok(Some(Release {
        executable: asset(&executable_name, MAX_EXE)?,
        checksums: asset("SHA256SUMS.txt", 16 * 1024)?,
        tag: release.tag_name,
    }))
}

struct Internet(*mut c_void);
impl Internet {
    fn new(handle: *mut c_void) -> Result<Self, String> {
        if handle.is_null() {
            Err(network_error())
        } else {
            Ok(Self(handle))
        }
    }
}
impl Drop for Internet {
    fn drop(&mut self) {
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn network_error() -> String {
    format!(
        "Could not contact GitHub ({}). Check your connection and try again.",
        std::io::Error::last_os_error()
    )
}

fn checked(ok: i32) -> Result<(), String> {
    if ok == 0 {
        Err(network_error())
    } else {
        Ok(())
    }
}

fn download(
    host: &str,
    path: &str,
    limit: usize,
    mut progress: impl FnMut(usize),
) -> Result<Vec<u8>, String> {
    unsafe {
        let session = Internet::new(WinHttpOpen(
            wide(&format!("WorldClock/{VERSION}")).as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        checked(WinHttpSetTimeouts(
            session.0, 10_000, 15_000, 15_000, 15_000,
        ))?;
        let connection = Internet::new(WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0))?;
        let request = Internet::new(WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            wide(path).as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
        checked(WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            (&policy as *const u32).cast(),
            4,
        ))?;
        let headers = wide(if host == "api.github.com" {
            "Accept: application/vnd.github+json\r\nCache-Control: no-cache\r\n"
        } else {
            "Accept: application/octet-stream\r\nCache-Control: no-cache\r\n"
        });
        checked(WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            (headers.len() - 1) as u32,
            null(),
            0,
            0,
            0,
        ))?;
        checked(WinHttpReceiveResponse(request.0, null_mut()))?;
        let mut status = 0u32;
        let mut size = 4;
        checked(WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            null_mut(),
        ))?;
        match status {
            200 => {}
            403 | 429 => {
                return Err("GitHub is limiting update requests. Please try again later.".into());
            }
            404 => return Err("No published release was found on GitHub.".into()),
            _ => {
                return Err(format!(
                    "GitHub returned HTTP {status}. Please try again later."
                ));
            }
        }
        let started = Instant::now();
        let mut body = Vec::new();
        let mut buffer = [0u8; 32 * 1024];
        loop {
            if started.elapsed() > Duration::from_secs(180) {
                return Err("The download timed out. Please try again.".into());
            }
            let mut read = 0;
            checked(WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            ))?;
            if read == 0 {
                break;
            }
            if body.len() + read as usize > limit {
                return Err("GitHub returned a file larger than expected.".into());
            }
            body.extend_from_slice(&buffer[..read as usize]);
            progress(body.len());
        }
        Ok(body)
    }
}

fn check_release() -> Result<Option<Release>, String> {
    let json = download(
        "api.github.com",
        &format!("/repos/{REPO}/releases/latest"),
        1024 * 1024,
        |_| {},
    )?;
    parse_release(&json, VERSION)
}

fn download_asset(asset: &Asset, progress: impl FnMut(usize)) -> Result<Vec<u8>, String> {
    // parse_release has already restricted the URL to this repository and exact tag/asset.
    let path = asset
        .browser_download_url
        .strip_prefix("https://github.com/")
        .ok_or("Invalid GitHub download URL.")?;
    let body = download("github.com", &format!("/{path}"), asset.size, progress)?;
    if body.len() != asset.size {
        return Err("The downloaded file is incomplete. Please try again.".into());
    }
    Ok(body)
}

fn checksum(text: &[u8], executable_name: &str) -> Result<String, String> {
    let text = std::str::from_utf8(text).map_err(|_| "Invalid release checksum file.")?;
    let mut result = None;
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() == 2 && fields[1].strip_prefix('*').unwrap_or(fields[1]) == executable_name
        {
            if result.is_some()
                || fields[0].len() != 64
                || !fields[0].bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err("Invalid release checksum file.".into());
            }
            result = Some(fields[0].to_ascii_lowercase());
        }
    }
    result.ok_or_else(|| format!("The release checksum for {executable_name} is missing."))
}

fn sha256(bytes: &[u8]) -> Result<String, String> {
    let mut hash = [0u8; 32];
    let status = unsafe {
        BCryptHash(
            BCRYPT_SHA256_ALG_HANDLE,
            null(),
            0,
            bytes.as_ptr(),
            bytes.len() as u32,
            hash.as_mut_ptr(),
            32,
        )
    };
    if status < 0 {
        return Err("Windows could not verify the update's SHA-256 checksum.".into());
    }
    Ok(hash.iter().map(|b| format!("{b:02x}")).collect())
}

fn verify(bytes: &[u8], expected: &str) -> Result<(), String> {
    if sha256(bytes)? != expected {
        return Err(
            "The update failed SHA-256 verification. Your current version has not been changed."
                .into(),
        );
    }
    // Reject non-Windows/x64 assets even if their checksum is internally consistent.
    let pe = bytes
        .get(0x3c..0x40)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    if !bytes.starts_with(b"MZ")
        || pe.is_none_or(|p| bytes.get(p..p.saturating_add(6)) != Some(b"PE\0\0\x64\x86"))
    {
        return Err("The release is not a 64-bit Windows application.".into());
    }
    Ok(())
}

pub enum State {
    Idle,
    Checking,
    Current,
    Available(Release),
    Downloading(u8),
    Ready(StagedUpdate),
    Installing,
    Failed(String),
}

impl State {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading(_) | Self::Installing
        )
    }

    pub fn action(&self) -> String {
        match self {
            Self::Checking => "Checking GitHub…".into(),
            Self::Available(_) => "Download and install…".into(),
            Self::Downloading(percent) => format!("Downloading… {percent}%"),
            Self::Ready(_) => "Restart to update…".into(),
            Self::Installing => "Restarting…".into(),
            _ => "Check for updates…".into(),
        }
    }

    pub fn description(&self) -> String {
        let detail = match self {
            Self::Idle => "Updates from GitHub Releases".into(),
            Self::Checking => "Checking GitHub…".into(),
            Self::Current => "You are up to date".into(),
            Self::Available(release) => format!("{} is available", release.tag),
            Self::Downloading(percent) => format!("Downloading update… {percent}%"),
            Self::Ready(_) => "Update ready. Save Settings to restart".into(),
            Self::Installing => "Preparing to restart…".into(),
            Self::Failed(_) => "Update failed. Check for updates to retry".into(),
        };
        format!("World Clock {VERSION} · {detail}")
    }
}

enum Event {
    Checked(Result<Option<Release>, String>),
    Progress(u8),
    Downloaded(Result<StagedUpdate, String>),
    Installer(Result<(), String>),
}

pub enum Notice {
    Changed,
    Checked,
    Downloaded,
    Exit,
}

pub struct Updater {
    pub state: State,
    pub manual: bool,
    rx: Option<Receiver<Event>>,
}

impl Updater {
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            manual: false,
            rx: None,
        }
    }

    fn worker(&mut self, work: impl FnOnce(Sender<Event>) + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        if let Err(e) = thread::Builder::new()
            .name("world-clock-updater".into())
            .spawn(move || work(tx))
        {
            self.rx = None;
            self.state = State::Failed(format!("Could not start the updater: {e}"));
        }
    }

    pub fn check(&mut self, manual: bool) {
        if self.state.busy() {
            return;
        }
        self.manual = manual;
        self.state = State::Checking;
        self.worker(|tx| {
            let _ = tx.send(Event::Checked(check_release()));
        });
    }

    pub fn download(&mut self, release: Release) {
        if self.state.busy() {
            return;
        }
        self.manual = true;
        self.state = State::Downloading(0);
        self.worker(move |tx| {
            let result = stage_update(&release, |percent| {
                let _ = tx.send(Event::Progress(percent));
            });
            let _ = tx.send(Event::Downloaded(result));
        });
    }

    pub fn install(&mut self) {
        if !matches!(self.state, State::Ready(_)) {
            return;
        }
        let State::Ready(stage) = std::mem::replace(&mut self.state, State::Installing) else {
            return;
        };
        self.worker(move |tx| {
            let _ = tx.send(Event::Installer(stage.launch()));
        });
    }

    pub fn poll(&mut self) -> Option<Notice> {
        let rx = self.rx.as_ref()?;
        let event = match rx.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                self.state =
                    State::Failed("The updater stopped unexpectedly. Please try again.".into());
                return Some(Notice::Checked);
            }
        };
        let (state, notice, finished) = match event {
            Event::Checked(Ok(Some(release))) => (State::Available(release), Notice::Checked, true),
            Event::Checked(Ok(None)) => (State::Current, Notice::Checked, true),
            Event::Checked(Err(e)) | Event::Downloaded(Err(e)) | Event::Installer(Err(e)) => {
                (State::Failed(e), Notice::Checked, true)
            }
            Event::Progress(percent) => (State::Downloading(percent), Notice::Changed, false),
            Event::Downloaded(Ok(stage)) => (State::Ready(stage), Notice::Downloaded, true),
            Event::Installer(Ok(())) => (State::Installing, Notice::Exit, true),
        };
        self.state = state;
        if finished {
            self.rx = None;
        }
        Some(notice)
    }
}

pub struct StagedUpdate {
    directory: PathBuf,
    target: PathBuf,
    hash: String,
    handed_off: bool,
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::create(path).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn stage_update(release: &Release, mut progress: impl FnMut(u8)) -> Result<StagedUpdate, String> {
    let target = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(io_error)?;
    let parent = target
        .parent()
        .ok_or("Could not find the application folder.")?;
    let directory = parent.join(format!(
        "{STAGE_PREFIX}{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&directory).map_err(|e| format!("Cannot write to the application folder: {e}. Move World Clock to a writable folder and try again."))?;
    let mut stage = StagedUpdate {
        directory,
        target,
        hash: String::new(),
        handed_off: false,
    };
    stage.hash = checksum(
        &download_asset(&release.checksums, |_| {})?,
        &release.executable.name,
    )?;
    let mut previous = 0;
    let bytes = download_asset(&release.executable, |count| {
        let percent = ((count * 100) / release.executable.size).min(100) as u8;
        if percent != previous {
            previous = percent;
            progress(percent);
        }
    })?;
    verify(&bytes, &stage.hash)?;
    write_file(&stage.directory.join("update.exe"), &bytes)?;
    fs::copy(&stage.target, stage.directory.join("updater.exe")).map_err(io_error)?;
    Ok(stage)
}

fn cleanup(directory: &Path) {
    // Never recurse: remove only the updater's own known files.
    for name in [
        "update.exe",
        "updater.exe",
        "ready",
        "error.txt",
        "previous.exe",
    ] {
        let _ = fs::remove_file(directory.join(name));
    }
    let _ = fs::remove_dir(directory);
}

impl Drop for StagedUpdate {
    fn drop(&mut self) {
        if !self.handed_off {
            cleanup(&self.directory);
        }
    }
}

fn hidden_command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

impl StagedUpdate {
    fn launch(mut self) -> Result<(), String> {
        let mut child = hidden_command(&self.directory.join("updater.exe"))
            .arg("--apply-update")
            .arg(std::process::id().to_string())
            .arg(&self.target)
            .arg(&self.hash)
            .spawn()
            .map_err(|e| format!("Could not start the update: {e}"))?;
        let started = Instant::now();
        loop {
            if self.directory.join("ready").is_file() {
                self.handed_off = true;
                return Ok(());
            }
            if let Ok(message) = fs::read_to_string(self.directory.join("error.txt")) {
                let _ = child.wait();
                return Err(message);
            }
            if child.try_wait().map_err(io_error)?.is_some() {
                return Err(
                    "The update helper could not start. Your current version has not been changed."
                        .into(),
                );
            }
            if started.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "The update helper timed out. Your current version has not been changed."
                        .into(),
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn path_wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn valid_stage(directory: &Path, target: &Path) -> bool {
    directory.parent() == target.parent()
        && directory
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(STAGE_PREFIX))
        && target.is_file()
}

fn replace(target: &Path, replacement: &Path, backup: &Path) -> Result<(), String> {
    if unsafe {
        ReplaceFileW(
            path_wide(target).as_ptr(),
            path_wide(replacement).as_ptr(),
            path_wide(backup).as_ptr(),
            0,
            null(),
            null(),
        )
    } == 0
    {
        return Err(format!(
            "Could not replace World Clock: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn restart(target: &Path, directory: &Path) -> std::io::Result<std::process::Child> {
    hidden_command(target)
        .arg("--settings")
        .arg("--cleanup-update")
        .arg(directory)
        .spawn()
}

fn replace_and_restart(target: &Path, directory: &Path) -> Result<(), String> {
    let backup = directory.join("previous.exe");
    if let Err(e) = replace(target, &directory.join("update.exe"), &backup) {
        // ReplaceFile can partially move the old file on some filesystem errors.
        if !target.exists() && backup.exists() {
            fs::rename(&backup, target).map_err(|restore| {
                format!(
                    "{e}\nThe old executable is at {}: {restore}",
                    backup.display()
                )
            })?;
        }
        let _ = restart(target, directory);
        return Err(e);
    }
    if let Err(e) = restart(target, directory) {
        // Roll back if Windows cannot start the downloaded executable.
        replace(target, &backup, &directory.join("update.exe"))?;
        let _ = restart(target, directory);
        return Err(format!(
            "Could not start the update: {e}. The previous version was restored."
        ));
    }
    Ok(())
}

fn run_helper(parent_pid: u32, target: &Path, hash: &str, directory: &Path) -> Result<(), String> {
    if !valid_stage(directory, target) {
        return Err("Invalid update staging folder.".into());
    }
    let parent = Process(unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            parent_pid,
        )
    });
    if parent.0.is_null() {
        return Err("Could not wait for World Clock to exit.".into());
    }
    let mut image = [0u16; 32768];
    let mut length = image.len() as u32;
    if unsafe { QueryFullProcessImageNameW(parent.0, 0, image.as_mut_ptr(), &mut length) } == 0
        || fs::canonicalize(PathBuf::from(unw(&image[..length as usize]))).map_err(io_error)?
            != target
    {
        return Err("The running application does not match the update destination.".into());
    }
    let update = fs::read(directory.join("update.exe")).map_err(io_error)?;
    if update.len() > MAX_EXE {
        return Err("The update is too large.".into());
    }
    verify(&update, hash)?;
    write_file(&directory.join("ready"), b"ready")?;
    if unsafe { WaitForSingleObject(parent.0, 90_000) } != WAIT_OBJECT_0 {
        return Err("World Clock did not exit. Please retry the update.".into());
    }
    drop(parent);
    if let Err(message) = replace_and_restart(target, directory) {
        unsafe {
            MessageBoxW(
                null_mut(),
                wide(&message).as_ptr(),
                w!("World Clock — Update"),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    Ok(())
}

pub fn handle_helper_args() -> bool {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|s| s == "--apply-update") {
        if args.len() == 5
            && let Ok(executable) = std::env::current_exe().and_then(fs::canonicalize)
            && let Some(directory) = executable.parent()
            && let Ok(pid) = args[2].to_string_lossy().parse()
            && let Ok(target) = fs::canonicalize(PathBuf::from(&args[3]))
            && let Err(message) = run_helper(pid, &target, &args[4].to_string_lossy(), directory)
        {
            let _ = write_file(&directory.join("error.txt"), message.as_bytes());
        }
        return true;
    }
    if let Some(index) = args.iter().position(|s| s == "--cleanup-update")
        && let Some(directory) = args.get(index + 1)
        && let Ok(directory) = fs::canonicalize(PathBuf::from(directory))
        && let Ok(target) = std::env::current_exe().and_then(fs::canonicalize)
        && valid_stage(&directory, &target)
    {
        let _ = thread::Builder::new()
            .name("world-clock-update-cleanup".into())
            .spawn(move || {
                // The helper can still be exiting when the new process starts.
                for _ in 0..100 {
                    cleanup(&directory);
                    if !directory.exists() {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            });
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str) -> serde_json::Value {
        let executable = format!("WorldClock-v{}.exe", tag.trim_start_matches('v'));
        json!({"tag_name": tag, "draft": false, "prerelease": false, "assets": [
            {"name": executable, "size": 1024, "browser_download_url": format!("https://github.com/{REPO}/releases/download/{tag}/{executable}")},
            {"name": "SHA256SUMS.txt", "size": 81, "browser_download_url": format!("https://github.com/{REPO}/releases/download/{tag}/SHA256SUMS.txt")}
        ]})
    }

    #[test]
    fn compares_stable_versions_numerically_and_rejects_invalid_tags() {
        assert!(Version::parse("v0.10.0") > Version::parse("0.9.9"));
        assert_eq!(Version::parse("v1.2.3"), Version::parse("1.2.3"));
        for tag in [
            "1.2",
            "1.2.3.4",
            "1.2.3-beta",
            "1.2.3+build",
            "01.2.3",
            "1.-2.3",
            "1.2/3",
            "1.2.3\n",
            "99999999999999999999999.0.0",
        ] {
            assert!(Version::parse(tag).is_none(), "{tag}");
        }
    }

    #[test]
    fn selects_only_newer_stable_releases_with_both_trusted_assets() {
        let mut json = release("v1.2.3");
        assert!(
            parse_release(&serde_json::to_vec(&json).unwrap(), "1.2.2")
                .unwrap()
                .is_some()
        );
        for current in ["1.2.3", "2.0.0"] {
            assert!(
                parse_release(&serde_json::to_vec(&json).unwrap(), current)
                    .unwrap()
                    .is_none()
            );
        }
        for flag in ["draft", "prerelease"] {
            json[flag] = true.into();
            assert!(
                parse_release(&serde_json::to_vec(&json).unwrap(), "1.0.0")
                    .unwrap()
                    .is_none()
            );
            json[flag] = false.into();
        }
        for url in [
            "http://github.com/evil.exe",
            "https://github.com/other/repo/releases/download/v1.2.3/WorldClock-v1.2.3.exe",
            "https://github.com.evil.test/file.exe",
        ] {
            json["assets"][0]["browser_download_url"] = url.into();
            assert!(parse_release(&serde_json::to_vec(&json).unwrap(), "1.0.0").is_err());
        }
        let mut json = release("v1.2.3");
        json["assets"].as_array_mut().unwrap().remove(1);
        assert!(parse_release(&serde_json::to_vec(&json).unwrap(), "1.0.0").is_err());
        let mut json = release("v1.2.3");
        json["assets"][0]["size"] = (MAX_EXE + 1).into();
        assert!(parse_release(&serde_json::to_vec(&json).unwrap(), "1.0.0").is_err());
        assert!(parse_release(b"not JSON", "1.0.0").is_err());
    }

    #[test]
    fn release_assets_must_match_the_tag_and_legacy_names_stay_historical() {
        for tag in ["v0.4.1", "v0.10.0", "1.2.3"] {
            let json = release(tag);
            let selected = parse_release(&serde_json::to_vec(&json).unwrap(), "0.0.0")
                .unwrap()
                .unwrap();
            assert_eq!(
                selected.executable.name,
                format!("WorldClock-v{}.exe", tag.trim_start_matches('v'))
            );
        }
        for name in [
            "WorldClock.exe",
            "WorldClock-v1.2.4.exe",
            "WorldClock-v1.2.3-x64.exe",
        ] {
            let mut json = release("v1.2.3");
            json["assets"][0]["name"] = name.into();
            json["assets"][0]["browser_download_url"] =
                format!("https://github.com/{REPO}/releases/download/v1.2.3/{name}").into();
            assert!(parse_release(&serde_json::to_vec(&json).unwrap(), "1.0.0").is_err());
        }
        let mut legacy = release("v0.4.0");
        legacy["assets"][0]["name"] = "WorldClock.exe".into();
        legacy["assets"][0]["browser_download_url"] =
            format!("https://github.com/{REPO}/releases/download/v0.4.0/WorldClock.exe").into();
        let selected = parse_release(&serde_json::to_vec(&legacy).unwrap(), "0.0.0")
            .unwrap()
            .unwrap();
        assert_eq!(selected.executable.name, "WorldClock.exe");
    }

    #[test]
    fn validates_checksums_and_rejects_tampering_and_non_windows_files() {
        let executable_name = "WorldClock-v1.2.3.exe";
        let known = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256(b"abc").unwrap(), known);
        assert_eq!(
            checksum(
                format!("{}  {executable_name}\r\n", known.to_uppercase()).as_bytes(),
                executable_name
            )
            .unwrap(),
            known
        );
        assert_eq!(
            checksum(
                format!("{known} *{executable_name}\n").as_bytes(),
                executable_name
            )
            .unwrap(),
            known
        );
        for text in [
            format!("{known}  other.exe"),
            "bad  WorldClock-v1.2.3.exe".into(),
            format!("{known}  {executable_name}\n{known}  {executable_name}"),
            format!("{known}  WorldClock-v1.2.4.exe"),
            format!("{known}  WorldClock.exe"),
        ] {
            assert!(checksum(text.as_bytes(), executable_name).is_err());
        }
        assert!(verify(b"abc", known).is_err());
        let mut exe = vec![0u8; 128];
        exe[..2].copy_from_slice(b"MZ");
        exe[0x3c] = 64;
        exe[64..70].copy_from_slice(b"PE\0\0\x64\x86");
        let hash = sha256(&exe).unwrap();
        verify(&exe, &hash).unwrap();
        exe[100] ^= 1;
        assert!(verify(&exe, &hash).is_err());
    }

    #[test]
    fn installer_restores_the_previous_file_when_the_replacement_cannot_start() {
        let root = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "update-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir(&root).unwrap();
        let target = root.join("Renamed clock Минск.exe");
        let directory = root.join(format!("{STAGE_PREFIX}test"));
        fs::create_dir(&directory).unwrap();
        write_file(&target, b"previous executable").unwrap();
        write_file(
            &directory.join("update.exe"),
            b"invalid replacement executable",
        )
        .unwrap();
        assert!(valid_stage(&directory, &target));
        assert!(!valid_stage(&root, &target));
        assert!(!valid_stage(&directory, &directory.join("update.exe")));
        assert!(replace_and_restart(&target, &directory).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"previous executable");
        assert_eq!(
            fs::read(directory.join("update.exe")).unwrap(),
            b"invalid replacement executable"
        );
        cleanup(&directory);
        fs::remove_file(&target).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn worker_failures_and_progress_are_recoverable_without_duplicate_operations() {
        let mut updater = Updater::new();
        updater.install();
        assert!(matches!(updater.state, State::Idle));
        let (tx, rx) = mpsc::channel();
        updater.state = State::Downloading(0);
        updater.rx = Some(rx);
        updater.check(true);
        assert!(matches!(updater.state, State::Downloading(0)));
        tx.send(Event::Progress(45)).unwrap();
        assert!(matches!(updater.poll(), Some(Notice::Changed)));
        assert!(matches!(updater.state, State::Downloading(45)));
        tx.send(Event::Downloaded(Err("checksum mismatch".into())))
            .unwrap();
        assert!(matches!(updater.poll(), Some(Notice::Checked)));
        assert!(matches!(updater.state, State::Failed(_)));
        assert!(!updater.state.busy());
        assert!(updater.poll().is_none());
    }

    #[test]
    #[ignore = "requires access to public GitHub Releases"]
    fn live_github_release_and_checksum() {
        let json = download(
            "api.github.com",
            &format!("/repos/{REPO}/releases/latest"),
            1024 * 1024,
            |_| {},
        )
        .unwrap();
        let release = parse_release(&json, "0.0.0").unwrap().unwrap();
        let expected = checksum(
            &download_asset(&release.checksums, |_| {}).unwrap(),
            &release.executable.name,
        )
        .unwrap();
        verify(
            &download_asset(&release.executable, |_| {}).unwrap(),
            &expected,
        )
        .unwrap();
    }
}
