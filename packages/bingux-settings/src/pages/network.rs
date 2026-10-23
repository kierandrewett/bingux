//! Network: the wired connections NetworkManager manages. Everything comes
//! from `nmcli`.

use std::time::Duration;

use freya::prelude::*;

use super::wifi_editor::{Target, use_editor};
use crate::icons::Icon;
use crate::system::{self, CommandError};
use crate::ui::{
    Group, PageHeader, Status, page_column, row, skeleton, status, switch_row, use_page_header,
};

const QUICK: Duration = Duration::from_secs(5);
const CONNECT: Duration = Duration::from_secs(60);

#[derive(Clone, PartialEq)]
struct WiredDevice {
    name: String,
    state: String,
    connection: String,
}

type Loaded = Result<Vec<WiredDevice>, CommandError>;

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

/// One VPN profile and whether it is up.
#[derive(Clone, PartialEq)]
struct Vpn {
    name: String,
    active: bool,
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
        .map(|(name, _)| Vpn {
            active: active_names.contains(&name),
            name,
        })
        .collect()
}

fn load_vpns() -> Result<Vec<Vpn>, CommandError> {
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

/// Bring a VPN profile up or down, then reload the VPN list.
fn toggle_vpn(model: State<Option<Result<Vec<Vpn>, CommandError>>>, name: String, on: bool) {
    let action = if on { "up" } else { "down" };
    system::background_then(
        move || system::run("nmcli", &["connection", action, "id", &name], CONNECT).map(|_| ()),
        move |_| system::background(model, load_vpns),
    );
}

/// The header line: the connected wired device, or that none is connected.
fn wired_subtitle(wired: &[WiredDevice]) -> String {
    match wired
        .iter()
        .find(|device| device.state.starts_with("connected"))
    {
        Some(device) if !device.connection.is_empty() && device.connection != "--" => {
            format!("Connected to {}", device.connection)
        }
        Some(_) => "Connected to Ethernet".to_string(),
        None => "Not connected".to_string(),
    }
}

#[derive(PartialEq)]
pub struct NetworkPage;

impl Component for NetworkPage {
    fn render(&self) -> impl IntoElement {
        let vpns = system::use_cached("vpn", load_vpns);
        let editor = use_editor();
        let model = system::use_cached("network", load);
        let subtitle = model
            .read()
            .as_ref()
            .and_then(|loaded| loaded.as_ref().ok())
            .map(|wired| wired_subtitle(wired));
        use_page_header(PageHeader {
            subtitle,
            action: None,
        });
        let Some(loaded) = model.read().clone() else {
            return page_column().child(skeleton());
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
        // The connection's name ("Wired connection 1") says more than the
        // interface name ("enp42s0"). Without a connection, say Ethernet.
        let wired_group = if wired.is_empty() {
            wired_group.row(status(Status::Empty("No wired network devices.".into())))
        } else {
            wired_group.rows(
                wired
                    .iter()
                    .enumerate()
                    .map(|(index, device)| {
                        let title = if !device.connection.is_empty() && device.connection != "--" {
                            device.connection.clone()
                        } else if wired.len() > 1 {
                            format!("Ethernet {}", index + 1)
                        } else {
                            "Ethernet".to_string()
                        };
                        let connection = device.connection.clone();
                        let mut item = row(title).leading(Icon::NetworkWired);
                        // A check mark says the device is connected; any other state is named.
                        item = if device.state.starts_with("connected") {
                            item.selected(true)
                        } else {
                            item.value(wired_state(&device.state))
                        };
                        if !connection.is_empty() && connection != "--" {
                            item = item.on_press(move |_| {
                                editor.open(Target {
                                    name: connection.clone(),
                                    wireless: false,
                                    facts: Vec::new(),
                                })
                            });
                        }
                        Element::from(item)
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let mut page = page_column().child(wired_group);
        if let Some(Ok(list)) = vpns.read().clone()
            && !list.is_empty()
        {
            let rows = list
                .into_iter()
                .map(|vpn| {
                    let name = vpn.name.clone();
                    Element::from(
                        switch_row(vpn.name.clone(), vpn.active, move |on: bool| {
                            toggle_vpn(vpns, name.clone(), on)
                        })
                        .leading(Icon::NetworkVpn),
                    )
                })
                .collect::<Vec<_>>();
            page = page.child(Group::titled("VPN").rows(rows));
        }
        page
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch("network", load);
}
