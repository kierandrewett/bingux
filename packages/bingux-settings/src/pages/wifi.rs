//! Wi-Fi: the Wi-Fi and airplane mode switches, the saved networks list, the
//! hidden network and hotspot forms, and the visible networks list. Everything
//! comes from `nmcli`. The lists are read once when the page opens and again
//! every few seconds while it stays open.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use freya::prelude::*;

use super::wifi_editor::{Editor, Target, use_editor};
use super::loading::veiled;
use crate::icons::{self, Icon};
use crate::routes::{Navigator, Route, use_navigator};
use crate::system::{self, CommandError, background_then};
use crate::ui::{
    Group, Overlays, PageHeader, PopupAction, PopupActions, ResponseKind, SpinnerSize, Status,
    Tokens, Window, action_strip, caption, centered_caption, dialog, heading, icon_button,
    page_column, popup, popup_column, row, spinner, status, strip_action, switch_row,
    text_field, use_overlays, use_page_busy, use_page_header, use_popup_actions, use_tokens,
    use_window,
};
use bingux_ui::layout::hstack;
use bingux_ui::shell::{MenuItem, menu_item, menu_separator};
use bingux_ui::style::*;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Switches whose command has started and has finished. A read taken while no
/// command is in flight shows the backend's real state.
static SWITCHES_STARTED: AtomicUsize = AtomicUsize::new(0);
static SWITCHES_FINISHED: AtomicUsize = AtomicUsize::new(0);

const QUICK: Duration = Duration::from_secs(5);
/// `device wifi list` takes about four seconds on a busy radio.
const SCAN: Duration = Duration::from_secs(10);
const CONNECT: Duration = Duration::from_secs(90);
/// Busy line shown while a scan runs; the heading shows a spinner for it.
const SCAN_LABEL: &str = "Scanning for Wi-Fi networks.";
/// The shortest password WPA allows for a hotspot.
const HOTSPOT_PASSWORD_MIN: usize = 8;

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

/// The shape of a loaded page with blank values, drawn under the veil while
/// the first load runs. Every group and row a loaded page shows is already
/// placed, so the values fill in without anything moving.
fn placeholder() -> Model {
    let blank = || Network {
        ssid: "—".to_string(),
        signal: 0,
        secured: false,
        in_use: false,
        saved: false,
        band: String::new(),
        rate: String::new(),
    };
    Model {
        wifi_hw: "enabled".into(),
        wifi_on: true,
        airplane: false,
        networks: vec![blank(), blank(), blank()],
        saved_names: vec!["—".to_string(), "—".to_string()],
        hotspot: None,
    }
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

/// Returns `(wifi_hardware, wifi_on)` from `radio -f WIFI-HW,WIFI`. Airplane
/// mode is not read here: it is a separate switch with its own source, so a
/// Wi-Fi radio that is simply off never reads as airplane mode.
fn parse_radio(output: &str) -> Result<(String, bool), CommandError> {
    let fields = split_terse(output.trim());
    if fields.len() < 2 {
        return Err(CommandError("Wi-Fi status could not be read.".into()));
    }
    let wifi_hw = fields[0].clone();
    let wifi_on = fields[1] == "enabled";
    Ok((wifi_hw, wifi_on))
}

/// True when the kernel has every radio blocked in software: the airplane
/// state. Each radio reports a `soft` flag in `/sys/class/rfkill`; airplane
/// mode is on only when there is at least one radio and all of them are
/// blocked. Snapshot builds can fake it with `BINGUX_SETTINGS_FAKE_AIRPLANE`.
fn airplane_blocked() -> bool {
    #[cfg(feature = "snapshot")]
    if let Some(value) = std::env::var_os("BINGUX_SETTINGS_FAKE_AIRPLANE") {
        return value == "1";
    }
    let Ok(entries) = std::fs::read_dir("/sys/class/rfkill") else {
        return false;
    };
    let mut any = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let soft = std::fs::read_to_string(path.join("soft")).unwrap_or_default();
        any = true;
        if soft.trim() != "1" {
            return false;
        }
    }
    any
}

