//! Connection details: the read-only facts of one connection, from
//! NetworkManager. An active connection shows what it really has (addresses,
//! gateway, DNS, routes, link, Wi-Fi signal); one that is not active shows its
//! saved configuration and says so. Every value can be copied: press and hold,
//! or right click it.

use std::time::Duration;

use freya::prelude::*;

use super::model::{DeviceKind, nmcli, split_terse, unescape};
use super::{Owner, Section};
use crate::routes;
use crate::system::{self, CommandError, background_then};
use crate::ui::{Group, PageHead, Status, page_column, row, skeleton_rows, status};
use bingux_ui::prelude::use_router;
use bingux_ui::style::*;

const QUICK: Duration = Duration::from_secs(5);
/// How often the facts are read again while the page is open.
const POLL: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// `key:value` lines of `nmcli -t`, with list indexes (`IP4.DNS[2]`) dropped
/// so a list is every line of one key, and the value's escapes undone.
#[derive(Default, Debug)]
struct Pairs(Vec<(String, String)>);

impl Pairs {
    fn parse(output: &str) -> Self {
        Self(
            output
                .lines()
                .filter_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    let key = key.split('[').next().unwrap_or(key);
                    let value = unescape(value).trim().to_string();
                    Some((
                        key.to_string(),
                        if value == "--" { String::new() } else { value },
                    ))
                })
                .collect(),
        )
    }

    /// The first non-empty value of `key`, or an empty string.
    fn get(&self, key: &str) -> String {
        self.all(key).into_iter().next().unwrap_or_default()
    }

    /// Every non-empty value of `key`, in order.
    fn all(&self, key: &str) -> Vec<String> {
        self.0
            .iter()
            .filter(|(k, v)| k == key && !v.is_empty())
            .map(|(_, v)| v.clone())
            .collect()
    }
}

/// One address family's facts.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
struct Family {
    /// The configured method of a saved connection, in words; empty when live.
    method: String,
    addresses: Vec<String>,
    gateway: String,
    dns: Vec<String>,
    domains: Vec<String>,
    routes: Vec<String>,
}

