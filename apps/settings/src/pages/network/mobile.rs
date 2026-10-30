//! Mobile broadband: the Mobile row of the Network page and the Mobile network
//! screen with its subpages.
//!
//! ModemManager (`mmcli -J`, read every ten seconds while a mobile screen is
//! open) says what the modem and the SIM are doing; NetworkManager (`nmcli`)
//! owns the data connection and the access point profiles. Files:
//!
//! - `mobile_modem.rs`: the modem model, the `mmcli` parsers and commands, the
//!   recorded output a snapshot build can stand in (`BINGUX_SETTINGS_FAKE_MODEM`).
//! - `mobile_apn.rs`: the `gsm` profiles, their list and editor.
//! - `mobile_carrier.rs`: carrier search and registration.
//! - `mobile_sim.rs`: the PIN and PUK popups.

#[path = "mobile_apn.rs"]
mod apn;
#[path = "mobile_carrier.rs"]
mod carrier;
#[path = "mobile_modem.rs"]
mod modem;
#[path = "mobile_sim.rs"]
mod sim;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use freya::prelude::*;

use super::model::{self, CONNECT, NetCtx, Overview, QUICK, nmcli, use_net_ctx};
use super::usage::{UsagePage, UsageScope, scope_usage_row};
use crate::pages::loading::veiled;
use crate::routes;
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, PageHead, Status, choice_row, page_column, property_row, row, status, switch,
};
use apn::Profile;
use modem::{Lock, Modem};
use sim::PinTask;

pub use apn::{ApnListPage, ApnPage};
pub use carrier::MobileNetworkPage;

/// How often the modem is read again while a mobile screen is open.
const POLL: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// The modem, read in the background
// ---------------------------------------------------------------------------

/// `Ok(None)` is a computer ModemManager sees no modem on.
pub type ModemState = State<Option<Result<Option<Modem>, CommandError>>>;

/// Milliseconds since the epoch of the last finished read, so two open
/// screens do not each read on every tick.
static LAST_READ: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Read the modem again now.
pub fn reload_modem(state: ModemState) {
    reload_modem_then(state, || {});
}

/// Read the modem again, and run `after` once the new value is in place.
fn reload_modem_then(state: ModemState, after: impl FnOnce() + 'static) {
    background_then(modem::load_modem, move |loaded| {
        LAST_READ.store(now_ms(), Ordering::SeqCst);
        let mut state = state;
        state.set_if_modified(Some(loaded));
        after();
    });
}

/// The modem as last read, read again every ten seconds while `watch` is true.
/// The Network page watches only when NetworkManager reports a modem, so a
/// computer without one never starts `mmcli`. Call from `render`.
pub fn use_modem(watch: bool) -> ModemState {
    let wanted = use_hook(|| Rc::new(Cell::new(watch)));
    wanted.set(watch);
    let state = use_hook(move || {
        let state = system::cached::<Result<Option<Modem>, CommandError>>("mobile-modem");
        let focus = system::window_focus();
        spawn(async move {
            loop {
                timer(POLL).await;
                if *focus.peek() && wanted.get() && now_ms().saturating_sub(LAST_READ.load(Ordering::SeqCst)) >= 8000
                {
                    reload_modem(state);
                }
            }
        });
        state
    });
    use_side_effect_with_deps(&watch, move |watch| {
        if *watch {
            reload_modem(state);
        }
    });
    state
}

// ---------------------------------------------------------------------------
// What NetworkManager says about the modem
// ---------------------------------------------------------------------------

/// The parts of the overview the mobile screens use.
#[derive(Clone)]
struct Link {
    /// NetworkManager sees a modem.
    present: bool,
    airplane: bool,
    /// The WWAN radio is on.
    radio: bool,
    /// The radio cannot be turned on: a hardware switch blocks it.
    blocked: bool,
    /// NetworkManager's name of the modem device.
    device: Option<String>,
    /// The data connection is up or coming up.
    connected: bool,
    /// The connection the device runs.
    connection: String,
}

impl Link {
    fn of(ov: &Overview) -> Self {
        let device = ov.modem_device();
        Self {
            present: ov.modem_present(),
            airplane: model::pending_for("airplane").unwrap_or(ov.airplane),
            radio: ov.wwan_on,
            blocked: ov.wwan_hw == "disabled",
            device: device.map(|device| device.name.clone()),
            connected: device.is_some_and(|device| device.state.starts_with("connect")),
            connection: device
                .map(|device| device.connection.clone())
                .unwrap_or_default(),
        }
    }

