//! Wi-Fi: the Wi-Fi and airplane mode switches, the saved networks list, the
//! hidden network and hotspot forms, and the visible networks list. Everything
//! comes from `nmcli`. The lists are read once when the page opens and again
//! every few seconds while it stays open.

use std::time::Duration;

use freya::prelude::*;

use super::wifi_editor::{Editor, Target, use_editor};
use crate::icons::{self, Icon};
use crate::routes::{Navigator, Route, use_navigator};
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{
    Group, Overlays, PageHeader, ResponseKind, SpinnerSize, Status, Tokens, action_strip,
    centered_caption, dialog, heading, icon_button, page_column, row, sheet, skeleton, spinner,
    status, strip_action, switch_row, text_field, use_overlays, use_page_busy, use_page_header,
    use_tokens,
};
use bingux_ui::scale::*;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const QUICK: Duration = Duration::from_secs(5);
/// `device wifi list` takes about four seconds on a busy radio.
const SCAN: Duration = Duration::from_secs(10);
const CONNECT: Duration = Duration::from_secs(90);
/// Busy line shown while a scan runs; the heading shows a spinner for it.
const SCAN_LABEL: &str = "Scanning for Wi-Fi networks.";
/// Width of the right-hand action column in the network list.
const ACTION_WIDTH: f32 = BOX_2XL;
/// The shortest password WPA allows for a hotspot.
const HOTSPOT_PASSWORD_MIN: usize = 8;

#[derive(Clone, PartialEq)]
struct Model {
    /// `enabled`, `disabled` or `missing`, from `nmcli radio`.
    wifi_hw: String,
    wifi_on: bool,
    airplane: bool,
    networks: Vec<Network>,
    /// Names of every saved Wi-Fi profile, in any range.
    saved_names: Vec<String>,
    /// The name of the active hotspot profile, when one is on.
    hotspot: Option<String>,
}

#[derive(Clone, PartialEq)]
struct Network {
    ssid: String,
    signal: u8,
    secured: bool,
    in_use: bool,
    saved: bool,
    /// "2.4 GHz", "5 GHz" or "6 GHz", or empty when unknown.
    band: String,
    rate: String,
}

fn nmcli(args: &[&str], timeout: Duration) -> Result<String, CommandError> {
    system::run("nmcli", args, timeout)
}

fn wifi_usable(hw: &str) -> bool {
    hw != "missing" && hw != "disabled"
}

/// Split one `nmcli -t` line on its unescaped colons, undoing `\:` and `\\`.
fn split_terse(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == ':' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    fields.push(current);
    fields
}

/// Returns `(wifi_hardware, wifi_on, airplane)` from `radio -f WIFI-HW,WIFI,WWAN-HW,WWAN`.
fn parse_radio(output: &str) -> Result<(String, bool, bool), CommandError> {
    let fields = split_terse(output.trim());
    if fields.len() < 4 {
        return Err(CommandError("Wi-Fi status could not be read.".into()));
    }
    let wifi_hw = fields[0].clone();
    let wifi_on = fields[1] == "enabled";
    let wwan_hw = &fields[2];
    let wwan = &fields[3];
    let airplane = !wifi_on && (wwan_hw == "missing" || wwan == "disabled");
    Ok((wifi_hw, wifi_on, airplane))
}

/// Names of the Wi-Fi connection profiles in `NAME,TYPE` output.
fn parse_saved_wifi(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let fields = split_terse(line);
            (fields.get(1).map(String::as_str) == Some("802-11-wireless"))
                .then(|| fields[0].clone())
        })
        .collect()
}

