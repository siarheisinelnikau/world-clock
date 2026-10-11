use crate::{
    appearance::CardStyle,
    time::{unw, wide},
};
use serde::{Deserialize, Serialize};
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS},
    System::Registry::*,
};

pub const MAX_CLOCKS: usize = 12;
const PATH: &str = "Software\\WorldClock";
const RUN_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    pub zone: String,
    pub label: String,
    pub desktop: bool,
    pub tray: bool,
    #[serde(default)]
    pub style: CardStyle,
}

impl Clock {
    pub fn is_local(&self) -> bool {
        self.zone == "@local"
    }

    pub fn has_valid_destination(&self) -> bool {
        self.is_local() || self.desktop || self.tray
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub clocks: Vec<Clock>,
    pub twelve: bool,
    pub seconds: bool,
    pub topmost: bool,
    pub visible: bool,
    pub x: i32,
    pub y: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clocks: vec![
                Clock {
                    zone: "@local".into(),
                    label: "Local".into(),
                    desktop: true,
                    tray: true,
                    style: CardStyle::default(),
                },
                Clock {
                    zone: "GMT Standard Time".into(),
                    label: "London".into(),
                    desktop: true,
                    tray: true,
                    style: CardStyle::default(),
                },
                Clock {
                    zone: "Eastern Standard Time".into(),
                    label: "New York".into(),
                    desktop: true,
                    tray: true,
                    style: CardStyle::default(),
                },
                Clock {
                    zone: "Tokyo Standard Time".into(),
                    label: "Tokyo".into(),
                    desktop: true,
                    tray: false,
                    style: CardStyle::default(),
                },
            ],
            twelve: false,
            seconds: false,
            topmost: false,
            visible: true,
            x: i32::MIN,
            y: i32::MIN,
        }
    }
}

fn unescape(s: &str) -> String {
    s.replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%09", "\t")
        .replace("%25", "%")
}

impl Settings {
    pub fn encode(&self) -> String {
        format!(
            "WORLD_CLOCK_3\n{}",
            serde_json::to_string(self).expect("serializable settings")
        )
    }

    pub fn decode(s: &str) -> Option<Self> {
        if let Some(json) = s.strip_prefix("WORLD_CLOCK_3\n") {
            let config: Self = serde_json::from_str(json).ok()?;
            if config.clocks.is_empty() || config.clocks.len() > MAX_CLOCKS {
                return None;
            }
            for c in &config.clocks {
                if c.zone.is_empty()
                    || c.zone.len() > 256
                    || c.label.trim().is_empty()
                    || c.label.chars().count() > 36
                    || !c.has_valid_destination()
                    || c.style.validate().is_err()
                {
                    return None;
                }
            }
            return Some(config);
        }
        let mut lines = s.lines();
        let version = lines.next()?;
        if !matches!(version, "WORLD_CLOCK_1" | "WORLD_CLOCK_2") {
            return None;
        }
        let flags: Vec<_> = lines.next()?.split('\t').collect();
        if flags.len() != if version == "WORLD_CLOCK_1" { 6 } else { 7 }
            || flags[..4].iter().any(|x| !matches!(*x, "0" | "1"))
        {
            return None;
        }
        let mut settings = Self {
            clocks: Vec::new(),
            twelve: flags[0] == "1",
            seconds: flags[1] == "1",
            topmost: flags[2] == "1",
            visible: flags[3] == "1",
            x: flags[4].parse().ok()?,
            y: flags[5].parse().ok()?,
        };
        let opacity = if version == "WORLD_CLOCK_1" {
            70
        } else {
            flags[6].parse::<u8>().ok().filter(|v| *v <= 100)?
        };
        for line in lines.take(MAX_CLOCKS) {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 4
                || !matches!(fields[2], "0" | "1")
                || !matches!(fields[3], "0" | "1")
            {
                return None;
            }
            let (zone, label) = (unescape(fields[0]), unescape(fields[1]));
            if zone.is_empty()
                || zone.len() > 256
                || label.trim().is_empty()
                || label.chars().count() > 36
            {
                return None;
            }
            if zone != "@local" && fields[2] == "0" && fields[3] == "0" {
                return None;
            }
            settings.clocks.push(Clock {
                zone,
                label,
                desktop: fields[2] == "1",
                tray: fields[3] == "1",
                style: CardStyle {
                    background_opacity: opacity,
                    ..CardStyle::default()
                },
            });
        }
        if settings.clocks.is_empty() {
            return None;
        }
        // Upgrade the initial taskbar set to three; later v3 choices stay independent.
        let mut selected = settings.clocks.iter().filter(|c| c.tray).count();
        if (1..3).contains(&selected) {
            for c in &mut settings.clocks {
                if !c.tray && (c.desktop || !c.is_local()) {
                    c.tray = true;
                    selected += 1;
                    if selected == 3 {
                        break;
                    }
                }
            }
        }
        Some(settings)
    }