/// Names of the Wi-Fi connection profiles in `NAME,TYPE` output.
fn parse_saved_wifi(output: &str) -> Vec<String> {
    let mut names: Vec<String> = output
        .lines()
        .filter_map(|line| {
            let fields = split_terse(line);
            (fields.get(1).map(String::as_str) == Some("802-11-wireless"))
                .then(|| fields[0].clone())
        })
        .collect();
    names.sort();
    names
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
    // Sorted by name alone. A rescan changes signal and connection, and a sort
    // on either would reorder rows on screen while the list is open.
    networks.sort_by(|a, b| a.ssid.cmp(&b.ssid));
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

/// Snapshot builds only: `BINGUX_SETTINGS_FAKE_WIFI` stands in a Wi-Fi radio
/// that is on, with one connected network, two saved and one visible only.
/// Nothing is read from `nmcli`, and no radio is changed.
#[cfg(feature = "snapshot")]
fn fake_model() -> Option<Model> {
    std::env::var_os("BINGUX_SETTINGS_FAKE_WIFI")?;
    let network = |ssid: &str, signal: u8, in_use: bool, saved: bool| Network {
        ssid: ssid.into(),
        signal,
        secured: true,
        in_use,
        saved,
        band: "5 GHz".into(),
        rate: "540 Mbit/s".into(),
    };
    Some(Model {
        wifi_hw: "enabled".into(),
        wifi_on: true,
        airplane: false,
        networks: vec![
            network("Home", 82, true, true),
            network("Office", 64, false, true),
            network("Neighbour", 41, false, false),
        ],
        saved_names: vec!["Home".into(), "Office".into()],
        hotspot: None,
    })
}

fn load() -> Result<Model, CommandError> {
    #[cfg(feature = "snapshot")]
    if let Some(model) = fake_model() {
        return Ok(model);
    }
    let (wifi_hw, wifi_on) = parse_radio(&nmcli(&["-t", "-f", "WIFI-HW,WIFI", "radio"], QUICK)?)?;
    let airplane = airplane_blocked();
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
    #[cfg(feature = "snapshot")]
    if std::env::var_os("BINGUX_SETTINGS_FAKE_WIFI").is_some() {
        return Ok(());
    }
    nmcli(&["radio", "wifi", if on { "on" } else { "off" }], QUICK).map(|_| ())
}

fn toggle_airplane(on: bool) -> Result<(), CommandError> {
    #[cfg(feature = "snapshot")]
    if std::env::var_os("BINGUX_SETTINGS_FAKE_AIRPLANE").is_some() {
        return Ok(());
    }
    // rfkill, not `nmcli radio all`: it blocks and unblocks every radio the
    // kernel knows about, which is what airplane mode means.
    system::run(
        "rfkill",
        &[if on { "block" } else { "unblock" }, "all"],
        QUICK,
    )
    .map(|_| ())
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

fn up_connection(name: &str) -> Result<(), CommandError> {
    nmcli(&["connection", "up", "id", name], CONNECT).map(|_| ())
}

/// The saved password of a profile, read from NetworkManager's secrets.
fn saved_password(name: &str) -> Result<String, CommandError> {
    let output = nmcli(
        &[
            "-s",
            "-g",
            "802-11-wireless-security.psk",
            "connection",
            "show",
            "id",
            name,
        ],
        QUICK,
    )?;
    // nmcli escapes colons in a terse value; split_terse undoes that.
    let password = split_terse(output.trim_end_matches(['\r', '\n']))
        .into_iter()
        .next()
        .unwrap_or_default();
    if password.is_empty() {
        Err(CommandError("This network has no saved password.".into()))
    } else {
        Ok(password)
    }
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
        IconSize::Control,
        Paint::Muted.color(&tokens),
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

/// The page's shared state. Copy, so every event handler owns a copy.
#[derive(Clone, Copy)]
struct Ui {
    model: State<Option<Result<Model, CommandError>>>,
    busy: State<Option<String>>,
    /// Shown inside the open popup, such as "Enter the Wi-Fi password."
    sheet_error: State<Option<String>>,
    password: State<String>,
    /// The Wi-Fi and airplane values the user just set, shown until the
    /// backend has applied them and the list has been read again.
    wifi_pending: State<Option<bool>>,
    airplane_pending: State<Option<bool>>,
    hidden_ssid: State<String>,
    hidden_password: State<String>,
    hotspot_name: State<String>,
    hotspot_password: State<String>,
    overlays: Overlays,
    window: Window,
    navigator: Navigator,
    editor: Editor,
}

/// Popup bodies are components that read the page state themselves, so their
/// props never change; the state handles carry the updates.
impl PartialEq for Ui {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

fn use_ui() -> Ui {
    Ui {
        model: use_hook(|| system::cached_persistent::<Model, CommandError>("wifi")),
        busy: use_state(|| None::<String>),
        sheet_error: use_state(|| None::<String>),
        password: use_state(String::new),
        wifi_pending: use_state(|| None::<bool>),
        airplane_pending: use_state(|| None::<bool>),
        hidden_ssid: use_state(String::new),
        hidden_password: use_state(String::new),
        hotspot_name: use_state(String::new),
        hotspot_password: use_state(String::new),
        overlays: use_overlays(),
        window: use_window(),
        navigator: use_navigator(),
        editor: use_editor(),
    }
}

impl Ui {
    fn reload(self) {
        // A read is the backend's truth only if no switch was changing while it
        // ran, so the optimistic values are cleared on such a read alone.
        let started = SWITCHES_STARTED.load(Ordering::SeqCst);
        let quiet = started == SWITCHES_FINISHED.load(Ordering::SeqCst);
        background_then(move || system::load_persistent("wifi", load), move |loaded| {
            if quiet && SWITCHES_STARTED.load(Ordering::SeqCst) == started {
                let mut wifi = self.wifi_pending;
                wifi.set(None);
                let mut airplane = self.airplane_pending;
                airplane.set(None);
            }
            let mut model = self.model;
            model.set(Some(loaded));
        });
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
        self.set_switch(self.wifi_pending, on, move || toggle_wifi(on));
    }

    fn set_airplane(self, on: bool) {
        self.set_switch(self.airplane_pending, on, move || toggle_airplane(on));
    }

    /// Flip a switch on screen at once and run its command in the background,
    /// through the shared optimistic helper: a failure reverts the switch and
    /// shows a toast. The next clean read of the backend clears the value.
    fn set_switch(
        self,
        pending: State<Option<bool>>,
        on: bool,
        work: impl FnOnce() -> Result<(), CommandError> + Send + 'static,
    ) {
        SWITCHES_STARTED.fetch_add(1, Ordering::SeqCst);
        bingux_ui::runtime::optimistic(pending, Some(self.overlays), None, Some(on), move || {
            let result = work();
            SWITCHES_FINISHED.fetch_add(1, Ordering::SeqCst);
            result
        });
    }

    /// The model as its switches show it: a switch that is changing shows the
    /// new value at once, before the backend has applied it.
    fn shown(self, mut model: Model) -> Model {
        if let Some(on) = *self.wifi_pending.read() {
            model.wifi_on = on;
        }
        if let Some(on) = *self.airplane_pending.read() {
            model.airplane = on;
        }
        model
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

    /// Bring up a saved network by its profile name.
    fn activate(self, name: String) {
        let label = format!("Connecting to {name}.");
        self.act(&label, move || up_connection(&name), || {});
    }

    /// Copy a saved network's password to the clipboard, and say so.
    fn copy_password(self, name: String) {
        background_then(
            move || saved_password(&name),
            move |result| match result {
                Ok(password) => match freya::clipboard::Clipboard::set(password) {
                    Ok(()) => self.overlays.toast("Copied."),
                    Err(_) => self.overlays.toast("The password could not be copied."),
                },
                Err(error) => self.overlays.toast(error.0),
            },
        );
    }

    /// Ask first: forgetting drops the saved password. `leave` pops the details
    /// page once the network is gone.
    fn confirm_forget(self, name: String, leave: bool) {
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
                    move || {
                        if leave {
                            navigator.pop();
                        }
                    },
                );
            }),
        );
    }

    /// The password popup for a secured network that is not saved yet.
    fn open_join(self, ssid: String) {
        let mut password = self.password;
        password.set(String::new());
        let mut error = self.sheet_error;
        error.set(None);
        self.show_join(ssid);
    }

    /// The join popup. Connect waits for a password.
    fn show_join(self, ssid: String) {
        self.window
            .open(popup(ssid.clone(), JoinBody { ui: self, ssid }));
    }

    fn join_now(self, ssid: String, window: Window) {
        let password = self.password.read().clone();
        if password.is_empty() {
            self.sheet_error("Enter the Wi-Fi password.");
            return;
        }
        window.close();
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
        let mut error = self.sheet_error;
        error.set(None);
        self.show_hidden();
    }

    /// The hidden network popup. Connect waits for a network name.
    fn show_hidden(self) {
        self.window
            .open(popup("Hidden network", HiddenBody { ui: self }));
    }

    fn connect_hidden(self, window: Window) {
        let ssid = self.hidden_ssid.read().trim().to_string();
        if ssid.is_empty() {
            self.sheet_error("Enter the network name.");
            return;
        }
        let password = self.hidden_password.read().clone();
        let password = (!password.is_empty()).then_some(password);
        window.close();
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
        let mut error = self.sheet_error;
        error.set(None);
        self.show_hotspot();
    }

    /// The hotspot popup. Turn on waits for a name and a password of the minimum length.
    fn show_hotspot(self) {
        self.window
            .open(popup("Wi-Fi hotspot", HotspotBody { ui: self }));
    }

    fn start_hotspot(self, window: Window) {
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
        window.close();
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

fn page(children: Vec<Element>) -> Rect {
    page_column().children(children)
}

/// The text of a value, or a dash when the network does not report one.
fn or_dash(text: &str) -> String {
    if text.is_empty() {
        "—".to_string()
    } else {
        text.to_string()
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

/// "1 network nearby" or "N networks nearby".
fn network_count(count: usize) -> String {
    if count == 1 {
        "1 network nearby".to_string()
    } else {
        format!("{count} networks nearby")
    }
}

/// The password popup body. It reads the page's state, so it redraws as the
/// fields change.
#[derive(Clone, PartialEq)]
struct JoinBody {
    ui: Ui,
    ssid: String,
}

impl Component for JoinBody {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let window = use_window();
        let press = self.ssid.clone();
        let submit = self.ssid.clone();
        let ready = !ui.password.read().is_empty();
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Connect".into(),
                enabled: ready,
                on_press: EventHandler::new(move |_: ()| ui.join_now(press.clone(), window)),
            }),
        });
        let field = text_field(ui.password, move |_: String| {
            ui.join_now(submit.clone(), window)
        })
        .autofocus()
        .password();
        let error = ui.sheet_error.read().clone();
        popup_column()
            .child(Group::new(None::<String>).row(row("Password").trailing(field)))
            .maybe_child(error.map(|text| Element::from(status(Status::Error(text)))))
    }
}

