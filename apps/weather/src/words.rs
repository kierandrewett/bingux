//! The app's words for the crate's enums and the sentences built from them.
//! Each enum maps to one message per variant; the crate keeps no English.

use bingux_ui::prelude::tr;
use bingux_weather::astro::MoonPhase;
use bingux_weather::codes::Condition;
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
        let direction = current.wind_direction.map(text::cardinal_long);
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
