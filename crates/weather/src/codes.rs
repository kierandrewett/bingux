//! WMO weather codes in words, and as the kind of sky they paint.

/// What the sky is doing, for choosing a picture and a background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sky {
    Clear,
    MostlyClear,
    PartlyCloudy,
    Overcast,
    Fog,
    Drizzle,
    Rain,
    HeavyRain,
    FreezingRain,
    Snow,
    HeavySnow,
    Thunderstorm,
}

impl Sky {
    /// The sky of a WMO weather code. A code that is not one is overcast.
    pub fn of(code: u8) -> Sky {
        match code {
            0 => Sky::Clear,
            1 => Sky::MostlyClear,
            2 => Sky::PartlyCloudy,
            3 => Sky::Overcast,
            45 | 48 => Sky::Fog,
            51 | 53 | 55 => Sky::Drizzle,
            56 | 57 | 66 | 67 => Sky::FreezingRain,
            61 | 63 | 80 | 81 => Sky::Rain,
            65 | 82 => Sky::HeavyRain,
            71 | 73 | 77 | 85 => Sky::Snow,
            75 | 86 => Sky::HeavySnow,
            95 | 96 | 99 => Sky::Thunderstorm,
            _ => Sky::Overcast,
        }
    }

    /// Whether water falls from it.
    pub fn is_wet(self) -> bool {
        matches!(
            self,
            Sky::Drizzle
                | Sky::Rain
                | Sky::HeavyRain
                | Sky::FreezingRain
                | Sky::Snow
                | Sky::HeavySnow
                | Sky::Thunderstorm
        )
    }

    /// Whether it falls as snow.
    pub fn is_snow(self) -> bool {
        matches!(self, Sky::Snow | Sky::HeavySnow)
    }
}

/// The condition in words: `Light rain`. Sentence case, for a line of its own.
pub fn describe(code: u8) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 => "Fog",
        48 => "Freezing fog",
        51 => "Light drizzle",
        53 => "Drizzle",
        55 => "Heavy drizzle",
        56 => "Light freezing drizzle",
        57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 => "Light freezing rain",
        67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 => "Light showers",
        81 => "Showers",
        82 => "Heavy showers",
        85 => "Light snow showers",
        86 => "Snow showers",
        95 => "Thunderstorm",
        96 => "Thunderstorm with hail",
        99 => "Severe thunderstorm with hail",
        _ => "Cloudy",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_map_to_skies() {
        assert_eq!(Sky::of(0), Sky::Clear);
        assert_eq!(Sky::of(63), Sky::Rain);
        assert_eq!(Sky::of(86), Sky::HeavySnow);
        assert_eq!(Sky::of(99), Sky::Thunderstorm);
        assert_eq!(Sky::of(200), Sky::Overcast);
        assert!(Sky::of(65).is_wet());
        assert!(!Sky::of(3).is_wet());
    }

    #[test]
    fn every_documented_code_has_words() {
        for code in [
            0, 1, 2, 3, 45, 48, 51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 71, 73, 75, 77, 80, 81, 82,
            85, 86, 95, 96, 99,
        ] {
            assert_ne!(describe(code), "Cloudy", "code {code}");
        }
    }
}
