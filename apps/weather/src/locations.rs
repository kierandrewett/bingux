//! The list of places: where the computer is, and the cities that were added,
//! each with its temperature and conditions, in an order the person sets.
//!
//! Each row loads its own forecast (the same cache the page reads, so a place
//! is fetched once). Drag a row's grip, or press Alt+Up or Alt+Down on it, to
//! move it. Delete, or the row's menu, takes it off.

use std::time::Duration;

use bingux_ui::component::ComponentName;
use bingux_ui::prelude::tr;
use bingux_ui::prelude::*;
use bingux_ui::task::{Resource, use_polled_resource};
use bingux_weather::codes::{self, Sky};
use bingux_weather::store::HERE;
use bingux_weather::{Here, Place, Units};
use freya::prelude::*;

use crate::format;
use crate::glyph;
use crate::hub::Hub;
use crate::load::{self, ForecastKey};

const ICON: f32 = 24.0;
/// How often a row looks for a newer forecast: the cache decides whether that
/// is a request.
const POLL: Duration = Duration::from_secs(600);

/// Where the computer is, as far as the list knows.
#[derive(Clone, Debug, PartialEq)]
pub enum HereView {
    Found(Here),
    /// The lookup is running.
    Locating,
    /// The lookup failed; the text is why (`load::OFFLINE`, `load::DENIED`, ...).
    Failed(String),
    /// "My location" is turned off.
    Off,
}

impl HereView {
    /// What the source of a position is called to the person.
    pub fn source_text(here: &Here) -> String {
        if here.source == "portal" {
            tr!("weather-source-portal")
        } else {
            tr!("weather-source-network")
        }
    }
}

/// The list.
#[derive(Clone, PartialEq)]
pub struct Locations {
    pub here: HereView,
    pub saved: Vec<Place>,
    /// The key of the place on show.
    pub selected: Option<String>,
    pub units: Units,
    pub refresh: u64,
    pub hub: Hub,
    pub add: EventHandler<()>,
}

/// One place's row.
#[derive(Clone, PartialEq)]
struct PlaceRow {
    /// `HERE`, or the place's key.
    key: String,
    place: Place,
    here: bool,
    selected: bool,
    units: Units,
    refresh: u64,
    hub: Hub,
}

impl Component for PlaceRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fetched = use_polled_resource(
            ForecastKey {
                place: Some(self.place.clone()),
                refresh: self.refresh,
            },
            load::forecast,
            POLL,
            load::same,
        );
        let id = self.place.key();
        let loaded = fetched.with(|state| match state {
            Resource::Ready(fetched) | Resource::Refreshing(fetched) if fetched.key == id => {
                Some(fetched.loaded.clone())
            }
            _ => None,
        });

        let title = if self.here {
            tr!("weather-my-location")
        } else {
            self.place.name.clone()
        };
        // What the row says under its title, a line each and each cut on its
        // own: for My location the city, then the conditions; for a saved
        // place the conditions. The day's range goes under the temperature,
        // where it cannot be cut.
        let mut lines: Vec<String> = Vec::new();
        if self.here {
            lines.push(self.place.name.clone());
        }
        let mut trailing: Option<Element> = None;
        if let Some(loaded) = &loaded {
            let forecast = &loaded.forecast;
            let current = &forecast.current;
            let code = current.code.unwrap_or(3);
            lines.push(codes::describe(code).to_string());
            let temperature = typography::HEADING
                .label(
                    &tokens,
                    format::temperature(&self.units, current.temperature),
                )
                .max_lines(1);
            let range = forecast.day_at(current.time).map(|day| {
                typography::CAPTION
                    .label(
                        &tokens,
                        format!(
                            "{} / {}",
                            format::temperature(&self.units, day.high),
                            format::temperature(&self.units, day.low)
                        ),
                    )
                    .max_lines(1)
            });
            trailing = Some(
                hstack(S2)
                    .child(glyph::view(Sky::of(code), current.is_day, ICON))
                    .child(
                        rect()
                            .tw(&tokens, &[flex_col(), items(Align::End)])
                            .child(temperature)
                            .maybe_child(range),
                    )
                    .into(),
            );
        }

        let hub = self.hub;
        let key = self.key.clone();
        let mut lines = lines.into_iter();
        let mut line = row(title)
            .maybe_subtitle(lines.next())
            .selected(self.selected && !is_compact())
            .selection_fill_only()
            .on_press({
                let key = key.clone();
                move |_| hub.select(&key)
            });
        if let Some(second) = lines.next() {
            line = line.second_line(second);
        }
        if is_compact() {
            line = line.navigates();
        }
        if self.here {
            line = line.leading(Icon::LocationArrow);
        } else {
            line = line.context_menu(vec![menu_item(tr!("weather-remove"), {
                let key = key.clone();
                move |_| hub.remove(&key)
            })]);
        }
        if let Some(trailing) = trailing {
            line = line.trailing(trailing);
        }
        let delete_key = key;
        let removable = !self.here;
        rect()
            .width(Size::fill())
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if removable && matches!(event.key, Key::Named(NamedKey::Delete)) {
                    hub.remove(&delete_key);
                }
            })
            .child(line)
    }
}