/// The hidden network popup body: a name, an optional password and the show toggle.
#[derive(Clone, PartialEq)]
struct HiddenBody {
    ui: Ui,
}

impl Component for HiddenBody {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let window = use_window();
        let ready = !ui.hidden_ssid.read().trim().is_empty();
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Connect".into(),
                enabled: ready,
                on_press: EventHandler::new(move |_: ()| ui.connect_hidden(window)),
            }),
        });
        let name = text_field(ui.hidden_ssid, move |_: String| ui.connect_hidden(window))
            .autofocus()
            .placeholder("Network name");
        let password = text_field(ui.hidden_password, move |_: String| {
            ui.connect_hidden(window)
        })
        .placeholder("Blank if open")
        .password();
        let error = ui.sheet_error.read().clone();
        popup_column()
            .child(
                Group::new(None::<String>)
                    .row(row("Network name").trailing(name))
                    .row(row("Password").trailing(password)),
            )
            .maybe_child(error.map(|text| Element::from(status(Status::Error(text)))))
    }
}

/// The hotspot popup body: a network name and a password of at least eight characters.
#[derive(Clone, PartialEq)]
struct HotspotBody {
    ui: Ui,
}

impl Component for HotspotBody {
    fn render(&self) -> impl IntoElement {
        let ui = self.ui;
        let window = use_window();
        let name_ok = !ui.hotspot_name.read().trim().is_empty();
        let password_ok = ui.hotspot_password.read().chars().count() >= HOTSPOT_PASSWORD_MIN;
        use_popup_actions(PopupActions {
            primary: Some(PopupAction {
                label: "Turn on".into(),
                enabled: name_ok && password_ok,
                on_press: EventHandler::new(move |_: ()| ui.start_hotspot(window)),
            }),
        });
        let name = text_field(ui.hotspot_name, move |_: String| ui.start_hotspot(window))
            .autofocus()
            .placeholder("Network name");
        let password = text_field(ui.hotspot_password, move |_: String| {
            ui.start_hotspot(window)
        })
        .placeholder("8+ characters")
        .password();
        let error = ui.sheet_error.read().clone();
        popup_column()
            .child(
                Group::new(None::<String>)
                    .row(row("Network name").trailing(name))
                    .row(row("Password").trailing(password)),
            )
            .maybe_child(error.map(|text| Element::from(status(Status::Error(text)))))
    }
}