    pub fn load() -> Self {
        read_string(PATH, "Config")
            .and_then(|s| Self::decode(&s))
            .unwrap_or_default()
    }

    pub fn can_remove_clock(&self, index: usize) -> bool {
        self.clocks.len() > 1 && self.clocks.get(index).is_some_and(|c| !c.is_local())
    }

    pub fn remove_clock(&mut self, index: usize) -> bool {
        if !self.can_remove_clock(index) {
            return false;
        }
        self.clocks.remove(index);
        true
    }

    pub fn save(&self) -> Result<(), String> {
        write_string(PATH, "Config", &self.encode())
    }
}

fn read_string(path: &str, name: &str) -> Option<String> {
    unsafe {
        let path = wide(path);
        let name = wide(name);
        // Size the buffer from the stored value; custom layouts can exceed 64K characters.
        // Retry if another instance changes the value between the size query and the read.
        for _ in 0..3 {
            let mut size = 0;
            if RegGetValueW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                null_mut(),
                &mut size,
            ) != ERROR_SUCCESS
                || size > 64 * 1024 * 1024
            {
                return None;
            }
            // Leave room for the terminator even if the registry value omits it.
            let mut data = vec![0u16; (size as usize).div_ceil(size_of::<u16>()) + 1];
            size = std::mem::size_of_val(data.as_slice()) as u32;
            match RegGetValueW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            ) {
                ERROR_SUCCESS => return Some(unw(&data)),
                ERROR_MORE_DATA => continue,
                _ => return None,
            }
        }
        None
    }
}

fn write_string(path: &str, name: &str, value: &str) -> Result<(), String> {
    unsafe {
        let mut key = null_mut();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(path).as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            null_mut(),
        );
        if status != ERROR_SUCCESS {
            return Err(format!("Cannot open settings (Windows error {status})."));
        }
        let data = wide(value);
        let status = RegSetValueExW(
            key,
            wide(name).as_ptr(),
            0,
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        );
        RegCloseKey(key);
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("Cannot save settings (Windows error {status})."))
        }
    }
}