impl Family {
    fn is_empty(&self) -> bool {
        self.method.is_empty()
            && self.addresses.is_empty()
            && self.gateway.is_empty()
            && self.dns.is_empty()
            && self.domains.is_empty()
            && self.routes.is_empty()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct WifiFacts {
    bssid: String,
    band: String,
    channel: String,
    security: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Live {
    Active,
    Activating,
    Saved,
}

#[derive(Clone, PartialEq, Debug)]
struct Details {
    uuid: String,
    kind_label: String,
    state: Live,
    device: String,
    v4: Family,
    v6: Family,
    mac: String,
    mtu: String,
    wifi: Option<WifiFacts>,
    owner: Option<Owner>,
}

/// A route in words: `192.168.0.0/24 direct, metric 101`: the destination, then how it is reached. Takes both the
/// live `dst = .., nh = .., mt = ..` form and the saved `ip = ..` form.
fn format_route(text: &str) -> String {
    let mut destination = String::new();
    let mut next_hop = String::new();
    let mut metric = String::new();
    for part in text.split(',') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "dst" | "ip" => destination = value,
            "nh" => next_hop = value,
            "mt" => metric = value,
            _ => {}
        }
    }
    if destination.is_empty() {
        return text.trim().to_string();
    }
    let mut line = destination;
    if next_hop.is_empty() || next_hop == "0.0.0.0" || next_hop == "::" {
        line.push_str(" direct");
    } else {
        line.push_str(&format!(" via {next_hop}"));
    }
    if !metric.is_empty() {
        line.push_str(&format!(", metric {metric}"));
    }
    line
}

/// The routes of a saved profile. nmcli prints them either as
/// `10.0.0.0/8 10.1.1.1 50, 172.16.0.0/12` (destination, next hop, metric) or
/// as `{ ip = 10.0.0.0/8, nh = 10.1.1.1, mt = 50 }, { ... }`.
fn parse_saved_routes(text: &str) -> Vec<String> {
    if text.contains('{') {
        return text
            .split('{')
            .skip(1)
            .filter_map(|block| block.split('}').next())
            .map(format_route)
            .collect();
    }
    text.split(',')
        .filter_map(|route| {
            let mut words = route.split_whitespace();
            let destination = words.next()?;
            let mut fields = vec![format!("dst = {destination}")];
            if let Some(next_hop) = words.next() {
                fields.push(format!("nh = {next_hop}"));
            }
            if let Some(metric) = words.next() {
                fields.push(format!("mt = {metric}"));
            }
            Some(format_route(&fields.join(", ")))
        })
        .collect()
}

/// A saved list that nmcli prints on one line, separated by commas or spaces.
fn parse_saved_list(text: &str) -> Vec<String> {
    text.split([',', ' ', ';'])
        .filter(|item| !item.is_empty() && *item != "--")
        .map(str::to_string)
        .collect()
}

/// An IP method in words.
fn method_label(method: &str) -> String {
    match method {
        "auto" => "Automatic",
        "dhcp" => "Automatic (DHCP)",
        "manual" => "Manual",
        "disabled" | "ignore" => "Off",
        "link-local" => "Link-local only",
        "shared" => "Shared with other computers",
        other => other,
    }
    .to_string()
}

/// The live facts of one family from an active connection.
fn live_family(pairs: &Pairs, prefix: &str) -> Family {
    Family {
        method: String::new(),
        addresses: pairs.all(&format!("{prefix}.ADDRESS")),
        gateway: pairs.get(&format!("{prefix}.GATEWAY")),
        dns: pairs.all(&format!("{prefix}.DNS")),
        domains: pairs.all(&format!("{prefix}.DOMAIN")),
        routes: pairs
            .all(&format!("{prefix}.ROUTE"))
            .iter()
            .map(|route| format_route(route))
            .collect(),
    }
}

/// The saved configuration of one family (`ipv4` or `ipv6`).
fn saved_family(pairs: &Pairs, prefix: &str) -> Family {
    Family {
        method: method_label(&pairs.get(&format!("{prefix}.method"))),
        addresses: parse_saved_list(&pairs.get(&format!("{prefix}.addresses"))),
        gateway: pairs.get(&format!("{prefix}.gateway")),
        dns: parse_saved_list(&pairs.get(&format!("{prefix}.dns"))),
        domains: parse_saved_list(&pairs.get(&format!("{prefix}.dns-search"))),
        routes: parse_saved_routes(&pairs.get(&format!("{prefix}.routes"))),
    }
}

/// What a connection type is called.
fn kind_label(kind: &str, service: &str) -> String {
    match kind {
        "802-3-ethernet" | "ethernet" => "Ethernet".into(),
        "802-11-wireless" | "wifi" => "Wi-Fi".into(),
        "wireguard" => "WireGuard".into(),
        "gsm" | "cdma" => "Mobile broadband".into(),
        "vpn" => match service.rsplit('.').next().unwrap_or("") {
            "openvpn" => "OpenVPN".into(),
            "openconnect" => "OpenConnect".into(),
            "vpnc" => "Cisco VPN (vpnc)".into(),
            "" => "VPN".into(),
            other => format!("VPN ({other})"),
        },
        "bridge" => "Bridge".into(),
        other => super::model::capitalise(other),
    }
}

/// Wi-Fi frequency in MHz as a band name.
fn band_of(frequency: &str) -> String {
    let mhz: u32 = frequency
        .split_whitespace()
        .next()
        .and_then(|number| number.parse().ok())
        .unwrap_or(0);
    match mhz {
        0 => String::new(),
        1..=3000 => "2.4 GHz".into(),
        3001..=5924 => "5 GHz".into(),
        _ => "6 GHz".into(),
    }
}

/// The page that edits a connection of `kind`, when it has one.
fn owner_for(kind: &str, name: &str, wired_device: &str) -> Option<Owner> {
    match kind {
        "802-11-wireless" => Some(Owner::Wifi {
            ssid: name.to_string(),
            saved_list: false,
        }),
        "802-3-ethernet" if !wired_device.is_empty() => Some(Owner::Wired {
            device: wired_device.to_string(),
            profile: name.to_string(),
            in_use: false,
        }),
        "vpn" | "wireguard" => Some(Owner::Vpn {
            name: name.to_string(),
        }),
        _ => None,
    }
}

/// The first Ethernet device, for a wired profile with no fixed interface.
fn first_wired_device() -> String {
    nmcli(&["-t", "-f", "DEVICE,TYPE", "device"], QUICK)
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let fields = split_terse(line);
                (fields.get(1).map(String::as_str) == Some("ethernet")).then(|| fields[0].clone())
            })
        })
        .unwrap_or_default()
}