    /// Mobile data as the switch shows it: a change in flight shows its
    /// target at once.
    fn data_on(&self) -> bool {
        model::pending_for("wwan").unwrap_or(self.radio && self.connected)
    }
}

/// The one-line state under the title and in the Network page's row.
fn status_line(modem: Option<&Modem>, link: &Link) -> String {
    if link.airplane {
        return "Off in airplane mode".to_string();
    }
    let Some(modem) = modem else {
        return if !link.radio {
            "Off".to_string()
        } else if link.connected {
            link.connection.clone()
        } else {
            "Not connected".to_string()
        };
    };
    if modem.locked() {
        return "SIM locked".to_string();
    }
    if modem.no_sim() {
        return "No SIM card".to_string();
    }
    if !link.radio || modem.off() {
        return "Off".to_string();
    }
    if !modem.registered() {
        return match modem.registration.as_str() {
            "searching" => "Searching for a network".to_string(),
            "denied" => "Registration refused".to_string(),
            _ => "Not registered".to_string(),
        };
    }
    let mut parts = vec![modem.carrier().unwrap_or_else(|| "Mobile network".into())];
    parts.extend(modem.technology().map(str::to_string));
    if modem.roaming() {
        parts.push("Roaming".to_string());
    }
    parts.join(" · ")
}

/// Whether a mobile data switch can be used.
fn can_switch(modem: Option<&Modem>, link: &Link) -> bool {
    !link.airplane && !link.blocked && modem.is_none_or(|modem| !modem.locked() && !modem.no_sim())
}

/// The modem device `nmcli -t -f DEVICE,TYPE,STATE device` lists as ready.
fn ready_modem_device(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let fields = model::split_terse(line);
        let (device, kind, state) = (fields.first()?, fields.get(1)?, fields.get(2)?);
        (matches!(kind.as_str(), "gsm" | "cdma")
            && !matches!(state.as_str(), "unavailable" | "unmanaged"))
        .then(|| device.clone())
    })
}

/// Turning the radio on makes NetworkManager start the modem, which takes a
/// few seconds; wait for the device to be ready to connect.
fn wait_for_modem_device() -> Result<String, CommandError> {
    for _ in 0..30 {
        let output = nmcli(&["-t", "-f", "DEVICE,TYPE,STATE", "device"], QUICK)?;
        if let Some(device) = ready_modem_device(&output) {
            return Ok(device);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err(CommandError(
        "The modem did not become ready. Try again in a moment.".into(),
    ))
}

/// Mobile data on: the radio, then the data connection. Off: the data
/// connection goes down and the radio stays on, so calls and texts still work.
fn data_command(device: Option<String>, radio_on: bool, on: bool) -> Result<(), CommandError> {
    if !on {
        return match device {
            Some(device) => nmcli(&["device", "disconnect", &device], CONNECT).map(|_| ()),
            None => Ok(()),
        };
    }
    if !radio_on {
        nmcli(&["radio", "wwan", "on"], QUICK)?;
    }
    let device = match device.filter(|_| radio_on) {
        Some(device) => device,
        None => wait_for_modem_device()?,
    };
    nmcli(&["device", "connect", &device], CONNECT).map(|_| ())
}

fn set_mobile_data(ctx: NetCtx, link: &Link, on: bool) {
    let (device, radio) = (link.device.clone(), link.radio);
    model::toggle_link(ctx, "wwan".to_string(), on, move || {
        data_command(device, radio, on)
    });
}

// ---------------------------------------------------------------------------
// The row on the Network page
// ---------------------------------------------------------------------------

/// The Mobile row of the Connections group; `None` without a modem.
pub fn mobile_row(ctx: NetCtx, ov: &Overview, airplane: bool) -> Option<Element> {
    let modem_state = use_modem(ov.modem_present());
    let known = modem_state.read().clone().and_then(Result::ok).flatten();
    if !ov.modem_present() && known.is_none() {
        return None;
    }
    let mut link = Link::of(ov);
    link.airplane = airplane;
    let usable = can_switch(known.as_ref(), &link);
    let bars = known.as_ref().map_or(0, Modem::bars);
    let on = usable && link.data_on();
    let toggle = link.clone();
    Some(Element::from(
        row("Mobile network")
            .leading_signal(bars, false)
            .subtitle(status_line(known.as_ref(), &link))
            .trailing(
                switch(on, move |next: bool| set_mobile_data(ctx, &toggle, next)).enabled(usable),
            )
            .navigates()
            .on_press(move |_| ctx.router.open(routes::mobile())),
    ))
}

// ---------------------------------------------------------------------------
// The screen
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct MobilePage;

impl Component for MobilePage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let modem_state = use_modem(true);
        let profiles = apn::use_profiles();
        let roaming_shown = use_state(|| None::<bool>);

        let (modem, link, loading) = match read_mobile(ctx, modem_state) {
            Ok(read) => read,
            Err(page) => return page,
        };
        let list: Vec<Profile> = profiles
            .read()
            .clone()
            .and_then(Result::ok)
            .unwrap_or_default();

        let head = PageHead::new().subtitle(if loading {
            "—".to_string()
        } else {
            status_line(Some(&modem), &link)
        });
        let mut page = page_column().child(head);

        if link.airplane {
            page = page.child(Group::new(None::<String>).row(Element::from(status(Status::Info(
                "Mobile network is off in airplane mode. Turn airplane mode off on the Network \
                 page to use it."
                    .into(),
            )))));
        }

        if modem.locked() {
            return veiled(
                page.children(
                    unlock_groups(ctx, modem_state, &modem)
                        .into_iter()
                        .map(Element::from),
                ),
                loading,
            )
            .into();
        }
        if modem.no_sim() {
            page = page.child(Group::new(None::<String>).row(Element::from(status(
                Status::Empty("No SIM card. Insert one to use mobile data.".into()),
            ))));
            return veiled(page.child(modem_group(&modem)), loading).into();
        }
        let off = !link.radio || modem.off();
        if !link.airplane {
            if off {
                page = page.child(Group::new(None::<String>).row(Element::from(status(
                    Status::Info(
                        "The mobile network is off. Turn on mobile data to use it.".into(),
                    ),
                ))));
            } else {
                page = page.child(status_group(&modem));
            }
        }
        page = page.child(data_group(
            ctx,
            &link,
            &modem,
            &list,
            profiles,
            roaming_shown,
        ));
        if !off && !link.airplane {
            page = page.child(usage_group(ctx, &link, &modem));
        }
        veiled(page.child(advanced_row(ctx)), loading).into()
    }
}

