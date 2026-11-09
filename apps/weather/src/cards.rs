//! The detail cards: UV, wind, the sun, the moon, how it feels, humidity,
//! visibility, pressure and the air quality, and the two charts of the next
//! day (temperature and rain).
//!
//! A card is a name, one figure, a picture of where that figure sits and one
//! line on what it means. The small cards sit two to a row, on a desktop and on
//! a phone; the air quality and the charts take the row.

use bingux_ui::charts::{SeriesColour, bar_chart, line_chart, sparkline};
use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use bingux_weather::astro::{self, Moon};
use bingux_weather::time::{HOUR, day_month, unix_now};
use bingux_weather::{AirQuality, Forecast, Hour, Units, text};
use freya::prelude::*;

use crate::format;
use crate::glyph;
use crate::hourly::HOURS;
use crate::sky;

/// A small card's height, at the regular and compact densities.
const CARD_HEIGHT: f32 = 220.0;
const MOON_SIZE: f32 = 84.0;

/// A card: its name, then `body`.
fn card(tokens: &Tokens, title: &str, body: Vec<Element>) -> Element {
    rect()
        .tw(
            tokens,
            &[
                flex_col(),
                gap(S2),
                p(S4),
                rounded(Radius::Card),
                bg(Paint::Card),
            ],
        )
        .height(Size::px(CARD_HEIGHT))
        .width(Size::flex(1.0))
        .child(typography::EMPHASIS.in_ink(Ink::Muted).label(tokens, title))
        .children(body)
        .into()
}

/// A card that takes the whole row, as tall as what it holds: a name, then
/// `body`.
pub fn section(tokens: &Tokens, title: &str, body: Vec<Element>) -> Element {
    rect()
        .tw(
            tokens,
            &[
                w_full(),
                flex_col(),
                gap(S2),
                p(S4),
                rounded(Radius::Card),
                bg(Paint::Card),
            ],
        )
        .child(typography::EMPHASIS.in_ink(Ink::Muted).label(tokens, title))
        .children(body)
        .into()
}

fn figure(tokens: &Tokens, value: impl Into<String>) -> Element {
    typography::TITLE.label(tokens, value).max_lines(1).into()
}

fn say(tokens: &Tokens, line: impl Into<String>) -> Element {
    typography::BODY
        .in_ink(Ink::Muted)
        .paragraph(tokens, line)
        .into()
}

fn note(tokens: &Tokens, line: impl Into<String>) -> Element {
    typography::CAPTION
        .in_ink(Ink::Muted)
        .paragraph(tokens, line)
        .into()
}

/// Cards two to a row; a last card alone keeps its half.
fn grid(tokens: &Tokens, cards: Vec<Element>) -> Element {
    let mut rows: Vec<Element> = Vec::new();
    let mut cards = cards.into_iter();
    while let Some(first) = cards.next() {
        let second = cards
            .next()
            .unwrap_or_else(|| rect().width(Size::flex(1.0)).into());
        rows.push(
            rect()
                .tw(tokens, &[w_full(), flex_row(), grow_children(), gap(S3)])
                .child(first)
                .child(second)
                .into(),
        );
    }
    rect()
        .tw(tokens, &[w_full(), flex_col(), gap(S3)])
        .children(rows)
        .into()
}

/// The small cards and the air quality: what the conditions are made of.
#[derive(Clone, PartialEq)]
pub struct Details {
    pub forecast: Forecast,
    pub units: Units,
    pub local: i64,
}

impl Details {
    fn uv(&self, tokens: &Tokens, hour: Option<&Hour>) -> Element {
        let uv = hour.and_then(|hour| hour.uv).unwrap_or(0.0);
        card(
            tokens,
            "UV index",
            vec![
                figure(tokens, format!("{}", uv.round() as i64)),
                typography::HEADING
                    .label(tokens, text::uv_category(uv))
                    .max_lines(1)
                    .into(),
                range_bar(0.0, 11.0)
                    .colours(sky::uv_stops())
                    .marker(uv.min(11.0))
                    .label(format!(
                        "UV index {}, {}",
                        uv.round() as i64,
                        text::uv_category(uv)
                    ))
                    .into(),
                note(tokens, text::uv_advice(uv)),
            ],
        )
    }

