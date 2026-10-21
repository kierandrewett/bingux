//! Network: the Wi-Fi and airplane mode switches, wired devices, and the
//! Wi-Fi networks list. Everything comes from `nmcli`. The list is read
//! once when the page opens and again every ten seconds while it stays open.

use std::time::Duration;

use freya::prelude::*;

use crate::icons::Icon;
use crate::system::{self, CommandError, background, background_then};
use crate::ui::{
    Group, Overlays, ResponseKind, Status, dialog, icon_button, page_column, primary_button, row,
    row_button, status, switch_row, text_row, use_overlays,
};
use bingux_ui::scale::*;

const POLL_INTERVAL: Duration = Duration::from_secs(10);
const QUICK: Duration = Duration::from_secs(5);
/// `device wifi list` takes about four seconds on a busy radio.
const SCAN: Duration = Duration::from_secs(10);
const CONNECT: Duration = Duration::from_secs(90);
/// Width of the right-hand action column in the network list.
const ACTION_WIDTH: f32 = BOX_2XL;

#[derive(Clone, PartialEq)]
struct Model {
    /// `enabled`, `disabled` or `missing`, from `nmcli radio`.
    wifi_hw: String,
    wifi_on: bool,
    airplane: bool,
    wired: Vec<WiredDevice>,
    networks: Vec<Network>,
}

#[derive(Clone, PartialEq)]
struct WiredDevice {
    name: String,
    state: String,
    connection: String,
}

#[derive(Clone, PartialEq)]
struct Network {
    ssid: String,
    signal: u8,
    secured: bool,
    in_use: bool,
    saved: bool,
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
        return Err(CommandError("Unexpected output from nmcli radio.".into()));
    }
    let wifi_hw = fields[0].clone();
    let wifi_on = fields[1] == "enabled";
    let wwan_hw = &fields[2];
    let wwan = &fields[3];
    let airplane = !wifi_on && (wwan_hw == "missing" || wwan == "disabled");
    Ok((wifi_hw, wifi_on, airplane))
}

/// Ethernet devices that NetworkManager manages.
fn parse_wired(output: &str) -> Vec<WiredDevice> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let fields = split_terse(line);
            let field = |index: usize| fields.get(index).cloned().unwrap_or_default();
            let kind = field(1);
            let state = field(2);
            if kind != "ethernet" || state == "unmanaged" {
                return None;
            }
            Some(WiredDevice {
                name: field(0),
                state,
                connection: field(3),
            })
        })
        .collect()
}

/// Names of the saved Wi-Fi connection profiles.
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
            existing.signal = existing.signal.max(signal);
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

fn load() -> Result<Model, CommandError> {
    let (wifi_hw, wifi_on, airplane) = parse_radio(&nmcli(
        &["-t", "-f", "WIFI-HW,WIFI,WWAN-HW,WWAN", "radio"],
        QUICK,
    )?)?;
    let wired = parse_wired(&nmcli(
        &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"],
        QUICK,
    )?);
    let networks = if wifi_on && wifi_usable(&wifi_hw) {
        let saved = parse_saved_wifi(&nmcli(
            &["-t", "-f", "NAME,TYPE", "connection", "show"],
            QUICK,
        )?);
        // `--rescan no` returns NetworkManager's cached list at once; a
        // fresh scan can block for ten seconds and the page would sit on
        // "Reading Wi-Fi networks." Refresh and the poll loop rescan.
        parse_networks(
            &nmcli(
                &[
                    "-t",
                    "-f",
                    "SSID,SIGNAL,SECURITY,IN-USE,BSSID",
                    "device",
                    "wifi",
                    "list",
                    "--rescan",
                    "no",
                ],
                QUICK,
            )?,
            &saved,
        )
    } else {
        Vec::new()
    };
    Ok(Model {
        wifi_hw,
        wifi_on,
        airplane,
        wired,
        networks,
    })
}