/// The visible networks section. Its heading shows a spinner while a scan runs.
fn visible_section(ui: Ui, model: &Model, tokens: Tokens, loading: bool) -> Element {
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
            "Wi-Fi is off because \"Airplane mode\" is on."
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
        // A blank box the size of the spinner, so the row does not shift.
        Element::from(rect().tw_layout(&[w(Dim::IconBox), h(Dim::IconBox)]))
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
    // A blank line of the same height holds the heading still until the count
    // is known.
    let count = if loading {
        " ".to_string()
    } else {
        network_count(model.networks.len())
    };
    Element::from(
        Group::titled(title)
            .description(count)
            .trailing(trailing)
            .rows(network_rows(ui, &model.networks, tokens)),
    )
}

/// One row per network. The signal icon leads; a lock marks a secured network;
/// a check mark marks the connected network and a caption says "Saved" for a
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
            let mut saved = false;
            if network.in_use {
                item = item.selected(true);
            } else if network.saved {
                saved = true;
            }

            let mut cluster = rect().tw_layout(&[flex_row(), gap(S2), items(Align::Center)]);
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
            let slot = action_slot(Some(Element::from(cluster)));
            let trailing = if saved {
                Element::from(hstack(S2).child(caption("Saved")).child(slot))
            } else {
                slot
            };
            Element::from(
                item.trailing(trailing)
                    .context_menu(network_menu(ui, network)),
            )
        })
        .collect()
}