fn read_wifi(device: &str) -> Option<WifiFacts> {
    let text = nmcli(
        &[
            "-t",
            "-f",
            "IN-USE,BSSID,CHAN,FREQ,SECURITY",
            "device",
            "wifi",
            "list",
            "ifname",
            device,
            "--rescan",
            "no",
        ],
        QUICK,
    )
    .ok()?;
    text.lines().find_map(|line| {
        let fields = split_terse(line);
        let field = |index: usize| fields.get(index).cloned().unwrap_or_default();
        (field(0) == "*").then(|| WifiFacts {
            bssid: field(1),
            channel: field(2),
            band: band_of(&field(3)),
            security: if field(4).is_empty() {
                "Open".into()
            } else {
                field(4)
            },
        })
    })
}

fn read(name: &str) -> Result<Details, CommandError> {
    #[cfg(feature = "snapshot")]
    if let Some(details) = fake::details(name) {
        return Ok(details);
    }
    let pairs = Pairs::parse(&nmcli(&["-t", "connection", "show", "id", name], QUICK)?);
    let general = pairs.get("GENERAL.STATE");
    let state = match general.as_str() {
        "activated" => Live::Active,
        "activating" => Live::Activating,
        _ => Live::Saved,
    };
    let kind = pairs.get("connection.type");
    // A tunnel's own interface (`wg0`) before the device it runs over.
    let device = [pairs.get("GENERAL.IP-IFACE"), pairs.get("GENERAL.DEVICES")]
        .into_iter()
        .find(|device| !device.is_empty())
        .unwrap_or_default();
    let live = state != Live::Saved;
    let mut details = Details {
        uuid: pairs.get("connection.uuid"),
        kind_label: kind_label(&kind, &pairs.get("vpn.service-type")),
        state,
        v4: if live {
            live_family(&pairs, "IP4")
        } else {
            saved_family(&pairs, "ipv4")
        },
        v6: if live {
            live_family(&pairs, "IP6")
        } else {
            saved_family(&pairs, "ipv6")
        },
        device: device.clone(),
        mac: pairs.get("802-3-ethernet.mac-address"),
        mtu: String::new(),
        wifi: None,
        owner: None,
    };
    let mut wired_device = if live { device.clone() } else { String::new() };
    if wired_device.is_empty() {
        wired_device = pairs.get("connection.interface-name");
    }
    if kind == "802-3-ethernet" && wired_device.is_empty() {
        wired_device = first_wired_device();
    }
    details.owner = owner_for(&kind, name, &wired_device);

    if live && !device.is_empty() {
        let facts = Pairs::parse(&nmcli(&["-t", "device", "show", &device], QUICK)?);
        details.mac = facts.get("GENERAL.HWADDR");
        details.mtu = facts.get("GENERAL.MTU");
        if super::model::DeviceKind::Wifi == device_kind(&facts.get("GENERAL.TYPE")) {
            details.wifi = read_wifi(&device);
        }
    }
    Ok(details)
}

/// Stand-in details for the fake network of snapshot builds: the fake connections have no
/// profile in NetworkManager, and the wired one is the real machine's name, so it is faked too.
#[cfg(feature = "snapshot")]
mod fake {
    use super::*;

    fn family(live: bool, addresses: &[&str], gateway: &str, dns: &[&str]) -> Family {
        let strings = |list: &[&str]| list.iter().map(|item| item.to_string()).collect();
        Family {
            method: if live { "" } else { "Automatic (DHCP)" }.to_string(),
            addresses: strings(addresses),
            gateway: gateway.to_string(),
            dns: strings(dns),
            domains: if live {
                strings(&["home.arpa"])
            } else {
                Vec::new()
            },
            routes: if live && !gateway.is_empty() {
                strings(&[
                    "192.168.0.0/24 direct, metric 100",
                    "0.0.0.0/0 via 192.168.0.1, metric 100",
                ])
            } else {
                Vec::new()
            },
        }
    }

