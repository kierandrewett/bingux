//! Wi-Fi: one screen for the radio, the connected network, the networks in
//! range and the ways to join others; a screen for one network and its
//! settings; the saved networks; and the hotspot. Everything comes from
//! NetworkManager (`nmcli`). The lists are read when a screen opens and again
//! every few seconds while it stays open.
//!
//! The radio and airplane state is the Network page's shared overview, so a
//! switch looks the same on every screen; the lists of networks and saved
//! profiles are this module's own (`nm`).

mod hotspot;
mod join;
mod nm;
mod share;

use std::time::Duration;

use freya::prelude::*;

use super::model::{self, NetCtx, Overview, use_net_ctx};
use super::profile::ProfileSettings;
use super::{Owner, UsageScope};
use crate::icons::{self, Icon};
use crate::pages::loading::veiled;
use crate::routes::Route;
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, PageHead, ResponseKind, SpinnerSize, Status, Tokens, action_button, action_strip,
    caption, dialog, icon_button, page_column, row, spinner, status, strip_action, use_page_busy,
    use_tokens,
};
use bingux_ui::layout::hstack;
use bingux_ui::shell::{MenuItem, menu_item, menu_separator};
use bingux_ui::style::*;

use nm::{Model, Network, Saved, Security};

pub use hotspot::HotspotPage;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Where the lists are cached between launches. A fake never shares the real
/// one's file.
fn cache_key() -> &'static str {
    if nm::faked() { "wifi-fake" } else { "wifi" }
}

type Loaded = State<Option<Result<Model, CommandError>>>;

/// The lists, shared by every Wi-Fi screen and alive for the whole window, so
/// a popup can reload them after the screen that opened it is gone.
fn model_state() -> Loaded {
    system::cached_persistent::<Model, CommandError>(cache_key())
}

fn reload_model() {
    reload_model_then(|| {});
}

/// Read the lists and, once they are stored, run `done` on the UI thread.
fn reload_model_then(done: impl FnOnce() + 'static) {
    background_then(
        || system::load_persistent(cache_key(), nm::load),
        move |loaded| {
            let mut state = model_state();
            state.set(Some(loaded));
            done();
        },
    );
}

/// Read the lists and the radio state again, for every open screen.
fn reload_all() {
    reload_model();
    model::reload_overview(system::cached_persistent::<Overview, CommandError>(
        "network-overview",
    ));
}

/// Start loading the lists before a Wi-Fi screen is opened.
pub fn prefetch() {
    system::prefetch_persistent(cache_key(), nm::load);
}

/// Where the Wi-Fi radio stands, which decides what a screen can offer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Radio {
    /// No Wi-Fi hardware.
    Missing,
    Airplane,
    /// Hardware that is switched off (a key or a firmware block).
    Blocked,
    Off,
    On,
}

/// The radio is on and nothing blocks it, as NetworkManager reports: the
/// optimistic state of a switch that is still changing does not count.
fn radio_is_on(ov: &Overview) -> bool {
    ov.wifi_usable() && ov.wifi_on && !ov.airplane
}

fn radio_of(ov: &Overview) -> Radio {
    let airplane = model::pending_for("airplane").unwrap_or(ov.airplane);
    if !ov.wifi_present() {
        Radio::Missing
    } else if airplane {
        Radio::Airplane
    } else if !ov.wifi_usable() {
        Radio::Blocked
    } else if !model::pending_for("wifi").unwrap_or(ov.wifi_on) {
        Radio::Off
    } else {
        Radio::On
    }
}

/// The shape of a loaded screen with blank values, drawn under the veil while
/// the first read runs, so every group and row is already placed.
fn placeholder_model() -> Model {
    let blank = |in_use| Network {
        ssid: "—".to_string(),
        signal: 0,
        security: Security::Wpa2,
        in_use,
        saved: false,
        band: String::new(),
        rate: String::new(),
        bssid: String::new(),
    };
    let profile = |active| Saved {
        name: "—".to_string(),
        uuid: String::new(),
        ssid: "—".to_string(),
        autoconnect: true,
        priority: 0,
        ap: false,
        active,
        key_mgmt: "wpa-psk".to_string(),
        metered: String::new(),
        band: String::new(),
    };
    Model {
        networks: vec![blank(true), blank(false), blank(false)],
        saved: vec![profile(true), profile(false)],
    }
}

/// What a screen draws from, with blanks while it loads.
struct Data {
    ov: Overview,
    model: Model,
    loading: bool,
    /// Why NetworkManager could not be asked, when it could not.
    error: Option<CommandError>,
}

fn data(ui: Ui) -> Data {
    let mut error = None;
    let mut loading = false;
    let ov = match ui.ctx.overview.read().clone() {
        Some(Ok(ov)) => ov,
        Some(Err(failure)) => {
            error = Some(failure);
            super::placeholder()
        }
        None => {
            loading = true;
            super::placeholder()
        }
    };
    let model = match ui.model.read().clone() {
        Some(Ok(model)) => model,
        Some(Err(failure)) => {
            error.get_or_insert(failure);
            placeholder_model()
        }
        None => {
            loading = true;
            placeholder_model()
        }
    };
    Data {
        ov,
        model,
        loading,
        error,
    }
}

