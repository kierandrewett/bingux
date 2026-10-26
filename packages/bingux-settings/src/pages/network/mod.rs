//! Network: one page for Wi-Fi, wired Ethernet, mobile broadband and VPNs.
//!
//! The page is a summary and a list of connections. Each connection opens its
//! own screen, where its state and its settings live; the heavy settings
//! (IPv4, DNS, Security...) are subpages of that screen. Everything comes from
//! NetworkManager (`nmcli`), ModemManager (`mmcli`) and gsettings, on
//! background threads. See `docs/settings.md`, "Network".

pub mod details;
pub mod ethernet;
pub mod mobile;
pub mod model;
pub mod profile;
pub mod proxy;
pub mod usage;
pub mod vpn;
pub mod vpn_add;
pub mod vpn_tunnel;
pub mod wifi;

use freya::prelude::*;

use super::loading::veiled;
use crate::icons::Icon;
use crate::routes::Route;
use crate::system::{self, CommandError};
use crate::ui::{Group, PageHead, Status, page_column, row, status, switch_row};
use bingux_ui::prelude::Address;

pub use ethernet::cached_ethernet_title;
pub use model::{Device, DeviceKind, NetCtx, Overview, QUICK, prefetch, use_net_ctx};
pub use usage::UsageScope;
pub use vpn_add::add_vpn_title;

/// Whose connection a section of settings belongs to. The owner names the
/// NetworkManager profile and the address of the screen the section is below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    /// A saved Wi-Fi network, by name (its profile has the same name).
    Wifi { ssid: String },
    /// One profile of a wired device.
    Wired { device: String, profile: String },
    /// A VPN profile.
    Vpn { name: String },
}

impl Owner {
    /// The NetworkManager profile id.
    pub fn connection_name(&self) -> &str {
        match self {
            Self::Wifi { ssid } => ssid,
            Self::Wired { profile, .. } => profile,
            Self::Vpn { name } => name,
        }
    }

    /// The address of the screen that shows this connection's settings.
    pub fn address(&self) -> Address {
        let root = crate::pages::PageId::Network.address();
        match self {
            Self::Wifi { ssid } => root.join("wifi").join("network").join(ssid),
            Self::Wired { device, profile } => root
                .join("ethernet")
                .join(device)
                .join("profile")
                .join(profile),
            Self::Vpn { name } => root.join("vpn").join(name),
        }
    }
}

/// One section of a connection's settings, a subpage of its screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Section {
    Ipv4,
    Ipv6,
    Dns,
    /// Wi-Fi key type and password, or wired 802.1X.
    Security,
    /// Name, MAC address, MTU and wake-on-LAN.
    Identity,
    /// The connection's own proxy: none or a PAC URL.
    Proxy,
    Advanced,
    /// A VPN's tunnel: WireGuard interface and peers, or a plugin's gateway and authentication.
    Tunnel,
    /// One WireGuard peer, by place in the list, below the tunnel.
    Peer(usize),
}

impl Section {
    pub fn title(&self) -> String {
        match self {
            Self::Ipv4 => "IPv4".to_string(),
            Self::Ipv6 => "IPv6".to_string(),
            Self::Dns => "DNS".to_string(),
            Self::Security => "Security".to_string(),
            Self::Identity => "Identity".to_string(),
            Self::Proxy => "Proxy".to_string(),
            Self::Advanced => "Advanced".to_string(),
            Self::Tunnel => "Tunnel".to_string(),
            Self::Peer(index) => format!("Peer {}", index + 1),
        }
    }

    /// The address of this section below the connection screen at `base`.
    pub fn address(&self, base: Address) -> Address {
        match self {
            Self::Ipv4 => base.join("ipv4"),
            Self::Ipv6 => base.join("ipv6"),
            Self::Dns => base.join("dns"),
            Self::Security => base.join("security"),
            Self::Identity => base.join("identity"),
            Self::Proxy => base.join("proxy"),
            Self::Advanced => base.join("advanced"),
            Self::Tunnel => base.join("tunnel"),
            Self::Peer(index) => base.join("tunnel").join("peer").join(index.to_string()),
        }
    }

