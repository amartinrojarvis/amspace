//! Allocation-free shake recognition. The adapter must first confirm a real
//! window move and owns identity, mouse-up, cancellation and the actual toggle.
use objc2_foundation::NSPoint;
use std::time::{Duration, Instant};

const LEG: f64 = 12.;
const EXCURSION: f64 = 96.;
const WINDOW: Duration = Duration::from_millis(650);
const PAUSE: Duration = Duration::from_millis(250);

#[derive(Clone)]
struct Axis {
    anchor: f64,
    peak: f64,
    low: f64,
    high: f64,
    direction: i8,
    reversals: u8,
    started: Instant,
}
impl Axis {
    fn new(value: f64, now: Instant) -> Self {
        Self {
            anchor: value,
            peak: value,
            low: value,
            high: value,
            direction: 0,
            reversals: 0,
            started: now,
        }
    }
    fn sample(&mut self, value: f64, now: Instant) -> bool {
        self.low = self.low.min(value);
        self.high = self.high.max(value);
        if self.high - self.low > EXCURSION || now.duration_since(self.started) > WINDOW {
            *self = Self::new(value, now);
            return false;
        }
        if self.direction == 0 {
            let delta = value - self.anchor;
            if delta.abs() >= LEG {
                self.direction = if delta > 0. { 1 } else { -1 };
                self.peak = value;
                self.started = now;
            }
            return false;
        }
        let progress = (value - self.peak) * f64::from(self.direction);
        if progress >= 0. {
            self.peak = value;
        } else if progress <= -LEG {
            self.direction = -self.direction;
            self.peak = value;
            self.reversals += 1;
        }
        self.reversals >= 3
    }
}

#[derive(Clone)]
pub struct Shake {
    horizontal: Axis,
    vertical: Axis,
    last: Instant,
    valid: bool,
    fired: bool,
}
impl Shake {
    pub fn new(point: NSPoint, now: Instant) -> Self {
        Self {
            horizontal: Axis::new(point.x, now),
            vertical: Axis::new(point.y, now),
            last: now,
            valid: point.x.is_finite() && point.y.is_finite(),
            fired: false,
        }
    }
    /// Three deliberate reversals in a small region, with no long pauses.
    /// At most one true result for this drag, even after a pause or another shake.
    pub fn sample(&mut self, point: NSPoint, now: Instant) -> bool {
        if self.fired {
            return false;
        }
        if !point.x.is_finite() || !point.y.is_finite() {
            self.valid = false;
            return false;
        }
        let elapsed = now.checked_duration_since(self.last);
        if !self.valid || elapsed.is_none_or(|elapsed| elapsed > PAUSE) {
            *self = Self::new(point, now);
            return false;
        }
        if elapsed.is_some_and(|elapsed| elapsed.is_zero()) {
            return false;
        }
        self.last = now;
        self.fired = self.horizontal.sample(point.x, now) || self.vertical.sample(point.y, now);
        self.fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deliberate_horizontal_vertical_and_diagonal_shakes_fire_on_third_reversal() {
        for (x, y) in [(1., 0.), (0., 1.), (1., -1.)] {
            let now = Instant::now();
            let mut shake = Shake::new(NSPoint::new(0., 0.), now);
            for (i, distance) in [20., -20., 20., -20.].into_iter().enumerate() {
                let fired = shake.sample(
                    NSPoint::new(x * distance, y * distance),
                    now + Duration::from_millis((i as u64 + 1) * 40),
                );
                assert_eq!(fired, i == 3);
            }
            for i in 1..=500 {
                let distance = if i % 2 == 0 { 20. } else { -20. };
                assert!(!shake.sample(
                    NSPoint::new(x * distance, y * distance),
                    now + Duration::from_millis(160 + i * 40),
                ));
            }
        }
    }

    #[test]
    fn ordinary_moves_and_small_jitter_never_toggle() {
        let now = Instant::now();
        for path in 0..3 {
            let mut shake = Shake::new(NSPoint::new(0., 0.), now);
            for i in 1..=10_000 {
                let point = match path {
                    0 => NSPoint::new(i as f64, -(i as f64) * 0.7),
                    1 => NSPoint::new(if i % 2 == 0 { 5. } else { -5. }, 0.),
                    _ => NSPoint::new(0., if i % 2 == 0 { 5. } else { -5. }),
                };
                assert!(!shake.sample(point, now + Duration::from_millis(i * 16)));
            }
        }
    }

    #[test]
    fn slow_reversals_and_long_pauses_do_not_accumulate() {
        let now = Instant::now();
        for interval in [240, 300] {
            let mut shake = Shake::new(NSPoint::new(0., 0.), now);
            for i in 1..=20 {
                assert!(!shake.sample(
                    NSPoint::new(if i % 2 == 0 { -20. } else { 20. }, 0.),
                    now + Duration::from_millis(i * interval),
                ));
            }
        }
    }

    #[test]
    fn large_excursions_are_moves_not_shakes() {
        let now = Instant::now();
        let mut shake = Shake::new(NSPoint::new(0., 0.), now);
        for i in 1..=100 {
            assert!(!shake.sample(
                NSPoint::new(if i % 2 == 0 { -160. } else { 160. }, 0.),
                now + Duration::from_millis(i * 40),
            ));
        }
    }

    #[test]
    fn repeated_or_backward_timestamps_cannot_complete_an_old_sequence() {
        let now = Instant::now();
        let mut shake = Shake::new(NSPoint::new(0., 0.), now);
        for x in [20., -20., 20., -20.] {
            assert!(!shake.sample(NSPoint::new(x, 0.), now));
        }
        assert!(!shake.sample(NSPoint::new(20., 0.), now + Duration::from_millis(40)));
        assert!(!shake.sample(NSPoint::new(-20., 0.), now + Duration::from_millis(80)));
        assert!(!shake.sample(NSPoint::new(20., 0.), now + Duration::from_millis(120)));
        assert!(!shake.sample(NSPoint::new(-20., 0.), now + Duration::from_millis(100)));
        assert!(!shake.sample(NSPoint::new(20., 0.), now + Duration::from_millis(140)));
    }

    #[test]
    fn invalid_coordinates_discard_the_partial_sequence() {
        let now = Instant::now();
        let mut shake = Shake::new(NSPoint::new(f64::NAN, 0.), now);
        assert!(!shake.sample(NSPoint::new(0., 0.), now + Duration::from_millis(10)));
        for (i, x) in [20., -20., 20., f64::INFINITY, -20., 20.]
            .into_iter()
            .enumerate()
        {
            assert!(!shake.sample(
                NSPoint::new(x, 0.),
                now + Duration::from_millis((i as u64 + 1) * 40),
            ));
        }
    }
}
