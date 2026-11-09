//! Network: one page for Wi-Fi, wired Ethernet, mobile broadband and VPNs.
//!
//! The first page is a status header and four groups: the active connection
//! with today's data, the connections, the VPNs and a few more things. Each
//! connection opens one page, built from one template (state and actions, the
//! everyday facts, the everyday settings, one Advanced row). Advanced is one
//! level down and holds the editors and the technical details. Everything
//! comes from NetworkManager (`nmcli`), ModemManager (`mmcli`) and gsettings,
//! on background threads. See `docs/settings.md`, "Network".

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
use crate::routes;
use crate::system::{self, CommandError};
use crate::ui::{Group, PageHead, Status, page_column, row, status, switch_row};
use bingux_ui::prelude::{Location, Target, tr};

pub use ethernet::cached_ethernet_title;
pub use model::{Device, DeviceKind, NetCtx, Overview, QUICK, prefetch, use_net_ctx};
pub use usage::UsageScope;
pub use vpn_add::add_vpn_title;

/// Whose connection a page of settings belongs to. The owner names the
/// NetworkManager profile and the address of the page the settings are below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    /// A saved Wi-Fi network, by profile name. `saved_list` says the page was
    /// opened from the Saved networks list, so Back returns there.
    Wifi { ssid: String, saved_list: bool },
    /// One profile of a wired device. `in_use` makes it the device's own page:
    /// whichever profile the device uses (`profile` is empty until the page has
    /// worked that out). Otherwise it is a profile page below the device's
    /// list of profiles.
    Wired {
        device: String,
        profile: String,
        in_use: bool,
    },
    /// A VPN profile.
    Vpn { name: String },
}

impl Owner {
    /// The NetworkManager profile id.
    pub fn connection_name(&self) -> &str {
        match self {
            Self::Wifi { ssid, .. } => ssid,
            Self::Wired { profile, .. } => profile,
            Self::Vpn { name } => name,
        }
    }

    /// The location of the page that shows this connection.
    pub fn location(&self) -> Location {
        let root = crate::pages::PageId::Network.location();
        match self {
            Self::Wifi {
                ssid,
                saved_list: true,
            } => root.join("wifi").join("saved").join(ssid),
            Self::Wifi { ssid, .. } => root.join("wifi").join("network").join(ssid),
            Self::Wired {
                device,
                in_use: true,
                ..
            } => root.join("ethernet").join(device),
            Self::Wired {
                device, profile, ..
            } => root
                .join("ethernet")
                .join(device)
                .join("profile")
                .join(profile),
            Self::Vpn { name } => root.join("vpn").join(name),
        }
    }
}

/// One page below a connection's page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Section {
    /// The Advanced page: every editor, and the technical details.
    Advanced,
    Ipv4,
    Ipv6,
    Dns,
    /// Wi-Fi key type and password, or wired 802.1X.
    Security,
    /// Name, MAC address, MTU and wake-on-LAN.
    Identity,
    /// The connection's own proxy: none or a PAC URL.
    Proxy,
    /// DHCP, retries, route metrics and the rest of the lesser options.
    Options,
    /// The connection's data usage.
    Usage,
    /// A VPN's tunnel: WireGuard interface and peers, or a plugin's gateway and authentication.
    Tunnel,
    /// One WireGuard peer, by place in the list, below the tunnel.
    Peer(usize),
}

impl Section {
    pub fn title(&self) -> String {
        match self {
            Self::Advanced => tr!("network-section-advanced"),
            Self::Ipv4 => tr!("network-section-ipv4"),
            Self::Ipv6 => tr!("network-section-ipv6"),
            Self::Dns => tr!("network-section-dns"),
            Self::Security => tr!("network-section-security"),
            Self::Identity => tr!("network-section-identity"),
            Self::Proxy => tr!("network-section-proxy"),
            Self::Options => tr!("network-section-options"),
            Self::Usage => tr!("network-section-usage"),
            Self::Tunnel => tr!("network-section-tunnel"),
            Self::Peer(index) => tr!("network-section-peer", number = index + 1),
        }
    }

    /// The location of this page below the connection page at `base`. The
    /// editors sit one level down, below Advanced.
    pub fn location(&self, base: Location) -> Location {
        match self {
            Self::Advanced => base.join("advanced"),
            Self::Ipv4 => base.join("advanced").join("ipv4"),
            Self::Ipv6 => base.join("advanced").join("ipv6"),
            Self::Dns => base.join("advanced").join("dns"),
            Self::Security => base.join("advanced").join("security"),
            Self::Identity => base.join("advanced").join("identity"),
            Self::Proxy => base.join("advanced").join("proxy"),
            Self::Options => base.join("advanced").join("options"),
            Self::Usage => base.join("usage"),
            Self::Tunnel => base.join("tunnel"),
            Self::Peer(index) => base.join("tunnel").join("peer").join(index.to_string()),
        }
    }
}

