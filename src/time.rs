use std::{mem::zeroed, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{ERROR_SUCCESS, FILETIME, SYSTEMTIME},
    System::{
        Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW},
        SystemInformation::{GetLocalTime, GetSystemTime},
        Time::{
            DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation, SystemTimeToFileTime,
            SystemTimeToTzSpecificLocalTimeEx,
        },
    },
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn unw(v: &[u16]) -> String {
    String::from_utf16_lossy(&v[..v.iter().position(|c| *c == 0).unwrap_or(v.len())])
}

#[derive(Clone)]
pub struct Zone {
    pub key: String,
    pub display: String,
    pub info: Option<DYNAMIC_TIME_ZONE_INFORMATION>,
}

pub fn zones() -> Vec<Zone> {
    let mut result = vec![Zone {
        key: "@local".into(),
        display: "Local — follow Windows time zone".into(),
        info: None,
    }];
    unsafe {
        for index in 0..1024 {
            let mut info: DYNAMIC_TIME_ZONE_INFORMATION = zeroed();
            if EnumDynamicTimeZoneInformation(index, &mut info) != ERROR_SUCCESS {
                break;
            }
            let key = unw(&info.TimeZoneKeyName);
            let path = wide(&format!(
                "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Time Zones\\{key}"
            ));
            let mut buf = [0u16; 512];
            let mut size = std::mem::size_of_val(&buf) as u32;
            let status = RegGetValueW(
                HKEY_LOCAL_MACHINE,
                path.as_ptr(),
                wide("Display").as_ptr(),
                RRF_RT_REG_SZ,
                null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            );
            let display = if status == ERROR_SUCCESS {
                unw(&buf)
            } else {
                key.clone()
            };
            result.push(Zone {
                key,
                display,
                info: Some(info),
            });
        }
    }
    result[1..].sort_by_key(|z| (-z.info.as_ref().unwrap().Bias, z.display.clone()));
    result
}

pub fn utc_now() -> SYSTEMTIME {
    unsafe {
        let mut s = zeroed();
        GetSystemTime(&mut s);
        s
    }
}
pub fn local_now() -> SYSTEMTIME {
    unsafe {
        let mut s = zeroed();
        GetLocalTime(&mut s);
        s
    }
}

pub fn convert(zone: &Zone, utc: &SYSTEMTIME) -> Option<SYSTEMTIME> {
    unsafe {
        let mut local = zeroed();
        let info = zone
            .info
            .as_ref()
            .map_or(std::ptr::null(), |i| i as *const _);
        (SystemTimeToTzSpecificLocalTimeEx(info, utc, &mut local) != 0).then_some(local)
    }
}

fn ticks(s: &SYSTEMTIME) -> i64 {
    unsafe {
        let mut f: FILETIME = zeroed();
        SystemTimeToFileTime(s, &mut f);
        ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as i64
    }
}

pub fn offset(local: &SYSTEMTIME, utc: &SYSTEMTIME) -> String {
    let minutes = (ticks(local) - ticks(utc)) / 600_000_000;
    format!(
        "UTC{}{hours:02}:{:02}",
        if minutes < 0 { "−" } else { "+" },
        minutes.abs() % 60,
        hours = minutes.abs() / 60
    )
}

pub fn clock(s: &SYSTEMTIME, twelve: bool, seconds: bool) -> String {
    let h = if twelve {
        match s.wHour % 12 {
            0 => 12,
            h => h,
        }
    } else {
        s.wHour
    };
    let mut text = if twelve {
        format!("{h}:{:02}", s.wMinute)
    } else {
        format!("{h:02}:{:02}", s.wMinute)
    };
    if seconds {
        text.push_str(&format!(":{:02}", s.wSecond));
    }
    if twelve {
        text.push_str(if s.wHour < 12 { " AM" } else { " PM" });
    }
    text
}

pub fn date(s: &SYSTEMTIME) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{}, {} {}",
        DAYS[(s.wDayOfWeek as usize).min(6)],
        MONTHS[(s.wMonth.saturating_sub(1) as usize).min(11)],
        s.wDay
    )
}

pub fn day_relation(local: &SYSTEMTIME, home: &SYSTEMTIME) -> &'static str {
    let a = (local.wYear, local.wMonth, local.wDay);
    let b = (home.wYear, home.wMonth, home.wDay);
    match a.cmp(&b) {
        std::cmp::Ordering::Less => "Yesterday",
        std::cmp::Ordering::Greater => "Tomorrow",
        _ => "Today",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(y: u16, m: u16, d: u16, h: u16, min: u16) -> SYSTEMTIME {
        SYSTEMTIME {
            wYear: y,
            wMonth: m,
            wDay: d,
            wHour: h,
            wMinute: min,
            ..unsafe { zeroed() }
        }
    }
    fn zone(key: &str) -> Zone {
        zones()
            .into_iter()
            .find(|z| z.key == key)
            .expect("Windows zone missing")
    }
    #[test]
    fn dst_spring_and_autumn_boundaries() {
        let ny = zone("Eastern Standard Time");
        for (utc, expected) in [
            (at(2026, 3, 8, 6, 59), (1, 59)),
            (at(2026, 3, 8, 7, 0), (3, 0)),
            (at(2026, 11, 1, 5, 59), (1, 59)),
            (at(2026, 11, 1, 6, 0), (1, 0)),
        ] {
            let t = convert(&ny, &utc).unwrap();
            assert_eq!((t.wHour, t.wMinute), expected);
        }
    }
    #[test]
    fn fractional_offsets_and_new_year() {
        let utc = at(2026, 12, 31, 23, 30);
        let india = convert(&zone("India Standard Time"), &utc).unwrap();
        assert_eq!(
            (
                india.wYear,
                india.wMonth,
                india.wDay,
                india.wHour,
                india.wMinute
            ),
            (2027, 1, 1, 5, 0)
        );
        assert_eq!(offset(&india, &utc), "UTC+05:30");
        let nepal = convert(&zone("Nepal Standard Time"), &utc).unwrap();
        assert_eq!(offset(&nepal, &utc), "UTC+05:45");
        assert_eq!(day_relation(&nepal, &utc), "Tomorrow");
        assert_eq!(clock(&at(2026, 1, 1, 0, 5), true, false), "12:05 AM");
        assert_eq!(clock(&at(2026, 1, 1, 12, 5), true, true), "12:05:00 PM");
    }
}