/// What the Mobile pages read: the modem, how NetworkManager sees it, and whether the modem has
/// been read yet (`true` while the modem is a blank). `Err` is a finished page for a computer
/// with no modem or one that cannot be read. Every way out draws through `veiled`, so the number
/// of hooks does not depend on what has been read.
fn read_mobile(ctx: NetCtx, modem_state: ModemState) -> Result<(Modem, Link, bool), Element> {
    let overview = ctx.overview.read().clone();
    let ov = match overview {
        Some(Ok(ov)) => Some(ov),
        Some(Err(error)) => {
            return Err(veiled(
                page_column().child(PageHead::new()).child(
                    status(Status::Error(error.0)).action("Try again", move |_| ctx.reload()),
                ),
                false,
            )
            .into());
        }
        None => None,
    };
    let link = ov.as_ref().map(Link::of);
    let empty = || {
        Err(veiled(
            page_column()
                .child(PageHead::new())
                .child(status(Status::Empty("No mobile broadband device.".into()))),
            false,
        )
        .into())
    };
    let loaded = modem_state.read().clone();
    let (modem, loading) = match (loaded, &link) {
        (Some(Ok(Some(modem))), _) => (modem, false),
        (Some(Ok(None)), _) => return empty(),
        (Some(Err(error)), Some(link)) if link.present => {
            return Err(veiled(
                page_column().child(PageHead::new()).child(
                    status(Status::Error(error.0))
                        .action("Try again", move |_| reload_modem(modem_state)),
                ),
                false,
            )
            .into());
        }
        (Some(Err(_)), _) => return empty(),
        // NetworkManager has said there is no modem; no need to wait for
        // ModemManager to say it too.
        (None, Some(link)) if !link.present => return empty(),
        (None, _) => (Modem::placeholder(), true),
    };
    let link = link.unwrap_or(Link {
        present: true,
        airplane: false,
        radio: true,
        blocked: false,
        device: None,
        connected: false,
        connection: String::new(),
    });
    Ok((modem, link, loading))
}

/// The one row to everything else on the Mobile page.
fn advanced_row(ctx: NetCtx) -> Group {
    Group::new(None::<String>).row(
        row("Advanced")
            .navigates()
            .on_press(move |_| ctx.router.open(routes::mobile_advanced())),
    )
}

