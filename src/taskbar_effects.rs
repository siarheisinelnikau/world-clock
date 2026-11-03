//! Finite transitions for the taskbar's optional, adaptive text backdrop.
use std::time::{Duration, Instant};

const TRANSITION: Duration = Duration::from_millis(180);

pub fn backdrop_target(enabled: bool, foreground: u32) -> f32 {
    if !enabled {
        0.0
    } else if foreground == 0x181818 {
        1.0
    } else {
        -1.0
    }
}

#[derive(Default)]
pub struct BackdropFade {
    from: f32,
    target: f32,
    started: Option<Instant>,
}

impl BackdropFade {
    pub fn value(&self, now: Instant) -> f32 {
        let Some(started) = self.started else {
            return self.target;
        };
        let t = (now.saturating_duration_since(started).as_secs_f32() / TRANSITION.as_secs_f32())
            .clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        self.from + (self.target - self.from) * t
    }

    pub fn retarget(&mut self, target: f32, now: Instant, animate: bool) {
        if !animate {
            self.from = target;
            self.target = target;
            self.started = None;
        } else if self.target != target {
            self.from = self.value(now);
            self.target = target;
            self.started = Some(now);
        }
    }

    pub fn active(&self, now: Instant) -> bool {
        self.started
            .is_some_and(|started| now.saturating_duration_since(started) < TRANSITION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_backdrop_is_transparent_on_both_system_themes() {
        assert_eq!(backdrop_target(false, 0xffffff), 0.0);
        assert_eq!(backdrop_target(false, 0x181818), 0.0);
        assert_eq!(backdrop_target(true, 0xffffff), -1.0);
        assert_eq!(backdrop_target(true, 0x181818), 1.0);
    }

    #[test]
    fn fades_finish_and_reversing_mid_transition_does_not_jump() {
        let now = Instant::now();
        let mut fade = BackdropFade::default();
        fade.retarget(-1.0, now, false);
        assert!(!fade.active(now));
        fade.retarget(0.0, now, true);
        let middle = now + TRANSITION / 2;
        assert!((fade.value(middle) + 0.5).abs() < 0.001);
        fade.retarget(1.0, middle, true);
        assert!((fade.value(middle) + 0.5).abs() < 0.001);
        let finished = middle + TRANSITION;
        assert_eq!(fade.value(finished), 1.0);
        assert!(!fade.active(finished));
        assert_eq!(fade.value(finished + Duration::from_secs(60)), 1.0);
        fade.retarget(0.0, finished, false);
        assert!(!fade.active(finished));
        assert_eq!(fade.value(finished), 0.0);
    }
}
