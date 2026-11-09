//! The hero: the place, the temperature and the conditions on a sky that
//! follows them. A clear day is blue, a clear night is dark with stars, rain
//! is grey with streaks, and the first and last hour of the sun are warm.
//!
//! The sky is a gradient and a picture over it. Both stand still, so a person
//! who asked for reduced motion loses nothing, and a change of place fades
//! from one sky to the next with the window's own motion.

use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use bingux_weather::codes::{self, Sky};
use bingux_weather::{Forecast, Units, text};
use bytes::Bytes;
use freya::prelude::*;

use crate::format::{self, light_of, rain_outlook};
use crate::glyph;
use crate::sky;

/// How tall the temperature is set, as a multiple of the hero style's size.
const TEMPERATURE_SCALE: f32 = 2.5;
const TEMPERATURE_SCALE_PHONE: f32 = 2.0;
/// The condition's picture beside its name.
const CONDITION_ICON: f32 = 32.0;

/// What the hero shows.
#[derive(Clone, PartialEq)]
pub struct Hero {
    pub name: String,
    /// How "My Location" was found, when this is it.
    pub here: Option<String>,
    pub forecast: Forecast,
    pub units: Units,
    /// The wall-clock time at the place now.
    pub local: i64,
}

impl Hero {
    /// What a screen reader says for the whole card.
    fn spoken(&self) -> String {
        let place = bingux_weather::Place::new(self.name.clone(), 0.0, 0.0);
        text::conditions(&place, &self.forecast, &self.units)
    }
}

impl Component for Hero {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let forecast = &self.forecast;
        let current = &forecast.current;
        let code = current.code.unwrap_or(3);
        let sky = Sky::of(code);
        let light = light_of(forecast, self.local, current.is_day);
        let gradient = sky::gradient(sky, light);

        let backdrop = gradient_fill(rect().width(Size::fill()).height(Size::fill()), {
            // Top to bottom is angle 0 in this engine.
            LinearGradient::new()
                .angle(0.0)
                .stop(GradientStop::new(gradient.top, 0.0))
                .stop(GradientStop::new(gradient.bottom, 100.0))
        });
        let art = sky::art(sky, light);
        let art = SvgViewer::new((art.clone(), Bytes::from(art)))
            .width(Size::fill())
            .height(Size::fill())
            .into_element();

        let day = forecast.day_at(self.local);
        let scale = if compact {
            TEMPERATURE_SCALE_PHONE
        } else {
            TEMPERATURE_SCALE
        };
        let on_sky = |style: typography::TextStyle| style.in_ink(Ink::OnFloating);

        // Every line is always there, in the same place, each with its own key:
        // a line that comes and goes with the data would change the number of
        // children between two renders of the card.
        let blank = || " ".to_string();
        let caption = match &self.here {
            Some(_) => "My Location".to_string(),
            None => blank(),
        };
        let range = match day {
            Some(day) => format!(
                "High {}  Low {}",
                format::temperature(&self.units, day.high),
                format::temperature(&self.units, day.low)
            ),
            None => blank(),
        };
        let feels = match current.apparent {
            Some(apparent) => format!("Feels like {}", self.units.temperature(apparent)),
            None => blank(),
        };
        let outlook = rain_outlook(forecast, self.local).unwrap_or_else(blank);
        let column = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_col(),
                    items(Align::Center),
                    gap(S1),
                    py(S8),
                    px(S4),
                ],
            )
            .child(
                on_sky(typography::CAPTION)
                    .label(&tokens, caption)
                    .max_lines(1)
                    .key("caption"),
            )
            .child(
                on_sky(typography::TITLE)
                    .label(&tokens, self.name.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .key("name"),
            )
            .child(
                on_sky(typography::HERO)
                    .label_scaled(
                        &tokens,
                        format::temperature(&self.units, current.temperature),
                        scale,
                    )
                    .max_lines(1)
                    .key("temperature"),
            )
            .child(
                hstack(S2)
                    .key("condition")
                    .child(glyph::view(sky, current.is_day, CONDITION_ICON))
                    .child(on_sky(typography::HEADING).label(&tokens, codes::describe(code))),
            )
            .child(
                on_sky(typography::BODY)
                    .label(&tokens, range)
                    .max_lines(1)
                    .key("range"),
            )
            .child(
                on_sky(typography::CAPTION)
                    .label(&tokens, feels)
                    .max_lines(1)
                    .key("feels"),
            )
            .child(
                on_sky(typography::CAPTION)
                    .label(&tokens, outlook)
                    .max_lines(1)
                    .key("outlook"),
            );

        rect()
            .component("Hero")
            .tw(&tokens, &[w_full(), rounded(Radius::Card), clip()])
            .a11y_role(AccessibilityRole::GenericContainer)
            .a11y_alt(self.spoken())
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(backdrop),
            )
            .child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(art),
            )
            // Freya paints by depth, and a tie goes either way: the text sits
            // deeper than the sky and its picture, so it is always over them.
            .child(rect().child(rect().child(column)))
    }
}
