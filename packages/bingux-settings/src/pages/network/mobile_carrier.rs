//! Choosing the carrier: automatic, or a search that lists the networks the
//! modem can see and registers on the one chosen.
//!
//! A manual choice is kept in the `gsm.network-id` of the mobile profiles, so
//! NetworkManager registers on that carrier each time it connects. ModemManager
//! does not report whether the modem was told to stay on one carrier.

use freya::prelude::*;

use super::apn::{self, Profile, use_profiles};
use super::modem::{self, Carrier};
use super::{ModemState, reload_modem, use_modem};
use crate::pages::loading::veiled;
use crate::pages::network::model::{NetCtx, use_net_ctx};
use crate::system::{CommandError, background_then};
use crate::ui::{
    Group, PageHead, SpinnerSize, Status, page_column, radio_row, radio_rows, row, spinner, status,
    switch_row, use_page_busy,
};

#[derive(Clone, PartialEq)]
enum Search {
    /// Not searched yet: the list is empty and says so.
    Idle,
    Searching,
    Found(Vec<Carrier>),
    Failed(String),
}

/// `23415` as the code NetworkManager keeps: MCC and MNC without a dash.
fn network_id(carrier: &Carrier) -> String {
    carrier.code.clone()
}

fn availability(carrier: &Carrier) -> &'static str {
    match carrier.availability.as_str() {
        "current" => "Connected",
        "forbidden" => "Not allowed by your SIM",
        "available" => "Available",
        _ => "",
    }
}

#[derive(PartialEq)]
pub struct MobileNetworkPage;

impl Component for MobileNetworkPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let modem_state = use_modem(true);
        let profiles = use_profiles();
        let search = use_state(|| Search::Idle);
        let registering = use_state(|| None::<String>);
        // What the Automatic switch shows once it has been used; before that,
        // what the profiles say.
        let manual = use_state(|| None::<bool>);
        use_page_busy(*search.read() == Search::Searching || registering.read().is_some());

        let loaded = modem_state.read().clone();
        // Every way out draws through `veiled`, so the hooks do not depend
        // on what has been read.
        let modem = match loaded {
            None => {
                return veiled(page_column().child(PageHead::new()), true);
            }
            Some(Err(error)) => {
                return veiled(
                    page_column().child(PageHead::new()).child(
                        status(Status::Error(error.0))
                            .action("Try again", move |_| reload_modem(modem_state)),
                    ),
                    false,
                );
            }
            Some(Ok(None)) => {
                return veiled(
                    page_column()
                        .child(PageHead::new())
                        .child(status(Status::Empty("No mobile broadband device.".into()))),
                    false,
                );
            }
            Some(Ok(Some(modem))) => modem,
        };
        if modem.locked() || modem.no_sim() || modem.off() {
            let text = if modem.locked() {
                "Unlock the SIM to choose a carrier."
            } else if modem.no_sim() {
                "No SIM card."
            } else {
                "Mobile network is off."
            };
            return veiled(
                page_column()
                    .child(PageHead::new())
                    .child(status(Status::Empty(text.into()))),
                false,
            );
        }
        let list = profiles
            .read()
            .clone()
            .and_then(Result::ok)
            .unwrap_or_default();
        let manual_now = manual.read().unwrap_or_else(|| apn::pinned(&list));
        let id = modem.id.clone();

        let start_search = {
            let id = id.clone();
            move || {
                let mut search = search;
                search.set(Search::Searching);
                let id = id.clone();
                background_then(
                    move || modem::scan(&id),
                    move |result| {
                        let mut search = search;
                        search.set(match result {
                            Ok(found) => Search::Found(found),
                            Err(error) => Search::Failed(error.0),
                        });
                    },
                );
            }
        };

        let automatic = {
            let (id, list, start) = (id.clone(), list.clone(), start_search.clone());
            switch_row("Automatic", !manual_now, move |on: bool| {
                let mut manual = manual;
                manual.set(Some(!on));
                if !on {
                    start();
                    return;
                }
                let (id, list) = (id.clone(), list.clone());
                let mut registering = registering;
                registering.set(Some(String::new()));
                background_then(
                    move || {
                        modem::register_home(&id)?;
                        apn::set_network_id(&list, "")
                    },
                    move |result: Result<(), CommandError>| {
                        let mut registering = registering;
                        registering.set(None);
                        if let Err(error) = result {
                            let mut manual = manual;
                            manual.set(Some(true));
                            ctx.overlays.toast(error.0);
                        }
                        apn::reload(profiles);
                        reload_modem(modem_state);
                    },
                );
            })
        };

        let mut page = page_column()
            .child(PageHead::new())
            .child(Group::new(None::<String>).row(automatic));

        if manual_now {
            let busy = registering.read().clone();
            let searching = *search.read() == Search::Searching;
            let list_group: Element = match search.read().clone() {
                Search::Idle => Group::titled("Networks")
                    .row(Element::from(status(Status::Empty(
                        "Search to see the carriers around you.".into(),
                    ))))
                    .into(),
                Search::Searching => Group::titled("Networks")
                    .row(Element::from(status(Status::Loading(
                        "Searching for networks. This can take a minute or two.".into(),
                    ))))
                    .into(),
                Search::Failed(text) => Group::titled("Networks")
                    .row(Element::from(status(Status::Error(text))))
                    .into(),
                Search::Found(found) if found.is_empty() => Group::titled("Networks")
                    .row(Element::from(status(Status::Empty(
                        "No networks were found.".into(),
                    ))))
                    .into(),
                Search::Found(found) => {
                    let rows = found
                        .iter()
                        .map(|carrier| {
                            let choice = carrier.clone();
                            let (id, list) = (id.clone(), list.clone());
                            let item = radio_row(carrier.title(), carrier.current(), move |_| {
                                choose(
                                    ctx,
                                    modem_state,
                                    profiles,
                                    search,
                                    registering,
                                    &id,
                                    &list,
                                    &choice,
                                )
                            })
                            .subtitle(
                                [carrier.technology.as_str(), availability(carrier)]
                                    .into_iter()
                                    .filter(|part| !part.is_empty())
                                    .collect::<Vec<_>>()
                                    .join(" · "),
                            )
                            .enabled(!carrier.forbidden() && busy.is_none());
                            if busy.as_deref() == Some(carrier.code.as_str()) {
                                item.trailing(spinner(SpinnerSize::Small))
                            } else {
                                item
                            }
                        })
                        .collect();
                    radio_rows(Some("Networks".to_string()), rows).into()
                }
            };
            page = page.child(list_group).child(
                Group::new(None::<String>).row(
                    row("Search again")
                        .navigates()
                        .enabled(!searching && busy.is_none())
                        .on_press(move |_| start_search()),
                ),
            );
        }
        veiled(page, false)
    }
}

