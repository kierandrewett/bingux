//! The app's words for the crate's enums and the sentences built from them.
//! Each enum maps to one message per variant; the crate keeps no English.

use bingux_ui::prelude::tr;
use bingux_weather::astro::MoonPhase;
use bingux_weather::codes::Condition;
use bingux_weather::notices::{Kind, Span};
use bingux_weather::text::{
    AqiBand, DewComfort, Point, PressureTrend, UvLevel, Visibility, WindForce,
};
use bingux_weather::time::{Clock, day_of, hour_of, minute_of};
use bingux_weather::{Forecast, Place, Units, text};

/// The sky's condition in words: `Light rain`. Sentence case.
pub fn condition(condition: Condition) -> String {
    match condition {
        Condition::Clear => tr!("weather-sky-clear"),
        Condition::MostlyClear => tr!("weather-sky-mostly-clear"),
        Condition::PartlyCloudy => tr!("weather-sky-partly-cloudy"),
        Condition::Overcast => tr!("weather-sky-overcast"),
        Condition::Fog => tr!("weather-sky-fog"),
        Condition::FreezingFog => tr!("weather-sky-freezing-fog"),
        Condition::LightDrizzle => tr!("weather-sky-light-drizzle"),
        Condition::Drizzle => tr!("weather-sky-drizzle"),
        Condition::HeavyDrizzle => tr!("weather-sky-heavy-drizzle"),
        Condition::LightFreezingDrizzle => tr!("weather-sky-light-freezing-drizzle"),
        Condition::FreezingDrizzle => tr!("weather-sky-freezing-drizzle"),
        Condition::LightRain => tr!("weather-sky-light-rain"),
        Condition::Rain => tr!("weather-sky-rain"),
        Condition::HeavyRain => tr!("weather-sky-heavy-rain"),
        Condition::LightFreezingRain => tr!("weather-sky-light-freezing-rain"),
        Condition::FreezingRain => tr!("weather-sky-freezing-rain"),
        Condition::LightSnow => tr!("weather-sky-light-snow"),
        Condition::Snow => tr!("weather-sky-snow"),
        Condition::HeavySnow => tr!("weather-sky-heavy-snow"),
        Condition::SnowGrains => tr!("weather-sky-snow-grains"),
        Condition::LightShowers => tr!("weather-sky-light-showers"),
        Condition::Showers => tr!("weather-sky-showers"),
        Condition::HeavyShowers => tr!("weather-sky-heavy-showers"),
        Condition::LightSnowShowers => tr!("weather-sky-light-snow-showers"),
        Condition::SnowShowers => tr!("weather-sky-snow-showers"),
        Condition::Thunderstorm => tr!("weather-sky-thunderstorm"),
        Condition::ThunderstormHail => tr!("weather-sky-thunderstorm-hail"),
        Condition::SevereThunderstormHail => tr!("weather-sky-severe-thunderstorm-hail"),
        Condition::Cloudy => tr!("weather-sky-cloudy"),
    }
}

/// The sky's condition in lowercase, to sit inside a sentence: `light rain`.
/// Its own message, so no sentence has its case changed in code.
pub fn condition_inline(condition: Condition) -> String {
    match condition {
        Condition::Clear => tr!("weather-sky-inline-clear"),
        Condition::MostlyClear => tr!("weather-sky-inline-mostly-clear"),
        Condition::PartlyCloudy => tr!("weather-sky-inline-partly-cloudy"),
        Condition::Overcast => tr!("weather-sky-inline-overcast"),
        Condition::Fog => tr!("weather-sky-inline-fog"),
        Condition::FreezingFog => tr!("weather-sky-inline-freezing-fog"),
        Condition::LightDrizzle => tr!("weather-sky-inline-light-drizzle"),
        Condition::Drizzle => tr!("weather-sky-inline-drizzle"),
        Condition::HeavyDrizzle => tr!("weather-sky-inline-heavy-drizzle"),
        Condition::LightFreezingDrizzle => tr!("weather-sky-inline-light-freezing-drizzle"),
        Condition::FreezingDrizzle => tr!("weather-sky-inline-freezing-drizzle"),
        Condition::LightRain => tr!("weather-sky-inline-light-rain"),
        Condition::Rain => tr!("weather-sky-inline-rain"),
        Condition::HeavyRain => tr!("weather-sky-inline-heavy-rain"),
        Condition::LightFreezingRain => tr!("weather-sky-inline-light-freezing-rain"),
        Condition::FreezingRain => tr!("weather-sky-inline-freezing-rain"),
        Condition::LightSnow => tr!("weather-sky-inline-light-snow"),
        Condition::Snow => tr!("weather-sky-inline-snow"),
        Condition::HeavySnow => tr!("weather-sky-inline-heavy-snow"),
        Condition::SnowGrains => tr!("weather-sky-inline-snow-grains"),
        Condition::LightShowers => tr!("weather-sky-inline-light-showers"),
        Condition::Showers => tr!("weather-sky-inline-showers"),
        Condition::HeavyShowers => tr!("weather-sky-inline-heavy-showers"),
        Condition::LightSnowShowers => tr!("weather-sky-inline-light-snow-showers"),
        Condition::SnowShowers => tr!("weather-sky-inline-snow-showers"),
        Condition::Thunderstorm => tr!("weather-sky-inline-thunderstorm"),
        Condition::ThunderstormHail => tr!("weather-sky-inline-thunderstorm-hail"),
        Condition::SevereThunderstormHail => tr!("weather-sky-inline-severe-thunderstorm-hail"),
        Condition::Cloudy => tr!("weather-sky-inline-cloudy"),
    }
}