/// The Advanced page of the mobile network: carrier, preferred network type and access point
/// names, the SIM, and what the modem is.
#[derive(PartialEq)]
pub struct MobileAdvancedPage;

impl Component for MobileAdvancedPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let modem_state = use_modem(true);
        let profiles = apn::use_profiles();
        let modes_shown = use_state(|| None::<String>);
        let (modem, link, loading) = match read_mobile(ctx, modem_state) {
            Ok(read) => read,
            Err(page) => return page,
        };
        let list: Vec<Profile> = profiles
            .read()
            .clone()
            .and_then(Result::ok)
            .unwrap_or_default();
        let head = PageHead::new();
        let mut page = page_column().child(head);
        if modem.locked() {
            return veiled(
                page.children(
                    unlock_groups(ctx, modem_state, &modem)
                        .into_iter()
                        .map(Element::from),
                ),
                loading,
            )
            .into();
        }
        let off = !link.radio || modem.off();
        if !off && !link.airplane {
            page = page
                .child(registration_group(&modem))
                .child(network_group(
                    ctx,
                    &link,
                    &modem,
                    &list,
                    modem_state,
                    modes_shown,
                ))
                .child(sim_group(ctx, modem_state, &modem));
        }
        veiled(page.child(modem_group(&modem)), loading).into()
    }
}

/// The data usage of the mobile network, below the Mobile page.
#[derive(PartialEq)]
pub struct MobileUsagePage;

impl Component for MobileUsagePage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        let modem_state = use_modem(true);
        match read_mobile(ctx, modem_state) {
            Ok((modem, link, _)) => UsagePage {
                scope: usage_scope(&link, &modem),
            }
            .into(),
            Err(page) => page,
        }
    }
}

fn status_group(modem: &Modem) -> Group {
    let signal = match modem.signal {
        Some(percent) => format!("{} · {percent}%", modem.signal_label()),
        None => "—".to_string(),
    };
    Group::new(None::<String>)
        .row(
            property_row("Carrier", modem.carrier().unwrap_or_else(|| "—".into()))
                .leading_signal(modem.bars(), false),
        )
        .row(property_row(
            "Network type",
            modem.technology().unwrap_or("—"),
        ))
        .row(property_row("Signal", signal))
}

/// Whether the modem is registered and roaming.
fn registration_group(modem: &Modem) -> Group {
    Group::titled("Registration")
        .row(property_row("Registration", modem.registration_label()))
        .row(property_row(
            "Roaming",
            if modem.roaming() { "Yes" } else { "No" },
        ))
}

/// Mobile data and data roaming. Roaming is one choice for every profile.
fn data_group(
    ctx: NetCtx,
    link: &Link,
    modem: &Modem,
    list: &[Profile],
    profiles: apn::Profiles,
    shown: State<Option<bool>>,
) -> Group {
    let usable = can_switch(Some(modem), link);
    let toggle = link.clone();
    let data = row("Mobile data")
        .trailing(
            switch(link.data_on() && usable, move |next: bool| {
                set_mobile_data(ctx, &toggle, next)
            })
            .enabled(usable),
        );
    let allowed = shown
        .read()
        .unwrap_or_else(|| list.iter().all(|profile| !profile.home_only));
    let can_roam = usable && !list.is_empty() && !link.airplane;
    let roaming = row("Data roaming")
        .trailing({
            let list = list.to_vec();
            switch(allowed, move |next: bool| {
                let all = list.clone();
                system::optimistic_then(
                    shown,
                    Some(ctx.overlays),
                    None,
                    Some(next),
                    move || apn::set_roaming(&all, next),
                    move || {
                        apn::reload_then(profiles, move || {
                            let mut shown = shown;
                            shown.set(None);
                        });
                        ctx.reload();
                    },
                );
            })
            .enabled(can_roam)
        });
    Group::new(None::<String>).row(data).row(roaming)
}

