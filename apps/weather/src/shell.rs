//! The window: a header bar over the list of places and the page of the place
//! on show. On a wide window they sit side by side; on a phone one fills the
//! window, the page by default and the list from its Back button.
//!
//! Zero setup is a chain that starts here. The settings are read, the position
//! is looked for (the location service, then the network address) and the
//! forecast of that place is loaded; the page shows the last forecast it has
//! while the new one comes, and a progress line while it has nothing.

use std::time::Duration;

use bingux_ui::component::ComponentName;
use bingux_ui::location::Source;
use bingux_ui::prelude::*;
use bingux_ui::task::{Resource, use_polled_resource, use_resource};
use bingux_weather::store::HERE;
use bingux_weather::{Here, Place, Settings, Speed, Temperature, Units, text};
use freya::prelude::*;

use crate::hub::Hub;
use crate::load::{self, Boot, ForecastKey, LocateKey};
use crate::locations::{HereView, Locations};
use crate::page::{Load, Page};
use crate::search;

/// How often the page looks for a newer forecast; the cache decides whether
/// that is a request.
const POLL: Duration = Duration::from_secs(600);

/// What the page is about.
#[derive(Clone, PartialEq)]
enum Current {
    Place {
        key: String,
        place: Place,
        here: Option<Here>,
    },
    /// The position is being looked for and nothing else is chosen.
    Locating,
    /// There is no place at all.
    Nothing,
}

/// The place the settings and the position say is on show.
fn current(settings: &Settings, here: &HereView) -> Current {
    let found = match here {
        HereView::Found(here) if settings.use_location => Some(here),
        _ => None,
    };
    let saved = |key: &str| settings.saved.iter().find(|place| place.key() == key);
    let here_current = |here: &Here| Current::Place {
        key: HERE.to_string(),
        place: here.place.clone(),
        here: Some(here.clone()),
    };
    let saved_current = |place: &Place| Current::Place {
        key: place.key(),
        place: place.clone(),
        here: None,
    };
    let wants_here = match settings.selected.as_deref() {
        Some(HERE) => true,
        Some(key) => saved(key).is_none() && settings.use_location,
        None => settings.use_location,
    };
    if wants_here {
        if let Some(here) = found {
            return here_current(here);
        }
        if matches!(here, HereView::Locating) && settings.use_location && settings.saved.is_empty()
        {
            return Current::Locating;
        }
    } else if let Some(place) = settings.selected.as_deref().and_then(saved) {
        return saved_current(place);
    }
    settings
        .saved
        .first()
        .map(saved_current)
        .or_else(|| found.map(here_current))
        .unwrap_or(Current::Nothing)
}

/// The window, once the settings have been read.
#[derive(Clone, PartialEq)]
pub struct Shell {
    pub boot: Boot,
}

impl Component for Shell {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let boot = &self.boot;
        let hub = Hub {
            settings: use_state({
                let settings = boot.settings.clone();
                move || settings
            }),
            refresh: use_state(|| 0),
            attempt: use_state(|| 0),
            list: use_state(|| false),
        };
        let compact = is_compact();
        let settings: Settings = (*hub.settings.read()).clone();
        let units = settings.units();

        // Where the computer is. The last place found is shown at once, and a
        // new lookup replaces it when it lands. The network address is used
        // only with the person's agreement, so a place kept from an earlier
        // network lookup is not shown without it.
        let network = settings.network_lookup == Some(true);
        let located = use_resource(
            LocateKey {
                on: settings.use_location,
                network,
                attempt: *hub.attempt.read(),
            },
            load::locate,
        );
        let allowed = |here: &Here| network || here.source != Source::Network.id();
        let cached = boot.here.clone().filter(allowed);
        let here = if !settings.use_location {
            HereView::Off
        } else {
            located.with(|state| match state {
                Resource::Ready(found) | Resource::Refreshing(found) if allowed(found) => {
                    HereView::Found(found.clone())
                }
                Resource::Ready(_) | Resource::Refreshing(_) | Resource::Loading => {
                    cached.clone().map_or(HereView::Locating, HereView::Found)
                }
                Resource::Error(code) => cached.clone().map_or_else(
                    || {
                        // Without the network lookup a failed portal has
                        // nothing to retry: the card asks instead.
                        if !network && code != load::DENIED {
                            HereView::Off
                        } else {
                            HereView::Failed(code.clone())
                        }
                    },
                    HereView::Found,
                ),
            })
        };

