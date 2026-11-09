//! The ten-day forecast: a day to a line, with the day's low and high as a bar
//! on the range of the whole period, and today's bar marked with now.
//!
//! The list is one keyboard stop: Up and Down move through the days (Home and
//! End go to the ends), and the line under the list says what the chosen day
//! holds. The bars share their colours (cold to hot, the same degree the same
//! colour on every bar), so the week reads as one picture.

use bingux_ui::component::ComponentName;
use bingux_ui::controls::focus_stop;
use bingux_ui::prelude::*;
use bingux_weather::codes::{Condition, Sky};
use bingux_weather::{Day, Forecast, Units};
use freya::prelude::*;

use crate::format;
use crate::glyph;
use crate::sky;
use crate::words;

const ICON: f32 = 28.0;
/// The chance of rain below which a day says nothing about it.
const CHANCE_SHOWN: f64 = 20.0;

/// The ten-day forecast.
#[derive(Clone, PartialEq)]
pub struct Week {
    pub forecast: Forecast,
    pub units: Units,
    pub local: i64,
}

impl Week {
    fn detail(&self, day: &Day) -> String {
        let mut parts = vec![format::day_long(day.time, self.local)];
        if let Some(code) = day.code {
            parts.push(words::condition(Condition::of(code)));
        }
        if let Some(chance) = day.precip_chance.filter(|c| *c >= CHANCE_SHOWN) {
            parts.push(tr!("weather-week-chance", chance = format::percent(chance)));
        }
        if let Some(rain) = day.precipitation.filter(|rain| *rain >= 0.1) {
            parts.push(self.units.rain(rain));
        }
        if let Some(wind) = day.wind_max {
            parts.push(tr!("weather-week-wind", speed = self.units.speed(wind)));
        }
        if let Some(uv) = day.uv_max.filter(|uv| *uv >= 3.0) {
            parts.push(tr!("weather-week-uv", index = uv.round() as i64));
        }
        words::join_list(&parts)
    }
}

impl Component for Week {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut chosen = use_state(|| None::<usize>);
        let days: Vec<Day> = self.forecast.days_from(self.local).to_vec();
        let count = days.len();
        let units = self.units;
        let range = Forecast::range(&days).unwrap_or((0.0, 1.0));
        let stops = sky::temperature_stops();

        let on_key = move |event: Event<KeyboardEventData>| {
            if count == 0 {
                return;
            }
            let current = *chosen.peek();
            let next = match event.key {
                Key::Named(NamedKey::ArrowDown) => {
                    current.map_or(0, |index| (index + 1).min(count - 1))
                }
                Key::Named(NamedKey::ArrowUp) => current.map_or(0, |index| index.saturating_sub(1)),
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
        };

        let selected = chosen().filter(|index| *index < count);
        // On a phone the fixed columns leave the bar too little room at S3,
        // so the high temperature runs off the card.
        let gap_token = if is_compact() { S2 } else { S3 };
        let mut rows = rect().tw(&tokens, &[w_full(), flex_col()]);
        for (index, day) in days.iter().enumerate() {
            let sky = Sky::of(day.code.unwrap_or(3));
            let lit = selected == Some(index);
            let today = index == 0;
            let mut bar = range_bar(range.0, range.1)
                .colours(stops.clone())
                .label(tr!(
                    "weather-week-low-high",
                    low = format::temperature(&units, day.low),
                    high = format::temperature(&units, day.high)
                ));
            if let (Some(low), Some(high)) = (day.low, day.high) {
                bar = bar.span(low, high);
            }
            if today && let Some(now) = self.forecast.current.temperature {
                bar = bar.marker(now);
            }
            let chance = day
                .precip_chance
                .filter(|chance| *chance >= CHANCE_SHOWN && sky.is_wet())
                .map(format::percent)
                .unwrap_or_else(|| " ".to_string());
            rows = rows.child(
                rect()
                    .key(index)
                    .tw(
                        &tokens,
                        &[
                            w_full(),
                            h(Dim::Row),
                            flex_row(),
                            items(Align::Center),
                            gap(gap_token),
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
                    .child(
                        rect().tw_layout(&[w(Dim::Box11)]).child(
                            if today {
                                typography::EMPHASIS
                            } else {
                                typography::BODY
                            }
                            .label(&tokens, format::day_short(day.time, self.local))
                            .max_lines(1),
                        ),
                    )
                    .child(glyph::view(sky, true, ICON))
                    .child(
                        rect().tw_layout(&[w(Dim::Box9)]).child(
                            typography::CAPTION
                                .in_ink(Ink::Accent)
                                .label(&tokens, chance)
                                .max_lines(1),
                        ),
                    )
                    .child(
                        rect().tw_layout(&[w(Dim::Box9)]).child(
                            typography::BODY
                                .in_ink(Ink::Muted)
                                .label(&tokens, format::temperature(&units, day.low))
                                .max_lines(1)
                                .text_align(TextAlign::Right),
                        ),
                    )
                    .child(
                        rect()
                            .tw(&tokens, &[w_flex()])
                            .min_width(Size::px(0.0))
                            .padding((0.0, Dim::Box9.px() + gap_token.px(), 0.0, 0.0))
                            .child(bar),
                    )
                    .child(
                        rect()
                            .position(Position::new_absolute().top(0.0).right(0.0))
                            .width(Size::px(Dim::Box9.px()))
                            .height(Size::fill())
                            .tw(&tokens, &[flex_row(), items(Align::Center)])
                            .child(
                                typography::BODY
                                    .label(&tokens, format::temperature(&units, day.high))
                                    .max_lines(1)
                                    .text_align(TextAlign::Right),
                            ),
                    ),
            );
        }

        let summary = {
            let highs: Vec<f64> = days.iter().filter_map(|day| day.high).collect();
            let lows: Vec<f64> = days.iter().filter_map(|day| day.low).collect();
            match (
                highs.iter().copied().reduce(f64::min),
                highs.iter().copied().reduce(f64::max),
                lows.iter().copied().reduce(f64::min),
                lows.iter().copied().reduce(f64::max),
            ) {
                (Some(high_low), Some(high_high), Some(low_low), Some(low_high)) => tr!(
                    "weather-week-highs-lows",
                    high_low = units.temperature(high_low),
                    high_high = units.temperature(high_high),
                    low_low = units.temperature(low_low),
                    low_high = units.temperature(low_high)
                ),
                _ => String::new(),
            }
        };
        let line = selected.map_or(summary, |index| self.detail(&days[index]));
        let stop = focus_stop(
            rect().width(Size::fill()).child(rows),
            &tr!("weather-week-title"),
            AccessibilityRole::List,
            count > 0,
            Radius::Control,
            |_: ()| {},
        );
        rect()
            .component("Week")
            .tw(&tokens, &[w_full(), flex_col(), gap(S2)])
            .on_key_down(on_key)
            .child(stop)
            .child(
                rect().tw_layout(&[px(S4)]).child(
                    typography::CAPTION
                        .label(&tokens, line)
                        .max_lines(2)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            )
    }
}