/// Carrier, preferred network type and access point names.
fn network_group(
    ctx: NetCtx,
    link: &Link,
    modem: &Modem,
    list: &[Profile],
    modem_state: ModemState,
    shown: State<Option<String>>,
) -> Group {
    let _ = link;
    let choices = modem.mode_choices();
    let current = modem
        .current
        .as_ref()
        .map(|modes| modes.allowed.clone())
        .map(|allowed| modem::mode_key(&allowed))
        .unwrap_or_default();
    let selected = shown.read().clone().unwrap_or(current);
    let options: Vec<(String, String)> = choices
        .iter()
        .map(|choice| (choice.key(), choice.label.clone()))
        .collect();
    let id = modem.id.clone();
    let preferred = choice_row(
        "Preferred network type",
        options,
        &selected,
        move |key: String| {
            let Some(choice) = choices.iter().find(|choice| choice.key() == key).cloned() else {
                return;
            };
            let id = id.clone();
            system::optimistic_then(
                shown,
                Some(ctx.overlays),
                None,
                Some(key),
                move || modem::set_modes(&id, &choice.modes),
                move || {
                    reload_modem_then(modem_state, move || {
                        let mut shown = shown;
                        shown.set(None);
                    })
                },
            );
        },
    );
    let carrier_value = if apn::pinned(list) {
        modem.carrier().unwrap_or_else(|| "Manual".into())
    } else {
        "Automatic".to_string()
    };
    let apn_value = apn::selected(list).map_or_else(
        || "None".to_string(),
        |profile| {
            if profile.apn.is_empty() && profile.auto_config {
                "Automatic".to_string()
            } else if profile.apn.is_empty() {
                "None".to_string()
            } else {
                profile.apn.clone()
            }
        },
    );
    Group::titled("Network")
        .row(
            row("Network")
                .value(carrier_value)
                .navigates()
                .on_press(move |_| ctx.router.open(routes::mobile_network())),
        )
        .row(preferred)
        .row(
            row("Access point names")
                .value(apn_value)
                .navigates()
                .on_press(move |_| ctx.router.open(routes::mobile_apns())),
        )
}

/// The SIM, its PIN lock and the PIN change.
fn sim_group(ctx: NetCtx, modem_state: ModemState, modem: &Modem) -> Group {
    let mut group = Group::titled("SIM");
    if let Some(sim) = &modem.sim {
        if !sim.operator.is_empty() {
            group = group.row(property_row("Operator", sim.operator.clone()));
        }
        if !sim.iccid.is_empty() {
            group = group.row(property_row("SIM number", sim.iccid.clone()));
        }
    }
    let Some(sim_id) = modem.sim_id.clone() else {
        return group;
    };
    let lock = sim_id.clone();
    group = group.row(
        row("Lock SIM with PIN")
            .trailing(switch(modem.pin_lock, move |on: bool| {
                let task = if on {
                    PinTask::Enable
                } else {
                    PinTask::Disable
                };
                sim::open(ctx, modem_state, task, lock.clone());
            })),
    );
    if modem.pin_lock {
        group = group.row(
            row("Change PIN…")
                .navigates()
                .on_press(move |_| sim::open(ctx, modem_state, PinTask::Change, sim_id.clone())),
        );
    }
    group
}

/// What a locked SIM shows: the way to unlock it, and nothing else.
fn unlock_groups(ctx: NetCtx, modem_state: ModemState, modem: &Modem) -> Vec<Group> {
    let note =
        |text: String| Group::new(None::<String>).row(Element::from(status(Status::Info(text))));
    let attempts = |task: PinTask| {
        modem
            .attempts_left()
            .map(|left| sim::attempts_text(task, left))
    };
    let (Some(sim_id), true) = (
        modem.sim_id.clone(),
        matches!(modem.lock, Lock::Pin | Lock::Puk),
    ) else {
        let code = match &modem.lock {
            Lock::Other(code) => format!("The SIM asks for a code Settings cannot enter ({code})."),
            _ => "The SIM is locked.".to_string(),
        };
        return vec![note(code)];
    };
    let (text, task, title) = if modem.lock == Lock::Puk {
        (
            "The SIM is blocked after too many wrong PINs.",
            PinTask::Puk,
            "Unlock with PUK",
        )
    } else {
        (
            "The SIM is locked with a PIN.",
            PinTask::Unlock,
            "Unlock with PIN",
        )
    };
    vec![
        note(text.to_string()),
        Group::titled("SIM").row(
            row(title)
                .maybe_subtitle(attempts(task))
                .navigates()
                .on_press(move |_| sim::open(ctx, modem_state, task, sim_id.clone())),
        ),
    ]
}

/// What the mobile network's usage is counted for: its interface, else everything.
fn usage_scope(link: &Link, modem: &Modem) -> UsageScope {
    let interface = modem.interface_or(link.device.as_deref().unwrap_or_default());
    if interface.is_empty() {
        UsageScope::All
    } else {
        UsageScope::Interface(interface)
    }
}