    pub fn details(name: &str) -> Option<Details> {
        super::super::model::fake_scenario()?;
        let (kind_label, device, live, wifi) = match name {
            "Home" => (
                "Wi-Fi",
                "wlo1",
                true,
                Some(WifiFacts {
                    bssid: "A4:2B:B0:11:22:52".to_string(),
                    band: "5 GHz".to_string(),
                    channel: "36".to_string(),
                    security: "WPA2".to_string(),
                }),
            ),
            "Office" | "Old flat" => ("Wi-Fi", "", false, None),
            "Wired connection 1" => ("Ethernet", "enp42s0", true, None),
            "Office desk" => ("Ethernet", "", false, None),
            "Work WireGuard" => ("WireGuard", "wg0", true, None),
            "Office OpenVPN" => ("OpenVPN", "", false, None),
            _ => return None,
        };
        let vpn = kind_label == "WireGuard" || kind_label == "OpenVPN";
        let addresses: &[&str] = match name {
            "Home" => &["192.168.0.41/24"],
            "Wired connection 1" => &["192.168.0.72/24"],
            "Work WireGuard" => &["10.8.0.2/32"],
            _ => &[],
        };
        let owner = Some(match kind_label {
            "Wi-Fi" => Owner::Wifi {
                ssid: name.to_string(),
                saved_list: false,
            },
            "Ethernet" => Owner::Wired {
                device: "enp42s0".to_string(),
                profile: name.to_string(),
                in_use: false,
            },
            _ => Owner::Vpn {
                name: name.to_string(),
            },
        });
        Some(Details {
            uuid: format!("00000000-0000-4000-8000-{:012}", name.len()),
            kind_label: kind_label.to_string(),
            state: if live { Live::Active } else { Live::Saved },
            device: device.to_string(),
            v4: family(
                live,
                if live && !vpn { addresses } else { &[] },
                if live && !vpn { "192.168.0.1" } else { "" },
                if live && !vpn {
                    &["1.1.1.1", "9.9.9.9"]
                } else {
                    &[]
                },
            ),
            v6: Family::default(),
            mac: if vpn { "" } else { "3C:52:82:11:9A:F0" }.to_string(),
            mtu: if vpn { "1420" } else { "1500" }.to_string(),
            wifi,
            owner,
        })
    }
}

fn device_kind(kind: &str) -> DeviceKind {
    match kind {
        "wifi" => DeviceKind::Wifi,
        "ethernet" => DeviceKind::Ethernet,
        _ => DeviceKind::Other,
    }
}

type Loaded = State<Option<Result<Details, CommandError>>>;

// ---------------------------------------------------------------------------
// The page
// ---------------------------------------------------------------------------

fn reload(loaded: Loaded, name: String) {
    background_then(
        move || read(&name),
        move |result| {
            let mut loaded = loaded;
            // A read that fails while facts are showing keeps them; a flicker to
            // an error every few seconds would be worse than a stale value.
            if result.is_err() && matches!(&*loaded.peek(), Some(Ok(_))) {
                return;
            }
            loaded.set_if_modified(Some(result));
        },
    );
}

/// One row of a fact.
fn fact(title: impl Into<String>, value: &str, mono: bool) -> Option<Element> {
    if value.is_empty() {
        return None;
    }
    let line = row(title);
    Some(if mono {
        line.mono_value(value).into()
    } else {
        line.value(value).into()
    })
}

/// A row for each item of a list, numbered when there are several.
fn list_facts(rows: &mut Vec<Element>, title: &str, items: &[String], mono: bool) {
    for (index, item) in items.iter().enumerate() {
        let label = if items.len() > 1 {
            format!("{title} {}", index + 1)
        } else {
            title.to_string()
        };
        rows.extend(fact(label, item, mono));
    }
}

/// The routing facts of one address family. The live IPv4 address is on the connection's page, so
/// `with_addresses` leaves it out there.
fn family_group(title: &str, family: &Family, with_addresses: bool) -> Option<Element> {
    if family.is_empty() {
        return None;
    }
    let mut rows: Vec<Element> = Vec::new();
    rows.extend(fact("Method", &family.method, false));
    if with_addresses {
        list_facts(&mut rows, "Address", &family.addresses, true);
    }
    rows.extend(fact("Gateway", &family.gateway, true));
    list_facts(&mut rows, "DNS server", &family.dns, true);
    list_facts(&mut rows, "Search domain", &family.domains, false);
    for route in &family.routes {
        // The destination is the row's title, how it is reached its value.
        let (destination, how) = route.split_once(' ').unwrap_or((route, ""));
        rows.extend(fact(format!("Route to {destination}"), how, true));
    }
    if rows.is_empty() {
        return None;
    }
    Some(Group::titled(title).rows(rows).into())
}