/// The page's shared state. Copy, so every event handler owns a copy.
#[derive(Clone, Copy)]
struct Ui {
    ctx: NetCtx,
    model: Loaded,
    /// What a one-off action is doing, such as "Forgetting the network.".
    busy: State<Option<String>>,
    scanning: State<bool>,
    /// Whether this screen has seen a read that started after it opened. The
    /// cached lists a screen starts from may be old (another launch, another
    /// place), so nothing may conclude "this network is gone" from them.
    fresh: State<bool>,
}

/// Rows and popups read the page's state themselves, so their props never
/// change; the state handles carry the updates.
impl PartialEq for Ui {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

fn use_ui() -> Ui {
    Ui {
        ctx: use_net_ctx(),
        model: use_hook(model_state),
        busy: use_state(|| None::<String>),
        scanning: use_state(|| false),
        fresh: use_state(|| false),
    }
}

fn connecting_label(ssid: &str) -> String {
    format!("Connecting to {ssid}.")
}

impl Ui {
    fn reload_all(self) {
        reload_all();
    }

    /// Whether the radio is on, as NetworkManager has it. Read with `peek`:
    /// this runs from effects, which would otherwise re-run whenever the
    /// state they looked at changed.
    fn radio_on(self) -> bool {
        matches!(&*self.ctx.overview.peek(), Some(Ok(ov)) if radio_is_on(ov))
    }

    /// Read the lists now and, when `scan` and the radio can, ask for a fresh
    /// scan whose result is read when it ends. A hotspot is never scanned
    /// over: a scan on its radio would drop the devices on it.
    fn refresh(self, scan: bool) {
        let mut fresh = self.fresh;
        reload_model_then(move || fresh.set(true));
        let hotspot = matches!(&*self.model.peek(), Some(Ok(model)) if model.hotspot().is_some());
        if !scan
            || hotspot
            || !self.radio_on()
            || *self.scanning.peek()
            || self.busy.peek().is_some()
        {
            return;
        }
        let mut scanning = self.scanning;
        scanning.set(true);
        background_then(nm::rescan, move |()| {
            scanning.set(false);
            reload_model();
        });
    }

    /// Run one action in the background with a busy line, then read again. A
    /// failure is a toast; `on_success` runs only on success.
    fn act(
        self,
        label: &str,
        work: impl FnOnce() -> Result<(), CommandError> + Send + 'static,
        on_success: impl FnOnce() + 'static,
    ) {
        if self.busy.read().is_some() {
            return;
        }
        let mut busy = self.busy;
        busy.set(Some(label.to_string()));
        background_then(work, move |result| {
            let mut busy = self.busy;
            busy.set(None);
            match result {
                Ok(()) => on_success(),
                Err(error) => self.ctx.overlays.toast(error.0),
            }
            reload_all();
        });
    }

    fn disconnect(self, uuid: String) {
        self.act("Disconnecting.", move || nm::disconnect(&uuid), || {});
    }

    /// Join a network: at once when it is saved or open, after asking for
    /// what it needs otherwise.
    fn join(self, target: &Join) {
        let ssid = target.ssid.clone();
        let label = connecting_label(&ssid);
        let window = self.ctx.window;
        match (&target.uuid, target.security) {
            (Some(uuid), _) => {
                let uuid = uuid.clone();
                self.act(&label, move || nm::connect_saved(&uuid), || {});
            }
            (None, Security::Enterprise) => join::open_enterprise(window, ssid),
            (None, security) if security.needs_password() => {
                join::open_password(window, ssid, security)
            }
            (None, _) => self.act(&label, move || nm::connect_new(&ssid, None), || {}),
        }
    }

    /// Copy a saved network's password to the clipboard, and say so.
    fn copy_password(self, saved: &Saved) {
        let (uuid, key_mgmt) = (saved.uuid.clone(), saved.key_mgmt.clone());
        background_then(
            move || nm::saved_secret(&uuid, &key_mgmt),
            move |result| match result {
                Ok(Some(password)) => match freya::clipboard::Clipboard::set(password) {
                    Ok(()) => self.ctx.overlays.toast("Copied."),
                    Err(_) => self.ctx.overlays.toast("The password could not be copied."),
                },
                Ok(None) => self
                    .ctx
                    .overlays
                    .toast("This network has no saved password."),
                Err(error) => self.ctx.overlays.toast(error.0),
            },
        );
    }

    fn share(self, target: share::Target) {
        share::open(self.ctx.window, target);
    }