        let now_showing = current(&settings, &here);
        // Asked once, after the desktop's location service has not given a
        // position: the network lookup sends this computer's address out.
        let ask_network = settings.network_lookup.is_none()
            && settings.use_location
            && matches!(now_showing, Current::Nothing);
        let place = match &now_showing {
            Current::Place { place, .. } => Some(place.clone()),
            _ => None,
        };
        let refresh = *hub.refresh.read();
        let fetched = use_polled_resource(
            ForecastKey {
                place: place.clone(),
                refresh,
            },
            load::forecast,
            POLL,
            load::same,
        );
        let id = place.as_ref().map(Place::key);
        let load = match &now_showing {
            Current::Locating => Load::Locating,
            Current::Nothing => Load::Loading,
            Current::Place { .. } => fetched.with(|state| match state {
                Resource::Ready(fetched) | Resource::Refreshing(fetched)
                    if Some(&fetched.key) == id.as_ref() =>
                {
                    Load::Ready(fetched.loaded.clone())
                }
                Resource::Error(error) => {
                    let (failure, key) = load::failure_of(error);
                    if Some(key) == id.as_deref() {
                        Load::Failed(failure)
                    } else {
                        Load::Loading
                    }
                }
                _ => Load::Loading,
            }),
        };
        let busy = fetched
            .with(|state| matches!(state, Resource::Loading | Resource::Refreshing(_)))
            && place.is_some();

        let add: EventHandler<()> = (move |_: ()| search::open(overlays, hub)).into();
        let retry: EventHandler<()> = (move |_: ()| hub.refresh_all()).into();
        let conditions: Option<String> = match (&load, &place) {
            (Load::Ready(loaded), Some(place)) => {
                Some(text::conditions(place, &loaded.forecast, &units))
            }
            _ => None,
        };
        let copy = {
            let conditions = conditions.clone();
            move || {
                if let Some(text) = conditions.clone() {
                    match freya::clipboard::Clipboard::set(text) {
                        Ok(()) => overlays.toast("Conditions copied"),
                        Err(_) => overlays.toast("Could not copy the conditions"),
                    }
                }
            }
        };

        // The places in the order the list shows them, for stepping through.
        let mut order: Vec<String> = Vec::new();
        if matches!(here, HereView::Found(_)) {
            order.push(HERE.to_string());
        }
        order.extend(settings.saved.iter().map(Place::key));
        let selected_key = match &now_showing {
            Current::Place { key, .. } => Some(key.clone()),
            _ => None,
        };
        let step = {
            let order = order.clone();
            let selected_key = selected_key.clone();
            move |by: isize| {
                if order.is_empty() {
                    return;
                }
                let at = selected_key
                    .as_ref()
                    .and_then(|key| order.iter().position(|other| other == key))
                    .unwrap_or(0) as isize;
                let next = (at + by).rem_euclid(order.len() as isize) as usize;
                hub.select(&order[next]);
            }
        };

