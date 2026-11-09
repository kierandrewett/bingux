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
        .map(|code| condition(Condition::of(code)).to_lowercase());
    let temperature = current.temperature.map(|t| units.temperature_with_unit(t));
    let first = match (temperature, sky) {
        (Some(temperature), Some(sky)) => {
            tr!(
                "weather-conditions-both",
                place = place.name.as_str(),
                temperature = temperature,
                sky = sky
            )
        }
        (Some(temperature), None) => {
            tr!(
                "weather-conditions-temperature",
                place = place.name.as_str(),
                temperature = temperature
            )
        }
        (None, Some(sky)) => tr!(
            "weather-conditions-sky",
            place = place.name.as_str(),
            sky = sky
        ),
        (None, None) => tr!("weather-conditions-place", place = place.name.as_str()),
    };

    let mut feel = Vec::new();
    if let Some(apparent) = current.apparent {
        feel.push(tr!(
            "weather-conditions-feels",
            temperature = units.temperature_with_unit(apparent)
        ));
    }
    if let Some(day) = forecast.day_at(local) {
        if let Some(high) = day.high {
            feel.push(tr!(
                "weather-conditions-high",
                temperature = units.temperature(high)
            ));
        }
        if let Some(low) = day.low {
            feel.push(tr!(
                "weather-conditions-low",
                temperature = units.temperature(low)
            ));
        }
    }

    let mut air = Vec::new();
    if let Some(speed) = current.wind_speed {
        let speed = units.speed(speed);
        match current.wind_direction {
            Some(degrees) => air.push(tr!(
                "weather-conditions-wind-from",
                speed = speed,
                direction = text::cardinal_long(degrees)
            )),
            None => air.push(tr!("weather-conditions-wind", speed = speed)),
        }
    }
    if let Some(humidity) = current.humidity {
        air.push(tr!(
            "weather-conditions-humidity",
            percent = humidity.round() as i64
        ));
    }

    [Some(first), sentence(feel), sentence(air)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fragments joined into one sentence, with a full stop at the end.
fn sentence(parts: Vec<String>) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    Some(tr!("weather-conditions-sentence", parts = parts.join(", ")))
}