/// Networks from `device wifi list`. Access points that share an SSID are
/// merged: the strongest signal wins, and the network counts as secured or
/// in use if any of its access points is.
fn parse_networks(output: &str, saved: &[String]) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let fields = split_terse(line);
        let field = |index: usize| fields.get(index).map(String::as_str).unwrap_or("");
        let ssid = field(0);
        if ssid.is_empty() {
            continue;
        }
        let signal = field(1).parse::<u8>().unwrap_or(0);
        let security = field(2);
        let secured = !security.is_empty() && security != "--";
        let in_use = field(3).trim() == "*";
        if let Some(existing) = networks.iter_mut().find(|network| network.ssid == ssid) {
            if signal > existing.signal {
                existing.signal = signal;
                existing.band = band_label(field(5));
                existing.rate = field(6).to_string();
            }
            existing.secured |= secured;
            existing.in_use |= in_use;
            continue;
        }
        networks.push(Network {
            ssid: ssid.to_string(),
            signal,
            secured,
            in_use,
            saved: saved.iter().any(|name| name == ssid),
            band: band_label(field(5)),
            rate: field(6).to_string(),
        });
    }
    networks.sort_by(|a, b| {
        b.in_use
            .cmp(&a.in_use)
            .then_with(|| b.signal.cmp(&a.signal))
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    networks
}

/// The name of the active access point profile, if one is running.
fn active_hotspot() -> Option<String> {
    let active = nmcli(
        &["-t", "-f", "NAME,TYPE", "connection", "show", "--active"],
        QUICK,
    )
    .ok()?;
    parse_saved_wifi(&active).into_iter().find(|name| {
        let name = name.as_str();
        nmcli(
            &["-g", "802-11-wireless.mode", "connection", "show", name],
            QUICK,
        )
        .is_ok_and(|mode| mode.trim() == "ap")
    })
}

fn load() -> Result<Model, CommandError> {
    let (wifi_hw, wifi_on, airplane) = parse_radio(&nmcli(
        &["-t", "-f", "WIFI-HW,WIFI,WWAN-HW,WWAN", "radio"],
        QUICK,
    )?)?;
    let saved_names = parse_saved_wifi(
        &nmcli(&["-t", "-f", "NAME,TYPE", "connection", "show"], QUICK).unwrap_or_default(),
    );
    let (networks, hotspot) = if wifi_on && wifi_usable(&wifi_hw) {
        // `--rescan no` returns NetworkManager's cached list at once; a
        // fresh scan can block for ten seconds and the page would sit on
        // "Reading Wi-Fi networks." Refresh and the poll loop rescan.
        let networks = parse_networks(
            &nmcli(
                &[
                    "-t",
                    "-f",
                    "SSID,SIGNAL,SECURITY,IN-USE,BSSID,FREQ,RATE",
                    "device",
                    "wifi",
                    "list",
                    "--rescan",
                    "no",
                ],
                QUICK,
            )?,
            &saved_names,
        );
        (networks, active_hotspot())
    } else {
        (Vec::new(), None)
    };
    Ok(Model {
        wifi_hw,
        wifi_on,
        airplane,
        networks,
        saved_names,
        hotspot,
    })
}

fn toggle_wifi(on: bool) -> Result<(), CommandError> {
    nmcli(&["radio", "wifi", if on { "on" } else { "off" }], QUICK).map(|_| ())
}

fn toggle_airplane(on: bool) -> Result<(), CommandError> {
    nmcli(&["radio", "all", if on { "on" } else { "off" }], QUICK).map(|_| ())
}

fn scan_networks() -> Result<(), CommandError> {
    let _ = nmcli(&["device", "wifi", "rescan"], SCAN);
    Ok(())
}

fn connect_network(ssid: &str, password: Option<&str>) -> Result<(), CommandError> {
    match password {
        Some(password) => nmcli(
            &["device", "wifi", "connect", ssid, "password", password],
            CONNECT,
        ),
        None => nmcli(&["device", "wifi", "connect", ssid], CONNECT),
    }
    .map(|_| ())
}

fn connect_hidden(ssid: &str, password: Option<&str>) -> Result<(), CommandError> {
    match password {
        Some(password) => nmcli(
            &[
                "device", "wifi", "connect", ssid, "password", password, "hidden", "yes",
            ],
            CONNECT,
        ),
        None => nmcli(
            &["device", "wifi", "connect", ssid, "hidden", "yes"],
            CONNECT,
        ),
    }
    .map(|_| ())
}

fn start_hotspot(name: &str, password: &str) -> Result<(), CommandError> {
    nmcli(
        &[
            "device", "wifi", "hotspot", "ssid", name, "password", password,
        ],
        CONNECT,
    )
    .map(|_| ())
}

fn forget_connection(name: &str) -> Result<(), CommandError> {
    nmcli(&["connection", "delete", "id", name], QUICK).map(|_| ())
}

fn down_connection(name: &str) -> Result<(), CommandError> {
    nmcli(&["connection", "down", "id", name], QUICK).map(|_| ())
}

/// Words for a signal strength from its percentage. Only the details view
/// uses them; the list shows the signal icon alone.
fn signal_detail(signal: u8) -> String {
    match signal {
        80..=u8::MAX => "Excellent",
        60..=79 => "Good",
        40..=59 => "Fair",
        _ => "Weak",
    }
    .to_string()
}

/// The large signal icon at the head of the details page.
fn signal_icon(signal: u8) -> Icon {
    match signal_level(signal) {
        4 => Icon::NetworkWirelessSignalExcellent,
        3 => Icon::NetworkWirelessSignalGood,
        2 => Icon::NetworkWirelessSignalOk,
        1 => Icon::NetworkWirelessSignalWeak,
        _ => Icon::NetworkWirelessSignalNone,
    }
}

/// Filled bars for a row, 0 to 4, from the signal percentage.
fn signal_level(signal: u8) -> u8 {
    match signal {
        80..=u8::MAX => 4,
        60..=79 => 3,
        40..=59 => 2,
        1..=39 => 1,
        0 => 0,
    }
}

/// The small lock shown on a secured network.
fn lock_icon(tokens: Tokens) -> Element {
    Element::from(icons::icon(
        Icon::NetworkWirelessEncrypted,
        ICON_MD,
        tokens.muted,
    ))
}

/// "2.4 GHz", "5 GHz" or "6 GHz" from a frequency such as "5180 MHz".
/// Empty when the frequency is missing or in no known band.
fn band_label(freq: &str) -> String {
    let mhz = freq
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<u32>().ok());
    match mhz {
        Some(2400..=2499) => "2.4 GHz",
        Some(4900..=5899) => "5 GHz",
        Some(5925..=7125) => "6 GHz",
        _ => "",
    }
    .to_string()
}