        // The window's commands. The menu, the palette and the shortcuts sheet
        // list them from here, and the router runs their keys. The hours, the
        // days and Delete are keys a view handles itself: listed for the sheet.
        let copy_enabled = conditions.is_some();
        use_window_commands(vec![
            Command::new("weather.refresh", "Refresh", move |_| hub.refresh_all())
                .shortcut("Ctrl+R")
                .shortcut("F5")
                .group(CommandGroup::File),
            Command::new("weather.add-location", "Add location…", move |_| {
                search::open(overlays, hub)
            })
            .shortcut("Ctrl+N")
            .group(CommandGroup::File),
            Command::new("weather.copy-conditions", "Copy conditions", move |_| {
                copy()
            })
            .shortcut("Ctrl+Shift+C")
            .enabled(copy_enabled)
            .group(CommandGroup::Edit),
            Command::new("weather.next-place", "Next place", {
                let step = step.clone();
                move |_| step(1)
            })
            .shortcut("Ctrl+PageDown")
            .group(CommandGroup::Go)
            .hidden_from_menu(),
            Command::new("weather.previous-place", "Previous place", move |_| {
                step(-1)
            })
            .shortcut("Ctrl+PageUp")
            .group(CommandGroup::Go)
            .hidden_from_menu(),
            Command::key_hint("weather.hours", "Look at each hour", "Left, Right")
                .group(CommandGroup::Go),
            Command::key_hint("weather.days", "Look at each day", "Up, Down")
                .group(CommandGroup::Go),
            Command::key_hint(
                "weather.remove-place",
                "Remove the place in the list",
                "Delete",
            )
            .group(CommandGroup::Edit),
        ]);

        // The menu.
        let automatic = settings.units.is_none();
        let pick = move |edit: fn(&mut Units)| {
            move |_: ()| {
                let mut chosen = hub.settings.peek().units();
                edit(&mut chosen);
                hub.units(Some(chosen));
            }
        };
        let temperature_set = |temperature: Temperature| {
            menu_item(
                match temperature {
                    Temperature::Celsius => "Celsius",
                    Temperature::Fahrenheit => "Fahrenheit",
                },
                pick(match temperature {
                    Temperature::Celsius => |u| u.temperature = Temperature::Celsius,
                    Temperature::Fahrenheit => |u| u.temperature = Temperature::Fahrenheit,
                }),
            )
            .checked(!automatic && units.temperature == temperature)
        };
        let speed_set = |speed: Speed| {
            menu_item(
                match speed {
                    Speed::KilometresPerHour => "Kilometres per hour",
                    Speed::MilesPerHour => "Miles per hour",
                    Speed::MetresPerSecond => "Metres per second",
                },
                pick(match speed {
                    Speed::KilometresPerHour => |u| u.speed = Speed::KilometresPerHour,
                    Speed::MilesPerHour => |u| u.speed = Speed::MilesPerHour,
                    Speed::MetresPerSecond => |u| u.speed = Speed::MetresPerSecond,
                }),
            )
            .checked(!automatic && units.speed == speed)
        };
        let rain_set = |rain: bingux_weather::units::Rain| {
            use bingux_weather::units::Rain;
            menu_item(
                match rain {
                    Rain::Millimetres => "Millimetres",
                    Rain::Inches => "Inches",
                },
                pick(match rain {
                    Rain::Millimetres => |u| u.rain = Rain::Millimetres,
                    Rain::Inches => |u| u.rain = Rain::Inches,
                }),
            )
            .checked(!automatic && units.rain == rain)
        };
        let units_menu = submenu(
            "Units",
            vec![
                menu_item("Automatic", move |_| hub.units(None)).checked(automatic),
                menu_separator(),
                temperature_set(Temperature::Celsius),
                temperature_set(Temperature::Fahrenheit),
                menu_separator(),
                speed_set(Speed::KilometresPerHour),
                speed_set(Speed::MilesPerHour),
                speed_set(Speed::MetresPerSecond),
                menu_separator(),
                rain_set(bingux_weather::units::Rain::Millimetres),
                rain_set(bingux_weather::units::Rain::Inches),
            ],
        );
        let use_location = settings.use_location;
        let mut menu = Vec::new();
        menu.push(units_menu);
        menu.push(
            menu_item("Use my location", move |_| hub.use_location(!use_location))
                .checked(use_location),
        );
        let lookup_on = settings.network_lookup == Some(true);
        let mut lookup_item =
            menu_item("Use network location", move |_| hub.set_network(!lookup_on))
                .checked(lookup_on);
        if !use_location {
            lookup_item = lookup_item.disabled();
        }
        menu.push(lookup_item);
        menu.push(menu_separator());