    /// Ask first: forgetting drops the saved password. `leave` goes back a
    /// level once the network is gone, for the screen that is about it.
    fn confirm_forget(self, saved: &Saved, leave: bool) {
        let (name, uuid) = (saved.name.clone(), saved.uuid.clone());
        let navigator = self.ctx.navigator;
        self.ctx.overlays.show_dialog(
            dialog(
                format!("Forget \"{name}\"?"),
                "The saved password is removed. Connecting again asks for it.",
            )
            .cancel("Cancel")
            .response("Forget", ResponseKind::Destructive, move |_| {
                let uuid = uuid.clone();
                self.act(
                    "Forgetting the network.",
                    move || nm::forget(&uuid),
                    move || {
                        if leave {
                            navigator.pop();
                        }
                    },
                );
            }),
        );
    }
}

/// Keep the lists the Wi-Fi row draws from fresh: read them now and every few
/// seconds while the calling screen stays mounted, but only while the radio is
/// on and usable. It reads NetworkManager's cached scan and never starts one,
/// so it is cheap enough for the Network page. Call it from `render`.
pub fn use_row_refresh() {
    let overview = model::use_overview();
    use_hook(move || {
        let refresh = move || {
            if matches!(&*overview.peek(), Some(Ok(ov)) if radio_is_on(ov)) {
                reload_model();
            }
        };
        refresh();
        spawn(async move {
            loop {
                timer(POLL_INTERVAL).await;
                refresh();
            }
        });
    });
}

/// Read the lists now and every few seconds while the screen is open.
fn use_polling(ui: Ui, scan: bool) {
    use_hook(move || {
        ui.refresh(scan);
        spawn(async move {
            loop {
                timer(POLL_INTERVAL).await;
                ui.refresh(scan);
            }
        });
    });
}

/// What it takes to join a network from a row or a screen.
#[derive(Clone)]
struct Join {
    ssid: String,
    /// The saved profile that joins it.
    uuid: Option<String>,
    security: Security,
}

fn join_target(model: &Model, network: &Network) -> Join {
    Join {
        ssid: network.ssid.clone(),
        uuid: model
            .joinable()
            .find(|saved| saved.ssid == network.ssid)
            .map(|saved| saved.uuid.clone()),
        security: network.security,
    }
}

/// The share popup's subject for a saved profile.
fn share_saved(saved: &Saved) -> share::Target {
    share::Target {
        ssid: saved.ssid.clone(),
        uuid: Some(saved.uuid.clone()),
        key_mgmt: saved.key_mgmt.clone(),
    }
}

/// Whether a saved profile can be shared as a code.
fn shareable(saved: &Saved) -> bool {
    share::Kind::of(&saved.key_mgmt).is_some()
}

/// The icon for a signal strength, large.
fn signal_icon(signal: u8) -> Icon {
    match nm::signal_level(signal) {
        4 => Icon::NetworkWirelessSignalExcellent,
        3 => Icon::NetworkWirelessSignalGood,
        2 => Icon::NetworkWirelessSignalOk,
        1 => Icon::NetworkWirelessSignalWeak,
        _ => Icon::NetworkWirelessSignalNone,
    }
}

/// The small lock shown on a secured network.
fn lock_icon(tokens: Tokens) -> Element {
    Element::from(icons::icon(
        Icon::NetworkWirelessEncrypted,
        IconSize::Control,
        Paint::Muted.color(&tokens),
    ))
}

/// The text of a value, or a dash when the network does not report one.
fn or_dash(text: &str) -> String {
    if text.is_empty() {
        "—".to_string()
    } else {
        text.to_string()
    }
}

/// "1 network nearby" or "N networks nearby".
fn network_count(count: usize) -> String {
    if count == 1 {
        "1 network nearby".to_string()
    } else {
        format!("{count} networks nearby")
    }
}

/// The right-hand column of a network row. Every row has one, so the icons
/// sit in the same place whether or not a row has them.
fn action_slot(action: Option<Element>) -> Element {
    rect()
        .tw_layout(&[
            w(Dim::Box24),
            flex_row(),
            justify(Align::End),
            items(Align::Center),
        ])
        .maybe_child(action)
        .into()
}

/// A screen that could not ask NetworkManager: the reason and Try again.
fn failure(ui: Ui, error: CommandError, head: Option<PageHead>) -> Rect {
    page_column()
        .maybe_child(head)
        .child(status(Status::Error(error.0)).action("Try again", move |_| ui.reload_all()))
}

/// The right-click menu of a network: connect or disconnect, what a saved
/// network offers, and its screen.
fn network_menu(ui: Ui, model: &Model, network: &Network) -> Vec<MenuItem> {
    let profile = model.joinable().find(|saved| saved.ssid == network.ssid);
    let mut items = Vec::new();
    match (network.in_use, profile) {
        (true, Some(saved)) => {
            let uuid = saved.uuid.clone();
            items.push(
                menu_item("Disconnect", move |_| ui.disconnect(uuid.clone()))
                    .icon(Icon::WindowClose),
            );
        }
        (true, None) => {}
        (false, _) => {
            let target = join_target(model, network);
            items.push(
                menu_item("Connect", move |_| ui.join(&target))
                    .icon(Icon::NetworkWirelessSignalExcellent),
            );
        }
    }
    if let Some(saved) = profile {
        items.push(menu_separator());
        items.extend(saved_items(ui, saved, false));
    } else if network.security == Security::Open {
        let target = share::Target {
            ssid: network.ssid.clone(),
            uuid: None,
            key_mgmt: String::new(),
        };
        items.push(menu_separator());
        items.push(menu_item("Share", move |_| ui.share(target.clone())).icon(Icon::SendTo));
    }
    items.push(menu_separator());
    let key = profile.map_or_else(|| network.ssid.clone(), |saved| saved.name.clone());
    items.push(
        menu_item("Network details", move |_| {
            ui.ctx
                .navigator
                .push(Route::WifiNetwork { ssid: key.clone() })
        })
        .icon(Icon::PreferencesSystem),
    );
    items
}

/// The actions every saved network offers: copy its password, share it and
/// forget it. `leave` says whether forgetting also leaves the screen it is
/// opened from.
fn saved_items(ui: Ui, saved: &Saved, leave: bool) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if saved.security().needs_password() {
        let copy = saved.clone();
        items.push(
            menu_item("Copy password", move |_| ui.copy_password(&copy)).icon(Icon::EditCopy),
        );
    }
    if shareable(saved) {
        let target = share_saved(saved);
        items.push(menu_item("Share", move |_| ui.share(target.clone())).icon(Icon::SendTo));
    }
    let forget = saved.clone();
    items.push(
        menu_item("Forget", move |_| ui.confirm_forget(&forget, leave))
            .icon(Icon::UserTrash)
            .destructive(),
    );
    items
}