/// The facts the connection editor shows for a visible network.
fn network_facts(network: &Network) -> Vec<(String, String)> {
    let mut facts = vec![
        ("Signal strength".to_string(), signal_detail(network.signal)),
        (
            "Security".to_string(),
            if network.secured {
                "Password protected"
            } else {
                "Open"
            }
            .to_string(),
        ),
    ];
    if !network.band.is_empty() {
        facts.push(("Band".to_string(), network.band.clone()));
    }
    if !network.rate.is_empty() {
        facts.push(("Speed".to_string(), network.rate.clone()));
    }
    facts
}

/// The page's shared state. Copy, so every event handler owns a copy.
#[derive(Clone, Copy)]
struct Ui {
    model: State<Option<Result<Model, CommandError>>>,
    busy: State<Option<String>>,
    /// Shown inside the open sheet, such as "Enter the Wi-Fi password."
    sheet_error: State<Option<String>>,
    password: State<String>,
    show_password: State<bool>,
    hidden_ssid: State<String>,
    hidden_password: State<String>,
    hotspot_name: State<String>,
    hotspot_password: State<String>,
    overlays: Overlays,
    navigator: Navigator,
    editor: Editor,
}

/// Sheet bodies are components that read the page state themselves, so their
/// props never change; the state handles carry the updates.
impl PartialEq for Ui {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

fn use_ui() -> Ui {
    Ui {
        model: system::cached("wifi"),
        busy: use_state(|| None::<String>),
        sheet_error: use_state(|| None::<String>),
        password: use_state(String::new),
        show_password: use_state(|| false),
        hidden_ssid: use_state(String::new),
        hidden_password: use_state(String::new),
        hotspot_name: use_state(String::new),
        hotspot_password: use_state(String::new),
        overlays: use_overlays(),
        navigator: use_navigator(),
        editor: use_editor(),
    }
}

impl Ui {
    fn reload(self) {
        background(self.model, load);
    }

