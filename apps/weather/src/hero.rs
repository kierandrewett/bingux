//! The hero: the place, the temperature and the conditions on a sky that
//! follows them. A clear day is blue, a clear night is dark with stars, rain
//! is grey with streaks, and the first and last hour of the sun are warm.
//!
//! The sky is a gradient, a still picture over it (the sun's or the moon's
//! haze, stars, rain, snow) and, over that, soft clouds in two layers that drift
//! very slowly in opposite directions. The clouds are blurred, faint and keep
//! to the edges, so they are weather and never a shape behind the text. Under
//! reduced motion they do not drift at all, and a change of place fades from
//! one sky to the next with the window's own motion.

use bingux_ui::component::ComponentName;
use bingux_ui::motion::use_ambient_cycle;
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
    /// How "My location" was found, when this is it.
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

/// How long a cloud layer takes to drift across and back: slow enough that no
/// one sees it move, only that the sky is not where it was.
const DRIFT_MS: u64 = 150_000;

/// The picture over the gradient. It is a component of its own so that the
/// clock of the drift draws this and nothing else.
#[derive(Clone, PartialEq)]
struct Atmosphere {
    sky: Sky,
    light: sky::Light,
}

impl Atmosphere {
    /// One SVG, `width` wide, placed `left` of the box's own left edge.
    fn layer(svg: String, width: Size, left: f32) -> SvgViewer {
        SvgViewer::new((svg.clone(), Bytes::from(svg)))
            .position(Position::new_absolute().top(0.0).left(left))
            .width(width)
            .height(Size::fill())
    }
}

impl Component for Atmosphere {
    fn render(&self) -> impl IntoElement {
        let phase = use_ambient_cycle(DRIFT_MS);
        let mut hero = use_state(|| 0.0_f32);
        // 0 to 1 and back: the far layer goes one way along its travel while
        // the near layer goes the other.
        let swing = 0.5 - 0.5 * (std::f32::consts::TAU * phase).cos();
        let width = *hero.read();
        // A layer is wider than the hero by the length of its travel, and is
        // moved left by how far along it is. (Until the hero is measured it is
        // the hero's own width.)
        let layer = |depth: sky::Depth, along: f32| {
            sky::clouds(self.sky, self.light, depth).map(|svg| {
                let (left, across) = if width > 0.0 {
                    (
                        -width * sky::DRIFT * along,
                        Size::px(width * (1.0 + sky::DRIFT)),
                    )
                } else {
                    (0.0, Size::fill())
                };
                Self::layer(svg, across, left).key(depth as u8)
            })
        };
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::fill())
            .height(Size::fill())
            .on_sized(move |event: Event<SizedEventData>| {
                let width = event.area.width();
                if (width - *hero.peek()).abs() > 0.5 {
                    *hero.write() = width;
                }
            })
            .child(Self::layer(
                sky::still(self.sky, self.light),
                Size::fill(),
                0.0,
            ))
            .maybe_child(layer(sky::Depth::Far, swing))
            .maybe_child(layer(sky::Depth::Near, 1.0 - swing))
    }
}

/// The sky alone, filling the box it is in: the gradient, and over it the
/// atmosphere. The hero stands its text on it, and the sheet of skies in the
/// snapshot mode shows it for every condition.
#[derive(Clone, PartialEq)]
pub struct Backdrop {
    pub sky: Sky,
    pub light: sky::Light,
}

impl Component for Backdrop {
    fn render(&self) -> impl IntoElement {
        let gradient = sky::gradient(self.sky, self.light);
        let fill = gradient_fill(
            rect()
                .position(Position::new_absolute().top(0.0).left(0.0))
                .width(Size::fill())
                .height(Size::fill()),
            {
                // Top to bottom is angle 0 in this engine.
                LinearGradient::new()
                    .angle(0.0)
                    .stop(GradientStop::new(gradient.top, 0.0))
                    .stop(GradientStop::new(gradient.bottom, 100.0))
            },
        );
        rect()
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::fill())
            .height(Size::fill())
            .child(fill)
            .child(Atmosphere {
                sky: self.sky,
                light: self.light,
            })
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
            Some(_) => tr!("weather-hero-here"),
            None => blank(),
        };
        let range = match day {
            Some(day) => tr!(
                "weather-hero-range",
                high = format::temperature(&self.units, day.high),
                low = format::temperature(&self.units, day.low)
            ),
            None => blank(),
        };
        let feels = match current.apparent {
            Some(apparent) => tr!(
                "weather-hero-feels",
                temperature = self.units.temperature(apparent)
            ),
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
            .child(Backdrop { sky, light })
            // Freya paints by depth, and a tie goes either way: the text sits
            // deeper than the sky and its picture, so it is always over them.
            .child(rect().child(rect().child(column)))
    }
}