fn usage_group(ctx: NetCtx, link: &Link, modem: &Modem) -> Group {
    Group::new(None::<String>).row(scope_usage_row(
        ctx,
        usage_scope(link, modem),
        routes::mobile_usage(),
    ))
}

/// What the modem is.
fn modem_group(modem: &Modem) -> Group {
    let mut group = Group::titled("Modem");
    let facts = [
        ("Manufacturer", modem.manufacturer.clone()),
        ("Model", modem.model.clone()),
        ("Firmware", modem.revision.clone()),
        ("IMEI", modem.imei.clone()),
        ("Phone number", modem.numbers.join(", ")),
        ("Port", modem.primary_port.clone()),
    ];
    for (title, value) in facts {
        if !value.is_empty() {
            group = group.row(property_row(title, value));
        }
    }
    group
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> Link {
        Link {
            present: true,
            airplane: false,
            radio: true,
            blocked: false,
            device: Some("wwan0".into()),
            connected: true,
            connection: "Vodafone".into(),
        }
    }

    fn parsed(json: &str) -> Modem {
        let mut modem = modem::parse_modem(json).unwrap();
        if modem.sim_id.is_some() {
            modem.sim = Some(modem::parse_sim(modem::fixtures::SIM).unwrap());
        }
        modem
    }

    #[test]
    fn status_lines() {
        let ok = parsed(modem::fixtures::MODEM_OK);
        assert_eq!(status_line(Some(&ok), &link()), "Vodafone UK · 5G");
        let roaming = parsed(modem::fixtures::MODEM_ROAMING);
        assert_eq!(
            status_line(Some(&roaming), &link()),
            "Orange F · 4G · Roaming"
        );
        let no_sim = parsed(modem::fixtures::MODEM_NO_SIM);
        assert_eq!(status_line(Some(&no_sim), &link()), "No SIM card");
        let locked = parsed(modem::fixtures::MODEM_LOCKED);
        assert_eq!(status_line(Some(&locked), &link()), "SIM locked");
        let off = Link {
            radio: false,
            ..link()
        };
        assert_eq!(status_line(Some(&ok), &off), "Off");
        let airplane = Link {
            airplane: true,
            ..link()
        };
        assert_eq!(status_line(Some(&ok), &airplane), "Off in airplane mode");
    }

    #[test]
    fn status_line_before_modemmanager_answers() {
        assert_eq!(status_line(None, &link()), "Vodafone");
        let idle = Link {
            connected: false,
            ..link()
        };
        assert_eq!(status_line(None, &idle), "Not connected");
        let off = Link {
            radio: false,
            ..link()
        };
        assert_eq!(status_line(None, &off), "Off");
    }

    #[test]
    fn an_unregistered_modem_says_so() {
        let mut modem = parsed(modem::fixtures::MODEM_OK);
        modem.registration = "searching".into();
        assert_eq!(
            status_line(Some(&modem), &link()),
            "Searching for a network"
        );
        modem.registration = "denied".into();
        assert_eq!(status_line(Some(&modem), &link()), "Registration refused");
        modem.registration = "idle".into();
        assert_eq!(status_line(Some(&modem), &link()), "Not registered");
    }

    #[test]
    fn switches_need_a_usable_sim() {
        let ok = parsed(modem::fixtures::MODEM_OK);
        assert!(can_switch(Some(&ok), &link()));
        assert!(can_switch(None, &link()));
        assert!(!can_switch(
            Some(&parsed(modem::fixtures::MODEM_NO_SIM)),
            &link()
        ));
        assert!(!can_switch(
            Some(&parsed(modem::fixtures::MODEM_LOCKED)),
            &link()
        ));
        assert!(!can_switch(
            Some(&ok),
            &Link {
                airplane: true,
                ..link()
            }
        ));
        assert!(!can_switch(
            Some(&ok),
            &Link {
                blocked: true,
                ..link()
            }
        ));
    }

    #[test]
    fn the_ready_modem_device() {
        let output = "enp42s0:ethernet:connected\ncdc-wdm0:gsm:unavailable\n";
        assert_eq!(ready_modem_device(output), None);
        let ready = "enp42s0:ethernet:connected\ncdc-wdm0:gsm:disconnected\n";
        assert_eq!(ready_modem_device(ready), Some("cdc-wdm0".to_string()));
        assert_eq!(ready_modem_device(""), None);
    }
}
