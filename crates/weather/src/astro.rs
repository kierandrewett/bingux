//! The moon, and the sun's day, from times alone.

use std::f64::consts::TAU;

/// The mean length of a lunar month, in days.
const SYNODIC: f64 = 29.530_588_853;
/// A new moon: 2000-01-06 18:14 UTC.
const NEW_MOON: f64 = 947_182_440.0;

/// The moon at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moon {
    /// Where it is in the month: 0 is new, 0.5 is full.
    pub phase: f64,
    /// Days since the new moon.
    pub age: f64,
    /// The lit share of the face, 0 to 1.
    pub illumination: f64,
}

impl Moon {
    /// The moon at Unix time `unix`. Mean motion only: right to within about
    /// half a day, which is the accuracy a phase picture needs.
    pub fn at(unix: i64) -> Moon {
        let days = (unix as f64 - NEW_MOON) / 86_400.0;
        let phase = (days / SYNODIC).rem_euclid(1.0);
        Moon {
            phase,
            age: phase * SYNODIC,
            illumination: (1.0 - (TAU * phase).cos()) / 2.0,
        }
    }

    /// `Waxing gibbous`.
    pub fn name(&self) -> &'static str {
        match ((self.phase * 8.0) + 0.5).floor() as u32 % 8 {
            0 => "New moon",
            1 => "Waxing crescent",
            2 => "First quarter",
            3 => "Waxing gibbous",
            4 => "Full moon",
            5 => "Waning gibbous",
            6 => "Last quarter",
            _ => "Waning crescent",
        }
    }

    /// Whether it is growing.
    pub fn is_waxing(&self) -> bool {
        self.phase < 0.5
    }

    /// Unix time of the next full moon after `unix`.
    pub fn next_full(unix: i64) -> i64 {
        Self::next(unix, 0.5)
    }

    /// Unix time of the next new moon after `unix`.
    pub fn next_new(unix: i64) -> i64 {
        Self::next(unix, 0.0)
    }

    fn next(unix: i64, target: f64) -> i64 {
        let phase = Moon::at(unix).phase;
        let mut ahead = (target - phase).rem_euclid(1.0);
        if ahead < 0.001 {
            ahead = 1.0;
        }
        unix + (ahead * SYNODIC * 86_400.0).round() as i64
    }
}

/// How far through the daylight `now` is, 0 at sunrise and 1 at sunset, or
/// `None` before the sun is up or after it is down. All three are wall-clock
/// seconds at the place.
pub fn sun_progress(sunrise: i64, sunset: i64, now: i64) -> Option<f64> {
    if sunset <= sunrise || now < sunrise || now > sunset {
        return None;
    }
    Some((now - sunrise) as f64 / (sunset - sunrise) as f64)
}

/// `9 h 12 min` of daylight.
pub fn day_length(sunrise: i64, sunset: i64) -> String {
    let minutes = ((sunset - sunrise).max(0) + 30) / 60;
    format!("{} h {:02} min", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::parse_local;

    #[test]
    fn known_phases() {
        // A full moon was on 2024-12-15 (09:02 UTC); a new moon on 2025-01-29.
        let full = Moon::at(parse_local("2024-12-15T09:00").unwrap());
        assert!(full.illumination > 0.98, "{}", full.illumination);
        assert_eq!(full.name(), "Full moon");
        let new = Moon::at(parse_local("2025-01-29T12:36").unwrap());
        assert!(new.illumination < 0.02, "{}", new.illumination);
        assert_eq!(new.name(), "New moon");
        let waxing = Moon::at(parse_local("2024-12-08T00:00").unwrap());
        assert!(waxing.is_waxing());
    }

    #[test]
    fn the_next_full_moon_is_ahead() {
        let now = parse_local("2026-11-09T12:00").unwrap();
        let full = Moon::next_full(now);
        assert!(full > now && full - now < 30 * 86_400);
        let at_full = Moon::at(full);
        assert!(at_full.illumination > 0.99);
    }

    #[test]
    fn sun_progress_is_inside_the_day() {
        assert_eq!(sun_progress(100, 300, 200), Some(0.5));
        assert_eq!(sun_progress(100, 300, 50), None);
        assert_eq!(sun_progress(100, 300, 301), None);
        assert_eq!(day_length(0, 9 * 3600 + 12 * 60), "9 h 12 min");
    }
}