/// The sentence at the top of the Wi-Fi screen.
fn wifi_subtitle(radio: Radio, model: &Model) -> String {
    match radio {
        Radio::Missing => "No Wi-Fi hardware".to_string(),
        Radio::Airplane => "Off in airplane mode".to_string(),
        Radio::Blocked => "Wi-Fi hardware is disabled".to_string(),
        Radio::Off => "Off".to_string(),
        Radio::On => match model.current() {
            Some((Some(network), _)) => format!("Connected to {}", network.ssid),
            Some((None, Some(saved))) => format!("Connected to {}", saved.ssid),
            _ => "Not connected".to_string(),
        },
    }
}

/// `Connected · 5 GHz · 540 Mbit/s · Secured`: what is known of the link.
fn link_line(network: Option<&Network>) -> String {
    let mut parts = vec!["Connected".to_string()];
    if let Some(network) = network {
        for part in [&network.band, &network.rate] {
            if !part.is_empty() {
                parts.push(part.clone());
            }
        }
        parts.push(
            if network.security.secured() {
                "Secured"
            } else {
                "Open"
            }
            .to_string(),
        );
    }
    parts.join(" · ")
}

/// The status group that stands for a Wi-Fi radio that is not on, or `None`
/// while it is.
fn radio_status(ui: Ui, radio: Radio) -> Option<Element> {
    let message = match radio {
        Radio::On => return None,
        Radio::Missing => Status::Empty("This computer has no Wi-Fi hardware.".into()),
        Radio::Blocked => Status::Info("Wi-Fi hardware is disabled.".into()),
        Radio::Airplane => Status::Info(
            "Wi-Fi is off because airplane mode is on. Turn airplane mode off on the Network page."
                .into(),
        ),
        Radio::Off => Status::Empty("Wi-Fi is off. Turn it on to see networks.".into()),
    };
    let block = status(message);
    let block = match radio {
        Radio::Off => block.action("Turn on", move |_| {
            model::toggle_link(ui.ctx, "wifi".to_string(), true, || {
                model::set_wifi_radio(true)
            })
        }),
        Radio::Airplane => block.action("Back", move |_| ui.ctx.navigator.pop()),
        _ => block,
    };
    Some(Element::from(
        Group::new(None::<String>).row(Element::from(block)),
    ))
}

/// The connected network, pinned at the top: its row, and Disconnect, Share
/// and Details under it.
fn connected_block(ui: Ui, model: &Model) -> Option<Element> {
    let (network, profile) = model.current()?;
    let ssid = network
        .map(|n| n.ssid.clone())
        .or_else(|| profile.map(|p| p.ssid.clone()))?;
    let key = profile.map_or_else(|| ssid.clone(), |saved| saved.name.clone());
    let open = key.clone();
    let mut hero = row(ssid.clone())
        .subtitle(link_line(network))
        .navigates()
        .on_press(move |_| {
            ui.ctx
                .navigator
                .push(Route::WifiNetwork { ssid: open.clone() })
        });
    hero = match network {
        Some(network) => {
            hero.leading_signal(nm::signal_level(network.signal), network.security.secured())
        }
        None => hero.leading(Icon::NetworkWirelessSignalExcellent),
    };
    let disconnect = profile.map(|saved| saved.uuid.clone());
    let share_target = profile.filter(|saved| shareable(saved)).map(share_saved);
    let details = key;
    let strip = action_strip(vec![
        strip_action(Icon::WindowClose, "Disconnect", move |_| {
            if let Some(uuid) = &disconnect {
                ui.disconnect(uuid.clone());
            }
        })
        .enabled(profile.is_some()),
        strip_action(Icon::SendTo, "Share", {
            let target = share_target.clone();
            move |_| {
                if let Some(target) = &target {
                    ui.share(target.clone());
                }
            }
        })
        .enabled(share_target.is_some()),
        strip_action(Icon::PreferencesSystem, "Details", move |_| {
            ui.ctx.navigator.push(Route::WifiNetwork {
                ssid: details.clone(),
            })
        }),
    ]);
    Some(Element::from(
        rect()
            .tw_layout(&[flex_col(), w_full(), gap(S2)])
            .child(Group::titled("Connected network").row(hero))
            .child(strip),
    ))
}

