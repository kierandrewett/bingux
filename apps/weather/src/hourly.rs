//! The hourly strip: the next day, an hour to a tile, with sunrise and sunset
//! where they fall.
//!
//! The strip is one keyboard stop. Left and Right move through the hours (and
//! the sun's two moments), Home and End go to the ends, and the line under the
//! strip says what the chosen hour holds. The pointer chooses an hour by
//! resting on it, a finger by touching it.

use bingux_ui::component::ComponentName;
use bingux_ui::controls::focus_stop;
use bingux_ui::prelude::*;
use bingux_weather::codes::{Condition, Sky};
use bingux_weather::time::HOUR;
use bingux_weather::{Forecast, Hour, Units, text};
use freya::prelude::*;

use crate::format;
use crate::glyph;
use crate::words;

/// How many hours the strip shows, the current one included.
pub const HOURS: usize = 25;
/// A tile's width, and its picture.
const TILE: f32 = 64.0;
const TILE_ICON: f32 = 28.0;
/// The chance of rain below which a tile says nothing about it.
const CHANCE_SHOWN: f64 = 20.0;

/// One place in the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Tile {
    Hour(usize),
    Sun { rise: bool, at: i64 },
}

/// The tiles of the strip, in time order: an hour each, and the sun's rising
/// and setting after the hour they fall in.
fn tiles(forecast: &Forecast, hours: &[Hour]) -> Vec<Tile> {
    let Some(last) = hours.last() else {
        return Vec::new();
    };
    let end = last.time + HOUR;
    let mut tiles: Vec<(i64, u8, Tile)> = hours
        .iter()
        .enumerate()
        .map(|(index, hour)| (hour.time, 0, Tile::Hour(index)))
        .collect();
    for day in &forecast.daily {
        for (at, rise) in [(day.sunrise, true), (day.sunset, false)] {
            if let Some(at) = at.filter(|at| *at > hours[0].time && *at < end) {
                tiles.push((at, 1, Tile::Sun { rise, at }));
            }
        }
    }
    tiles.sort_by_key(|(time, order, _)| (*time, *order));
    tiles.into_iter().map(|(_, _, tile)| tile).collect()
}

/// The hourly forecast.
#[derive(Clone, PartialEq)]
pub struct HourStrip {
    pub forecast: Forecast,
    pub units: Units,
    pub local: i64,
}

impl HourStrip {
    /// What the chosen tile says, in one line.
    fn detail(&self, hours: &[Hour], tile: Tile) -> String {
        let clock = self.units.clock;
        match tile {
            Tile::Sun { rise, at } => {
                let time = clock.time(at);
                if rise {
                    tr!("weather-hourly-sunrise", time = time)
                } else {
                    tr!("weather-hourly-sunset", time = time)
                }
            }
            Tile::Hour(index) => {
                let hour = &hours[index];
                let mut parts = vec![clock.time(hour.time)];
                if let Some(code) = hour.code {
                    parts.push(words::condition(Condition::of(code)));
                }
                if let Some(temperature) = hour.temperature {
                    parts.push(self.units.temperature(temperature));
                }
                if let Some(chance) = hour.precip_chance.filter(|c| *c >= CHANCE_SHOWN) {
                    parts.push(tr!(
                        "weather-hourly-chance",
                        chance = format::percent(chance)
                    ));
                }
                if let Some(speed) = hour.wind_speed {
                    parts.push(match hour.wind_direction {
                        Some(degrees) => tr!(
                            "weather-hourly-wind-from",
                            speed = self.units.speed(speed),
                            point = words::point_short(text::point(degrees))
                        ),
                        None => tr!("weather-hourly-wind", speed = self.units.speed(speed)),
                    });
                }
                words::join_list(&parts)
            }
        }
    }

    /// The line under the strip when no tile is chosen: the next day's range.
    fn summary(&self, hours: &[Hour]) -> String {
        let temperatures: Vec<f64> = hours.iter().filter_map(|hour| hour.temperature).collect();
        let low = temperatures.iter().copied().reduce(f64::min);
        let high = temperatures.iter().copied().reduce(f64::max);
        match (low, high) {
            (Some(low), Some(high)) => tr!(
                "weather-hourly-range",
                low = self.units.temperature(low),
                high = self.units.temperature(high)
            ),
            _ => String::new(),
        }
    }
}

