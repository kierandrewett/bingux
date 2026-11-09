//! Conditions worth a warning, read from the forecast.
//!
//! Open-Meteo does not carry the warnings that national weather services
//! issue, so these are not official alerts. They are what the forecast itself
//! says about the next day and a half: a thunderstorm, a heavy downpour, a
//! gale, extreme heat or cold, ice, heavy snow, a high UV index and poor air.

use crate::model::{Forecast, Hour};
use crate::time::{HOUR, day_of};
use crate::units::Units;

/// How much a notice matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing.
    Advisory,
    /// Take care.
    Warning,
}

/// One thing the forecast says to watch.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub severity: Severity,
    pub title: String,
    pub detail: String,
}

/// How far ahead to look.
const LOOKAHEAD: usize = 36;

/// The first and last hours of `hours` that match, as a span of wall times.
fn span(hours: &[&Hour], matches: impl Fn(&Hour) -> bool) -> Option<(i64, i64)> {
    let mut found = hours.iter().filter(|hour| matches(hour));
    let first = found.next()?;
    let last = found.last().unwrap_or(first);
    Some((first.time, last.time + HOUR))
}

/// `Until 15:00.`, `From 13:00 to 15:00.` or `From 03:00 to 06:00 tomorrow.`
fn when(units: &Units, local: i64, (from, to): (i64, i64)) -> String {
    let clock = units.clock;
    let tag = |time: i64| {
        if day_of(time) == day_of(local) {
            ""
        } else {
            " tomorrow"
        }
    };
    let (start_tag, end_tag) = (tag(from), tag(to - 1));
    if from <= local {
        format!("Until {}{end_tag}.", clock.time(to))
    } else if start_tag == end_tag {
        format!("From {} to {}{end_tag}.", clock.time(from), clock.time(to))
    } else {
        format!(
            "From {}{start_tag} to {}{end_tag}.",
            clock.time(from),
            clock.time(to)
        )
    }
}

