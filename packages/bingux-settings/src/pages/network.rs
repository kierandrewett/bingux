//! Network: the wired devices and the VPN profiles NetworkManager manages.
//! Each one is a row that opens its own subpage, where its state and actions
//! live. Everything comes from `nmcli`, on background threads.

use std::time::Duration;

use freya::prelude::*;

use super::wifi_editor::{Target, use_editor};
use crate::icons::{self, Icon};
use crate::routes::{Navigator, Route, use_navigator};
use crate::system::{self, CommandError};
use crate::ui::{
    Group, Overlays, PageHeader, ResponseKind, Status, Tokens, action_strip, centered_caption,
    dialog, heading, page_column, row, skeleton, skeleton_shape, status, strip_action,
    use_overlays, use_page_header, use_tokens,
};
use bingux_ui::shell::{MenuItem, menu_item, menu_separator};
use bingux_ui::style::*;

const QUICK: Duration = Duration::from_secs(5);
const CONNECT: Duration = Duration::from_secs(60);

#[derive(Clone, PartialEq)]
struct WiredDevice {
    name: String,
    state: String,
    connection: String,
}

type Loaded = Result<Vec<WiredDevice>, CommandError>;

/// One VPN profile and whether it is up.
#[derive(Clone, PartialEq)]
struct Vpn {
    name: String,
    wireguard: bool,
    active: bool,
}

type VpnLoaded = Result<Vec<Vpn>, CommandError>;