    /// Run one nmcli action in the background with a busy line, then reload
    /// the list. A failure is shown as an error; `on_success` runs only on success.
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
                Ok(()) => {
                    on_success();
                }
                Err(error) => self.overlays.toast(error.0),
            }
            self.reload();
        });
    }

    fn set_wifi(self, on: bool) {
        let label = if on {
            "Turning Wi-Fi on."
        } else {
            "Turning Wi-Fi off."
        };
        self.act(label, move || toggle_wifi(on), || {});
    }

    fn set_airplane(self, on: bool) {
        let label = if on {
            "Turning airplane mode on."
        } else {
            "Turning airplane mode off."
        };
        self.act(label, move || toggle_airplane(on), || {});
    }

    fn rescan(self) {
        self.act(SCAN_LABEL, scan_networks, || {});
    }

    fn connect(self, ssid: String, password: Option<String>) {
        let label = format!("Connecting to {ssid}.");
        self.act(
            &label,
            move || connect_network(&ssid, password.as_deref()),
            || {},
        );
    }

    /// Connect to a network: straight away when it is open or saved, after
    /// asking for the password otherwise.
    fn join(self, network: &Network) {
        if network.saved || !network.secured {
            self.connect(network.ssid.clone(), None);
        } else {
            self.open_join(network.ssid.clone());
        }
    }

    fn disconnect(self, name: String) {
        self.act("Disconnecting.", move || down_connection(&name), || {});
    }

    /// Ask first: forgetting drops the saved password.
    fn confirm_forget(self, name: String) {
        let navigator = self.navigator;
        self.overlays.show_dialog(
            dialog(
                format!("Forget \"{name}\"?"),
                "The saved password is removed. Connecting again asks for it.",
            )
            .cancel("Cancel")
            .response("Forget", ResponseKind::Destructive, move |_| {
                let target = name.clone();
                self.act(
                    "Forgetting the network.",
                    move || forget_connection(&target),
                    move || navigator.pop(),
                );
            }),
        );
    }

    /// The password sheet for a secured network that is not saved yet.
    fn open_join(self, ssid: String) {
        let mut password = self.password;
        password.set(String::new());
        let mut show = self.show_password;
        show.set(false);
        let mut error = self.sheet_error;
        error.set(None);
        let press = ssid.clone();
        self.overlays.show_sheet(
            sheet(ssid.clone(), JoinBody { ui: self, ssid })
                .primary("Connect", move |_| self.join_now(press.clone())),
        );
    }

    fn join_now(self, ssid: String) {
        let password = self.password.read().clone();
        if password.is_empty() {
            self.sheet_error("Enter the Wi-Fi password.");
            return;
        }
        self.overlays.close_sheet();
        self.connect(ssid, Some(password));
    }

    fn sheet_error(self, text: &str) {
        let mut error = self.sheet_error;
        error.set(Some(text.to_string()));
    }

    fn open_hidden(self) {
        let mut ssid = self.hidden_ssid;
        ssid.set(String::new());
        let mut password = self.hidden_password;
        password.set(String::new());
        let mut show = self.show_password;
        show.set(false);
        let mut error = self.sheet_error;
        error.set(None);
        self.overlays.show_sheet(
            sheet("Connect to hidden network", HiddenBody { ui: self })
                .primary("Connect", move |_| self.connect_hidden()),
        );
    }

    fn connect_hidden(self) {
        let ssid = self.hidden_ssid.read().trim().to_string();
        if ssid.is_empty() {
            self.sheet_error("Enter the network name.");
            return;
        }
        let password = self.hidden_password.read().clone();
        let password = (!password.is_empty()).then_some(password);
        self.overlays.close_sheet();
        let label = format!("Connecting to {ssid}.");
        self.act(
            &label,
            move || connect_hidden(&ssid, password.as_deref()),
            || {},
        );
    }

    fn open_hotspot(self) {
        let mut name = self.hotspot_name;
        name.set("Hotspot".to_string());
        let mut password = self.hotspot_password;
        password.set(String::new());
        let mut show = self.show_password;
        show.set(false);
        let mut error = self.sheet_error;
        error.set(None);
        self.overlays.show_sheet(
            sheet("Wi-Fi hotspot", HotspotBody { ui: self })
                .primary("Turn on", move |_| self.start_hotspot()),
        );
    }

    fn start_hotspot(self) {
        let name = self.hotspot_name.read().trim().to_string();
        let password = self.hotspot_password.read().clone();
        if name.is_empty() {
            self.sheet_error("Enter a network name.");
            return;
        }
        if password.chars().count() < HOTSPOT_PASSWORD_MIN {
            self.sheet_error("The password needs at least 8 characters.");
            return;
        }
        self.overlays.close_sheet();
        self.act(
            "Turning on the Wi-Fi hotspot.",
            move || start_hotspot(&name, &password),
            || {},
        );
    }

    fn stop_hotspot(self, name: String) {
        let label = format!("Turning off {name}.");
        self.act(&label, move || down_connection(&name), || {});
    }
}