    /// The section named by the address segments below a connection screen.
    pub fn parse(segments: &[&str]) -> Option<Self> {
        Some(match segments {
            ["ipv4"] => Self::Ipv4,
            ["ipv6"] => Self::Ipv6,
            ["dns"] => Self::Dns,
            ["security"] => Self::Security,
            ["identity"] => Self::Identity,
            ["proxy"] => Self::Proxy,
            ["advanced"] => Self::Advanced,
            ["tunnel"] => Self::Tunnel,
            ["tunnel", "peer", index] => Self::Peer(index.parse().ok()?),
            _ => return None,
        })
    }
}

/// The one-line status under the page title.
fn summary(ov: &Overview, airplane: bool) -> String {
    if airplane {
        return "Airplane mode".to_string();
    }
    if ov.connectivity == "portal" {
        return "Sign-in required".to_string();
    }
    match ov.primary() {
        Some(device) => match device.kind {
            DeviceKind::Wifi => format!("Online through Wi-Fi · {}", device.connection),
            DeviceKind::Modem => "Online through the mobile network".to_string(),
            _ => "Online through Ethernet".to_string(),
        },
        None if ov.state == "connected" || ov.state.starts_with("connected") => {
            "Connected, no internet route".to_string()
        }
        None => "Offline".to_string(),
    }
}

/// The icon for a device kind.
pub fn device_icon(kind: DeviceKind) -> Icon {
    match kind {
        DeviceKind::Wifi => Icon::NetworkWirelessSignalExcellent,
        DeviceKind::Modem => Icon::Network,
        DeviceKind::Ethernet => Icon::NetworkWired,
        DeviceKind::WireGuard | DeviceKind::Tun => Icon::NetworkVpn,
        DeviceKind::Other => Icon::Network,
    }
}

/// A blank overview with the shape of a loaded one, drawn under the veil
/// while the first read runs, so every group is placed before values arrive.
fn placeholder() -> Overview {
    let device = |name: &str, kind| Device {
        name: name.to_string(),
        kind,
        state: "connected".to_string(),
        connection: "—".to_string(),
        uuid: String::new(),
        hwaddr: String::new(),
        speed: None,
        carrier: true,
        mtu: None,
        ipv4: vec!["—".to_string()],
        ipv6: Vec::new(),
        gateway4: "—".to_string(),
        gateway6: String::new(),
        dns: vec!["—".to_string()],
    };
    Overview {
        state: "connected".to_string(),
        connectivity: "full".to_string(),
        wifi_hw: "enabled".to_string(),
        wifi_on: true,
        wwan_hw: "missing".to_string(),
        wwan_on: false,
        airplane: false,
        devices: vec![
            device("—", DeviceKind::Wifi),
            device("—", DeviceKind::Ethernet),
        ],
        vpns: Vec::new(),
    }
}

/// NetworkManager's connectivity check page, which a captive portal redirects.
fn portal_uri() -> String {
    let fallback = "http://nmcheck.gnome.org/".to_string();
    let Ok(text) = system::run(
        "busctl",
        &[
            "--system",
            "get-property",
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager",
            "org.freedesktop.NetworkManager",
            "ConnectivityCheckUri",
        ],
        QUICK,
    ) else {
        return fallback;
    };
    // `s "http://..."`
    text.split('"')
        .nth(1)
        .filter(|uri| !uri.is_empty())
        .map(str::to_string)
        .unwrap_or(fallback)
}

fn open_portal(ctx: NetCtx) {
    system::background_then(
        || system::run("xdg-open", &[portal_uri().as_str()], QUICK),
        move |result| {
            if let Err(error) = result {
                ctx.overlays.toast(error.0);
            }
        },
    );
}