/// The right-click menu for one network row: connect or disconnect, the saved
/// network actions, and the details page.
fn network_menu(ui: Ui, network: &Network) -> Vec<MenuItem> {
    let ssid = network.ssid.clone();
    let mut items = Vec::new();
    if network.in_use {
        let name = ssid.clone();
        items.push(
            menu_item("Disconnect", move |_| ui.disconnect(name.clone())).icon(Icon::WindowClose),
        );
    } else {
        let press = network.clone();
        items.push(
            menu_item("Connect", move |_| ui.join(&press))
                .icon(Icon::NetworkWirelessSignalExcellent),
        );
    }
    if network.saved {
        items.push(menu_separator());
        items.extend(saved_items(ui, &ssid, false));
    }
    items.push(menu_separator());
    let details = ssid;
    items.push(
        menu_item("Network details", move |_| {
            ui.navigator.push(Route::Network {
                ssid: details.clone(),
            })
        })
        .icon(Icon::PreferencesSystem),
    );
    items
}

/// The actions every saved network offers: copy its password and forget it.
/// `leave` says whether forgetting also leaves the details page it is opened from.
fn saved_items(ui: Ui, name: &str, leave: bool) -> Vec<MenuItem> {
    let copy = name.to_string();
    let forget = name.to_string();
    vec![
        menu_item("Copy password", move |_| ui.copy_password(copy.clone())).icon(Icon::EditCopy),
        menu_item("Forget", move |_| ui.confirm_forget(forget.clone(), leave))
            .icon(Icon::UserTrash)
            .destructive(),
    ]
}