fn page(children: Vec<Element>) -> impl IntoElement {
    page_column().children(children)
}

/// The right-hand column of a network row. Every row has one, so the icons
/// sit in the same place whether or not a row has them.
fn action_slot(action: Option<Element>) -> Element {
    rect()
        .width(Size::px(ACTION_WIDTH))
        .direction(Direction::Horizontal)
        .main_align(Alignment::End)
        .cross_align(Alignment::Center)
        .maybe_child(action)
        .into()
}

/// "1 network nearby" or "N networks nearby".
fn network_count(count: usize) -> String {
    if count == 1 {
        "1 network nearby".to_string()
    } else {
        format!("{count} networks nearby")
    }
}

/// The password sheet body. It reads the page's state, so it redraws as the
/// fields change.
#[derive(Clone, PartialEq)]
struct JoinBody {
    ui: Ui,
    ssid: String,
}

impl Component for JoinBody {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let submit = self.ssid.clone();
        let field = text_field(ui.password, move |_: String| ui.join_now(submit.clone()));
        let field = field.password();
        let error = ui.sheet_error.read().clone();
        page_column()
            .child(Group::new(None::<String>).row(row("Password").trailing(field)))
            .maybe_child(error.map(|text| Element::from(status(Status::Error(text)))))
    }
}

/// The hidden network sheet body: a name, an optional password and the show toggle.
#[derive(Clone, PartialEq)]
struct HiddenBody {
    ui: Ui,
}

impl Component for HiddenBody {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let name = text_field(ui.hidden_ssid, move |_: String| ui.connect_hidden())
            .placeholder("Network name");
        let password = text_field(ui.hidden_password, move |_: String| ui.connect_hidden())
            .placeholder("Leave empty for an open network");
        let password = password.password();
        let error = ui.sheet_error.read().clone();
        page_column()
            .child(
                Group::new(None::<String>)
                    .row(row("Network name").trailing(name))
                    .row(row("Password").trailing(password)),
            )
            .maybe_child(error.map(|text| Element::from(status(Status::Error(text)))))
    }
}

/// The hotspot sheet body: a network name, a password of at least eight characters and the show toggle.
#[derive(Clone, PartialEq)]
struct HotspotBody {
    ui: Ui,
}

impl Component for HotspotBody {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let name = text_field(ui.hotspot_name, move |_: String| ui.start_hotspot())
            .placeholder("Network name");
        let password = text_field(ui.hotspot_password, move |_: String| ui.start_hotspot())
            .placeholder("At least 8 characters");
        let password = password.password();
        let error = ui.sheet_error.read().clone();
        page_column()
            .child(
                Group::new(None::<String>)
                    .row(row("Network name").trailing(name))
                    .row(row("Password").trailing(password)),
            )
            .maybe_child(error.map(|text| Element::from(status(Status::Error(text)))))
    }
}