/// The groups of technical details. What a connection's page already says (its state, its IP
/// address, its speed and signal, whether it connects by itself) is left out.
fn view(details: &Details) -> Vec<Element> {
    let mut blocks: Vec<Element> = Vec::new();
    if details.state == Live::Saved {
        blocks.push(
            status(Status::Info(
                "This connection is not active. These are its saved settings, not live values."
                    .into(),
            ))
            .into(),
        );
    }

    let mut general: Vec<Element> = Vec::new();
    general.extend(fact("Device", &details.device, false));
    general.extend(fact("Type", &details.kind_label, false));
    general.extend(fact("UUID", &details.uuid, true));
    general.extend(fact("MAC address", &details.mac, true));
    general.extend(fact("MTU", &details.mtu, false));
    if let Some(wifi) = &details.wifi {
        general.extend(fact("Access point", &wifi.bssid, true));
        general.extend(fact("Band", &wifi.band, false));
        general.extend(fact("Channel", &wifi.channel, false));
        general.extend(fact("Security", &wifi.security, false));
    }
    blocks.push(Group::titled("Technical details").rows(general).into());

    blocks.extend(family_group(
        "IPv4",
        &details.v4,
        details.state == Live::Saved,
    ));
    blocks.extend(family_group("IPv6", &details.v6, true));
    blocks
}

/// The read-only technical details of one connection, read again every few seconds while it is
/// on screen. The Advanced page ends with it.
#[derive(PartialEq)]
pub struct TechnicalDetails {
    pub connection: String,
}

impl Component for TechnicalDetails {
    fn render(&self) -> impl IntoElement {
        let loaded: Loaded = use_state(|| None);
        use_hook({
            let name = self.connection.clone();
            move || {
                reload(loaded, name.clone());
                let focus = system::window_focus();
                spawn(async move {
                    loop {
                        timer(POLL).await;
                        if *focus.peek() {
                            reload(loaded, name.clone());
                        }
                    }
                });
            }
        });

        let column = rect().tw_layout(&[flex_col(), w_full(), gap(S6)]);
        match loaded.read().clone() {
            None => column.child(skeleton_rows(&["==--", "----"])),
            Some(Err(error)) => column.child(status(Status::Error(error.0)).action("Try again", {
                let name = self.connection.clone();
                move |_| reload(loaded, name.clone())
            })),
            Some(Ok(details)) => column.children(view(&details)),
        }
    }
}

/// An old address, `network/details/<connection>`: it opens the Advanced page of the connection,
/// where its details are now. A connection with no page of settings shows its details here.
#[derive(PartialEq)]
pub struct DetailsPage {
    pub connection: String,
}