/// Split one `nmcli -t` line on its unescaped colons, undoing `\:` and `\\`.
fn split_terse(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ':' => fields.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    fields.push(current);
    fields
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

fn load() -> Loaded {
    system::run(
        "nmcli",
        &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"],
        QUICK,
    )
    .map(|text| parse_wired(&text))
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A device state as one plain word, such as "Connected" or "Not connected".
fn wired_state(state: &str) -> String {
    match state.split(' ').next().unwrap_or(state) {
        "disconnected" => "Not connected".to_string(),
        word => capitalise(word),
    }
}

fn is_connected(device: &WiredDevice) -> bool {
    device.state.starts_with("connected")
}

/// The connection's name, when the device has one. NetworkManager writes
/// `--` for a device with no connection.
fn has_connection(device: &WiredDevice) -> bool {
    !device.connection.is_empty() && device.connection != "--"
}

/// The row title for a wired device: its connection's name, else its place
/// in the list.
fn wired_title(devices: &[WiredDevice], index: usize) -> String {
    let device = &devices[index];
    if has_connection(device) {
        device.connection.clone()
    } else if devices.len() > 1 {
        format!("Ethernet {}", index + 1)
    } else {
        "Ethernet".to_string()
    }
}

/// The header title of the wired device `device`, from the device list loaded
/// at start-up. `None` until that list has loaded. A page opened by address
/// knows only the device, so this gives it the same title its row shows.
pub fn cached_wired_title(device: &str) -> Option<String> {
    let model = system::cached::<Loaded>("network");
    let loaded = model.read();
    let devices = loaded.as_ref()?.as_ref().ok()?;
    let index = devices.iter().position(|known| known.name == device)?;
    Some(wired_title(devices, index))
}

/// Split a terse listing of `NAME,TYPE` pairs into `(name, type)` pairs.
fn parse_pairs(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(split_terse)
        .map(|fields| {
            (
                fields.first().cloned().unwrap_or_default(),
                fields.get(1).cloned().unwrap_or_default(),
            )
        })
        .collect()
}

fn is_vpn(kind: &str) -> bool {
    kind == "vpn" || kind == "wireguard"
}

/// Every VPN profile, with the ones that are up marked.
fn parse_vpns(all: &str, active: &str) -> Vec<Vpn> {
    let active_names: Vec<String> = parse_pairs(active)
        .into_iter()
        .filter(|(_, kind)| is_vpn(kind))
        .map(|(name, _)| name)
        .collect();
    parse_pairs(all)
        .into_iter()
        .filter(|(_, kind)| is_vpn(kind))
        .map(|(name, kind)| Vpn {
            active: active_names.contains(&name),
            wireguard: kind == "wireguard",
            name,
        })
        .collect()
}

fn load_vpns() -> VpnLoaded {
    let all = system::run(
        "nmcli",
        &["-t", "-f", "NAME,TYPE", "connection", "show"],
        QUICK,
    )?;
    let active = system::run(
        "nmcli",
        &["-t", "-f", "NAME,TYPE", "connection", "show", "--active"],
        QUICK,
    )?;
    Ok(parse_vpns(&all, &active))
}

/// Pending values for switches that are changing, by key (`wired:enp3s0`,
/// `vpn:Home`). The page and its subpages share it, so a change shows the
/// same everywhere at once.
fn pending() -> State<Vec<(String, bool)>> {
    thread_local! {
        static PENDING: State<Vec<(String, bool)>> = State::create_global(Vec::new());
    }
    PENDING.with(|pending| *pending)
}

fn wired_key(device: &str) -> String {
    format!("wired:{device}")
}

fn vpn_key(name: &str) -> String {
    format!("vpn:{name}")
}

/// The value a change has set for `key`, while it is in flight.
fn pending_for(key: &str) -> Option<bool> {
    pending()
        .read()
        .iter()
        .find(|(other, _)| other == key)
        .map(|(_, on)| *on)
}

/// Show `on` for `key` at once and run `command` in the background. On
/// success the list is read again, and that read drops the pending value. A
/// failure puts the old value back and shows the error.
fn toggle_link<T>(
    model: State<Option<Result<Vec<T>, CommandError>>>,
    load: fn() -> Result<Vec<T>, CommandError>,
    overlays: Overlays,
    key: String,
    on: bool,
    command: impl FnOnce() -> Result<(), CommandError> + Send + 'static,
) where
    T: Clone + PartialEq + Send + 'static,
{
    let shown = pending();
    let previous = shown.read().clone();
    let mut value = previous.clone();
    value.retain(|(other, _)| *other != key);
    value.push((key.clone(), on));
    bingux_ui::runtime::optimistic_then(
        shown,
        Some(overlays),
        previous,
        value,
        command,
        move || {
            system::background_then(load, move |loaded| {
                let mut model = model;
                model.set(Some(loaded));
                let mut shown = shown;
                let mut list = shown.read().clone();
                list.retain(|(other, _)| *other != key);
                shown.set(list);
            });
        },
    );
}

fn set_wired(model: State<Option<Loaded>>, overlays: Overlays, device: String, connect: bool) {
    let action = if connect { "connect" } else { "disconnect" };
    let key = wired_key(&device);
    toggle_link(model, load, overlays, key, connect, move || {
        system::run("nmcli", &["device", action, &device], CONNECT).map(|_| ())
    });
}

fn set_vpn(model: State<Option<VpnLoaded>>, overlays: Overlays, name: String, up: bool) {
    let action = if up { "up" } else { "down" };
    let key = vpn_key(&name);
    toggle_link(model, load_vpns, overlays, key, up, move || {
        system::run("nmcli", &["connection", action, "id", &name], CONNECT).map(|_| ())
    });
}

/// Ask first: forgetting deletes the VPN profile from this computer.
fn confirm_forget_vpn(model: State<Option<VpnLoaded>>, overlays: Overlays, name: String) {
    overlays.show_dialog(
        dialog(
            format!("Forget \"{name}\"?"),
            "The VPN profile is deleted from this computer.",
        )
        .cancel("Cancel")
        .response("Forget", ResponseKind::Destructive, move |_| {
            let target = name.clone();
            system::background_then(
                move || {
                    system::run("nmcli", &["connection", "delete", "id", &target], CONNECT)
                        .map(|_| ())
                },
                move |result| {
                    if let Err(error) = result {
                        overlays.toast(error.0);
                    }
                    system::background(model, load_vpns);
                },
            );
        }),
    );
}

/// The right-click menu for one wired device: connect or disconnect, and its
/// details.
fn wired_menu(
    navigator: Navigator,
    model: State<Option<Loaded>>,
    overlays: Overlays,
    device: &WiredDevice,
    title: &str,
) -> Vec<MenuItem> {
    let name = device.name.clone();
    let connect = !is_connected(device);
    let toggle = if connect {
        menu_item("Connect", move |_| {
            set_wired(model, overlays, name.clone(), true)
        })
        .icon(Icon::NetworkWired)
    } else {
        menu_item("Disconnect", move |_| {
            set_wired(model, overlays, name.clone(), false)
        })
        .icon(Icon::WindowClose)
    };
    let route = Route::Wired {
        device: device.name.clone(),
        name: title.to_string(),
    };
    vec![
        toggle,
        menu_item("Details", move |_| navigator.push(route.clone())).icon(Icon::PreferencesSystem),
    ]
}

/// The right-click menu for one VPN: connect or disconnect, its details, and
/// forget.
fn vpn_menu(
    navigator: Navigator,
    model: State<Option<VpnLoaded>>,
    overlays: Overlays,
    name: &str,
    up: bool,
) -> Vec<MenuItem> {
    let toggle_name = name.to_string();
    let forget = name.to_string();
    let route = Route::Vpn {
        name: name.to_string(),
    };
    let toggle = if up {
        menu_item("Disconnect", move |_| {
            set_vpn(model, overlays, toggle_name.clone(), false)
        })
        .icon(Icon::WindowClose)
    } else {
        menu_item("Connect", move |_| {
            set_vpn(model, overlays, toggle_name.clone(), true)
        })
        .icon(Icon::NetworkVpn)
    };
    vec![
        toggle,
        menu_item("Details", move |_| navigator.push(route.clone())).icon(Icon::PreferencesSystem),
        menu_separator(),
        menu_item("Forget", move |_| {
            confirm_forget_vpn(model, overlays, forget.clone())
        })
        .icon(Icon::UserTrash)
        .destructive(),
    ]
}

/// The header line: the connected wired device, or that none is connected.
fn wired_subtitle(wired: &[WiredDevice]) -> String {
    match wired.iter().find(|device| is_connected(device)) {
        Some(device) if has_connection(device) => format!("Connected to {}", device.connection),
        Some(_) => "Connected to Ethernet".to_string(),
        None => "Not connected".to_string(),
    }
}

/// The state a row or subpage shows: the pending value while a change is in
/// flight, else the state that was loaded.
fn wired_label(device: &WiredDevice) -> String {
    match pending_for(&wired_key(&device.name)) {
        Some(true) => "Connected".to_string(),
        Some(false) => "Not connected".to_string(),
        None => wired_state(&device.state),
    }
}

fn vpn_label(on: bool) -> &'static str {
    if on { "Connected" } else { "Not connected" }
}

/// The icon, title and state at the top of a subpage.
fn hero(tokens: Tokens, icon: Icon, title: String, state: String) -> Element {
    rect()
        .tw(&tokens, &[w_full(), items(Align::Center), gap(S2), py(S2)])
        .child(icons::icon(
            icon,
            IconSize::Hero,
            Paint::Text.color(&tokens),
        ))
        .child(heading(title))
        .child(centered_caption(state))
        .into()
}

/// One wired device: its state, its actions and its facts.
#[derive(PartialEq)]
pub struct WiredPage {
    pub device: String,
    pub name: String,
}

impl Component for WiredPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let navigator = use_navigator();
        let overlays = use_overlays();
        let editor = use_editor();
        let model = system::use_cached("network", load);
        use_page_header(PageHeader::default().hero_title());

        // Go back to the list once the device is gone.
        let gone = self.device.clone();
        use_side_effect(move || {
            if let Some(Ok(devices)) = &*model.read()
                && !devices.iter().any(|device| device.name == gone)
            {
                navigator.pop();
            }
        });

        let loaded = model.read().clone();
        let Some(loaded) = loaded else {
            return page_column().child(skeleton());
        };
        let devices = match loaded {
            Ok(devices) => devices,
            Err(failure) => {
                return page_column().child(
                    status(Status::Error(failure.0))
                        .action("Try again", move |_| system::background(model, load)),
                );
            }
        };
        let Some(device) = devices.iter().find(|device| device.name == self.device) else {
            return page_column();
        };
        let state = wired_label(device);
        let connected = pending_for(&wired_key(&device.name)).unwrap_or(is_connected(device));

        let mut blocks = vec![hero(
            tokens,
            Icon::NetworkWired,
            self.name.clone(),
            state.clone(),
        )];

        let mut actions = Vec::new();
        let name = device.name.clone();
        if connected {
            actions.push(strip_action(Icon::WindowClose, "Disconnect", move |_| {
                set_wired(model, overlays, name.clone(), false)
            }));
        } else {
            actions.push(strip_action(Icon::NetworkWired, "Connect", move |_| {
                set_wired(model, overlays, name.clone(), true)
            }));
        }
        if has_connection(device) {
            let target = Target {
                name: device.connection.clone(),
                wireless: false,
                facts: Vec::new(),
            };
            actions.push(strip_action(
                Icon::PreferencesSystem,
                "Settings",
                move |_| editor.open(target.clone()),
            ));
        }
        blocks.push(Element::from(action_strip(actions)));

        let connection = if has_connection(device) {
            device.connection.clone()
        } else {
            "None".to_string()
        };
        let details = Group::titled("Details")
            .row(row("Status").value(state))
            .row(row("Device").value(device.name.clone()))
            .row(row("Connection").value(connection));
        blocks.push(Element::from(details));
        page_column().children(blocks)
    }
}