/// The visible networks section. Its heading shows a spinner while a scan runs.
fn visible_section(ui: Ui, model: &Model, tokens: Tokens) -> Element {
    let title = "Visible networks";
    if model.wifi_hw == "missing" {
        return Element::from(Group::titled(title).row(Element::from(status(Status::Empty(
            "This computer has no Wi-Fi hardware.".into(),
        )))));
    }
    if !wifi_usable(&model.wifi_hw) {
        return Element::from(Group::titled(title).row(Element::from(status(Status::Info(
            "Wi-Fi hardware is disabled.".into(),
        )))));
    }
    if !model.wifi_on {
        let text = if model.airplane {
            "Wi-Fi is off in airplane mode."
        } else {
            "Wi-Fi is off."
        };
        return Element::from(
            Group::titled(title).row(Element::from(status(Status::Empty(text.into())))),
        );
    }

    let scanning = ui.busy.read().as_deref() == Some(SCAN_LABEL);
    let trailing = if scanning {
        Element::from(spinner(SpinnerSize::Small))
    } else {
        Element::from(rect().width(Size::px(ICON_MD)).height(Size::px(ICON_MD)))
    };
    if model.networks.is_empty() {
        return Element::from(
            Group::titled(title)
                .trailing(trailing)
                .row(Element::from(status(Status::Empty(
                    "No Wi-Fi networks found.".into(),
                )))),
        );
    }
    Element::from(
        Group::titled(title)
            .description(network_count(model.networks.len()))
            .trailing(trailing)
            .rows(network_rows(ui, &model.networks, tokens)),
    )
}

/// One row per network. The signal icon leads; a lock marks a secured network;
/// a check mark marks the connected network and the subtitle says "Saved" for a
/// saved one. A gear opens the connection editor for a network that is connected
/// or saved.
fn network_rows(ui: Ui, networks: &[Network], tokens: Tokens) -> Vec<Element> {
    networks
        .iter()
        .map(|network| {
            let mut item = row(network.ssid.clone())
                .leading_signal(signal_level(network.signal), network.secured);
            let connecting = ui.busy.read().as_deref()
                == Some(format!("Connecting to {}.", network.ssid).as_str());
            if network.in_use {
                item = item.selected(true);
            } else if network.saved {
                item = item.subtitle("Saved");
            }

            let mut cluster = rect()
                .direction(Direction::Horizontal)
                .spacing(S2)
                .cross_align(Alignment::Center);
            if connecting {
                cluster = cluster.child(Element::from(spinner(SpinnerSize::Small)));
            }
            if network.secured {
                cluster = cluster.child(lock_icon(tokens));
            }
            if network.in_use || network.saved {
                let ssid = network.ssid.clone();
                cluster = cluster.child(Element::from(
                    icon_button(Icon::PreferencesSystem, move |_| {
                        ui.navigator.push(Route::Network { ssid: ssid.clone() })
                    })
                    .tooltip("Network details"),
                ));
            }

            let press = network.clone();
            let item = item.on_press(move |_| {
                if press.in_use {
                    ui.navigator.push(Route::Network {
                        ssid: press.ssid.clone(),
                    });
                } else {
                    ui.join(&press);
                }
            });
            Element::from(item.trailing(action_slot(Some(Element::from(cluster)))))
        })
        .collect()
}

/// The hotspot row: the state and a turn-off row while one runs, or the form otherwise.
fn hotspot_row(ui: Ui, model: &Model) -> Element {
    match &model.hotspot {
        Some(name) => {
            let stop = name.clone();
            Element::from(
                row("Turn off Wi-Fi hotspot")
                    .subtitle(format!("On: {name}"))
                    .on_press(move |_| ui.stop_hotspot(stop.clone())),
            )
        }
        None => Element::from(
            row("Turn on Wi-Fi hotspot...")
                .navigates()
                .on_press(move |_| ui.open_hotspot()),
        ),
    }
}
/// The header line: the connected network, or whether Wi-Fi is off.
fn wifi_subtitle(model: Option<&Result<Model, CommandError>>) -> String {
    match model {
        Some(Ok(model)) if !model.wifi_on => "Off".to_string(),
        Some(Ok(model)) => match model.networks.iter().find(|network| network.in_use) {
            Some(network) => format!("Connected to {}", network.ssid),
            None => "Not connected".to_string(),
        },
        _ => String::new(),
    }
}