/// The hotspot row: the state and a turn-off row while one runs, or the form otherwise.
fn hotspot_row(ui: Ui, model: &Model) -> Element {
    match &model.hotspot {
        Some(name) => {
            let stop = name.clone();
            Element::from(
                row("Turn off Wi-Fi hotspot")
                    .value(format!("On: {name}"))
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
        // What the switches show: a switch being changed shows its new value
        // at once, before the backend has applied it.
        let loaded = ui.model.read().clone();
        // The switch and the subtitle wait for real data, so the placeholder
        // under the veil can never write a guess to the system.
        let shown = loaded.map(|loaded| loaded.map(|model| ui.shown(model)));
        let loading = shown.is_none();
        // The master switch lives in the header bar, as in GNOME.
        let master = match shown.as_ref() {
            Some(Ok(model)) if wifi_usable(&model.wifi_hw) => Some(model.wifi_on),
            _ => None,
        };
        let mut header = PageHeader {
            subtitle: Some(wifi_subtitle(shown.as_ref())),
            action: None,
            ..Default::default()
        };
        if let Some(on) = master {
            header = header.switch("Wi-Fi", on, move |next: bool| ui.set_wifi(next));
        }
        use_page_header(header);

        let model = match shown {
            None => placeholder(),
            Some(Ok(model)) => model,
            Some(Err(error)) => {
                return page(vec![Element::from(
                    status(Status::Error(error.0)).action("Try again", move |_| ui.reload()),
                )]);
            }
        };

        let mut blocks: Vec<Element> = Vec::new();
        // Airplane mode comes first: it switches every radio, Wi-Fi included.
        blocks.push(Element::from(Group::new(None::<String>).row(switch_row(
            "Airplane mode",
            model.airplane,
            move |on: bool| ui.set_airplane(on),
        ))));

        // Wi-Fi settings show only while Wi-Fi is on; they depend on it.
        if !wifi_usable(&model.wifi_hw) {
            blocks.push(Element::from(
                Group::new(None::<String>).row(
                    row("Wi-Fi")
                        .subtitle("Wi-Fi is not available on this computer.")
                        .enabled(false),
                ),
            ));
        } else if model.wifi_on {
            let navigator = ui.navigator;
            blocks.push(Element::from(Group::new(None::<String>).rows(vec![
                    Element::from(
                        row("Saved networks")
                            .navigates()
                            .on_press(move |_| navigator.push(Route::WifiSaved)),
                    ),
                    Element::from(
                        row("Connect to hidden network...")
                            .navigates()
                            .on_press(move |_| ui.open_hidden()),
                    ),
                    hotspot_row(ui, &model),
                ])));
        }

        blocks.push(visible_section(ui, &model, tokens, loading));

        veiled(page(blocks), loading)
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
        // The saved list draws its rows under the veil until the first load lands.
        let (model, loading) = match loaded {
            None => (placeholder(), true),
            Some(Ok(model)) => (model, false),
            Some(Err(error)) => {
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
            let connect = name.clone();
            let mut menu = vec![
                menu_item("Connect", move |_| ui.activate(connect.clone()))
                    .icon(Icon::NetworkWirelessSignalExcellent),
                menu_separator(),
            ];
            menu.extend(saved_items(ui, name, false));
            Element::from(
                row(name.clone())
                    .navigates()
                    .on_press(move |_| {
                        ui.navigator.push(Route::Network {
                            ssid: target.clone(),
                        })
                    })
                    .context_menu(menu),
            )
        });
        veiled(
            page(vec![Element::from(
                Group::new(None::<String>).rows(rows.collect::<Vec<_>>()),
            )]),
            loading,
        )
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
        use_page_header(PageHeader::default().hero_title());

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
        // Before the first load the page draws its real rows from a placeholder
        // under the veil.
        let (model, loading) = match loaded {
            None => (placeholder(), true),
            Some(Ok(model)) => (model, false),
            Some(Err(error)) => {
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

        let state = if loading {
            "—"
        } else if connecting {
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
            .tw(&tokens, &[w_full(), items(Align::Center), gap(S2), py(S2)])
            .child(icons::icon(
                hero_icon,
                IconSize::Hero,
                Paint::Text.color(&tokens),
            ))
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
                ui.confirm_forget(name.clone(), true)
            }));
            let target = Target {
                name: ssid.clone(),
                wireless: true,
                facts: Vec::new(),
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
        // The four facts always show, with a dash for any the network does not
        // report, so the group keeps its place when the network comes or goes.
        let (signal, band, security, speed) = match visible {
            Some(network) => (
                signal_detail(network.signal),
                or_dash(&network.band),
                (if network.secured { "Password protected" } else { "Open" }).to_string(),
                or_dash(&network.rate),
            ),
            None => (
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
            ),
        };
        blocks.push(Element::from(
            Group::titled("Details")
                .row(row("Signal strength").value(signal))
                .row(row("Frequency").value(band))
                .row(row("Security").value(security))
                .row(row("Speed").value(speed)),
        ));
        veiled(page(blocks), loading)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_persistent("wifi", load);
}