pub fn startup_enabled() -> bool {
    read_string(RUN_PATH, "WorldClock").is_some()
}
pub fn set_startup(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        write_string(RUN_PATH, "WorldClock", &format!("\"{}\"", exe.display()))
    } else {
        let status = unsafe {
            RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                wide(RUN_PATH).as_ptr(),
                wide("WorldClock").as_ptr(),
            )
        };
        if status == ERROR_SUCCESS || status == 2 {
            Ok(())
        } else {
            Err(format!(
                "Cannot change startup setting (Windows error {status})."
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mechanical_styles_round_trip_and_old_settings_stay_digital() {
        use crate::appearance::{ClockFace, Numerals};
        let mut settings = Settings::default();
        let style = &mut settings.clocks[1].style;
        style.face = ClockFace::Mechanical;
        style.dial.numerals = Numerals::Roman;
        style.dial.sweep = false;
        style.dial.second_hand = false;
        style.dial.date = true;
        style.dial.hand_width = 7;
        style.dial.hour_color = 0x112233;
        style.dial.minute_color = 0x445566;
        style.dial.second_color = 0x778899;
        assert_eq!(
            Settings::decode(&settings.encode()).unwrap().clocks,
            settings.clocks
        );
        let mut old = serde_json::to_value(Settings::default()).unwrap();
        for clock in old["clocks"].as_array_mut().unwrap() {
            let style = clock["style"].as_object_mut().unwrap();
            style.remove("face");
            style.remove("dial");
        }
        let restored = Settings::decode(&format!("WORLD_CLOCK_3\n{old}")).unwrap();
        assert_eq!(restored.clocks, Settings::default().clocks);
        settings.clocks[1].style.dial.hand_width = 0;
        assert!(Settings::decode(&settings.encode()).is_none());
        settings.clocks[1].style.dial.hand_width = 4;
        settings.clocks[1].style.dial.hour_color = 0x1000000;
        assert!(Settings::decode(&settings.encode()).is_none());
    }

    #[test]
    fn registry_round_trip_handles_large_valid_styles_and_unicode() {
        struct TestKey(String);
        impl Drop for TestKey {
            fn drop(&mut self) {
                unsafe {
                    RegDeleteTreeW(HKEY_CURRENT_USER, wide(&self.0).as_ptr());
                }
            }
        }
        let key = TestKey(format!(
            "Software\\WorldClock-Test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(read_string(&key.0, "Missing").is_none());
        for value in ["", "Минск 🕒"] {
            write_string(&key.0, "Config", value).unwrap();
            assert_eq!(read_string(&key.0, "Config").as_deref(), Some(value));
        }
        let mut settings = Settings::default();
        for clock in &mut settings.clocks {
            clock.style.elements = (0..40)
                .map(|i| crate::appearance::TextElement {
                    text: "x".repeat(512),
                    x: 0,
                    y: i * 10,
                    large: false,
                })
                .collect();
        }
        let encoded = settings.encode();
        assert!(encoded.encode_utf16().count() > 65536);
        assert!(Settings::decode(&encoded).is_some());
        write_string(&key.0, "Config", &encoded).unwrap();
        let restored = read_string(&key.0, "Config").unwrap();
        assert_eq!(restored, encoded);
        assert_eq!(Settings::decode(&restored).unwrap().clocks, settings.clocks);
    }

    #[test]
    fn settings_round_trip_unicode_and_independent_sets() {
        let mut s = Settings::default();
        s.clocks[0].label = "Минск %09\t\n".into();
        s.clocks[0].desktop = false;
        s.x = -1400;
        s.clocks[0].style.background_opacity = 37;
        s.clocks[0].style.text_color = 0x27EACC;
        s.clocks[0].style.text_opacity = 43;
        s.clocks[0].style.height = 137;
        s.clocks[0].style.elements = vec![crate::appearance::TextElement {
            text: "{time}".into(),
            x: 24,
            y: 35,
            large: true,
        }];
        let loaded = Settings::decode(&s.encode()).unwrap();
        assert_eq!(s.clocks, loaded.clocks);
        assert_eq!(loaded.x, -1400);
        assert_eq!(loaded.clocks[0].style.background_opacity, 37);
    }
    #[test]
    fn hidden_local_survives_save_and_legacy_migration() {
        let mut settings = Settings::default();
        settings.clocks[0].desktop = false;
        settings.clocks[0].tray = false;
        let loaded = Settings::decode(&settings.encode()).unwrap();
        assert_eq!(loaded.clocks, settings.clocks);
        let old = "WORLD_CLOCK_2\n0\t0\t0\t1\t100\t200\t70\n@local\tLocal\t0\t0\nGMT Standard Time\tLondon\t1\t1\nEastern Standard Time\tNew York\t1\t0\n";
        let loaded = Settings::decode(old).unwrap();
        assert!(!loaded.clocks[0].desktop && !loaded.clocks[0].tray);
        settings.clocks[1].desktop = false;
        settings.clocks[1].tray = false;
        assert!(Settings::decode(&settings.encode()).is_none());
    }

    #[test]
    fn local_cannot_be_removed_after_renaming_or_reordering() {
        let mut settings = Settings::default();
        settings.clocks[0].label = "This computer".into();
        settings.clocks.swap(0, 3);
        let original = settings.clocks.clone();
        assert!(!settings.can_remove_clock(3));
        assert!(!settings.remove_clock(3));
        assert!(!settings.remove_clock(99));
        assert_eq!(settings.clocks, original);
        assert!(settings.remove_clock(0));
        assert!(settings.clocks.iter().any(Clock::is_local));
    }
    #[test]
    fn migrate_opacity_and_reject_out_of_range() {
        let old = "WORLD_CLOCK_1\n0\t0\t0\t1\t100\t200\n@local\tLocal\t1\t1\n";
        assert_eq!(
            Settings::decode(old).unwrap().clocks[0]
                .style
                .background_opacity,
            70
        );
        assert!(
            Settings::decode("WORLD_CLOCK_2\n0\t0\t0\t1\t100\t200\t101\n@local\tLocal\t1\t1\n")
                .is_none()
        );
    }
    #[test]
    fn reject_invalid_or_future_config() {
        assert!(Settings::decode("").is_none());
        assert!(Settings::decode("WORLD_CLOCK_2\n").is_none());
        assert!(Settings::decode("WORLD_CLOCK_1\n0\t0\t0\t1\t0\t0\nUTC\tUTC\t0\t0\n").is_none());
        assert!(Settings::decode("WORLD_CLOCK_1\n0\t0\t0\t1\t0\t0\n").is_none());
    }
}