/// Read the lists now and every few seconds while the page is open.
fn use_polling(ui: Ui) {
    use_hook(move || {
        ui.reload();
        spawn(async move {
            loop {
                timer(POLL_INTERVAL).await;
                ui.rescan();
            }
        });
    });
}

#[derive(PartialEq)]
pub struct WifiPage;

impl Component for WifiPage {
    fn render(&self) -> impl IntoElement {
        let ui = use_ui();
        let tokens = use_tokens();
        use_polling(ui);
        use_page_busy(ui.busy.read().is_some());
        // The master switch lives in the header bar, as in GNOME.
        let master = match ui.model.read().as_ref() {
            Some(Ok(model)) if wifi_usable(&model.wifi_hw) => Some(model.wifi_on),
            _ => None,
        };
        let mut header = PageHeader {
            subtitle: Some(wifi_subtitle(ui.model.read().as_ref())),
            action: None,
        };
        if let Some(on) = master {
            header = header.switch("Wi-Fi", on, move |next: bool| ui.set_wifi(next));
        }
        use_page_header(header);

        let loaded = ui.model.read().clone();
        let Some(loaded) = loaded else {
            return page(vec![skeleton()]);
        };
        let model = match loaded {
            Ok(model) => model,
            Err(error) => {
                return page(vec![Element::from(
                    status(Status::Error(error.0)).action("Try again", move |_| ui.reload()),
                )]);
            }
        };

        let mut blocks: Vec<Element> = Vec::new();

        let usable = wifi_usable(&model.wifi_hw);
        let mut items: Vec<Element> = Vec::new();
        if !usable {
            items.push(Element::from(
                row("Wi-Fi")
                    .subtitle("Wi-Fi is not available on this computer.")
                    .enabled(false),
            ));
        } else if !model.wifi_on {
            items.push(Element::from(
                row("Wi-Fi is off").subtitle("Use the switch at the top to turn it on."),
            ));
        }
        if model.wifi_on && usable {
            let navigator = ui.navigator;
            items.push(Element::from(
                row("Saved networks")
                    .navigates()
                    .on_press(move |_| navigator.push(Route::WifiSaved)),
            ));
            items.push(Element::from(
                row("Connect to hidden network...")
                    .navigates()
                    .on_press(move |_| ui.open_hidden()),
            ));
            items.push(hotspot_row(ui, &model));
        }
        blocks.push(Element::from(Group::new(None::<String>).rows(items)));
        blocks.push(Element::from(Group::new(None::<String>).row(switch_row(
            "Airplane mode",
            model.airplane,
            move |on: bool| ui.set_airplane(on),
        ))));

        blocks.push(visible_section(ui, &model, tokens));

        page(blocks)
    }
}

/// The saved Wi-Fi profiles. A press opens the connection editor for one.
#[derive(PartialEq)]
pub struct SavedNetworksPage;

impl Component for SavedNetworksPage {
    fn render(&self) -> impl IntoElement {
        let ui = use_ui();
        use_polling(ui);

        let loaded = ui.model.read().clone();
        let Some(loaded) = loaded else {
            return page(vec![skeleton()]);
        };
        let model = match loaded {
            Ok(model) => model,
            Err(error) => {
                return page(vec![Element::from(
                    status(Status::Error(error.0)).action("Try again", move |_| ui.reload()),
                )]);
            }
        };

        if model.saved_names.is_empty() {
            return page(vec![Element::from(status(Status::Empty(
                "No saved networks.".into(),
            )))]);
        }
        let rows = model.saved_names.iter().map(|name| {
            let target = name.clone();
            Element::from(row(name.clone()).navigates().on_press(move |_| {
                ui.navigator.push(Route::Network {
                    ssid: target.clone(),
                })
            }))
        });
        page(vec![Element::from(
            Group::new(None::<String>).rows(rows.collect::<Vec<_>>()),
        )])
    }
}

