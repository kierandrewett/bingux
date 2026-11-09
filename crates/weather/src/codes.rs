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

/// The condition a WMO code names, for its words in the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Condition {
    Clear,
    MostlyClear,
    PartlyCloudy,
    Overcast,
    Fog,
    FreezingFog,
    LightDrizzle,
    Drizzle,
    HeavyDrizzle,
    LightFreezingDrizzle,
    FreezingDrizzle,
    LightRain,
    Rain,
    HeavyRain,
    LightFreezingRain,
    FreezingRain,
    LightSnow,
    Snow,
    HeavySnow,
    SnowGrains,
    LightShowers,
    Showers,
    HeavyShowers,
    LightSnowShowers,
    SnowShowers,
    Thunderstorm,
    ThunderstormHail,
    SevereThunderstormHail,
    /// Any code not listed.
    Cloudy,
}

impl Condition {
    /// The condition for a WMO weather code.
    pub fn of(code: u8) -> Self {
        match code {
            0 => Self::Clear,
            1 => Self::MostlyClear,
            2 => Self::PartlyCloudy,
            3 => Self::Overcast,
            45 => Self::Fog,
            48 => Self::FreezingFog,
            51 => Self::LightDrizzle,
            53 => Self::Drizzle,
            55 => Self::HeavyDrizzle,
            56 => Self::LightFreezingDrizzle,
            57 => Self::FreezingDrizzle,
            61 => Self::LightRain,
            63 => Self::Rain,
            65 => Self::HeavyRain,
            66 => Self::LightFreezingRain,
            67 => Self::FreezingRain,
            71 => Self::LightSnow,
            73 => Self::Snow,
            75 => Self::HeavySnow,
            77 => Self::SnowGrains,
            80 => Self::LightShowers,
            81 => Self::Showers,
            82 => Self::HeavyShowers,
            85 => Self::LightSnowShowers,
            86 => Self::SnowShowers,
            95 => Self::Thunderstorm,
            96 => Self::ThunderstormHail,
            99 => Self::SevereThunderstormHail,
            _ => Self::Cloudy,
        }
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
            assert_ne!(Condition::of(code), Condition::Cloudy, "code {code}");
        }
    }
}
