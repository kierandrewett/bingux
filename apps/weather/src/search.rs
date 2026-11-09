//! The sheet that adds a city: a field, and the cities that match as the
//! person types, found with Open-Meteo's search. Choosing one saves it and
//! shows its forecast.
//!
//! The sheet also offers "My location" when it is turned off, so a person who
//! turned it off can bring it back from the place they would look for it.

use std::time::Duration;

use crate::words;
use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use bingux_ui::shell::{FORM_HEIGHT, FORM_WIDTH};
use bingux_ui::task::{Resource, use_resource};
use freya::prelude::*;

use crate::hub::Hub;
use crate::load;

/// How long the field rests before a lookup starts, so a word being typed is
/// one request and not one for each letter.
const REST: Duration = Duration::from_millis(350);

/// Opens the sheet.
pub fn open(overlays: Overlays, hub: Hub) {
    let dialog = framed_dialog("Add location", Sheet { hub }, FORM_WIDTH, FORM_HEIGHT);
    overlays.show_dialog(if is_compact() {
        dialog.fill_window()
    } else {
        dialog
    });
}

#[derive(Clone, Copy, PartialEq)]
struct Sheet {
    hub: Hub,
}

impl Component for Sheet {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let hub = self.hub;
        let query = use_state(String::new);
        // The text the lookup runs for: the field's, after it has rested.
        let mut asked = use_state(String::new);
        let found = use_resource((*asked.read()).clone(), load::search);

        let on_change = move |text: String| {
            let typed = text.clone();
            let mut query = query;
            query.set(text);
            spawn(async move {
                timer(REST).await;
                // A newer keystroke makes this one stale.
                if *query.peek() == typed {
                    asked.set_if_modified(typed);
                }
            });
        };

        let header = HeaderBar::new(tr!("weather-add-title"))
            .draggable(false)
            .trailing(
                icon_button(Icon::WindowClose, move |_| overlays.dismiss_dialog())
                    .tooltip(&tr!("weather-add-close")),
            );
        let field = search_field(query, on_change)
            .placeholder(tr!("weather-add-placeholder"))
            .fill_width()
            .autofocus();

        let typed: String = (*asked.read()).clone();
        let settled = typed.trim() == query.read().trim();
        let mut results = rect().tw(&tokens, &[w_full(), flex_col(), gap(S3)]);
        if typed.trim().chars().count() < 2 {
            results = results.child(
                status_page(Icon::Search, tr!("weather-add-hint-title"))
                    .description(tr!("weather-add-hint-body")),
            );
        } else {
            match found.with(|state| match state {
                Resource::Ready(places) | Resource::Refreshing(places) => Some(places.clone()),
                _ => None,
            }) {
                Some(places) if !places.is_empty() && settled => {
                    let rows = places.into_iter().map(|place| {
                        let chosen = place.clone();
                        row(place.name.clone())
                            .subtitle(words::join_list(&place.detail))
                            .on_press(move |_| {
                                hub.add(chosen.clone());
                                overlays.dismiss_dialog();
                            })
                            .into()
                    });
                    results = results.child(Group::new(None::<String>).rows(rows));
                }
                Some(_) if settled => {
                    results = results.child(
                        status_page(
                            Icon::Search,
                            tr!("weather-add-no-results-title", query = typed.clone()),
                        )
                        .description(tr!("weather-add-no-results-body")),
                    );
                }
                _ => {
                    let failed = found.with(|state| state.error().map(str::to_string));
                    results = results.child(match failed {
                        Some(code) if code == load::OFFLINE => {
                            banner(tr!("weather-add-offline-body"))
                                .title(tr!("weather-add-offline-title"))
                                .kind(BannerKind::Warning)
                                .into_element()
                        }
                        Some(_) => banner(tr!("weather-add-failed-body"))
                            .title(tr!("weather-add-failed-title"))
                            .kind(BannerKind::Warning)
                            .into_element(),
                        None => status(Status::Loading(tr!("weather-add-searching").into()))
                            .into_element(),
                    });
                }
            }
        }

        let location_off = !hub.settings.read().use_location;
        let bring_back = location_off.then(|| {
            Group::new(None::<String>).row(
                row("Use my location")
                    .leading(Icon::LocationArrow)
                    .on_press(move |_| {
                        hub.use_location(true);
                        overlays.dismiss_dialog();
                    }),
            )
        });

        rect()
            .component("AddLocation")
            .tw(
                &tokens,
                &[flex_col(), w_full(), h_full(), bg(Paint::Popover)],
            )
            .child(header)
            .child(
                rect().tw(&tokens, &[w_full(), h_flex(), flex_col()]).child(
                    ScrollView::new().child(
                        rect()
                            .tw(&tokens, &[w_full(), flex_col(), gap(S4), p(S4)])
                            .child(field)
                            .child(results)
                            .maybe_child(bring_back),
                    ),
                ),
            )
    }
}