        // The header. The hero names the place, so the header does not say it
        // again: it names the app over the list and is empty over a page. (The
        // window's own title, for the task list, stays "Weather".)
        let showing_list = compact && *hub.list.read();
        let mut header = HeaderBar::new(if showing_list {
            "Weather".to_string()
        } else {
            String::new()
        })
        .busy(busy && !showing_list)
        .trailing(
            icon_button(Icon::ListAdd, move |_| search::open(overlays, hub))
                .tooltip("Add location (Ctrl+N)"),
        );
        if !showing_list {
            header = header.trailing(
                icon_button(Icon::Refresh, move |_| hub.refresh_all())
                    .tooltip("Refresh (Ctrl+R)")
                    .busy(busy),
            );
        }
        header = header.trailing(app_menu(menu));
        if compact && !showing_list {
            let mut list = hub.list;
            header = header
                .leading(icon_button(Icon::GoPrevious, move |_| list.set(true)).tooltip("Places"));
        }
        if !compact {
            header = header.trailing(WindowControls);
        }

        let selected = selected_key.clone();
        let list = Locations {
            here: here.clone(),
            saved: settings.saved.clone(),
            selected,
            units,
            refresh,
            hub,
            add: add.clone(),
        };
        let here_text = match &now_showing {
            Current::Place {
                here: Some(here), ..
            } => Some(HereView::source_text(here).to_string()),
            _ => None,
        };
        let page: Element = match &now_showing {
            Current::Nothing if ask_network => {
                let stack = if compact { flex_col() } else { flex_row() };
                status_page(Icon::LocationArrow, "Use your approximate location?")
                    .description(
                        "Find the nearest town from this computer's network address. The address \
                         goes to ipwho.is, GeoJS or ipapi.co.",
                    )
                    .child(
                        rect()
                            .tw(&tokens, &[stack, gap(S3), mt(S3)])
                            .child(primary_button("Use approximate location", move |_| {
                                hub.set_network(true)
                            }))
                            .child(action_button("Search by city", move |_| {
                                hub.change(|settings| settings.network_lookup = Some(false));
                                search::open(overlays, hub);
                            })),
                    )
                    .into()
            }
            Current::Nothing => status_page(Icon::LocationArrow, "Add a city")
                .description(
                    "Your location is not available. Search for a city to see its forecast.",
                )
                .action("Add location", move |_| search::open(overlays, hub))
                .into(),
            _ => Page {
                place: place.clone().unwrap_or_else(|| Place::new("", 0.0, 0.0)),
                here: here_text,
                units,
                load,
                retry,
            }
            .into(),
        };

        let body: Element = if compact {
            if showing_list {
                rect()
                    .tw(&tokens, &[w_full(), h_flex(), bg(Paint::Bg)])
                    .child(list)
                    .into()
            } else {
                rect().tw(&tokens, &[w_full(), h_flex()]).child(page).into()
            }
        } else {
            rect()
                .tw(&tokens, &[w_full(), h_flex(), flex_row()])
                .child(
                    rect()
                        .tw(&tokens, &[w(Dim::Sidebar), h_full(), bg(Paint::Sidebar)])
                        .child(list),
                )
                .child(rect().tw(&tokens, &[w(Dim::Hairline), h_full(), bg(Paint::Separator)]))
                .child(rect().tw(&tokens, &[w_flex(), h_full()]).child(page))
                .into()
        };

        rect()
            .component("Weather")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .child(header)
            .child(body)
    }
}
