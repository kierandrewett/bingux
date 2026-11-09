//! The bands behind the words: wind directions, the UV and air quality scales,
//! the dew point, visibility, the pressure trend and the Beaufort force. The
//! crate names the bands; the app turns each one into words.

use crate::model::Forecast;

/// A compass point, sixteen to the circle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
    N,
    Nne,
    Ne,
    Ene,
    E,
    Ese,
    Se,
    Sse,
    S,
    Ssw,
    Sw,
    Wsw,
    W,
    Wnw,
    Nw,
    Nnw,
}

/// The compass point a bearing in degrees is nearest.
pub fn point(degrees: f64) -> Point {
    use Point::*;
    const POINTS: [Point; 16] = [
        N, Nne, Ne, Ene, E, Ese, Se, Sse, S, Ssw, Sw, Wsw, W, Wnw, Nw, Nnw,
    ];
    let index = ((degrees.rem_euclid(360.0) / 22.5) + 0.5).floor() as usize % 16;
    POINTS[index]
}

/// The UV scale of the WHO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UvLevel {
    Low,
    Moderate,
    High,
    VeryHigh,
    Extreme,
}

/// The UV band for an index.
pub fn uv_level(index: f64) -> UvLevel {
    match index {
        i if i < 3.0 => UvLevel::Low,
        i if i < 6.0 => UvLevel::Moderate,
        i if i < 8.0 => UvLevel::High,
        i if i < 11.0 => UvLevel::VeryHigh,
        _ => UvLevel::Extreme,
    }
}

/// The US air quality index band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AqiBand {
    Good,
    Moderate,
    UnhealthySensitive,
    Unhealthy,
    VeryUnhealthy,
    Hazardous,
}

/// The air quality band for an index.
pub fn aqi_band(aqi: f64) -> AqiBand {
    match aqi {
        a if a <= 50.0 => AqiBand::Good,
        a if a <= 100.0 => AqiBand::Moderate,
        a if a <= 150.0 => AqiBand::UnhealthySensitive,
        a if a <= 200.0 => AqiBand::Unhealthy,
        a if a <= 300.0 => AqiBand::VeryUnhealthy,
        _ => AqiBand::Hazardous,
    }
}

/// How humid the air feels, from the dew point in °C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DewComfort {
    Dry,
    Comfortable,
    Humid,
    VeryHumid,
    Oppressive,
}

/// The dew point band for a temperature in °C.
pub fn dew_comfort(dew_point: f64) -> DewComfort {
    match dew_point {
        d if d < 10.0 => DewComfort::Dry,
        d if d < 16.0 => DewComfort::Comfortable,
        d if d < 18.0 => DewComfort::Humid,
        d if d < 21.0 => DewComfort::VeryHumid,
        _ => DewComfort::Oppressive,
    }
}

/// How far a person can see, from the visibility in metres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Excellent,
    Clear,
    Moderate,
    Poor,
    VeryPoor,
}

/// The visibility band for a distance in metres.
pub fn visibility(metres: f64) -> Visibility {
    match metres {
        m if m >= 20_000.0 => Visibility::Excellent,
        m if m >= 10_000.0 => Visibility::Clear,
        m if m >= 4_000.0 => Visibility::Moderate,
        m if m >= 1_000.0 => Visibility::Poor,
        _ => Visibility::VeryPoor,
    }
}

/// The pressure now against three hours ago.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressureTrend {
    Rising,
    Falling,
    Steady,
}

/// The pressure trend, if the forecast has both readings.
pub fn pressure_trend(forecast: &Forecast, local: i64) -> Option<PressureTrend> {
    let now = forecast.hour_at(local)?.pressure?;
    let before = forecast.hour_before(local, 3)?.pressure?;
    let change = now - before;
    Some(if change > 1.0 {
        PressureTrend::Rising
    } else if change < -1.0 {
        PressureTrend::Falling
    } else {
        PressureTrend::Steady
    })
}

/// How windy it is, on the Beaufort scale, from km/h.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindForce {
    Calm,
    LightBreeze,
    Breeze,
    StrongBreeze,
    Gale,
    Storm,
    HurricaneForce,
}

/// The Beaufort band for a speed in km/h.
pub fn wind_force(kmh: f64) -> WindForce {
    match kmh {
        k if k < 2.0 => WindForce::Calm,
        k if k < 12.0 => WindForce::LightBreeze,
        k if k < 29.0 => WindForce::Breeze,
        k if k < 50.0 => WindForce::StrongBreeze,
        k if k < 75.0 => WindForce::Gale,
        k if k < 103.0 => WindForce::Storm,
        _ => WindForce::HurricaneForce,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_name_points() {
        assert_eq!(point(0.0), Point::N);
        assert_eq!(point(359.0), Point::N);
        assert_eq!(point(45.0), Point::Ne);
        assert_eq!(point(202.4), Point::Ssw);
        assert_eq!(point(270.0), Point::W);
    }

    #[test]
    fn scales_have_edges() {
        assert_eq!(uv_level(2.9), UvLevel::Low);
        assert_eq!(uv_level(8.0), UvLevel::VeryHigh);
        assert_eq!(uv_level(12.0), UvLevel::Extreme);
        assert_eq!(aqi_band(50.0), AqiBand::Good);
        assert_eq!(aqi_band(151.0), AqiBand::Unhealthy);
        assert_eq!(dew_comfort(12.0), DewComfort::Comfortable);
        assert_eq!(visibility(24_000.0), Visibility::Excellent);
        assert_eq!(wind_force(103.0), WindForce::HurricaneForce);
    }
}