/// One VPN profile: whether it is up, what you can do with it, and its type.
#[derive(PartialEq)]
pub struct VpnPage {
    pub name: String,
}

impl Component for VpnPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let navigator = use_navigator();
        let overlays = use_overlays();
        let model = system::use_cached("vpn", load_vpns);
        use_page_header(PageHeader::default().hero_title());

        // Go back to the list once the profile is gone.
        let gone = self.name.clone();
        use_side_effect(move || {
            if let Some(Ok(profiles)) = &*model.read()
                && !profiles.iter().any(|profile| profile.name == gone)
            {
                navigator.pop();
            }
        });

        let loaded = model.read().clone();
        let Some(loaded) = loaded else {
            return page_column().child(skeleton());
        };
        let profiles = match loaded {
            Ok(profiles) => profiles,
            Err(failure) => {
                return page_column().child(
                    status(Status::Error(failure.0))
                        .action("Try again", move |_| system::background(model, load_vpns)),
                );
            }
        };
        let Some(profile) = profiles.iter().find(|profile| profile.name == self.name) else {
            return page_column();
        };
        let up = pending_for(&vpn_key(&profile.name)).unwrap_or(profile.active);

        let mut actions = Vec::new();
        let name = profile.name.clone();
        if up {
            actions.push(strip_action(Icon::WindowClose, "Disconnect", move |_| {
                set_vpn(model, overlays, name.clone(), false)
            }));
        } else {
            actions.push(strip_action(Icon::NetworkVpn, "Connect", move |_| {
                set_vpn(model, overlays, name.clone(), true)
            }));
        }
        let forget = profile.name.clone();
        actions.push(strip_action(Icon::UserTrash, "Forget", move |_| {
            confirm_forget_vpn(model, overlays, forget.clone())
        }));

        let kind = if profile.wireguard {
            "WireGuard"
        } else {
            "VPN"
        };
        let details = Group::titled("Details").row(row("Type").value(kind));
        page_column().children(vec![
            hero(
                tokens,
                Icon::NetworkVpn,
                self.name.clone(),
                vpn_label(up).to_string(),
            ),
            Element::from(action_strip(actions)),
            Element::from(details),
        ])
    }
}