impl Component for DetailsPage {
    fn render(&self) -> impl IntoElement {
        let router = use_router();
        let owner = use_state(|| None::<Result<Option<Owner>, CommandError>>);
        use_hook({
            let name = self.connection.clone();
            move || {
                background_then(
                    move || read(&name).map(|details| details.owner),
                    move |result| {
                        let mut owner = owner;
                        owner.set(Some(result));
                    },
                );
            }
        });
        let found = owner.read().clone();
        use_side_effect_with_deps(&found, move |found| {
            if let Some(Ok(Some(owner))) = found {
                router.open(routes::section(&owner.clone(), &Section::Advanced));
            }
        });

        let page = page_column().child(PageHead::new());
        match found {
            None | Some(Ok(Some(_))) => {
                page.child(status(Status::Loading("Reading connection.".into())))
            }
            Some(Err(error)) => page.child(status(Status::Error(error.0))),
            Some(Ok(None)) => page.child(TechnicalDetails {
                connection: self.connection.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_read_in_words() {
        assert_eq!(
            format_route("dst = 192.168.0.0/24, nh = 0.0.0.0, mt = 101"),
            "192.168.0.0/24 direct, metric 101"
        );
        assert_eq!(
            format_route("dst = 0.0.0.0/0, nh = 192.168.0.1, mt = 101"),
            "0.0.0.0/0 via 192.168.0.1, metric 101"
        );
        assert_eq!(
            format_route("dst = fe80::/64, nh = ::, mt = 1024"),
            "fe80::/64 direct, metric 1024"
        );
        assert_eq!(format_route("something odd"), "something odd");
    }

    #[test]
    fn saved_routes_and_lists() {
        assert_eq!(
            parse_saved_routes(
                "{ ip = 10.0.0.0/8, nh = 10.1.1.1, mt = 50 }, { ip = 172.16.0.0/12 }"
            ),
            ["10.0.0.0/8 via 10.1.1.1, metric 50", "172.16.0.0/12 direct"]
        );
        assert_eq!(
            parse_saved_routes("10.9.0.0/16 192.168.77.2 50, 172.16.0.0/12"),
            [
                "10.9.0.0/16 via 192.168.77.2, metric 50",
                "172.16.0.0/12 direct"
            ]
        );
        assert!(parse_saved_routes("").is_empty());
        assert_eq!(
            parse_saved_list("1.1.1.1,9.9.9.9; 8.8.8.8"),
            ["1.1.1.1", "9.9.9.9", "8.8.8.8"]
        );
        assert!(parse_saved_list("--").is_empty());
    }

    #[test]
    fn live_facts_come_from_indexed_keys() {
        let pairs = Pairs::parse(
            "IP4.ADDRESS[1]:192.168.0.72/24\n\
             IP4.GATEWAY:192.168.0.1\n\
             IP4.ROUTE[1]:dst = 192.168.0.0/24, nh = 0.0.0.0, mt = 101\n\
             IP4.ROUTE[2]:dst = 0.0.0.0/0, nh = 192.168.0.1, mt = 101\n\
             IP4.DNS[1]:1.1.1.1\n\
             IP4.DNS[2]:194.168.4.100\n\
             IP4.DOMAIN[1]:cable.virginm.net\n\
             IP6.ADDRESS[1]:fe80\\:\\:31cc\\:ca7d\\:dd41\\:b7a5/64\n\
             IP6.GATEWAY:\n",
        );
        let v4 = live_family(&pairs, "IP4");
        assert_eq!(v4.addresses, ["192.168.0.72/24"]);
        assert_eq!(v4.dns, ["1.1.1.1", "194.168.4.100"]);
        assert_eq!(v4.domains, ["cable.virginm.net"]);
        assert_eq!(v4.routes.len(), 2);
        let v6 = live_family(&pairs, "IP6");
        assert_eq!(v6.addresses, ["fe80::31cc:ca7d:dd41:b7a5/64"]);
        assert!(v6.gateway.is_empty());
    }

    #[test]
    fn saved_facts_come_from_profile_keys() {
        let pairs = Pairs::parse(
            "ipv4.method:manual\n\
             ipv4.addresses:192.168.1.5/24\n\
             ipv4.gateway:192.168.1.1\n\
             ipv4.dns:1.1.1.1,9.9.9.9\n\
             ipv4.dns-search:home.arpa\n\
             ipv4.routes:{ ip = 10.0.0.0/8, nh = 192.168.1.2 }\n",
        );
        let v4 = saved_family(&pairs, "ipv4");
        assert_eq!(v4.method, "Manual");
        assert_eq!(v4.dns, ["1.1.1.1", "9.9.9.9"]);
        assert_eq!(v4.domains, ["home.arpa"]);
        assert_eq!(v4.routes, ["10.0.0.0/8 via 192.168.1.2"]);
    }

    #[test]
    fn bands_and_owners() {
        assert_eq!(band_of("2437 MHz"), "2.4 GHz");
        assert_eq!(band_of("5180 MHz"), "5 GHz");
        assert_eq!(band_of("5955 MHz"), "6 GHz");
        assert_eq!(band_of(""), "");
        assert_eq!(
            owner_for("802-11-wireless", "Home", ""),
            Some(Owner::Wifi {
                ssid: "Home".into(),
                saved_list: false
            })
        );
        assert_eq!(
            owner_for("802-3-ethernet", "Wired", "enp1s0"),
            Some(Owner::Wired {
                device: "enp1s0".into(),
                profile: "Wired".into(),
                in_use: false
            })
        );
        assert_eq!(owner_for("802-3-ethernet", "Wired", ""), None);
        assert_eq!(
            owner_for("wireguard", "Work", ""),
            Some(Owner::Vpn {
                name: "Work".into()
            })
        );
        assert_eq!(owner_for("bridge", "br0", ""), None);
    }

    #[test]
    fn type_names() {
        assert_eq!(kind_label("802-3-ethernet", ""), "Ethernet");
        assert_eq!(
            kind_label("vpn", "org.freedesktop.NetworkManager.openvpn"),
            "OpenVPN"
        );
        assert_eq!(kind_label("wireguard", ""), "WireGuard");
    }
}