    fn wind(&self, tokens: &Tokens) -> Element {
        let current = &self.forecast.current;
        let speed = current.wind_speed.unwrap_or(0.0);
        let value = self.units.speed_value(speed);
        let shown = match self.units.speed {
            bingux_weather::Speed::MetresPerSecond => format!("{value:.1}"),
            _ => format!("{}", value.round() as i64),
        };
        let mut body: Vec<Element> = Vec::new();
        let dial = compass(current.wind_direction.unwrap_or(0.0))
            .centre(shown, self.units.speed_unit())
            .label(match current.wind_direction {
                Some(degrees) => format!(
                    "Wind from the {} at {}",
                    text::cardinal_long(degrees),
                    self.units.speed(speed)
                ),
                None => format!("Wind at {}", self.units.speed(speed)),
            });
        body.push(
            rect()
                .tw(tokens, &[w_full(), items(Align::Center)])
                .child(dial)
                .into(),
        );
        body.push(
            typography::CAPTION
                .in_ink(Ink::Muted)
                .label(
                    tokens,
                    match current.wind_gusts {
                        Some(gusts) if gusts > speed + 5.0 => {
                            format!("Gusts up to {}", self.units.speed(gusts))
                        }
                        _ => text::wind_force(speed).to_string(),
                    },
                )
                .max_lines(1)
                .into(),
        );
        card(tokens, "Wind", body)
    }

    fn sun(&self, tokens: &Tokens) -> Element {
        let day = self.forecast.day_at(self.local);
        let (rise, set) = day.map_or((None, None), |day| (day.sunrise, day.sunset));
        let clock = self.units.clock;
        let progress = match (rise, set) {
            (Some(rise), Some(set)) => astro::sun_progress(rise, set, self.local),
            _ => None,
        };
        let mut arc = day_arc(progress);
        if let (Some(rise), Some(set)) = (rise, set) {
            arc = arc.times(clock.time(rise), clock.time(set)).label(format!(
                "Sunrise {}, sunset {}",
                clock.time(rise),
                clock.time(set)
            ));
        }
        let headline = match (rise, set) {
            (Some(rise), Some(_)) if self.local < rise => format!("Sunrise {}", clock.time(rise)),
            (Some(_), Some(set)) if self.local < set => format!("Sunset {}", clock.time(set)),
            (Some(rise), Some(_)) => format!("Sunrise {} tomorrow", clock.time(rise)),
            _ => "No sunrise or sunset".to_string(),
        };
        let mut body = vec![figure(tokens, headline), arc.into()];
        if let (Some(rise), Some(set)) = (rise, set) {
            body.push(note(
                tokens,
                format!("Daylight {}", astro::day_length(rise, set)),
            ));
        }
        card(tokens, "Sun", body)
    }

    fn moon(&self, tokens: &Tokens) -> Element {
        let unix = unix_now();
        let moon = Moon::at(unix);
        let full = Moon::next_full(unix) + i64::from(self.forecast.utc_offset);
        let new = Moon::next_new(unix) + i64::from(self.forecast.utc_offset);
        let (word, at) = if moon.phase < 0.5 {
            ("Full moon", full)
        } else {
            ("New moon", new)
        };
        card(
            tokens,
            "Moon",
            vec![
                rect()
                    .tw(tokens, &[w_full(), items(Align::Center)])
                    .child(glyph::moon_view(moon.phase, MOON_SIZE))
                    .into(),
                typography::HEADING
                    .label(tokens, moon.name())
                    .max_lines(1)
                    .into(),
                note(
                    tokens,
                    format!(
                        "{}% lit. {word} on {}.",
                        (moon.illumination * 100.0).round() as i64,
                        day_month(at)
                    ),
                ),
            ],
        )
    }

