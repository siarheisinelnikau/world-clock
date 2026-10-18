//! Schedule CPU-rendered frames only when their displayed time can change.
use crate::appearance::{CardStyle, ClockFace};
use std::cell::Cell;
use windows_sys::Win32::Foundation::SYSTEMTIME;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cadence {
    Minute,
    Second,
    Sweep,
}

impl Cadence {
    pub fn for_style(style: &CardStyle, seconds: bool) -> Self {
        match style.face {
            ClockFace::Mechanical if style.dial.second_hand && style.dial.sweep => Self::Sweep,
            ClockFace::Mechanical => Self::Second,
            ClockFace::Digital => {
                let has_time = if style.elements.is_empty() {
                    style.template.contains("{time}")
                } else {
                    style
                        .elements
                        .iter()
                        .any(|element| element.text.contains("{time}"))
                };
                if seconds && has_time {
                    Self::Second
                } else {
                    Self::Minute
                }
            }
        }
    }

    pub fn timer_ms(self) -> u32 {
        if self == Self::Sweep { 50 } else { 250 }
    }

    fn stamp(self, utc: &SYSTEMTIME) -> u64 {
        let minute = (((utc.wYear as u64 * 13 + utc.wMonth as u64) * 32 + utc.wDay as u64) * 24
            + utc.wHour as u64)
            * 60
            + utc.wMinute as u64;
        match self {
            Self::Minute => minute,
            Self::Second => minute * 60 + utc.wSecond as u64,
            Self::Sweep => (minute * 60 + utc.wSecond as u64) * 20 + utc.wMilliseconds as u64 / 50,
        }
    }
}

#[derive(Default)]
pub struct RefreshGate {
    last: Cell<Option<(Cadence, u64)>>,
}

impl RefreshGate {
    pub fn invalidate(&self) {
        self.last.set(None);
    }

    pub fn due(&self, cadence: Option<Cadence>, utc: &SYSTEMTIME) -> bool {
        let next = cadence.map(|cadence| (cadence, cadence.stamp(utc)));
        let changed = self.last.replace(next) != next;
        next.is_some() && changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(milliseconds: u32) -> SYSTEMTIME {
        SYSTEMTIME {
            wYear: 2026,
            wMonth: 10,
            wDay: 30,
            wMinute: (milliseconds / 60_000) as u16,
            wSecond: ((milliseconds / 1000) % 60) as u16,
            wMilliseconds: (milliseconds % 1000) as u16,
            ..unsafe { std::mem::zeroed() }
        }
    }

    #[test]
    fn overlapping_timers_do_not_duplicate_frames_and_hidden_clocks_do_no_work() {
        for (cadence, expected) in [
            (Cadence::Minute, 1),
            (Cadence::Second, 60),
            (Cadence::Sweep, 1200),
        ] {
            let gate = RefreshGate::default();
            let mut frames = 0;
            for ms in (0..60_000).step_by(50) {
                frames += usize::from(gate.due(Some(cadence), &at(ms)));
                if ms % 250 == 0 {
                    frames += usize::from(gate.due(Some(cadence), &at(ms)));
                }
            }
            assert_eq!(frames, expected);
            for ms in (60_000..120_000).step_by(50) {
                assert!(!gate.due(None, &at(ms)));
            }
            assert!(gate.due(Some(cadence), &at(120_000)));
        }
    }

    #[test]
    fn cadence_changes_and_clock_adjustments_refresh_immediately() {
        let gate = RefreshGate::default();
        assert!(gate.due(Some(Cadence::Minute), &at(10_000)));
        assert!(!gate.due(Some(Cadence::Minute), &at(59_000)));
        assert!(gate.due(Some(Cadence::Second), &at(59_000)));
        assert!(gate.due(Some(Cadence::Minute), &at(60_000)));
        assert!(gate.due(Some(Cadence::Minute), &at(0)));
        let mut style = CardStyle {
            face: ClockFace::Mechanical,
            ..CardStyle::default()
        };
        assert_eq!(Cadence::for_style(&style, false), Cadence::Sweep);
        style.dial.second_hand = false;
        assert_eq!(Cadence::for_style(&style, false), Cadence::Second);
        style.face = ClockFace::Digital;
        assert_eq!(Cadence::for_style(&style, false), Cadence::Minute);
        assert_eq!(Cadence::for_style(&style, true), Cadence::Second);
    }
}