/// Register on `carrier` and remember it in the profiles.
#[allow(clippy::too_many_arguments)]
fn choose(
    ctx: NetCtx,
    modem_state: ModemState,
    profiles: State<Option<Result<Vec<Profile>, CommandError>>>,
    search: State<Search>,
    registering: State<Option<String>>,
    id: &str,
    list: &[Profile],
    carrier: &Carrier,
) {
    if carrier.current() {
        return;
    }
    let mut registering_now = registering;
    registering_now.set(Some(carrier.code.clone()));
    let (id, list, chosen) = (id.to_string(), list.to_vec(), carrier.clone());
    background_then(
        move || {
            modem::register(&id, &chosen.code)?;
            apn::set_network_id(&list, &network_id(&chosen))?;
            Ok(chosen)
        },
        move |result: Result<Carrier, CommandError>| {
            let mut registering = registering;
            registering.set(None);
            match result {
                Ok(chosen) => {
                    // The scan is minutes old by now; mark the choice in it
                    // instead of searching again.
                    let mut search = search;
                    let found = match search.read().clone() {
                        Search::Found(found) => found,
                        _ => Vec::new(),
                    };
                    search.set(Search::Found(
                        found
                            .into_iter()
                            .map(|mut carrier| {
                                if carrier.current() {
                                    carrier.availability = "available".into();
                                }
                                if carrier.code == chosen.code {
                                    carrier.availability = "current".into();
                                }
                                carrier
                            })
                            .collect(),
                    ));
                }
                Err(error) => ctx.overlays.toast(error.0),
            }
            apn::reload(profiles);
            reload_modem(modem_state);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carrier(availability: &str) -> Carrier {
        Carrier {
            code: "23415".into(),
            name: "Vodafone UK".into(),
            technology: "4G".into(),
            availability: availability.into(),
        }
    }

    #[test]
    fn availability_words() {
        assert_eq!(availability(&carrier("current")), "Connected");
        assert_eq!(
            availability(&carrier("forbidden")),
            "Not allowed by your SIM"
        );
        assert_eq!(availability(&carrier("available")), "Available");
        assert_eq!(availability(&carrier("unknown")), "");
    }

    #[test]
    fn network_id_is_the_operator_code() {
        assert_eq!(network_id(&carrier("available")), "23415");
    }

    #[test]
    fn a_pinned_profile_means_manual() {
        let mut profile = apn::parse_profile("connection.id:x\n");
        assert!(!apn::pinned(std::slice::from_ref(&profile)));
        profile.network_id = "23415".into();
        assert!(apn::pinned(&[profile]));
    }
}