    fn feels_like(&self, tokens: &Tokens) -> Element {
        let current = &self.forecast.current;
        let apparent = current.apparent;
        let line = match (apparent, current.temperature) {
            (Some(feels), Some(actual)) if feels < actual - 1.5 => "Wind is making it feel cooler.",
            (Some(feels), Some(actual)) if feels > actual + 1.5 => {
                "Humidity is making it feel warmer."
            }
            _ => "Similar to the actual temperature.",
        };
        card(
            tokens,
            "Feels like",
            vec![
                figure(tokens, format::temperature(&self.units, apparent)),
                say(tokens, line),
            ],
        )
    }

    fn humidity(&self, tokens: &Tokens, hour: Option<&Hour>) -> Element {
        let humidity = self.forecast.current.humidity;
        let dew = hour.and_then(|hour| hour.dew_point);
        let mut body = vec![figure(
            tokens,
            humidity.map_or_else(|| "–".to_string(), format::percent),
        )];
        if let Some(dew) = dew {
            body.push(say(
                tokens,
                format!(
                    "The dew point is {} right now.",
                    self.units.temperature(dew)
                ),
            ));
            body.push(note(tokens, text::dew_comfort(dew)));
        }
        card(tokens, "Humidity", body)
    }

    fn visibility(&self, tokens: &Tokens, hour: Option<&Hour>) -> Element {
        let metres = hour.and_then(|hour| hour.visibility);
        let mut body = vec![figure(
            tokens,
            metres.map_or_else(|| "–".to_string(), |metres| self.units.distance(metres)),
        )];
        if let Some(metres) = metres {
            body.push(say(tokens, text::visibility_note(metres)));
        }
        card(tokens, "Visibility", body)
    }

    fn pressure(&self, tokens: &Tokens) -> Element {
        let pressure = self.forecast.current.pressure;
        let units = self.units;
        let mut body = vec![figure(
            tokens,
            pressure.map_or_else(|| "–".to_string(), |hpa| units.pressure(hpa)),
        )];
        if let Some(trend) = text::pressure_trend(&self.forecast, self.local) {
            body.push(say(tokens, trend));
        }
        // The pressure round now: three hours back and a day ahead.
        let values: Vec<f64> = self
            .forecast
            .hourly
            .iter()
            .filter(|hour| {
                hour.time >= self.local - 3 * HOUR && hour.time <= self.local + 24 * HOUR
            })
            .filter_map(|hour| hour.pressure)
            .collect();
        if values.len() > 2 {
            body.push(
                sparkline(values)
                    .format(move |hpa| units.pressure(hpa))
                    .label("Pressure from three hours ago to a day ahead")
                    .into(),
            );
        }
        card(tokens, "Pressure", body)
    }

    /// The air quality: the index on its bands, then each pollutant.
    fn air(&self, tokens: &Tokens, air: &AirQuality) -> Option<Element> {
        let aqi = air.us_aqi?;
        let mut body: Vec<Element> = vec![
            hstack(S3)
                .child(figure(tokens, format!("{}", aqi.round() as i64)))
                .child(
                    typography::HEADING
                        .label(tokens, text::aqi_category(aqi))
                        .max_lines(1),
                )
                .into(),
            range_bar(0.0, 300.0)
                .colours(sky::aqi_stops())
                .marker(aqi.min(300.0))
                .label(format!(
                    "Air quality index {}, {}",
                    aqi.round() as i64,
                    text::aqi_category(aqi)
                ))
                .into(),
            say(tokens, text::aqi_advice(aqi)),
        ];
        let pollutants = [
            ("PM2.5", air.pm2_5),
            ("PM10", air.pm10),
            ("Ozone", air.ozone),
            ("Nitrogen dioxide", air.nitrogen_dioxide),
            ("Sulphur dioxide", air.sulphur_dioxide),
            ("Carbon monoxide", air.carbon_monoxide),
        ];
        let mut lines: Vec<Element> = Vec::new();
        for (name, value) in pollutants {
            if let Some(value) = value {
                lines.push(
                    hstack(S3)
                        .tw_layout(&[w_full(), grow_children()])
                        .child(
                            rect().width(Size::flex(1.0)).child(
                                typography::BODY
                                    .in_ink(Ink::Muted)
                                    .label(tokens, name)
                                    .max_lines(1),
                            ),
                        )
                        .child(
                            typography::BODY
                                .label(tokens, format!("{} µg/m³", value.round() as i64))
                                .max_lines(1),
                        )
                        .into(),
                );
            }
        }
        if !lines.is_empty() {
            body.push(
                rect()
                    .tw(tokens, &[w_full(), flex_col(), gap(S1), pt(S2)])
                    .children(lines)
                    .into(),
            );
        }
        Some(section(tokens, "Air quality", body))
    }
}