/// The moon's phase in words: `Waxing gibbous`.
pub fn moon_phase(phase: MoonPhase) -> String {
    match phase {
        MoonPhase::New => tr!("weather-moon-new"),
        MoonPhase::WaxingCrescent => tr!("weather-moon-waxing-crescent"),
        MoonPhase::FirstQuarter => tr!("weather-moon-first-quarter"),
        MoonPhase::WaxingGibbous => tr!("weather-moon-waxing-gibbous"),
        MoonPhase::Full => tr!("weather-moon-full"),
        MoonPhase::WaningGibbous => tr!("weather-moon-waning-gibbous"),
        MoonPhase::LastQuarter => tr!("weather-moon-last-quarter"),
        MoonPhase::WaningCrescent => tr!("weather-moon-waning-crescent"),
    }
}

/// The conditions now in three sentences, for the clipboard and for a screen
/// reader: the place, the temperature and the sky; how it feels and the day's
/// range; the wind and the humidity.
pub fn conditions(place: &Place, forecast: &Forecast, units: &Units) -> String {
    let current = &forecast.current;
    let local = current.time;
    let sky = current
        .code
        .map(|code| condition_inline(Condition::of(code)));
    let temperature = current.temperature.map(|t| units.temperature_with_unit(t));
    let first = match (temperature, sky) {
        (Some(temperature), Some(sky)) => tr!(
            "weather-conditions-both",
            place = place.name.as_str(),
            temperature = temperature,
            sky = sky
        ),
        (Some(temperature), None) => tr!(
            "weather-conditions-temperature",
            place = place.name.as_str(),
            temperature = temperature
        ),
        (None, Some(sky)) => tr!(
            "weather-conditions-sky",
            place = place.name.as_str(),
            sky = sky
        ),
        (None, None) => tr!("weather-conditions-place", place = place.name.as_str()),
    };

    let day = forecast.day_at(local);
    let feel = feel_sentence(
        current.apparent.map(|t| units.temperature_with_unit(t)),
        day.and_then(|day| day.high).map(|t| units.temperature(t)),
        day.and_then(|day| day.low).map(|t| units.temperature(t)),
    );

    let wind = current.wind_speed.map(|speed| {
        let speed = units.speed(speed);
        let direction = current
            .wind_direction
            .map(|degrees| point_long(text::point(degrees)));
        (speed, direction)
    });
    let air = air_sentence(wind, current.humidity.map(|h| h.round() as i64));

    [Some(first), feel, air]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

/// How it feels and the day's range, as one whole sentence for the shape present.
fn feel_sentence(
    feels: Option<String>,
    high: Option<String>,
    low: Option<String>,
) -> Option<String> {
    Some(match (feels, high, low) {
        (Some(feels), Some(high), Some(low)) => {
            tr!("weather-feel-all", feels = feels, high = high, low = low)
        }
        (Some(feels), Some(high), None) => {
            tr!("weather-feel-feels-high", feels = feels, high = high)
        }
        (Some(feels), None, Some(low)) => tr!("weather-feel-feels-low", feels = feels, low = low),
        (None, Some(high), Some(low)) => tr!("weather-feel-high-low", high = high, low = low),
        (Some(feels), None, None) => tr!("weather-feel-feels", feels = feels),
        (None, Some(high), None) => tr!("weather-feel-high", high = high),
        (None, None, Some(low)) => tr!("weather-feel-low", low = low),
        (None, None, None) => return None,
    })
}

/// The wind and the humidity, as one whole sentence for the shape present.
fn air_sentence(wind: Option<(String, Option<String>)>, humidity: Option<i64>) -> Option<String> {
    Some(match (wind, humidity) {
        (Some((speed, Some(direction))), Some(percent)) => tr!(
            "weather-air-wind-from-humidity",
            speed = speed,
            direction = direction,
            percent = percent
        ),
        (Some((speed, Some(direction))), None) => tr!(
            "weather-air-wind-from",
            speed = speed,
            direction = direction
        ),
        (Some((speed, None)), Some(percent)) => tr!(
            "weather-air-wind-humidity",
            speed = speed,
            percent = percent
        ),
        (Some((speed, None)), None) => tr!("weather-air-wind", speed = speed),
        (None, Some(percent)) => tr!("weather-air-humidity", percent = percent),
        (None, None) => return None,
    })
}

/// The UV band's name, as a title, e.g. `Very high`.
pub fn uv_category(level: UvLevel) -> String {
    match level {
        UvLevel::Low => tr!("weather-uv-low"),
        UvLevel::Moderate => tr!("weather-uv-moderate"),
        UvLevel::High => tr!("weather-uv-high"),
        UvLevel::VeryHigh => tr!("weather-uv-very-high"),
        UvLevel::Extreme => tr!("weather-uv-extreme"),
    }
}

/// The UV band lowercase, for a sentence.
pub fn uv_inline(level: UvLevel) -> String {
    match level {
        UvLevel::Low => tr!("weather-uv-inline-low"),
        UvLevel::Moderate => tr!("weather-uv-inline-moderate"),
        UvLevel::High => tr!("weather-uv-inline-high"),
        UvLevel::VeryHigh => tr!("weather-uv-inline-very-high"),
        UvLevel::Extreme => tr!("weather-uv-inline-extreme"),
    }
}

/// What a UV band asks of a person, in one line.
pub fn uv_advice(level: UvLevel) -> String {
    match level {
        UvLevel::Low => tr!("weather-uv-advice-low"),
        UvLevel::Moderate => tr!("weather-uv-advice-moderate"),
        UvLevel::High => tr!("weather-uv-advice-high"),
        UvLevel::VeryHigh => tr!("weather-uv-advice-very-high"),
        UvLevel::Extreme => tr!("weather-uv-advice-extreme"),
    }
}

/// The US air quality band's name, as a title.
pub fn aqi_category(band: AqiBand) -> String {
    match band {
        AqiBand::Good => tr!("weather-aqi-good"),
        AqiBand::Moderate => tr!("weather-aqi-moderate"),
        AqiBand::UnhealthySensitive => tr!("weather-aqi-unhealthy-sensitive"),
        AqiBand::Unhealthy => tr!("weather-aqi-unhealthy"),
        AqiBand::VeryUnhealthy => tr!("weather-aqi-very-unhealthy"),
        AqiBand::Hazardous => tr!("weather-aqi-hazardous"),
    }
}

/// The air quality band lowercase, for a sentence.
pub fn aqi_inline(band: AqiBand) -> String {
    match band {
        AqiBand::Good => tr!("weather-aqi-inline-good"),
        AqiBand::Moderate => tr!("weather-aqi-inline-moderate"),
        AqiBand::UnhealthySensitive => tr!("weather-aqi-inline-unhealthy-sensitive"),
        AqiBand::Unhealthy => tr!("weather-aqi-inline-unhealthy"),
        AqiBand::VeryUnhealthy => tr!("weather-aqi-inline-very-unhealthy"),
        AqiBand::Hazardous => tr!("weather-aqi-inline-hazardous"),
    }
}

/// What the air quality band means for a person, in one line.
pub fn aqi_advice(band: AqiBand) -> String {
    match band {
        AqiBand::Good => tr!("weather-aqi-advice-good"),
        AqiBand::Moderate => tr!("weather-aqi-advice-moderate"),
        AqiBand::UnhealthySensitive => tr!("weather-aqi-advice-unhealthy-sensitive"),
        AqiBand::Unhealthy => tr!("weather-aqi-advice-unhealthy"),
        AqiBand::VeryUnhealthy => tr!("weather-aqi-advice-very-unhealthy"),
        AqiBand::Hazardous => tr!("weather-aqi-advice-hazardous"),
    }
}

/// How humid the air feels.
pub fn dew_comfort(comfort: DewComfort) -> String {
    match comfort {
        DewComfort::Dry => tr!("weather-dew-dry"),
        DewComfort::Comfortable => tr!("weather-dew-comfortable"),
        DewComfort::Humid => tr!("weather-dew-humid"),
        DewComfort::VeryHumid => tr!("weather-dew-very-humid"),
        DewComfort::Oppressive => tr!("weather-dew-oppressive"),
    }
}

/// How far a person can see.
pub fn visibility(band: Visibility) -> String {
    match band {
        Visibility::Excellent => tr!("weather-visibility-excellent"),
        Visibility::Clear => tr!("weather-visibility-clear"),
        Visibility::Moderate => tr!("weather-visibility-moderate"),
        Visibility::Poor => tr!("weather-visibility-poor"),
        Visibility::VeryPoor => tr!("weather-visibility-very-poor"),
    }
}

/// The pressure trend over three hours.
pub fn pressure_trend(trend: PressureTrend) -> String {
    match trend {
        PressureTrend::Rising => tr!("weather-pressure-rising"),
        PressureTrend::Falling => tr!("weather-pressure-falling"),
        PressureTrend::Steady => tr!("weather-pressure-steady"),
    }
}

/// How windy it is, on the Beaufort scale's plain names.
pub fn wind_force(force: WindForce) -> String {
    match force {
        WindForce::Calm => tr!("weather-force-calm"),
        WindForce::LightBreeze => tr!("weather-force-light-breeze"),
        WindForce::Breeze => tr!("weather-force-breeze"),
        WindForce::StrongBreeze => tr!("weather-force-strong-breeze"),
        WindForce::Gale => tr!("weather-force-gale"),
        WindForce::Storm => tr!("weather-force-storm"),
        WindForce::HurricaneForce => tr!("weather-force-hurricane-force"),
    }
}

/// The compass point, short: `NE`.
pub fn point_short(point: Point) -> String {
    match point {
        Point::N => tr!("weather-point-short-n"),
        Point::Nne => tr!("weather-point-short-nne"),
        Point::Ne => tr!("weather-point-short-ne"),
        Point::Ene => tr!("weather-point-short-ene"),
        Point::E => tr!("weather-point-short-e"),
        Point::Ese => tr!("weather-point-short-ese"),
        Point::Se => tr!("weather-point-short-se"),
        Point::Sse => tr!("weather-point-short-sse"),
        Point::S => tr!("weather-point-short-s"),
        Point::Ssw => tr!("weather-point-short-ssw"),
        Point::Sw => tr!("weather-point-short-sw"),
        Point::Wsw => tr!("weather-point-short-wsw"),
        Point::W => tr!("weather-point-short-w"),
        Point::Wnw => tr!("weather-point-short-wnw"),
        Point::Nw => tr!("weather-point-short-nw"),
        Point::Nnw => tr!("weather-point-short-nnw"),
    }
}

/// The compass point spelled out, lower case: `north-east`.
pub fn point_long(point: Point) -> String {
    match point {
        Point::N => tr!("weather-point-long-n"),
        Point::Nne => tr!("weather-point-long-nne"),
        Point::Ne => tr!("weather-point-long-ne"),
        Point::Ene => tr!("weather-point-long-ene"),
        Point::E => tr!("weather-point-long-e"),
        Point::Ese => tr!("weather-point-long-ese"),
        Point::Se => tr!("weather-point-long-se"),
        Point::Sse => tr!("weather-point-long-sse"),
        Point::S => tr!("weather-point-long-s"),
        Point::Ssw => tr!("weather-point-long-ssw"),
        Point::Sw => tr!("weather-point-long-sw"),
        Point::Wsw => tr!("weather-point-long-wsw"),
        Point::W => tr!("weather-point-long-w"),
        Point::Wnw => tr!("weather-point-long-wnw"),
        Point::Nw => tr!("weather-point-long-nw"),
        Point::Nnw => tr!("weather-point-long-nnw"),
    }
}

/// The compass point spelled out, as a title: `North-east`.
pub fn point_title(point: Point) -> String {
    match point {
        Point::N => tr!("weather-point-title-n"),
        Point::Nne => tr!("weather-point-title-nne"),
        Point::Ne => tr!("weather-point-title-ne"),
        Point::Ene => tr!("weather-point-title-ene"),
        Point::E => tr!("weather-point-title-e"),
        Point::Ese => tr!("weather-point-title-ese"),
        Point::Se => tr!("weather-point-title-se"),
        Point::Sse => tr!("weather-point-title-sse"),
        Point::S => tr!("weather-point-title-s"),
        Point::Ssw => tr!("weather-point-title-ssw"),
        Point::Sw => tr!("weather-point-title-sw"),
        Point::Wsw => tr!("weather-point-title-wsw"),
        Point::W => tr!("weather-point-title-w"),
        Point::Wnw => tr!("weather-point-title-wnw"),
        Point::Nw => tr!("weather-point-title-nw"),
        Point::Nnw => tr!("weather-point-title-nnw"),
    }
}

/// Join the parts of a detail line with the translated separator.
pub fn join_list(parts: &[String]) -> String {
    let separator = tr!("weather-list-separator");
    parts.join(&separator)
}

/// The heading of a notice.
pub fn notice_title(kind: Kind) -> String {
    match kind {
        Kind::Thunderstorms { hail: true } => tr!("weather-notice-thunder-hail-title"),
        Kind::Thunderstorms { hail: false } => tr!("weather-notice-thunder-title"),
        Kind::HeavyRain { .. } => tr!("weather-notice-rain-title"),
        Kind::FreezingRain => tr!("weather-notice-ice-title"),
        Kind::HeavySnow => tr!("weather-notice-snow-title"),
        Kind::Wind { strong: true, .. } => tr!("weather-notice-wind-strong-title"),
        Kind::Wind { strong: false, .. } => tr!("weather-notice-wind-gusty-title"),
        Kind::ExtremeHeat { .. } => tr!("weather-notice-heat-title"),
        Kind::ExtremeCold { .. } => tr!("weather-notice-cold-title"),
        Kind::VeryHighUv => tr!("weather-notice-uv-title"),
        Kind::Air {
            unhealthy: true, ..
        } => tr!("weather-notice-air-unhealthy-title"),
        Kind::Air {
            unhealthy: false, ..
        } => tr!("weather-notice-air-poor-title"),
    }
}

/// The body of a notice: what it is, and when, when the forecast gives a time.
pub fn notice_detail(kind: Kind, span: Option<Span>, units: &Units, local: i64) -> String {
    let timed = || span.map_or_else(String::new, |span| when_text(units, local, span));
    match kind {
        Kind::Thunderstorms { .. } | Kind::HeavySnow => timed(),
        Kind::HeavyRain { per_hour } => match span {
            Some(span) => tr!(
                "weather-notice-rain-detail",
                amount = units.rain(per_hour),
                when = when_text(units, local, span)
            ),
            None => tr!(
                "weather-notice-rain-detail-untimed",
                amount = units.rain(per_hour)
            ),
        },
        Kind::FreezingRain => tr!("weather-notice-ice-detail", when = timed()),
        Kind::Wind { gusts, .. } => match span {
            Some(span) => tr!(
                "weather-notice-wind-detail",
                speed = units.speed(gusts),
                when = when_text(units, local, span)
            ),
            None => tr!(
                "weather-notice-wind-detail-untimed",
                speed = units.speed(gusts)
            ),
        },
        Kind::ExtremeHeat { feels_like } => tr!(
            "weather-notice-heat-detail",
            temperature = units.temperature_with_unit(feels_like)
        ),
        Kind::ExtremeCold { feels_like } => tr!(
            "weather-notice-cold-detail",
            temperature = units.temperature_with_unit(feels_like)
        ),
        Kind::VeryHighUv => tr!("weather-notice-uv-detail", when = timed()),
        Kind::Air { aqi, .. } => tr!("weather-notice-air-detail", index = aqi.round() as i64),
    }
}

/// A stretch of wall times as a phrase: `Until 15:00 tomorrow.`,
/// `From 03:00 to 06:00.`, `From 23:00 tomorrow to 02:00 tomorrow.`
fn when_text(units: &Units, local: i64, span: Span) -> String {
    let clock = units.clock;
    let (from, to) = (span.from, span.to);
    let start_tomorrow = day_of(from) != day_of(local);
    let end_tomorrow = day_of(to - 1) != day_of(local);
    if from <= local {
        if end_tomorrow {
            tr!("weather-when-until-tomorrow", time = clock.time(to))
        } else {
            tr!("weather-when-until", time = clock.time(to))
        }
    } else if start_tomorrow && !end_tomorrow {
        tr!(
            "weather-when-from-tomorrow-to",
            from = clock.time(from),
            to = clock.time(to)
        )
    } else if end_tomorrow {
        tr!(
            "weather-when-between-tomorrow",
            from = clock.time(from),
            to = clock.time(to)
        )
    } else {
        tr!(
            "weather-when-between",
            from = clock.time(from),
            to = clock.time(to)
        )
    }
}

/// The weekday, `index` 0 for Monday to 6 for Sunday, in full.
pub fn weekday_name(index: usize) -> String {
    match index {
        0 => tr!("weather-weekday-monday"),
        1 => tr!("weather-weekday-tuesday"),
        2 => tr!("weather-weekday-wednesday"),
        3 => tr!("weather-weekday-thursday"),
        4 => tr!("weather-weekday-friday"),
        5 => tr!("weather-weekday-saturday"),
        _ => tr!("weather-weekday-sunday"),
    }
}

/// The weekday's first three letters, `index` as for [`weekday_name`].
pub fn weekday_short(index: usize) -> String {
    match index {
        0 => tr!("weather-weekday-short-monday"),
        1 => tr!("weather-weekday-short-tuesday"),
        2 => tr!("weather-weekday-short-wednesday"),
        3 => tr!("weather-weekday-short-thursday"),
        4 => tr!("weather-weekday-short-friday"),
        5 => tr!("weather-weekday-short-saturday"),
        _ => tr!("weather-weekday-short-sunday"),
    }
}

/// The month, `index` 0 for January to 11 for December, in full.
pub fn month_name(index: usize) -> String {
    match index {
        0 => tr!("weather-month-january"),
        1 => tr!("weather-month-february"),
        2 => tr!("weather-month-march"),
        3 => tr!("weather-month-april"),
        4 => tr!("weather-month-may"),
        5 => tr!("weather-month-june"),
        6 => tr!("weather-month-july"),
        7 => tr!("weather-month-august"),
        8 => tr!("weather-month-september"),
        9 => tr!("weather-month-october"),
        10 => tr!("weather-month-november"),
        _ => tr!("weather-month-december"),
    }
}

/// A date as a day and month, `9 November`, for a sentence.
pub fn day_month(wall: i64) -> String {
    let (month, day) = bingux_weather::time::month_day(wall);
    tr!(
        "weather-day-month",
        day = i64::from(day),
        month = month_name(month)
    )
}

/// Clock wording for a forecast's wall-clock time: `14:05` or `2:05 PM`, and
/// the hour alone for a strip of hours: `14` or `2 PM`.
pub trait ClockText {
    fn time(self, wall: i64) -> String;
    fn hour(self, wall: i64) -> String;
}

impl ClockText for Clock {
    fn time(self, wall: i64) -> String {
        let (hour, minute) = (hour_of(wall), minute_of(wall));
        match self {
            Clock::TwentyFour => format!("{hour:02}:{minute:02}"),
            Clock::Twelve => {
                let (shown, period) = twelve(hour);
                tr!(
                    "weather-clock-time-12h",
                    hour = i64::from(shown),
                    minute = format!("{minute:02}"),
                    period = period
                )
            }
        }
    }

    fn hour(self, wall: i64) -> String {
        let hour = hour_of(wall);
        match self {
            Clock::TwentyFour => format!("{hour:02}"),
            Clock::Twelve => {
                let (shown, period) = twelve(hour);
                tr!(
                    "weather-clock-hour-12h",
                    hour = i64::from(shown),
                    period = period
                )
            }
        }
    }
}

/// The 12-hour figure and its period for a 24-hour `hour`.
fn twelve(hour: u32) -> (u32, String) {
    let shown = match hour % 12 {
        0 => 12,
        other => other,
    };
    let period = if hour < 12 {
        tr!("weather-period-am")
    } else {
        tr!("weather-period-pm")
    };
    (shown, period)
}
