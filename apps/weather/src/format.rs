//! Words and small decisions the pages share: how the light is, when rain
//! starts and stops, what a day is called, and how long a stretch of minutes
//! reads.

use bingux_weather::time::{day_of, weekday};
use bingux_weather::{Forecast, Units};

use bingux_ui::prelude::tr;

use crate::sky::Light;
use crate::words;

/// How near the sun's rising or setting is still twilight, in seconds.
const TWILIGHT: i64 = 40 * 60;
/// Rain in a quarter of an hour that counts as rain, in mm.
const WET: f64 = 0.05;

/// The light at `local`: twilight near sunrise and sunset, else by the
/// current conditions' own day flag.
pub fn light_of(forecast: &Forecast, local: i64, is_day: bool) -> Light {
    if let Some(day) = forecast.day_at(local) {
        for edge in [day.sunrise, day.sunset].into_iter().flatten() {
            if (local - edge).abs() <= TWILIGHT {
                return Light::Twilight;
            }
        }
    }
    if is_day { Light::Day } else { Light::Night }
}

/// `20 min`, `1 h`, `1 h 15 min`.
pub fn duration(minutes: i64) -> String {
    let (hours, minutes) = (minutes / 60, minutes % 60);
    match (hours, minutes) {
        (0, minutes) => tr!("weather-duration-minutes", minutes = minutes),
        (hours, 0) => tr!("weather-duration-hours", hours = hours),
        (hours, minutes) => tr!(
            "weather-duration-hours-minutes",
            hours = hours,
            minutes = minutes
        ),
    }
}

/// A line on what the rain does in the next hours, from the quarter-hour
/// forecast: `Rain starts in 20 min`, `Rain stops in 45 min`, or nothing when
/// it stays dry (or the forecast has no quarter hours).
pub fn rain_outlook(forecast: &Forecast, local: i64) -> Option<String> {
    let steps: Vec<(i64, bool)> = forecast
        .minutely
        .iter()
        // The quarter hour that holds now, and later ones.
        .filter(|minute| minute.time + 900 > local)
        .filter_map(|minute| Some((minute.time, minute.precipitation? >= WET)))
        .collect();
    let now_wet = steps.first().map(|step| step.1)?;
    let word = if forecast.current.temperature.unwrap_or(10.0) <= 0.5 {
        tr!("weather-word-snow")
    } else {
        tr!("weather-word-rain")
    };
    let after = |wet: bool| {
        steps
            .iter()
            .find(|step| step.1 == wet && step.0 > local)
            .map(|step| ((step.0 - local) / 60).max(1))
    };
    if now_wet {
        match after(false) {
            Some(minutes) => Some(tr!(
                "weather-outlook-stops",
                word = word,
                duration = duration(minutes)
            )),
            None => Some(tr!(
                "weather-outlook-for-next",
                word = word,
                duration = duration(steps.len() as i64 * 15)
            )),
        }
    } else {
        after(true).map(|minutes| {
            tr!(
                "weather-outlook-starts",
                word = word,
                duration = duration(minutes)
            )
        })
    }
}

/// `Today` for the day that holds `now`, else the weekday: `Tue`.
pub fn day_short(day: i64, now: i64) -> String {
    if day_of(day) == day_of(now) {
        tr!("weather-today")
    } else {
        words::weekday_short(weekday(day))
    }
}

/// `Today`, `Tomorrow`, or the weekday in full, for a spoken label.
pub fn day_long(day: i64, now: i64) -> String {
    match day_of(day) - day_of(now) {
        0 => tr!("weather-today"),
        1 => tr!("weather-tomorrow"),
        _ => words::weekday_name(weekday(day)),
    }
}

/// `54%`.
pub fn percent(value: f64) -> String {
    format!("{}%", value.round() as i64)
}

/// A temperature, or a dash for a value the service did not give.
pub fn temperature(units: &Units, value: Option<f64>) -> String {
    value.map_or_else(|| "–".to_string(), |value| units.temperature(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_weather::Minute;
    use bingux_weather::time::parse_local;

    /// The app's messages, for the `tr!` the formatters call.
    fn catalogue() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| bingux_ui::i18n::register(bingux_ui::catalogue!("weather")));
    }

    fn forecast(steps: &[(&str, f64)]) -> Forecast {
        Forecast {
            minutely: steps
                .iter()
                .map(|(at, rain)| Minute {
                    time: parse_local(at).unwrap(),
                    precipitation: Some(*rain),
                })
                .collect(),
            ..Forecast::default()
        }
    }

    #[test]
    fn durations_read_plainly() {
        catalogue();
        assert_eq!(duration(20), "20 min");
        assert_eq!(duration(60), "1 h");
        assert_eq!(duration(75), "1 h 15 min");
    }

    #[test]
    fn rain_is_announced_before_and_after() {
        catalogue();
        let now = parse_local("2026-11-09T12:05").unwrap();
        let coming = forecast(&[
            ("2026-11-09T12:00", 0.0),
            ("2026-11-09T12:15", 0.0),
            ("2026-11-09T12:30", 0.4),
        ]);
        assert_eq!(
            rain_outlook(&coming, now).as_deref(),
            Some("Rain starts in 25 min")
        );
        let ending = forecast(&[
            ("2026-11-09T12:00", 0.4),
            ("2026-11-09T12:15", 0.2),
            ("2026-11-09T12:30", 0.0),
        ]);
        assert_eq!(
            rain_outlook(&ending, now).as_deref(),
            Some("Rain stops in 25 min")
        );
        let dry = forecast(&[("2026-11-09T12:00", 0.0), ("2026-11-09T12:15", 0.0)]);
        assert_eq!(rain_outlook(&dry, now), None);
        assert_eq!(rain_outlook(&Forecast::default(), now), None);
    }

    #[test]
    fn days_are_named_from_today() {
        catalogue();
        let now = parse_local("2026-11-09T12:00").unwrap();
        let tomorrow = parse_local("2026-11-10").unwrap();
        assert_eq!(day_short(parse_local("2026-11-09").unwrap(), now), "Today");
        assert_eq!(day_short(tomorrow, now), "Tue");
        assert_eq!(day_long(tomorrow, now), "Tomorrow");
        assert_eq!(
            day_long(parse_local("2026-11-12").unwrap(), now),
            "Thursday"
        );
    }
}