impl Component for Details {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let hour = self.forecast.hour_at(self.local);
        let small = vec![
            self.uv(&tokens, hour),
            self.wind(&tokens),
            self.sun(&tokens),
            self.moon(&tokens),
            self.feels_like(&tokens),
            self.humidity(&tokens, hour),
            self.visibility(&tokens, hour),
            self.pressure(&tokens),
        ];
        let air = self
            .forecast
            .air
            .as_ref()
            .and_then(|air| self.air(&tokens, air));
        let mut column = rect()
            .component("Details")
            .tw(&tokens, &[w_full(), flex_col(), gap(S3)]);
        column = column.child(grid(&tokens, small));
        column.maybe_child(air)
    }
}

/// The two charts of the next day.
#[derive(Clone, PartialEq)]
pub struct Trends {
    pub forecast: Forecast,
    pub units: Units,
    pub local: i64,
}

impl Component for Trends {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let units = self.units;
        let hours: Vec<Hour> = self
            .forecast
            .hours_from(self.local)
            .iter()
            .take(HOURS)
            .cloned()
            .collect();
        let Some(first) = hours.first().map(|hour| hour.time) else {
            return rect().into_element();
        };
        let clock = units.clock;
        let hour_name = move |x: f64| clock.hour(first + x.round() as i64 * HOUR);

        let points = |pick: fn(&Hour) -> Option<f64>| {
            hours
                .iter()
                .enumerate()
                .filter_map(|(index, hour)| {
                    Some((index as f64, units.temperature_value(pick(hour)?)))
                })
                .collect::<Vec<_>>()
        };
        let temperature = line_chart([
            bingux_ui::charts::Series::new("Temperature", points(|hour| hour.temperature))
                .colour(SeriesColour::Accent),
            bingux_ui::charts::Series::new("Feels like", points(|hour| hour.apparent))
                .colour(SeriesColour::Muted),
        ])
        .x_format(hour_name)
        .y_format(move |value| format!("{}°", value.round() as i64))
        .label("Temperature over the next 24 hours");

        let rain_points: Vec<(f64, f64)> = hours
            .iter()
            .enumerate()
            .map(|(index, hour)| {
                (
                    index as f64,
                    units.rain_value(hour.precipitation.unwrap_or(0.0)),
                )
            })
            .collect();
        let total: f64 = hours
            .iter()
            .map(|hour| hour.precipitation.unwrap_or(0.0))
            .sum();
        let rain_unit = units.rain_unit();
        let rain = bar_chart(
            [bingux_ui::charts::Series::new("Precipitation", rain_points)
                .colour(SeriesColour::Accent)],
        )
        .categories(hours.iter().map(|hour| clock.hour(hour.time)))
        .details(hours.iter().map(|hour| {
            format!(
                "{} to {}",
                clock.time(hour.time),
                clock.time(hour.time + HOUR)
            )
        }))
        .y_format(move |value| {
            if value >= 10.0 || rain_unit == "mm" && value.fract() == 0.0 {
                format!("{} {rain_unit}", value.round() as i64)
            } else {
                format!("{value:.2} {rain_unit}")
            }
        })
        .label("Precipitation over the next 24 hours");

        rect()
            .component("Trends")
            .tw(&tokens, &[w_full(), flex_col(), gap(S3)])
            .child(section(&tokens, "Temperature", vec![temperature.into()]))
            .child(section(
                &tokens,
                "Precipitation",
                vec![
                    say(
                        &tokens,
                        if total >= 0.1 {
                            format!("{} in the next 24 hours.", units.rain(total))
                        } else {
                            "None in the next 24 hours.".to_string()
                        },
                    ),
                    rain.into(),
                ],
            ))
            .into_element()
    }
}