/// One network: its signal, what you can do with it, and the facts about it.
#[derive(PartialEq)]
pub struct NetworkPage {
    pub ssid: String,
}

impl Component for NetworkPage {
    fn render(&self) -> impl IntoElement {
        let ui = use_ui();
        let tokens = use_tokens();
        use_polling(ui);
        use_page_busy(ui.busy.read().is_some());

        // Go back to the list once the network is neither in range nor saved.
        let gone = self.ssid.clone();
        use_side_effect(move || {
            if let Some(Ok(model)) = &*ui.model.read()
                && !model.networks.iter().any(|network| network.ssid == gone)
                && !model.saved_names.contains(&gone)
            {
                ui.navigator.pop();
            }
        });

        let loaded = ui.model.read().clone();
        let Some(loaded) = loaded else {
            return page(vec![skeleton()]);
        };
        let model = match loaded {
            Ok(model) => model,
            Err(error) => {
                return page(vec![Element::from(
                    status(Status::Error(error.0)).action("Try again", move |_| ui.reload()),
                )]);
            }
        };

        let ssid = self.ssid.clone();
        let visible = model.networks.iter().find(|network| network.ssid == ssid);
        let saved =
            visible.is_some_and(|network| network.saved) || model.saved_names.contains(&ssid);
        let connected = visible.is_some_and(|network| network.in_use);
        let connecting =
            ui.busy.read().as_deref() == Some(format!("Connecting to {ssid}.").as_str());

        let state = if connecting {
            "Connecting"
        } else if connected {
            "Connected"
        } else if saved && visible.is_none() {
            "Saved, out of range"
        } else if saved {
            "Saved"
        } else if visible.is_none() {
            "Out of range"
        } else {
            "Not connected"
        };
        let hero_icon = visible.map_or(Icon::NetworkWirelessSignalNone, |network| {
            signal_icon(network.signal)
        });
        let hero = rect()
            .width(Size::fill())
            .cross_align(Alignment::Center)
            .spacing(S2)
            .padding((S2, 0.0, S2, 0.0))
            .child(icons::icon(hero_icon, ICON_XL, tokens.text))
            .child(heading(ssid.clone()))
            .child(centered_caption(state));

        let mut actions = Vec::new();
        if connected {
            let name = ssid.clone();
            actions.push(strip_action(Icon::WindowClose, "Disconnect", move |_| {
                ui.disconnect(name.clone())
            }));
        } else if let Some(network) = visible {
            let network = network.clone();
            actions.push(strip_action(
                Icon::NetworkWirelessSignalExcellent,
                "Connect",
                move |_| ui.join(&network),
            ));
        }
        if saved {
            let name = ssid.clone();
            actions.push(strip_action(Icon::UserTrash, "Forget", move |_| {
                ui.confirm_forget(name.clone())
            }));
            let target = Target {
                name: ssid.clone(),
                wireless: true,
                facts: visible.map(network_facts).unwrap_or_default(),
            };
            actions.push(strip_action(
                Icon::PreferencesSystem,
                "Settings",
                move |_| ui.editor.open(target.clone()),
            ));
        }

        let mut blocks = vec![Element::from(hero)];
        if !actions.is_empty() {
            blocks.push(Element::from(action_strip(actions)));
        }
        if let Some(network) = visible {
            let mut facts = Group::titled("Details")
                .row(row("Signal strength").value(signal_detail(network.signal)));
            if !network.band.is_empty() {
                facts = facts.row(row("Frequency").value(network.band.clone()));
            }
            facts = facts.row(row("Security").value(if network.secured {
                "Password protected"
            } else {
                "Open"
            }));
            if !network.rate.is_empty() {
                facts = facts.row(row("Speed").value(network.rate.clone()));
            }
            blocks.push(Element::from(facts));
        }
        page(blocks)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("wifi", load);
}