/// Whether NetworkManager is bringing `ssid` up now: this screen started it,
/// or the Wi-Fi device says so.
fn connecting_to(ui: Ui, ov: &Overview, ssid: &str, profile: Option<&Saved>) -> bool {
    if ui.busy.read().as_deref() == Some(connecting_label(ssid).as_str()) {
        return true;
    }
    let name = profile.map_or(ssid, |saved| saved.name.as_str());
    ov.wifi_device()
        .is_some_and(|device| device.state.starts_with("connecting") && device.connection == name)
}

/// One row per network in range, the connected one left to its own group.
/// The signal icon leads; a lock marks a secured network; a caption says
/// "Saved"; a spinner shows while it connects; a gear opens its screen.
fn network_rows(ui: Ui, data: &Data, tokens: Tokens) -> Vec<Element> {
    data.model
        .networks
        .iter()
        .filter(|network| !network.in_use)
        .map(|network| {
            let profile = data
                .model
                .joinable()
                .find(|saved| saved.ssid == network.ssid);
            let connecting = connecting_to(ui, &data.ov, &network.ssid, profile);
            let key = profile.map_or_else(|| network.ssid.clone(), |saved| saved.name.clone());

            let mut cluster = rect().tw_layout(&[flex_row(), gap(S2), items(Align::Center)]);
            if connecting {
                cluster = cluster.child(Element::from(spinner(SpinnerSize::Small)));
            }
            if network.security.secured() {
                cluster = cluster.child(lock_icon(tokens));
            }
            cluster = cluster.child(Element::from(
                icon_button(Icon::PreferencesSystem, move |_| {
                    ui.ctx
                        .navigator
                        .push(Route::WifiNetwork { ssid: key.clone() })
                })
                .tooltip("Network details"),
            ));
            let slot = action_slot(Some(Element::from(cluster)));
            let trailing = if profile.is_some() {
                Element::from(hstack(S2).child(caption("Saved")).child(slot))
            } else {
                slot
            };
            let target = join_target(&data.model, network);
            Element::from(
                row(network.ssid.clone())
                    .leading_signal(nm::signal_level(network.signal), network.security.secured())
                    .on_press(move |_| ui.join(&target))
                    .trailing(trailing)
                    .context_menu(network_menu(ui, &data.model, network)),
            )
        })
        .collect()
}

/// The available networks group, with its scan state.
fn available_group(ui: Ui, data: &Data, tokens: Tokens) -> Element {
    let title = "Available networks";
    let scanning = *ui.scanning.read();
    let trailing = if scanning {
        Element::from(spinner(SpinnerSize::Small))
    } else {
        // A blank box the size of the spinner, so the heading does not shift.
        Element::from(rect().tw_layout(&[w(Dim::IconBox), h(Dim::IconBox)]))
    };
    let rows = network_rows(ui, data, tokens);
    if rows.is_empty() {
        let block = if scanning {
            status(Status::Loading("Scanning for Wi-Fi networks…".into()))
        } else if data.model.current().is_some() {
            status(Status::Empty("No other Wi-Fi networks found.".into()))
                .action("Scan again", move |_| ui.refresh(true))
        } else {
            status(Status::Empty("No Wi-Fi networks found.".into()))
                .action("Scan again", move |_| ui.refresh(true))
        };
        return Element::from(
            Group::titled(title)
                .trailing(trailing)
                .row(Element::from(block)),
        );
    }
    // A blank line of the same height holds the heading still until the count
    // is known.
    let count = if data.loading {
        " ".to_string()
    } else {
        network_count(rows.len())
    };
    Element::from(
        Group::titled(title)
            .description(count)
            .trailing(trailing)
            .rows(rows),
    )
}

/// Saved networks, the hidden network popup and the hotspot.
fn manage_group(ui: Ui, data: &Data, radio: Radio) -> Element {
    let saved = data.model.joinable().count();
    let hotspot = data.model.hotspot().map(|ap| format!("On: {}", ap.ssid));
    let window = ui.ctx.window;
    let navigator = ui.ctx.navigator;
    Element::from(
        Group::titled("Manage")
            .row(
                row("Saved networks")
                    .value(if saved == 0 {
                        "None".to_string()
                    } else {
                        saved.to_string()
                    })
                    .navigates()
                    .on_press(move |_| navigator.push(Route::WifiSaved)),
            )
            .row(
                row("Connect to hidden network…")
                    .navigates()
                    .enabled(radio == Radio::On)
                    .on_press(move |_| join::open_hidden(window)),
            )
            .row(
                row("Wi-Fi hotspot")
                    .value(hotspot.unwrap_or_default())
                    .navigates()
                    .on_press(move |_| navigator.push(Route::WifiHotspot)),
            ),
    )
}

/// The Wi-Fi screen.
#[derive(PartialEq)]
pub struct WifiPage;