/// The one-line status under the page title.
fn summary(ov: &Overview, airplane: bool) -> String {
    if airplane {
        return tr!("network-summary-airplane");
    }
    if ov.connectivity == "portal" {
        return tr!("network-summary-sign-in");
    }
    match ov.primary() {
        Some(device) => match device.kind {
            DeviceKind::Wifi => tr!(
                "network-summary-wifi",
                connection = device.connection.as_str()
            ),
            DeviceKind::Modem => tr!("network-summary-modem"),
            _ => tr!("network-summary-ethernet"),
        },
        None if ov.state == "connected" || ov.state.starts_with("connected") => {
            tr!("network-summary-no-internet")
        }
        None => tr!("network-summary-offline"),
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
                ctx.overlays
                    .toast(plain(&error.0, &tr!("network-portal-open-failed")));
            }
        },
    );
}

/// Where the active connection's row leads: the page of that connection.
fn connection_target(ov: &Overview, device: &Device) -> Target {
    match device.kind {
        DeviceKind::Wifi => routes::wifi_network(&device.connection, false),
        DeviceKind::Modem => routes::mobile(),
        _ => routes::ethernet(&device.name, &ethernet::title_of(ov, device)),
    }
}

/// The first group: the active connection, then its data usage today. A
/// captive portal's sign-in row follows when one is needed.
fn status_group(ctx: NetCtx, ov: &Overview, airplane: bool) -> Element {
    let mut group = Group::new(None::<String>);
    match (ov.primary(), airplane) {
        (Some(device), false) => {
            let target = connection_target(ov, device);
            let internet = match ov.connectivity.as_str() {
                "limited" => tr!("network-status-limited"),
                // The sign-in row under this one says it, so it is not said twice.
                "portal" => String::new(),
                "none" => tr!("network-status-no-internet"),
                _ => String::new(),
            };
            let address = device.address4();
            let mut line = row(device.connection.clone())
                .leading(device_icon(device.kind))
                .subtitle(if address.is_empty() {
                    tr!("network-status-connected")
                } else {
                    tr!(
                        "network-status-connected-address",
                        address = address.as_str()
                    )
                })
                .value(internet.clone())
                .navigates()
                .on_press(move |_| ctx.router.open(target.clone()));
            if internet.is_empty() && ov.connectivity != "portal" {
                line = line.leading_tone(crate::ui::Tone::Accent);
            }
            group = group.row(line);
            if ov.connectivity == "portal" {
                group = group.row(
                    row(tr!("network-summary-sign-in"))
                        .navigates()
                        .on_press(move |_| open_portal(ctx)),
                );
            }
        }
        (_, true) => {
            group = group.row(
                row(tr!("network-airplane-on"))
                    .leading(Icon::AirplaneMode)
                    .subtitle(tr!("network-airplane-on-hint")),
            );
        }
        _ => {
            group = group.row(
                row(tr!("network-not-connected"))
                    .leading(Icon::Network)
                    .subtitle(tr!("network-not-connected-hint")),
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
                let message = plain(&failure.0, &tr!("network-status-read-failed"));
                return page_column().child(PageHead::new()).child(
                    status(Status::Error(message))
                        .action(&tr!("network-try-again"), move |_| ctx.reload()),
                );
            }
        };
        let airplane = model::pending_for("airplane").unwrap_or(ov.airplane);
        let head = PageHead::new().subtitle(if loading {
            tr!("network-loading")
        } else {
            summary(&ov, airplane)
        });

        let mut blocks: Vec<Element> = vec![status_group(ctx, &ov, airplane)];

        let mut connections: Vec<Element> = Vec::new();
        connections.extend(wifi::wifi_row(ctx, &ov, airplane));
        connections.extend(ethernet::ethernet_rows(ctx, &ov));
        connections.extend(mobile::mobile_row(ctx, &ov, airplane));
        let connections = if connections.is_empty() {
            Group::titled(&tr!("network-group-connections")).row(Element::from(status(
                Status::Empty(tr!("network-connections-empty").into()),
            )))
        } else {
            Group::titled(&tr!("network-group-connections")).rows(connections)
        };
        blocks.push(Element::from(connections));
        blocks.push(vpn::vpn_group(ctx, &ov));

        let mut more: Vec<Element> = vec![Element::from(switch_row(
            tr!("network-airplane-mode"),
            airplane,
            move |on: bool| {
                model::toggle_link(ctx, "airplane".to_string(), on, move || {
                    model::set_airplane_mode(on)
                })
            },
        ))];
        more.extend(wifi::hotspot_row(ctx, &ov));
        more.push(proxy::proxy_row(ctx));
        blocks.push(Element::from(
            Group::titled(&tr!("network-group-more")).rows(more),
        ));

        veiled(page_column().child(head).children(blocks), loading)
    }
}

/// Log what a command said and return the sentence the person sees instead.
pub fn plain(raw: &str, message: &str) -> String {
    eprintln!("bingux-settings: network: {message} ({raw})");
    message.to_string()
}

/// The error a screen shows when the network settings cannot be read.
pub fn failure_page(error: CommandError, retry: impl Fn() + 'static) -> Element {
    let message = plain(&error.0, &tr!("network-read-failed"));
    page_column()
        .child(status(Status::Error(message)).action(&tr!("network-try-again"), move |_| retry()))
        .into()
}