impl Component for HourStrip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut chosen = use_state(|| None::<usize>);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut view = use_state(Area::default);

        let hours: Vec<Hour> = self
            .forecast
            .hours_from(self.local)
            .iter()
            .take(HOURS)
            .cloned()
            .collect();
        let tiles = tiles(&self.forecast, &hours);
        let count = tiles.len();
        let units = self.units;

        // Bring a tile into view: the strip scrolls only as far as it must.
        let mut reveal = move |index: usize| {
            let left = index as f32 * TILE;
            let (offset, _): (i32, i32) = scroll.into();
            let seen_from = -offset as f32;
            let width = view.peek().width();
            if left < seen_from {
                scroll.scroll_to_x(-(left as i32));
            } else if left + TILE > seen_from + width {
                scroll.scroll_to_x(-((left + TILE - width).ceil() as i32));
            }
        };
        let on_key = move |event: Event<KeyboardEventData>| {
            if count == 0 {
                return;
            }
            let current = *chosen.peek();
            let next = match event.key {
                Key::Named(NamedKey::ArrowRight) => {
                    current.map_or(0, |index| (index + 1).min(count - 1))
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    current.map_or(0, |index| index.saturating_sub(1))
                }
                Key::Named(NamedKey::Home) => 0,
                Key::Named(NamedKey::End) => count - 1,
                Key::Named(NamedKey::Escape) if current.is_some() => {
                    event.stop_propagation();
                    chosen.set(None);
                    return;
                }
                _ => return,
            };
            event.stop_propagation();
            chosen.set_if_modified(Some(next));
            reveal(next);
        };

        let selected = chosen().filter(|index| *index < count);
        let line = match selected {
            Some(index) => self.detail(&hours, tiles[index]),
            None => self.summary(&hours),
        };

        let mut row = rect().tw(&tokens, &[flex_row()]);
        for (index, tile) in tiles.iter().enumerate() {
            let lit = selected == Some(index);
            let (top, picture, middle, bottom): (String, Element, Option<String>, String) =
                match *tile {
                    Tile::Hour(position) => {
                        let hour = &hours[position];
                        let sky = Sky::of(hour.code.unwrap_or(3));
                        let chance = hour
                            .precip_chance
                            .filter(|chance| *chance >= CHANCE_SHOWN && sky.is_wet())
                            .map(format::percent);
                        let label = if index == 0 {
                            tr!("weather-hourly-now")
                        } else {
                            units.clock.hour(hour.time)
                        };
                        (
                            label,
                            glyph::view(sky, hour.is_day, TILE_ICON),
                            chance,
                            format::temperature(&units, hour.temperature),
                        )
                    }
                    Tile::Sun { rise, at } => (
                        units.clock.time(at),
                        glyph::view(Sky::Clear, true, TILE_ICON),
                        None,
                        if rise {
                            tr!("weather-hourly-sunrise-short")
                        } else {
                            tr!("weather-hourly-sunset-short")
                        },
                    ),
                };
            let top_style = if index == 0 {
                typography::EMPHASIS
            } else {
                typography::CAPTION
            };
            row = row.child(
                rect()
                    .key(index)
                    .width(Size::px(TILE))
                    .tw(
                        &tokens,
                        &[
                            flex_col(),
                            items(Align::Center),
                            gap(S1),
                            py(S2),
                            rounded(Radius::Control),
                            bg(if lit {
                                Paint::Selection
                            } else {
                                Paint::Transparent
                            }),
                        ],
                    )
                    .on_pointer_enter(move |_| {
                        chosen.set_if_modified(Some(index));
                    })
                    .on_pointer_down(move |_| {
                        chosen.set_if_modified(Some(index));
                    })
                    .child(top_style.label(&tokens, top).max_lines(1))
                    .child(picture)
                    .child(
                        typography::CAPTION
                            .in_ink(Ink::Accent)
                            // A blank line keeps the temperatures level.
                            .label(&tokens, middle.unwrap_or_else(|| " ".to_string()))
                            .max_lines(1),
                    )
                    .child(
                        typography::BODY
                            .in_ink(if matches!(tile, Tile::Sun { .. }) {
                                Ink::Muted
                            } else {
                                Ink::Text
                            })
                            .label(&tokens, bottom)
                            .max_lines(1),
                    ),
            );
        }

        let strip = ScrollView::new_controlled(scroll)
            // ui-lint: allow(a scroll view's axis, not a box's direction)
            .direction(Direction::Horizontal)
            .scroll_with_arrows(false)
            .width(Size::fill())
            .height(Size::auto())
            .on_sized(move |event: Event<SizedEventData>| view.set_if_modified(event.area))
            .child(row);
        let stop = focus_stop(
            rect().width(Size::fill()).child(strip),
            &tr!("weather-hourly-title"),
            AccessibilityRole::List,
            count > 0,
            Radius::Control,
            |_: ()| {},
        );
        rect()
            .component("HourStrip")
            .tw(&tokens, &[w_full(), flex_col(), gap(S2)])
            .on_key_down(on_key)
            .child(stop)
            .child(
                typography::CAPTION
                    .label(&tokens, line)
                    .max_lines(2)
                    .text_overflow(TextOverflow::Ellipsis),
            )
    }
}
