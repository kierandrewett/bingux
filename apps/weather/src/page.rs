//! The page of one place: its forecast from the hero down, or the state of
//! getting it (loading, no network, the service down), with the last forecast
//! kept on screen, and said to be old, when a new one cannot be fetched.

use std::time::Duration;

use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use bingux_weather::notices::{self, Severity};
use bingux_weather::time::{ago, unix_now};
use bingux_weather::{Failure, Loaded, Place, Units};
use freya::prelude::*;

use crate::cards::{Details, Trends, section};
use crate::hero::Hero;
use crate::hourly::HourStrip;
use crate::week::Week;

/// The most notices shown above the hero.
const NOTICES_SHOWN: usize = 3;
const DATA_URL: &str = "https://open-meteo.com/";

/// Where getting the forecast stands.
#[derive(Clone, PartialEq)]
pub enum Load {
    /// The first load, with nothing to show yet.
    Loading,
    /// The computer's position is being looked for.
    Locating,
    /// There is nothing to show and nothing could be fetched.
    Failed(Failure),
    /// A forecast, current or the last one saved.
    Ready(Loaded),
}

/// The page of a place.
#[derive(Clone, PartialEq)]
pub struct Page {
    pub place: Place,
    /// How "My location" was found, when this is it.
    pub here: Option<String>,
    pub units: Units,
    pub load: Load,
    /// Fetch again.
    pub retry: EventHandler<()>,
}

/// Opens Open-Meteo's site in the person's browser.
fn open_data_source() {
    spawn(async {
        let _ = bingux_ui::task::spawn_blocking(|| {
            bingux_ui::runtime::run("xdg-open", &[DATA_URL], Duration::from_secs(5))
        })
        .await;
    });
}

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        match &self.load {
            Load::Loading | Load::Locating => {
                let words = if matches!(self.load, Load::Locating) {
                    "Finding your location…"
                } else {
                    "Loading the forecast…"
                };
                return rect()
                    .component("Page")
                    .tw(&tokens, &[w_full(), h_full()])
                    .child(clamp(
                        Dim::Page,
                        rect()
                            .tw(&tokens, &[w_full(), p(S4)])
                            .child(status(Status::Loading(words.into()))),
                    ))
                    .into_element();
            }
            Load::Failed(failure) => {
                let page = match failure {
                    Failure::Offline => status_page(Icon::NetworkOffline, "No connection")
                        .description(
                            "Weather needs the internet. Check the connection, then try again.",
                        ),
                    Failure::Service => status_page(Icon::NetworkError, "Forecast not available")
                        .description("The forecast service did not answer. Try again in a moment."),
                };
                let retry = self.retry.clone();
                return page
                    .action("Try again", move |_| retry.call(()))
                    .into_element();
            }
            Load::Ready(_) => {}
        }
        let Load::Ready(loaded) = &self.load else {
            return rect().into_element();
        };
        let forecast = &loaded.forecast;
        let now = unix_now();
        let local = forecast.local(now);

        let mut column = rect().tw(&tokens, &[w_full(), flex_col(), gap(S3), p(S4)]);
        if let Some(failure) = loaded.stale {
            let retry = self.retry.clone();
            column = column.child(
                banner(match failure {
                    Failure::Offline => {
                        "Showing the last forecast. Connect to the internet to update it."
                    }
                    Failure::Service => "Showing the last forecast. The service did not answer.",
                })
                .title(format!("Updated {}", ago(now - forecast.fetched_at)))
                .kind(BannerKind::Warning)
                .action("Try again", move |_| retry.call(())),
            );
        }
        for notice in notices::notices(forecast, &self.units, local)
            .into_iter()
            .take(NOTICES_SHOWN)
        {
            column = column.child(banner(notice.detail).title(notice.title).kind(
                match notice.severity {
                    Severity::Warning => BannerKind::Warning,
                    Severity::Advisory => BannerKind::Info,
                },
            ));
        }

        column = column
            .child(Hero {
                name: self.place.name.clone(),
                here: self.here.clone(),
                forecast: forecast.clone(),
                units: self.units,
                local,
            })
            .child(section(
                &tokens,
                "Hourly forecast",
                vec![
                    HourStrip {
                        forecast: forecast.clone(),
                        units: self.units,
                        local,
                    }
                    .into(),
                ],
            ))
            .child(Trends {
                forecast: forecast.clone(),
                units: self.units,
                local,
            })
            .child(section(
                &tokens,
                "10-day forecast",
                vec![
                    Week {
                        forecast: forecast.clone(),
                        units: self.units,
                        local,
                    }
                    .into(),
                ],
            ))
            .child(Details {
                forecast: forecast.clone(),
                units: self.units,
                local,
            });

        let mut about = Group::new(None::<String>)
            .row(row("Updated").value(ago(now - forecast.fetched_at)))
            .row(row("Place").value(self.place.coordinates()));
        if let Some(how) = &self.here {
            about = about.row(row("Position").value(how.clone()));
        }
        column = column.child(
            about.row(
                row("Weather data by Open-Meteo.com")
                    .subtitle("Licensed CC BY 4.0")
                    .navigates()
                    .on_press(|_| open_data_source()),
            ),
        );

        rect()
            .component("Page")
            .tw(&tokens, &[w_full(), h_full()])
            .child(
                ScrollView::new()
                    .key(self.place.key())
                    .child(clamp(Dim::Page, column)),
            )
            .into_element()
    }
}