impl Component for WifiPage {
    fn render(&self) -> impl IntoElement {
        let ui = use_ui();
        let tokens = use_tokens();
        use_polling(ui, true);
        use_page_busy(ui.busy.read().is_some());
        let data = data(ui);
        let radio = radio_of(&data.ov);
        // A radio that has just come on shows its networks as soon as they are
        // scanned, not at the next tick.
        use_side_effect_with_deps(&radio_is_on(&data.ov), move |on| {
            if *on {
                ui.refresh(true);
            }
        });

        // The switch waits for real data, so the placeholder under the veil
        // can never write a guess to the system.
        let mut head = PageHead::new().subtitle(if data.loading {
            "—".to_string()
        } else {
            wifi_subtitle(radio, &data.model)
        });
        if !data.loading && matches!(radio, Radio::On | Radio::Off) {
            head = head.switch("Wi-Fi", radio == Radio::On, move |next: bool| {
                model::toggle_link(ui.ctx, "wifi".to_string(), next, move || {
                    model::set_wifi_radio(next)
                })
            });
        }
        if let Some(error) = data.error.clone() {
            return failure(ui, error, Some(head));
        }

        let mut blocks: Vec<Element> = Vec::new();
        blocks.extend(radio_status(ui, radio));
        if radio == Radio::On {
            blocks.extend(connected_block(ui, &data.model));
            blocks.push(available_group(ui, &data, tokens));
        }
        if radio != Radio::Missing {
            blocks.push(manage_group(ui, &data, radio));
        }
        veiled(page_column().child(head).children(blocks), data.loading)
    }
}

/// The saved Wi-Fi profiles: in range or not, joined automatically or not.
#[derive(PartialEq)]
pub struct SavedNetworksPage;

impl Component for SavedNetworksPage {
    fn render(&self) -> impl IntoElement {
        let ui = use_ui();
        use_polling(ui, false);
        let data = data(ui);
        let radio = radio_of(&data.ov);
        let count = data.model.joinable().count();
        let head = PageHead::new().subtitle(if data.loading {
            "—".to_string()
        } else if count == 1 {
            "1 saved network".to_string()
        } else {
            format!("{count} saved networks")
        });
        if let Some(error) = data.error.clone() {
            return failure(ui, error, Some(head));
        }
        if count == 0 && !data.loading {
            return page_column().child(head).child(status(Status::Empty(
                "No saved networks. A network you join is saved here.".into(),
            )));
        }

        let rows = data.model.joinable().map(|saved| {
            let in_range = data.model.networks.iter().any(|n| n.ssid == saved.ssid);
            let mut parts: Vec<String> = Vec::new();
            if saved.active {
                parts.push("Connected".to_string());
            } else if radio == Radio::On {
                parts.push(if in_range { "In range" } else { "Out of range" }.to_string());
            }
            parts.push(
                if saved.autoconnect {
                    "Connects automatically"
                } else {
                    "Manual"
                }
                .to_string(),
            );
            if saved.priority != 0 {
                parts.push(format!("Priority {}", saved.priority));
            }
            let key = saved.name.clone();
            let mut menu = Vec::new();
            if !saved.active {
                let target = Join {
                    ssid: saved.ssid.clone(),
                    uuid: Some(saved.uuid.clone()),
                    security: saved.security(),
                };
                menu.push(
                    menu_item("Connect", move |_| ui.join(&target))
                        .icon(Icon::NetworkWirelessSignalExcellent),
                );
                menu.push(menu_separator());
            }
            menu.extend(saved_items(ui, saved, false));
            Element::from(
                row(saved.name.clone())
                    .leading_signal(
                        data.model
                            .networks
                            .iter()
                            .find(|n| n.ssid == saved.ssid)
                            .map_or(0, |n| nm::signal_level(n.signal)),
                        saved.security().secured(),
                    )
                    .subtitle(parts.join(" · "))
                    .navigates()
                    .on_press(move |_| {
                        ui.ctx
                            .navigator
                            .push(Route::WifiNetwork { ssid: key.clone() })
                    })
                    .context_menu(menu),
            )
        });
        veiled(
            page_column()
                .child(head)
                .child(Group::new(None::<String>).rows(rows.collect::<Vec<_>>())),
            data.loading,
        )
    }
}

/// One network: its signal, what you can do with it, the facts about it, and
/// (when it is saved) its settings.
#[derive(PartialEq)]
pub struct WifiNetworkPage {
    pub ssid: String,
}

/// Words for what NetworkManager's connectivity check found.
fn internet_words(connectivity: &str) -> Option<&'static str> {
    match connectivity {
        "full" => Some("Yes"),
        "limited" => Some("Limited"),
        "portal" => Some("Sign-in required"),
        "none" => Some("No"),
        _ => None,
    }
}

/// "Automatic", "Yes" or "No" for a profile's metered setting.
fn metered_words(metered: &str) -> &'static str {
    match metered {
        "yes" | "guess-yes" => "Yes",
        "no" | "guess-no" => "No",
        _ => "Automatic",
    }
}