/// The notices for `forecast`, most serious first. `local` is the time at the
/// place now.
pub fn notices(forecast: &Forecast, units: &Units, local: i64) -> Vec<Notice> {
    let hours: Vec<&Hour> = forecast.hours_from(local).iter().take(LOOKAHEAD).collect();
    let mut out = Vec::new();

    if let Some(range) = span(&hours, |hour| matches!(hour.code, Some(96 | 99))) {
        out.push(Notice {
            severity: Severity::Warning,
            title: "Thunderstorms with hail".into(),
            detail: when(units, local, range),
        });
    } else if let Some(range) = span(&hours, |hour| matches!(hour.code, Some(95))) {
        out.push(Notice {
            severity: Severity::Advisory,
            title: "Thunderstorms".into(),
            detail: when(units, local, range),
        });
    }

    let heaviest = hours
        .iter()
        .filter_map(|hour| hour.precipitation)
        .fold(0.0_f64, f64::max);
    if heaviest >= 8.0 {
        let range = span(&hours, |hour| hour.precipitation.unwrap_or(0.0) >= 8.0);
        out.push(Notice {
            severity: Severity::Warning,
            title: "Heavy rain".into(),
            detail: format!(
                "Up to {} an hour. {}",
                units.rain(heaviest),
                range
                    .map(|range| when(units, local, range))
                    .unwrap_or_default()
            )
            .trim()
            .to_string(),
        });
    }

    if let Some(range) = span(&hours, |hour| matches!(hour.code, Some(56 | 57 | 66 | 67))) {
        out.push(Notice {
            severity: Severity::Warning,
            title: "Freezing rain".into(),
            detail: format!("Roads and paths may be icy. {}", when(units, local, range)),
        });
    }

    if let Some(range) = span(&hours, |hour| matches!(hour.code, Some(75 | 86))) {
        out.push(Notice {
            severity: Severity::Warning,
            title: "Heavy snow".into(),
            detail: when(units, local, range),
        });
    }

    let gusts = hours
        .iter()
        .filter_map(|hour| hour.wind_gusts)
        .fold(0.0_f64, f64::max);
    if gusts >= 55.0 {
        let range = span(&hours, |hour| hour.wind_gusts.unwrap_or(0.0) >= 55.0);
        out.push(Notice {
            severity: if gusts >= 75.0 {
                Severity::Warning
            } else {
                Severity::Advisory
            },
            title: if gusts >= 75.0 {
                "Strong wind".into()
            } else {
                "Gusty wind".into()
            },
            detail: format!(
                "Gusts up to {}. {}",
                units.speed(gusts),
                range
                    .map(|range| when(units, local, range))
                    .unwrap_or_default()
            )
            .trim()
            .to_string(),
        });
    }

    let hottest = hours
        .iter()
        .filter_map(|hour| hour.apparent)
        .fold(f64::MIN, f64::max);
    if hottest >= 35.0 {
        out.push(Notice {
            severity: Severity::Warning,
            title: "Extreme heat".into(),
            detail: format!(
                "It feels like {}. Drink water and avoid the afternoon sun.",
                units.temperature_with_unit(hottest)
            ),
        });
    }
    let coldest = hours
        .iter()
        .filter_map(|hour| hour.apparent)
        .fold(f64::MAX, f64::min);
    if coldest <= -15.0 {
        out.push(Notice {
            severity: Severity::Warning,
            title: "Extreme cold".into(),
            detail: format!(
                "It feels like {}. Cover exposed skin.",
                units.temperature_with_unit(coldest)
            ),
        });
    }

    if let Some(range) = span(&hours, |hour| hour.uv.unwrap_or(0.0) >= 8.0) {
        out.push(Notice {
            severity: Severity::Advisory,
            title: "Very high UV".into(),
            detail: format!("UV index 8 or more. {}", when(units, local, range)),
        });
    }

    if let Some(aqi) = forecast.air.as_ref().and_then(|air| air.us_aqi) {
        if aqi > 150.0 {
            out.push(Notice {
                severity: Severity::Warning,
                title: "Unhealthy air".into(),
                detail: format!("Air quality index {}.", aqi.round() as i64),
            });
        } else if aqi > 100.0 {
            out.push(Notice {
                severity: Severity::Advisory,
                title: "Poor air for sensitive groups".into(),
                detail: format!("Air quality index {}.", aqi.round() as i64),
            });
        }
    }

    out.sort_by_key(|notice| std::cmp::Reverse(notice.severity));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Hour;
    use crate::time::parse_local;

    fn forecast(hours: Vec<Hour>) -> Forecast {
        Forecast {
            hourly: hours,
            ..Forecast::default()
        }
    }

    fn hour(at: &str, code: u8, gusts: f64, rain: f64) -> Hour {
        Hour {
            time: parse_local(at).unwrap(),
            code: Some(code),
            wind_gusts: Some(gusts),
            precipitation: Some(rain),
            ..Hour::default()
        }
    }

    #[test]
    fn a_quiet_day_has_no_notices() {
        let now = parse_local("2026-11-09T12:30").unwrap();
        let hours = (12..30)
            .map(|h| hour(&format!("2026-11-09T{:02}:00", h % 24), 1, 15.0, 0.0))
            .collect();
        assert!(notices(&forecast(hours), &Units::METRIC, now).is_empty());
    }

    #[test]
    fn storms_wind_and_rain_are_found_and_ordered() {
        let now = parse_local("2026-11-09T12:30").unwrap();
        let hours = vec![
            hour("2026-11-09T12:00", 3, 20.0, 0.0),
            hour("2026-11-09T13:00", 95, 80.0, 9.0),
            hour("2026-11-09T14:00", 95, 60.0, 2.0),
        ];
        let found = notices(&forecast(hours), &Units::METRIC, now);
        let titles: Vec<&str> = found.iter().map(|notice| notice.title.as_str()).collect();
        assert!(titles.contains(&"Thunderstorms"));
        assert!(titles.contains(&"Heavy rain"));
        assert!(titles.contains(&"Strong wind"));
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found.last().unwrap().severity, Severity::Advisory);
    }
}