#[derive(PartialEq)]
pub struct NetworkPage;

impl Component for NetworkPage {
    fn render(&self) -> impl IntoElement {
        let navigator = use_navigator();
        let vpns = system::use_cached("vpn", load_vpns);
        let model = system::use_cached("network", load);
        let overlays = use_overlays();
        // Snapshot only: BINGUX_SETTINGS_SNAPSHOT_EDITOR=<connection> opens that
        // connection's editor once the page has loaded (see Editor::snapshot_open).
        #[cfg(feature = "snapshot")]
        {
            let editor = use_editor();
            use_hook(move || {
                if let Ok(name) = std::env::var("BINGUX_SETTINGS_SNAPSHOT_EDITOR") {
                    editor.snapshot_open(Target {
                        name,
                        wireless: false,
                        facts: Vec::new(),
                    });
                }
            });
        }
        let subtitle = model
            .read()
            .as_ref()
            .and_then(|loaded| loaded.as_ref().ok())
            .map(|wired| wired_subtitle(wired));
        use_page_header(PageHeader {
            subtitle,
            action: None,
            ..Default::default()
        });
        // Wired and VPN are both drawn on the first frame, so no group
        // appears in a second pass once its data arrives.
        let vpn_loaded = vpns.read().clone();
        let (Some(loaded), Some(vpn_loaded)) = (model.read().clone(), vpn_loaded) else {
            return page_column().child(skeleton_shape(&[1, 1]));
        };
        let wired = match loaded {
            Ok(wired) => wired,
            Err(failure) => {
                return page_column().child(
                    status(Status::Error(failure.0))
                        .action("Try again", move |_| system::background(model, load)),
                );
            }
        };
        let wired_group = Group::titled("Wired");
        let wired_group = if wired.is_empty() {
            wired_group.row(status(Status::Empty("No wired network devices.".into())))
        } else {
            wired_group.rows(
                wired
                    .iter()
                    .enumerate()
                    .map(|(index, device)| {
                        let title = wired_title(&wired, index);
                        let route = Route::Wired {
                            device: device.name.clone(),
                            name: title.clone(),
                        };
                        let menu = wired_menu(navigator, model, overlays, device, &title);
                        Element::from(
                            row(title)
                                .leading(Icon::NetworkWired)
                                .value(wired_label(device))
                                .navigates()
                                .on_press(move |_| navigator.push(route.clone()))
                                .context_menu(menu),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let vpn_group = match vpn_loaded {
            Ok(list) if list.is_empty() => {
                Group::titled("VPN").row(status(Status::Empty("No VPN connections.".into())))
            }
            Ok(list) => {
                let rows = list
                    .into_iter()
                    .map(|vpn| {
                        let up = pending_for(&vpn_key(&vpn.name)).unwrap_or(vpn.active);
                        let route = Route::Vpn {
                            name: vpn.name.clone(),
                        };
                        let menu = vpn_menu(navigator, vpns, overlays, &vpn.name, up);
                        Element::from(
                            row(vpn.name)
                                .leading(Icon::NetworkVpn)
                                .value(vpn_label(up))
                                .navigates()
                                .on_press(move |_| navigator.push(route.clone()))
                                .context_menu(menu),
                        )
                    })
                    .collect::<Vec<_>>();
                Group::titled("VPN").rows(rows)
            }
            Err(failure) => Group::titled("VPN").row(
                status(Status::Error(failure.0))
                    .action("Try again", move |_| system::background(vpns, load_vpns)),
            ),
        };
        page_column().child(wired_group).child(vpn_group)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("network", load);
}