impl Component for WifiNetworkPage {
    fn render(&self) -> impl IntoElement {
        let ui = use_ui();
        let tokens = use_tokens();
        use_polling(ui, true);
        use_page_busy(ui.busy.read().is_some());

        // Leave once the network is neither in range nor saved. The effect
        // reads the name and the lists from its dependencies, so it still
        // judges the right network if this screen is reused for another.
        let fresh = *ui.fresh.read();
        let lists = match &*ui.model.read() {
            Some(Ok(model)) => Some(model.clone()),
            _ => None,
        };
        use_side_effect_with_deps(
            &(self.ssid.clone(), lists, fresh),
            move |(ssid, lists, fresh): &(String, Option<Model>, bool)| {
                if let (true, Some(model)) = (*fresh, lists) {
                    let found = model.resolve(ssid);
                    if found.profile.is_none() && found.network.is_none() {
                        ui.ctx.navigator.pop();
                    }
                }
            },
        );

        let data = data(ui);
        if let Some(error) = data.error.clone() {
            return failure(ui, error, None);
        }
        let radio = radio_of(&data.ov);
        let found = data.model.resolve(&self.ssid);
        let ssid = found.ssid.clone();
        let profile = found.profile;
        let network = found.network;
        let connected =
            network.is_some_and(|n| n.in_use) || profile.is_some_and(|saved| saved.active);
        let connecting = connecting_to(ui, &data.ov, &ssid, profile);
        let radio_words = match radio {
            Radio::On => "out of range",
            Radio::Airplane => "airplane mode is on",
            _ => "Wi-Fi is off",
        };

        let state = if data.loading {
            "—".to_string()
        } else if connecting {
            "Connecting…".to_string()
        } else if connected {
            "Connected".to_string()
        } else if profile.is_some() && network.is_none() {
            format!("Saved · {radio_words}")
        } else if profile.is_some() {
            "Saved".to_string()
        } else if network.is_none() {
            "Out of range".to_string()
        } else {
            "Not saved".to_string()
        };
        let hero = model::hero(
            tokens,
            network.map_or(Icon::NetworkWirelessSignalNone, |n| signal_icon(n.signal)),
            ssid.clone(),
            state,
        );

        let mut actions = Vec::new();
        if connected {
            let uuid = profile.map(|saved| saved.uuid.clone());
            actions.push(
                strip_action(Icon::WindowClose, "Disconnect", move |_| {
                    if let Some(uuid) = &uuid {
                        ui.disconnect(uuid.clone());
                    }
                })
                .enabled(profile.is_some()),
            );
        } else {
            let target = match (network, profile) {
                (Some(network), _) => Some(join_target(&data.model, network)),
                (None, Some(saved)) => Some(Join {
                    ssid: saved.ssid.clone(),
                    uuid: Some(saved.uuid.clone()),
                    security: saved.security(),
                }),
                (None, None) => None,
            };
            actions.push(
                strip_action(Icon::NetworkWirelessSignalExcellent, "Connect", move |_| {
                    if let Some(target) = &target {
                        ui.join(target);
                    }
                })
                .enabled(network.is_some() && radio == Radio::On && !connecting),
            );
        }
        let share_target = match (profile, network) {
            (Some(saved), _) if shareable(saved) => Some(share_saved(saved)),
            (None, Some(network)) if network.security == Security::Open => Some(share::Target {
                ssid: ssid.clone(),
                uuid: None,
                key_mgmt: String::new(),
            }),
            _ => None,
        };
        if let Some(target) = share_target {
            actions.push(strip_action(Icon::SendTo, "Share", move |_| {
                ui.share(target.clone())
            }));
        }
        if let Some(saved) = profile {
            let forget = saved.clone();
            actions.push(strip_action(Icon::UserTrash, "Forget", move |_| {
                ui.confirm_forget(&forget, true)
            }));
        }

        let mut blocks = vec![hero];
        blocks.push(Element::from(action_strip(actions)));

        // A captive portal on this network: the way to sign in.
        if connected && data.ov.connectivity == "portal" {
            let ctx = ui.ctx;
            blocks.push(Element::from(
                Group::new(None::<String>).row(
                    row("Sign-in required")
                        .subtitle(
                            "This network needs a sign-in before it gives access to the internet.",
                        )
                        .trailing(Element::from(action_button(
                            "Open sign-in page",
                            move |_| super::open_portal(ctx),
                        )))
                        .on_press(move |_| super::open_portal(ctx)),
                ),
            ));
        }

        // Five facts always show, with a dash for any the network does not
        // report, so the group keeps its place as the network comes and goes.
        let mut details = Group::titled("Details");
        details = match network {
            Some(network) => details
                .row(row("Signal strength").value(format!(
                    "{} ({}%)",
                    nm::signal_words(network.signal),
                    network.signal
                )))
                .row(row("Frequency").value(or_dash(&network.band)))
                .row(row("Security").value(network.security.label()))
                .row(row("Speed").value(or_dash(&network.rate)))
                .row(row("Access point").mono_value(or_dash(&network.bssid))),
            None => details
                .row(row("Signal strength").value("—"))
                .row(row("Frequency").value("—"))
                .row(row("Security").value(profile.map_or("—", |saved| saved.security().label())))
                .row(row("Speed").value("—"))
                .row(row("Access point").value("—")),
        };
        if connected
            && let Some(device) = data.ov.wifi_device()
            && !device.address4().is_empty()
        {
            details = details.row(row("IP address").mono_value(device.address4()));
        }
        if connected && let Some(words) = internet_words(&data.ov.connectivity) {
            details = details.row(row("Internet access").value(words));
        }
        if let Some(saved) = profile {
            details = details.row(row("Metered connection").value(metered_words(&saved.metered)));
        }
        blocks.push(Element::from(details));

        if let Some(saved) = profile {
            blocks.push(Element::from(ProfileSettings {
                owner: Owner::Wifi {
                    ssid: saved.name.clone(),
                },
            }));
            let name = saved.name.clone();
            let usage = saved.name.clone();
            let mut more = Group::new(None::<String>);
            if connected {
                more = more.row(row("Connection details").navigates().on_press(move |_| {
                    ui.ctx.navigator.push(Route::Details {
                        connection: name.clone(),
                    })
                }));
            }
            more = more.row(row("Data usage").navigates().on_press(move |_| {
                ui.ctx.navigator.push(Route::Usage {
                    scope: UsageScope::Connection(usage.clone()),
                })
            }));
            blocks.push(Element::from(more));
        }
        veiled(page_column().children(blocks), data.loading)
    }
}

