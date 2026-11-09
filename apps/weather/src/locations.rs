//! The list of places: where the computer is, and the cities that were added,
//! each with its temperature and conditions, in an order the person sets.
//!
//! Each row loads its own forecast (the same cache the page reads, so a place
//! is fetched once). Drag a row's grip, or press Alt+Up or Alt+Down on it, to
//! move it. Delete, or the row's menu, takes it off.

use std::time::Duration;

use bingux_ui::component::ComponentName;
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
    /// "My Location" is turned off.
    Off,
}

impl HereView {
    /// What the source of a position is called to the person.
    pub fn source_text(here: &Here) -> &'static str {
        if here.source == "portal" {
            "From the location service"
        } else {
            "Approximate, from the network address"
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
            "My Location".to_string()
        } else {
            self.place.name.clone()
        };
        let mut subtitle: Vec<String> = Vec::new();
        if self.here {
            subtitle.push(self.place.name.clone());
        }
        let mut trailing: Option<Element> = None;
        if let Some(loaded) = &loaded {
            let forecast = &loaded.forecast;
            let current = &forecast.current;
            let code = current.code.unwrap_or(3);
            if let Some(day) = forecast.day_at(current.time) {
                subtitle.push(format!(
                    "{}, {} / {}",
                    codes::describe(code),
                    format::temperature(&self.units, day.high),
                    format::temperature(&self.units, day.low)
                ));
            } else {
                subtitle.push(codes::describe(code).to_string());
            }
            trailing = Some(
                hstack(S2)
                    .child(glyph::view(Sky::of(code), current.is_day, ICON))
                    .child(
                        typography::HEADING
                            .label(
                                &tokens,
                                format::temperature(&self.units, current.temperature),
                            )
                            .max_lines(1),
                    )
                    .into(),
            );
        }

        let hub = self.hub;
        let key = self.key.clone();
        let mut line = row(title)
            .subtitle(subtitle.join(" · "))
            .selected(self.selected && !is_compact())
            .on_press({
                let key = key.clone();
                move |_| hub.select(&key)
            });
        if is_compact() {
            line = line.navigates();
        }
        if self.here {
            line = line.leading(Icon::FindLocation);
        } else {
            line = line.context_menu(vec![menu_item("Remove", {
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
fn why(code: &str) -> &'static str {
    match code {
        load::DENIED => "Location is turned off for this app",
        load::OFFLINE => "No connection",
        _ => "Could not find your location",
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
                column = column.child(Group::new(Some("My location".to_string())).row(PlaceRow {
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
                    Group::new(Some("My location".to_string())).row(
                        row("My Location")
                            .leading(Icon::FindLocation)
                            .subtitle("Finding your location…"),
                    ),
                );
            }
            HereView::Failed(code) => {
                column = column.child(
                    Group::new(Some("My location".to_string())).row(
                        row("My Location")
                            .leading(Icon::FindLocation)
                            .subtitle(why(code))
                            .badge("Try again")
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
            column = column.child(section_label("Saved places")).child(
                sortable(rows, move |(from, to): (usize, usize)| {
                    hub.reorder(from, to)
                })
                .labels(names),
            );
        }

        let add = self.add.clone();
        column = column.child(
            Group::new(None::<String>).row(
                row("Add location")
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