fn toggle_wifi(on: bool) -> Result<(), CommandError> {
    nmcli(&["radio", "wifi", if on { "on" } else { "off" }], QUICK).map(|_| ())
}

fn toggle_airplane(on: bool) -> Result<(), CommandError> {
    nmcli(&["radio", "all", if on { "on" } else { "off" }], QUICK).map(|_| ())
}

fn scan_networks() -> Result<(), CommandError> {
    nmcli(&["device", "wifi", "rescan"], SCAN).map(|_| ())
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

fn forget_network(ssid: &str) -> Result<(), CommandError> {
    nmcli(&["connection", "delete", "id", ssid], QUICK).map(|_| ())
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The page's shared state. Copy, so every event handler owns a copy.
#[derive(Clone, Copy)]
struct Ui {
    model: State<Option<Result<Model, CommandError>>>,
    busy: State<Option<String>>,
    action_error: State<Option<String>>,
    /// The SSID whose password row is open.
    selected: State<Option<String>>,
    password: State<String>,
    overlays: Overlays,
}

impl Ui {
    fn reload(self) {
        background(self.model, load);
    }

    fn fail(self, message: String) {
        let mut action_error = self.action_error;
        action_error.set(Some(message));
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
            let mut action_error = self.action_error;
            match result {
                Ok(()) => {
                    action_error.set(None);
                    on_success();
                }
                Err(error) => action_error.set(Some(error.0)),
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
        self.act("Scanning for Wi-Fi networks.", scan_networks, || {});
    }

    fn connect(self, ssid: String, password: Option<String>) {
        if password.as_deref() == Some("") {
            self.fail(format!("Enter the Wi-Fi password for {ssid}."));
            return;
        }
        let label = format!("Connecting to {ssid}.");
        self.act(
            &label,
            move || connect_network(&ssid, password.as_deref()),
            move || {
                let mut selected = self.selected;
                selected.set(None);
                let mut password = self.password;
                password.set(String::new());
            },
        );
    }

    /// Ask first: forgetting drops the saved password.
    fn confirm_forget(self, ssid: String) {
        let title = format!("Forget \"{ssid}\"?");
        self.overlays.show_dialog(
            dialog(
                title,
                "The saved password is removed. Connecting again asks for it.",
            )
            .cancel("Cancel")
            .response("Forget", ResponseKind::Destructive, move |_| {
                self.forget(ssid.clone())
            }),
        );
    }

    fn forget(self, ssid: String) {
        let label = format!("Forgetting {ssid}.");
        self.act(&label, move || forget_network(&ssid), || {});
    }
}

fn page(children: Vec<Element>) -> impl IntoElement {
    page_column().children(children)
}

/// The right-hand column of a network row. Every row has one, so Forget and
/// Connect sit in the same place whether or not a row has an action.
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

/// The Wi-Fi section. Its title and hairline stay the same in every state,
/// so the page keeps one shape whether Wi-Fi is off, missing or empty.
fn wifi_section(ui: Ui, model: &Model) -> Element {
    let title = "Wi-Fi networks";
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

    let refresh = icon_button(Icon::Refresh, move |_| ui.rescan()).tooltip("Refresh");
    if model.networks.is_empty() {
        return Element::from(
            Group::titled(title)
                .trailing(refresh)
                .row(Element::from(status(Status::Empty(
                    "No Wi-Fi networks found. Press refresh to scan again.".into(),
                )))),
        );
    }
    let selected = ui.selected.read().clone();
    Element::from(
        Group::titled(title)
            .description(network_count(model.networks.len()))
            .trailing(refresh)
            .rows(network_rows(ui, &model.networks, selected.as_deref())),
    )
}

/// The Wired section: one row per Ethernet device, or a note when there are none.
fn wired_section(wired: &[WiredDevice]) -> Element {
    let group = Group::titled("Wired");
    if wired.is_empty() {
        return Element::from(group.row(Element::from(status(Status::Empty(
            "No wired network devices.".into(),
        )))));
    }
    Element::from(group.rows(wired.iter().map(|device| {
        Element::from(
            row(device.name.clone())
                .maybe_subtitle(Some(device.connection.clone()))
                .value(capitalise(&device.state)),
        )
    })))
}

/// One row per network, plus the password rows under the selected one.
fn network_rows(ui: Ui, networks: &[Network], selected: Option<&str>) -> Vec<Element> {
    let mut rows = Vec::new();
    for network in networks {
        let needs_password = network.secured && !network.saved && !network.in_use;

        let mut parts = vec![format!("Signal {}%", network.signal)];
        parts.push(if network.secured { "Secured" } else { "Open" }.to_string());
        if network.saved {
            parts.push("Saved".to_string());
        }
        let subtitle = parts.join(" · ");

        let forget = network.saved.then(|| {
            let ssid = network.ssid.clone();
            Element::from(row_button("Forget", move |_| {
                ui.confirm_forget(ssid.clone())
            }))
        });

        let ssid = network.ssid.clone();
        let mut item = row(network.ssid.clone())
            .subtitle(subtitle)
            .trailing(action_slot(forget));
        if network.in_use {
            item = item.badge("Connected");
        } else if needs_password {
            let target = ssid.clone();
            item = item.on_press(move |_| {
                let is_open = ui.selected.read().as_deref() == Some(target.as_str());
                let mut selected = ui.selected;
                selected.set(if is_open { None } else { Some(target.clone()) });
            });
        } else {
            let target = ssid.clone();
            item = item.on_press(move |_| ui.connect(target.clone(), None));
        }
        rows.push(Element::from(item));

        if needs_password && selected == Some(ssid.as_str()) {
            let target = ssid.clone();
            rows.push(Element::from(text_row(
                "Password",
                ui.password,
                move |password: String| ui.connect(target.clone(), Some(password)),
            )));
            let target = ssid.clone();
            let connect = primary_button("Connect", move |_| {
                let password = ui.password.read().clone();
                ui.connect(target.clone(), Some(password));
            });
            rows.push(Element::from(
                row(format!("Join {ssid}")).trailing(action_slot(Some(Element::from(connect)))),
            ));
        }
    }
    rows
}

#[derive(PartialEq)]
pub struct NetworkPage;

impl Component for NetworkPage {
    fn render(&self) -> impl IntoElement {
        let model = use_state::<Option<Result<Model, CommandError>>>(|| None);
        let busy = use_state::<Option<String>>(|| None);
        let action_error = use_state::<Option<String>>(|| None);
        let selected = use_state::<Option<String>>(|| None);
        let password = use_state::<String>(String::new);
        let overlays = use_overlays();
        let ui = Ui {
            model,
            busy,
            action_error,
            selected,
            password,
            overlays,
        };

        use_hook(move || {
            ui.reload();
            spawn(async move {
                loop {
                    timer(POLL_INTERVAL).await;
                    ui.reload();
                }
            });
        });

        let loaded = model.read().clone();
        let Some(loaded) = loaded else {
            return page(vec![Element::from(status(Status::Loading(
                "Reading Wi-Fi networks.".into(),
            )))]);
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

        if let Some(text) = busy.read().clone() {
            blocks.push(Element::from(status(Status::Loading(text))));
        }
        if let Some(text) = action_error.read().clone() {
            blocks.push(Element::from(status(Status::Error(text))));
        }

        let usable = wifi_usable(&model.wifi_hw);
        blocks.push(Element::from(
            Group::new(None::<String>)
                .row(
                    switch_row("Wi-Fi", model.wifi_on, move |on: bool| ui.set_wifi(on))
                        .enabled(usable),
                )
                .row(switch_row(
                    "Airplane mode",
                    model.airplane,
                    move |on: bool| ui.set_airplane(on),
                )),
        ));

        blocks.push(wifi_section(ui, &model));
        blocks.push(wired_section(&model.wired));

        page(blocks)
    }
}