/// The summary group: what the computer is online through, its addresses,
/// the sign-in hint for a captive portal, and the data usage.
fn status_group(ctx: NetCtx, ov: &Overview, airplane: bool) -> Element {
    let mut group = Group::new(None::<String>);
    match (ov.primary(), airplane) {
        (Some(device), false) => {
            let connection = device.connection.clone();
            let internet = match ov.connectivity.as_str() {
                "full" => "Internet",
                "limited" => "Limited",
                "portal" => "Sign-in required",
                "none" => "No internet",
                _ => "",
            };
            let kind = match device.kind {
                DeviceKind::Wifi => "Wi-Fi",
                DeviceKind::Modem => "Mobile network",
                _ => "Ethernet",
            };
            group = group.row(
                row(device.connection.clone())
                    .leading(device_icon(device.kind))
                    .subtitle(format!("{kind} · {}", device.name))
                    .value(internet)
                    .navigates()
                    .on_press(move |_| {
                        ctx.navigator.push(Route::Details {
                            connection: connection.clone(),
                        })
                    }),
            );
            if !device.ipv4.is_empty() {
                group = group.row(row("IP address").value(device.address4()));
            }
            if !device.gateway4.is_empty() {
                group = group.row(row("Gateway").value(device.gateway4.clone()));
            }
            if !device.dns.is_empty() {
                group = group.row(row("DNS").value(device.dns.join(", ")));
            }
            if ov.connectivity == "portal" {
                group = group.row(
                    row("Sign-in required")
                        .subtitle("Open the network's sign-in page to get internet access.")
                        .navigates()
                        .on_press(move |_| open_portal(ctx)),
                );
            }
        }
        (_, true) => {
            group = group.row(
                row("Airplane mode is on")
                    .subtitle("Every radio is off. Turn airplane mode off to connect."),
            );
        }
        _ => {
            group = group.row(
                row("Not connected").subtitle("Choose a connection below, or plug in a cable."),
            );
        }
    }
    group = group.row(usage::usage_row(ctx));
    Element::from(group)
}

#[derive(PartialEq)]
pub struct NetworkPage;

impl Component for NetworkPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_net_ctx();
        wifi::use_row_refresh();
        let loaded = ctx.overview.read().clone();
        let loading = loaded.is_none();
        let ov = match loaded {
            None => placeholder(),
            Some(Ok(ov)) => ov,
            Some(Err(failure)) => {
                return page_column().child(PageHead::new()).child(
                    status(Status::Error(failure.0)).action("Try again", move |_| ctx.reload()),
                );
            }
        };
        let airplane = model::pending_for("airplane").unwrap_or(ov.airplane);
        let head = PageHead::new().subtitle(if loading {
            "—".to_string()
        } else {
            summary(&ov, airplane)
        });

        let mut blocks: Vec<Element> = vec![status_group(ctx, &ov, airplane)];
        blocks.push(Element::from(Group::new(None::<String>).row(switch_row(
            "Airplane mode",
            airplane,
            move |on: bool| {
                model::toggle_link(ctx, "airplane".to_string(), on, move || {
                    model::set_airplane_mode(on)
                })
            },
        ))));

        let mut connections: Vec<Element> = Vec::new();
        connections.extend(wifi::wifi_row(ctx, &ov, airplane));
        connections.extend(ethernet::ethernet_rows(ctx, &ov));
        connections.extend(mobile::mobile_row(ctx, &ov, airplane));
        let connections = if connections.is_empty() {
            Group::titled("Connections").row(Element::from(status(Status::Empty(
                "No network devices.".into(),
            ))))
        } else {
            Group::titled("Connections").rows(connections)
        };
        blocks.push(Element::from(connections));
        blocks.push(vpn::vpn_group(ctx, &ov));

        let mut more: Vec<Element> = vec![proxy::proxy_row(ctx)];
        more.extend(wifi::hotspot_row(ctx, &ov));
        blocks.push(Element::from(Group::titled("More").rows(more)));

        veiled(page_column().child(head).children(blocks), loading)
    }
}

/// The error a screen shows when NetworkManager cannot be asked.
pub fn failure_page(error: CommandError, retry: impl Fn() + 'static) -> Element {
    page_column()
        .child(status(Status::Error(error.0)).action("Try again", move |_| retry()))
        .into()
}