/// The signal and band of the network the Wi-Fi device is joined to, from
/// the lists the Wi-Fi screens last read. `None` when those have not been
/// read, or do not agree with the overview (the list is older than the
/// connection).
fn joined_network<'a>(model: &'a Model, connection: &str) -> Option<&'a Network> {
    let ssid = model
        .joinable()
        .find(|saved| saved.name == connection)
        .map_or(connection, |saved| saved.ssid.as_str());
    model
        .networks
        .iter()
        .find(|network| network.in_use && network.ssid == ssid)
}

/// The Wi-Fi row of the Connections group; `None` on a computer with no Wi-Fi
/// hardware.
pub fn wifi_row(ctx: NetCtx, ov: &Overview, airplane: bool) -> Option<Element> {
    if !ov.wifi_present() {
        return None;
    }
    let usable = ov.wifi_usable() && !airplane;
    let on = model::pending_for("wifi").unwrap_or(ov.wifi_on);
    let loaded = model_state().read().clone();
    let known = match &loaded {
        Some(Ok(model)) => Some(model),
        _ => None,
    };
    let device = ov.wifi_device().filter(|device| device.has_connection());
    let hotspot = device.is_some_and(|device| {
        known
            .and_then(Model::hotspot)
            .is_some_and(|ap| ap.name == device.connection)
    });
    let joined = match (device, known) {
        (Some(device), Some(model)) if !hotspot => joined_network(model, &device.connection),
        _ => None,
    };
    let subtitle = if airplane {
        "Off in airplane mode".to_string()
    } else if !ov.wifi_usable() {
        "Wi-Fi hardware is disabled".to_string()
    } else if !on {
        "Off".to_string()
    } else {
        match device {
            Some(device) if hotspot => format!("Hotspot on · {}", device.connection),
            Some(device) if device.state.starts_with("connecting") => {
                format!("Connecting to {}…", device.connection)
            }
            Some(device) if device.connected() => {
                let name = joined.map_or(device.connection.as_str(), |n| n.ssid.as_str());
                match joined
                    .map(|n| n.band.as_str())
                    .filter(|band| !band.is_empty())
                {
                    Some(band) => format!("Connected to {name} · {band}"),
                    None => format!("Connected to {name}"),
                }
            }
            _ => "Not connected".to_string(),
        }
    };
    let mut item = row("Wi-Fi").subtitle(subtitle);
    item = match joined {
        Some(network) if usable && on => {
            item.leading_signal(nm::signal_level(network.signal), network.security.secured())
        }
        _ if usable && on => item.leading(Icon::Network),
        _ => item.leading(Icon::NetworkWirelessDisabled),
    };
    Some(Element::from(
        item.trailing(
            bingux_ui::prelude::switch(on && usable, move |next: bool| {
                model::toggle_link(ctx, "wifi".to_string(), next, move || {
                    model::set_wifi_radio(next)
                })
            })
            .enabled(usable),
        )
        .navigates()
        .on_press(move |_| ctx.navigator.push(Route::Wifi)),
    ))
}

/// The hotspot row of the More group; `None` without usable Wi-Fi hardware.
pub fn hotspot_row(ctx: NetCtx, ov: &Overview) -> Option<Element> {
    ov.wifi_usable().then(|| {
        let loaded = model_state().read().clone();
        // Only a hotspot the device is running now counts: the lists can be older.
        let running = match (&loaded, ov.wifi_device()) {
            (Some(Ok(model)), Some(device)) => model
                .hotspot()
                .filter(|ap| ap.name == device.connection)
                .map(|ap| format!("On: {}", ap.ssid)),
            _ => None,
        };
        Element::from(
            row("Wi-Fi hotspot")
                .value(running.unwrap_or_default())
                .navigates()
                .on_press(move |_| ctx.navigator.push(Route::WifiHotspot)),
        )
    })
}