/// What a lookup that failed says, in a row's subtitle.
fn why(code: &str) -> String {
    match code {
        load::DENIED => tr!("weather-why-denied"),
        load::OFFLINE => tr!("weather-no-connection"),
        _ => tr!("weather-why-unknown"),
    }
}

impl Component for Locations {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let hub = self.hub;
        let mut column = rect().tw(&tokens, &[w_full(), flex_col(), gap(S4), p(S3)]);

        match &self.here {
            HereView::Off => {}
            HereView::Found(here) => {
                column = column.child(Group::new(None::<String>).row(PlaceRow {
                    key: HERE.to_string(),
                    place: here.place.clone(),
                    here: true,
                    selected: self.selected.as_deref() == Some(HERE),
                    units: self.units,
                    refresh: self.refresh,
                    hub,
                }));
            }
            HereView::Locating => {
                column = column.child(
                    Group::new(None::<String>).row(
                        row(tr!("weather-my-location"))
                            .leading(Icon::LocationArrow)
                            .subtitle(tr!("weather-locating")),
                    ),
                );
            }
            HereView::Failed(code) => {
                column = column.child(
                    Group::new(None::<String>).row(
                        row(tr!("weather-my-location"))
                            .leading(Icon::LocationArrow)
                            .subtitle(why(code))
                            .badge(tr!("weather-try-again"))
                            .on_press(move |_| hub.locate_again()),
                    ),
                );
            }
        }

        if !self.saved.is_empty() {
            let rows: Vec<Element> = self
                .saved
                .iter()
                .map(|place| {
                    PlaceRow {
                        key: place.key(),
                        place: place.clone(),
                        here: false,
                        selected: self.selected.as_deref() == Some(place.key().as_str()),
                        units: self.units,
                        refresh: self.refresh,
                        hub,
                    }
                    .into()
                })
                .collect();
            let names: Vec<String> = self.saved.iter().map(|place| place.name.clone()).collect();
            let showing = self
                .saved
                .iter()
                .position(|place| self.selected.as_deref() == Some(place.key().as_str()))
                .filter(|_| !is_compact());
            column = column
                .child(section_label(tr!("weather-saved-places")))
                .child(
                    sortable(rows, move |(from, to): (usize, usize)| {
                        hub.reorder(from, to)
                    })
                    .labels(names)
                    .selected(showing),
                );
        }

        let add = self.add.clone();
        column = column.child(
            Group::new(None::<String>).row(
                row(tr!("weather-add-location"))
                    .leading(Icon::ListAdd)
                    .on_press(move |_| add.call(())),
            ),
        );

        rect()
            .component("Locations")
            .tw(&tokens, &[w_full(), h_full()])
            .child(ScrollView::new().child(column))
    }
}
