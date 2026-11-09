//! Conditions worth a warning, read from the forecast.
//!
//! Open-Meteo does not carry the warnings that national weather services
//! issue, so these are not official alerts. They are what the forecast itself
//! says about the next day and a half: a thunderstorm, a heavy downpour, a
//! gale, extreme heat or cold, ice, heavy snow, a high UV index and poor air.
//!
//! The crate says what is found and the numbers behind it; the app words it.

use crate::model::{Forecast, Hour};
use crate::time::HOUR;

/// How much a notice matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing.
    Advisory,
    /// Take care.
    Warning,
}

/// What the forecast says to watch for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Thunder, with hail when the forecast says so.
    Thunderstorms {
        hail: bool,
    },
    /// Rain at `per_hour` millimetres an hour or more.
    HeavyRain {
        per_hour: f64,
    },
    /// Rain that freezes on contact.
    FreezingRain,
    HeavySnow,
    /// Gusts in km/h. `strong` is a gale-strength gust.
    Wind {
        gusts: f64,
        strong: bool,
    },
    /// The apparent temperature in degrees Celsius, at or above 35.
    ExtremeHeat {
        feels_like: f64,
    },
    /// The apparent temperature in degrees Celsius, at or below -15.
    ExtremeCold {
        feels_like: f64,
    },
    /// A UV index of 8 or more.
    VeryHighUv,
    /// A US air quality index. `unhealthy` is above 150; otherwise above 100.
    Air {
        aqi: f64,
        unhealthy: bool,
    },
}

/// A stretch of wall times, from the start of the first hour to the end of the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub from: i64,
    pub to: i64,
}

/// One thing the forecast says to watch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Notice {
    pub severity: Severity,
    pub kind: Kind,
    /// When it happens; `None` when the forecast gives no time.
    pub span: Option<Span>,
}

/// How far ahead to look.
const LOOKAHEAD: usize = 36;

/// The first and last hours of `hours` that match, as a span of wall times.
fn span(hours: &[&Hour], matches: impl Fn(&Hour) -> bool) -> Option<Span> {
    let mut found = hours.iter().filter(|hour| matches(hour));
    let first = found.next()?;
    let last = found.last().unwrap_or(first);
    Some(Span {
        from: first.time,
        to: last.time + HOUR,
    })
}

/// The notices for `forecast`, most serious first. `local` is the time at the
/// place now.
pub fn notices(forecast: &Forecast, local: i64) -> Vec<Notice> {
    let hours: Vec<&Hour> = forecast.hours_from(local).iter().take(LOOKAHEAD).collect();
    let mut out = Vec::new();

    if let Some(span) = span(&hours, |hour| matches!(hour.code, Some(96 | 99))) {
        out.push(Notice {
            severity: Severity::Warning,
            kind: Kind::Thunderstorms { hail: true },
            span: Some(span),
        });
    } else if let Some(span) = span(&hours, |hour| matches!(hour.code, Some(95))) {
        out.push(Notice {
            severity: Severity::Advisory,
            kind: Kind::Thunderstorms { hail: false },
            span: Some(span),
        });
    }

    let heaviest = hours
        .iter()
        .filter_map(|hour| hour.precipitation)
        .fold(0.0_f64, f64::max);
    if heaviest >= 8.0 {
        out.push(Notice {
            severity: Severity::Warning,
            kind: Kind::HeavyRain { per_hour: heaviest },
            span: span(&hours, |hour| hour.precipitation.unwrap_or(0.0) >= 8.0),
        });
    }

    if let Some(span) = span(&hours, |hour| matches!(hour.code, Some(56 | 57 | 66 | 67))) {
        out.push(Notice {
            severity: Severity::Warning,
            kind: Kind::FreezingRain,
            span: Some(span),
        });
    }

    if let Some(span) = span(&hours, |hour| matches!(hour.code, Some(75 | 86))) {
        out.push(Notice {
            severity: Severity::Warning,
            kind: Kind::HeavySnow,
            span: Some(span),
        });
    }

    let gusts = hours
        .iter()
        .filter_map(|hour| hour.wind_gusts)
        .fold(0.0_f64, f64::max);
    if gusts >= 55.0 {
        out.push(Notice {
            severity: if gusts >= 75.0 {
                Severity::Warning
            } else {
                Severity::Advisory
            },
            kind: Kind::Wind {
                gusts,
                strong: gusts >= 75.0,
            },
            span: span(&hours, |hour| hour.wind_gusts.unwrap_or(0.0) >= 55.0),
        });
    }

    let hottest = hours
        .iter()
        .filter_map(|hour| hour.apparent)
        .fold(f64::MIN, f64::max);
    if hottest >= 35.0 {
        out.push(Notice {
            severity: Severity::Warning,
            kind: Kind::ExtremeHeat {
                feels_like: hottest,
            },
            span: None,
        });
    }
    let coldest = hours
        .iter()
        .filter_map(|hour| hour.apparent)
        .fold(f64::MAX, f64::min);
    if coldest <= -15.0 {
        out.push(Notice {
            severity: Severity::Warning,
            kind: Kind::ExtremeCold {
                feels_like: coldest,
            },
            span: None,
        });
    }

    if let Some(span) = span(&hours, |hour| hour.uv.unwrap_or(0.0) >= 8.0) {
        out.push(Notice {
            severity: Severity::Advisory,
            kind: Kind::VeryHighUv,
            span: Some(span),
        });
    }

    if let Some(aqi) = forecast.air.as_ref().and_then(|air| air.us_aqi) {
        if aqi > 150.0 {
            out.push(Notice {
                severity: Severity::Warning,
                kind: Kind::Air {
                    aqi,
                    unhealthy: true,
                },
                span: None,
            });
        } else if aqi > 100.0 {
            out.push(Notice {
                severity: Severity::Advisory,
                kind: Kind::Air {
                    aqi,
                    unhealthy: false,
                },
                span: None,
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
        assert!(notices(&forecast(hours), now).is_empty());
    }

    #[test]
    fn storms_wind_and_rain_are_found_and_ordered() {
        let now = parse_local("2026-11-09T12:30").unwrap();
        let hours = vec![
            hour("2026-11-09T12:00", 3, 20.0, 0.0),
            hour("2026-11-09T13:00", 95, 80.0, 9.0),
            hour("2026-11-09T14:00", 95, 60.0, 2.0),
        ];
        let found = notices(&forecast(hours), now);
        let kinds: Vec<Kind> = found.iter().map(|notice| notice.kind).collect();
        assert!(kinds.contains(&Kind::Thunderstorms { hail: false }));
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, Kind::HeavyRain { .. }))
        );
        assert!(kinds.contains(&Kind::Wind {
            gusts: 80.0,
            strong: true
        }));
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found.last().unwrap().severity, Severity::Advisory);
    }
}
