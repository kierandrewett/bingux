//! Words for numbers: wind directions, the UV and air quality scales, the
//! trend of the pressure.

use crate::model::Forecast;

const POINTS: [&str; 16] = [
    "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW",
    "NNW",
];

/// The compass point a bearing in degrees is nearest: `NE`.
pub fn cardinal(degrees: f64) -> &'static str {
    let index = ((degrees.rem_euclid(360.0) / 22.5) + 0.5).floor() as usize % 16;
    POINTS[index]
}

/// The compass point spelled out, for a screen reader: `north-east`.
pub fn cardinal_long(degrees: f64) -> String {
    let short = cardinal(degrees);
    let word = |letter: char| match letter {
        'N' => "north",
        'E' => "east",
        'S' => "south",
        _ => "west",
    };
    let letters: Vec<char> = short.chars().collect();
    match letters.len() {
        1 => word(letters[0]).to_string(),
        2 => format!("{}-{}", word(letters[0]), word(letters[1])),
        _ => format!(
            "{}-{}-{}",
            word(letters[0]),
            word(letters[1]),
            word(letters[2])
        ),
    }
}

/// The UV scale of the WHO: `Low` to `Extreme`.
pub fn uv_category(index: f64) -> &'static str {
    match index {
        i if i < 3.0 => "Low",
        i if i < 6.0 => "Moderate",
        i if i < 8.0 => "High",
        i if i < 11.0 => "Very high",
        _ => "Extreme",
    }
}

/// What a UV index asks of a person, in one line.
pub fn uv_advice(index: f64) -> &'static str {
    match index {
        i if i < 3.0 => "No protection needed.",
        i if i < 6.0 => "Wear sunscreen if you are outside for long.",
        i if i < 8.0 => "Wear sunscreen and a hat. Seek shade at midday.",
        i if i < 11.0 => "Cover up and wear sunscreen. Avoid the midday sun.",
        _ => "Stay out of the sun at midday.",
    }
}

/// The US air quality index in words.
pub fn aqi_category(aqi: f64) -> &'static str {
    match aqi {
        a if a <= 50.0 => "Good",
        a if a <= 100.0 => "Moderate",
        a if a <= 150.0 => "Unhealthy for sensitive groups",
        a if a <= 200.0 => "Unhealthy",
        a if a <= 300.0 => "Very unhealthy",
        _ => "Hazardous",
    }
}

/// One line on what the air quality means for a person.
pub fn aqi_advice(aqi: f64) -> &'static str {
    match aqi {
        a if a <= 50.0 => "Air quality is satisfactory.",
        a if a <= 100.0 => "Unusually sensitive people may notice effects.",
        a if a <= 150.0 => "People with lung or heart disease should limit time outside.",
        a if a <= 200.0 => "Everyone may notice effects. Limit time outside.",
        a if a <= 300.0 => "Avoid exertion outside.",
        _ => "Stay indoors.",
    }
}

/// How humid the air feels, from the dew point in °C.
pub fn dew_comfort(dew_point: f64) -> &'static str {
    match dew_point {
        d if d < 10.0 => "Dry",
        d if d < 16.0 => "Comfortable",
        d if d < 18.0 => "Humid",
        d if d < 21.0 => "Very humid",
        _ => "Oppressive",
    }
}

/// How far a person can see, in words, from the visibility in metres.
pub fn visibility_note(metres: f64) -> &'static str {
    match metres {
        m if m >= 20_000.0 => "Excellent",
        m if m >= 10_000.0 => "Clear",
        m if m >= 4_000.0 => "Moderate",
        m if m >= 1_000.0 => "Poor",
        _ => "Very poor",
    }
}

/// `Rising`, `Falling` or `Steady`: the pressure now against three hours ago.
pub fn pressure_trend(forecast: &Forecast, local: i64) -> Option<&'static str> {
    let now = forecast.hour_at(local)?.pressure?;
    let before = forecast.hour_before(local, 3)?.pressure?;
    let change = now - before;
    Some(if change > 1.0 {
        "Rising"
    } else if change < -1.0 {
        "Falling"
    } else {
        "Steady"
    })
}

/// How windy it is, on the Beaufort scale's plain names, from km/h.
pub fn wind_force(kmh: f64) -> &'static str {
    match kmh {
        k if k < 2.0 => "Calm",
        k if k < 12.0 => "Light breeze",
        k if k < 29.0 => "Breeze",
        k if k < 50.0 => "Strong breeze",
        k if k < 75.0 => "Gale",
        k if k < 103.0 => "Storm",
        _ => "Hurricane force",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_name_points() {
        assert_eq!(cardinal(0.0), "N");
        assert_eq!(cardinal(359.0), "N");
        assert_eq!(cardinal(45.0), "NE");
        assert_eq!(cardinal(202.4), "SSW");
        assert_eq!(cardinal_long(45.0), "north-east");
        assert_eq!(cardinal_long(202.4), "south-south-west");
        assert_eq!(cardinal_long(270.0), "west");
    }

    #[test]
    fn scales_have_edges() {
        assert_eq!(uv_category(2.9), "Low");
        assert_eq!(uv_category(8.0), "Very high");
        assert_eq!(uv_category(12.0), "Extreme");
        assert_eq!(aqi_category(50.0), "Good");
        assert_eq!(aqi_category(151.0), "Unhealthy");
        assert_eq!(dew_comfort(12.0), "Comfortable");
        assert_eq!(visibility_note(24_000.0), "Excellent");
    }
}
