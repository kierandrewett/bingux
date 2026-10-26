//! Pages and subpages, addressed.
//!
//! Every sidebar page and every subpage has an address, such as
//! `bingux-settings://wifi/saved` or `bingux-settings://applications/default`.
//! The window shows the address it holds ([`bingux_ui::prelude::use_address`]),
//! so the sidebar, the rows, Back, search and a command-line or `xdg-open`
//! link all move the window the same way: by setting the address.
//!
//! Names (a Wi-Fi network's name, a device's name) are only needed for a
//! header, so they are not looked up when an address is read. A row that opens
//! a subpage remembers the name it had, and the header shows that name. An
//! address opened from outside shows its fallback name, which is the id,
//! MAC or interface the address holds.

use std::cell::RefCell;
use std::collections::HashMap;

use bingux_ui::prelude::{Address, current_address, navigate};
use freya::prelude::*;

use crate::pages::network::{self, Owner, Section, UsageScope};
use crate::pages::{
    PageId, appearance, applications, bluetooth, datetime, displays, keyboard, users,
};

/// The scheme of this application's addresses.
pub const SCHEME: &str = "bingux-settings";

thread_local! {
    /// The name each opened subpage was given by the row that opened it.
    static TITLES: RefCell<HashMap<Address, String>> = RefCell::new(HashMap::new());
}

/// The name remembered for `address`, or `fallback` when none was.
fn title_or(address: &Address, fallback: String) -> String {
    TITLES
        .with(|titles| titles.borrow().get(address).cloned())
        .unwrap_or(fallback)
}

/// The sidebar page an address belongs to: its first segment is a panel
/// name, so `wifi/saved` is on the Network page. `None` when it names no page.
pub fn page_of(address: &Address) -> Option<PageId> {
    PageId::from_panel(address.segments().first()?)
}

impl PageId {
    /// The address of this page's top level, such as `bingux-settings://wifi`.
    pub fn address(self) -> Address {
        Address::new(SCHEME).join(self.panel_name())
    }
}

/// A page below a sidebar page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// One application, by desktop id, with its name for the header.
    App { id: String, name: String },
    /// The default application for each kind of file and link.
    DefaultApps,
    /// One kind of file or link, by its place in the default applications
    /// list, with its name for the header. Its applications are listed there.
    DefaultApp { kind: usize, name: String },
    /// One Bluetooth device, by address, with its name for the header.
    Device { mac: String, name: String },
    /// The Wi-Fi screen: the connected network and the networks in range.
    Wifi,
    /// The saved Wi-Fi networks.
    WifiSaved,
    /// Turning this computer into a Wi-Fi hotspot.
    WifiHotspot,
    /// One Wi-Fi network's settings, by name.
    WifiNetwork { ssid: String },
    /// One wired network device, by interface name, with its connection name for the header.
    Ethernet { device: String, name: String },
    /// One wired profile of a device, by profile name.
    Profile { device: String, name: String },
    /// One VPN profile, by connection name.
    Vpn { name: String },
    /// Adding a VPN: the chooser, or the form for one kind (`wireguard`,
    /// `openvpn`, a plugin's short name, `import`).
    AddVpn { kind: Option<String> },
    /// A section of one connection's settings: IPv4, DNS, Security and so on.
    Section { owner: Owner, section: Section },
    /// The mobile network (modem) screen.
    Mobile,
    /// Choosing the mobile carrier.
    MobileNetwork,
    /// The access point names.
    MobileApns,
    /// One access point name, or a new one when `name` is `None`.
    MobileApn { name: Option<String> },
    /// The system proxy.
    Proxy,
    /// Data usage, for everything, one interface or one connection.
    Usage { scope: UsageScope },
    /// A connection's live addresses, gateway, DNS and routes.
    Details { connection: String },
    /// One user account, by account path, with its full name for the header.
    Account { path: String, name: String },
    /// The time zone list.
    TimeZone,
    /// The installed fonts, for the window and the desktop.
    Font,
    /// One display on the Displays page, by its place in the list, with its
    /// label for the header.
    Display { index: usize, name: String },
    /// The keyboard shortcuts, with a search box.
    Shortcuts,
    /// One input source, by its value in the configuration file, with its
    /// name for the header.
    InputSource {
        source: keyboard::sources::Source,
        name: String,
    },
}

/// The address of the Network page.
fn network_root() -> Address {
    PageId::Network.address()
}

/// `address` with an old-style first segment rewritten to its place under
/// `network/`, so every address that ever opened a Network screen still does:
/// `wifi`, `wifi/saved`, `wifi/network/<ssid>`, `wired/<device>`,
/// `wired/vpn/<name>`, `ethernet/<device>`, `vpn/<name>`, `mobile`, `wwan`,
/// `cellular`, `proxy`, `usage`. Other addresses come back unchanged.
pub fn canonical_address(address: &Address) -> Address {
    let segments: Vec<&str> = address.segments().iter().map(String::as_str).collect();
    let mapped: Vec<&str> = match segments.as_slice() {
        ["network", ..] => return address.clone(),
        ["wifi" | "wi-fi", rest @ ..] => ["network", "wifi"]
            .into_iter()
            .chain(rest.iter().copied())
            .collect(),
        ["wired" | "ethernet" | "vpn", "vpn", name] => vec!["network", "vpn", name],
        ["wired" | "ethernet", device] => vec!["network", "ethernet", device],
        ["vpn", name] => vec!["network", "vpn", name],
        ["wired" | "ethernet" | "vpn"] => vec!["network"],
        ["mobile" | "wwan" | "cellular", rest @ ..] => ["network", "mobile"]
            .into_iter()
            .chain(rest.iter().copied())
            .collect(),
        ["proxy"] => vec!["network", "proxy"],
        ["usage" | "data-usage", rest @ ..] => ["network", "usage"]
            .into_iter()
            .chain(rest.iter().copied())
            .collect(),
        _ => return address.clone(),
    };
    let mut canonical = Address::new(address.scheme());
    for segment in mapped {
        canonical = canonical.join(segment);
    }
    canonical
}

impl Route {
    /// The route for a connection's own address plus the segments below it:
    /// the connection's screen, or one of its sections.
    fn owner_route(owner: Owner, rest: &[&str]) -> Option<Self> {
        if rest.is_empty() {
            return Some(match owner {
                Owner::Wifi { ssid } => Self::WifiNetwork { ssid },
                Owner::Wired { device, profile } => Self::Profile {
                    device,
                    name: profile,
                },
                Owner::Vpn { name } => Self::Vpn { name },
            });
        }
        Some(Self::Section {
            owner,
            section: Section::parse(rest)?,
        })
    }

    pub fn title(&self) -> String {
        match self {
            Self::App { name, .. } => name.clone(),
            Self::Display { name, .. } => name.clone(),
            Self::DefaultApps => "Default applications".to_string(),
            Self::DefaultApp { name, .. } => name.clone(),
            Self::Device { name, .. } => name.clone(),
            Self::Wifi => "Wi-Fi".to_string(),
            Self::WifiSaved => "Saved networks".to_string(),
            Self::WifiHotspot => "Wi-Fi hotspot".to_string(),
            Self::WifiNetwork { ssid } => ssid.clone(),
            Self::Ethernet { device, name } => {
                network::cached_ethernet_title(device).unwrap_or_else(|| name.clone())
            }
            Self::Profile { name, .. } => name.clone(),
            Self::Vpn { name } => name.clone(),
            Self::AddVpn { kind } => network::add_vpn_title(kind.as_deref()),
            Self::Section { section, .. } => section.title(),
            Self::Mobile => "Mobile network".to_string(),
            Self::MobileNetwork => "Network".to_string(),
            Self::MobileApns => "Access point names".to_string(),
            Self::MobileApn { name } => name.clone().unwrap_or_else(|| "New access point".into()),
            Self::Proxy => "Proxy".to_string(),
            Self::Usage { scope } => scope.title(),
            Self::Details { connection } => connection.clone(),
            Self::Account { name, .. } => name.clone(),
            Self::TimeZone => "Time zone".to_string(),
            Self::Font => "Font".to_string(),
            Self::Shortcuts => "Shortcuts".to_string(),
            Self::InputSource { name, .. } => name.clone(),
        }
    }

    /// The address of this subpage.
    pub fn address(&self) -> Address {
        let applications = PageId::Applications.address();
        match self {
            Self::App { id, .. } => applications.join(id),
            Self::DefaultApps => applications.join("default"),
            Self::DefaultApp { kind, .. } => applications.join("default").join(
                applications::KINDS
                    .get(*kind)
                    .map(|kind| kind.title.to_ascii_lowercase())
                    .unwrap_or_default(),
            ),
            Self::Device { mac, .. } => PageId::Bluetooth.address().join(mac),
            Self::Wifi => network_root().join("wifi"),
            Self::WifiSaved => network_root().join("wifi").join("saved"),
            Self::WifiHotspot => network_root().join("wifi").join("hotspot"),
            Self::WifiNetwork { ssid } => Owner::Wifi { ssid: ssid.clone() }.address(),
            Self::Ethernet { device, .. } => network_root().join("ethernet").join(device),
            Self::Profile { device, name } => Owner::Wired {
                device: device.clone(),
                profile: name.clone(),
            }
            .address(),
            Self::Vpn { name } => Owner::Vpn { name: name.clone() }.address(),
            Self::AddVpn { kind } => match kind {
                Some(kind) => network_root().join("add-vpn").join(kind),
                None => network_root().join("add-vpn"),
            },
            Self::Section { owner, section } => section.address(owner.address()),
            Self::Mobile => network_root().join("mobile"),
            Self::MobileNetwork => network_root().join("mobile").join("network"),
            Self::MobileApns => network_root().join("mobile").join("apn"),
            Self::MobileApn { name } => match name {
                Some(name) => network_root().join("mobile").join("apn").join(name),
                None => network_root().join("mobile").join("apn-add"),
            },
            Self::Proxy => network_root().join("proxy"),
            Self::Usage { scope } => scope.address(network_root().join("usage")),
            Self::Details { connection } => network_root().join("details").join(connection),
            Self::Account { path, .. } => PageId::Users.address().join(path),
            Self::TimeZone => PageId::DateTime.address().join("timezone"),
            Self::Font => PageId::Appearance.address().join("font"),
            Self::Display { index, .. } => PageId::Displays.address().join(index.to_string()),
            Self::Shortcuts => PageId::Keyboard.address().join("shortcuts"),
            Self::InputSource { source, .. } => PageId::Keyboard
                .address()
                .join("input")
                .join(&source.kind)
                .join(&source.id),
        }
    }

    /// The subpage `address` names, or `None` when it names a sidebar page
    /// or nothing. Reads only the address and the names remembered for it,
    /// so it is cheap enough to run on every render.
    pub fn from_address(address: &Address) -> Option<Self> {
        if address.scheme() != SCHEME {
            return None;
        }
        let canonical = canonical_address(address);
        let address = &canonical;
        let segments: Vec<&str> = address.segments().iter().map(String::as_str).collect();
        let route = match segments.as_slice() {
            ["applications", "default"] => Self::DefaultApps,
            ["applications", "default", kind] => {
                let index = applications::KINDS
                    .iter()
                    .position(|known| known.title.eq_ignore_ascii_case(kind))?;
                Self::DefaultApp {
                    kind: index,
                    name: title_or(address, applications::KINDS[index].title.to_string()),
                }
            }
            ["applications", id] => Self::App {
                id: id.to_string(),
                name: title_or(
                    address,
                    crate::apps::cached_name(id).unwrap_or_else(|| id.to_string()),
                ),
            },
            ["bluetooth", mac] => Self::Device {
                mac: mac.to_string(),
                name: title_or(address, mac.to_string()),
            },
            ["network", "wifi"] => Self::Wifi,
            ["network", "wifi", "saved"] => Self::WifiSaved,
            ["network", "wifi", "hotspot"] => Self::WifiHotspot,
            ["network", "ethernet", device] => Self::Ethernet {
                device: device.to_string(),
                name: title_or(address, device.to_string()),
            },
            ["network", "ethernet", device, "profile", name, rest @ ..] => Self::owner_route(
                Owner::Wired {
                    device: device.to_string(),
                    profile: name.to_string(),
                },
                rest,
            )?,
            ["network", "wifi", "network", ssid, rest @ ..] => Self::owner_route(
                Owner::Wifi {
                    ssid: ssid.to_string(),
                },
                rest,
            )?,
            ["network", "vpn", name, rest @ ..] => Self::owner_route(
                Owner::Vpn {
                    name: name.to_string(),
                },
                rest,
            )?,
            ["network", "add-vpn"] => Self::AddVpn { kind: None },
            ["network", "add-vpn", kind] => Self::AddVpn {
                kind: Some(kind.to_string()),
            },
            ["network", "mobile"] => Self::Mobile,
            ["network", "mobile", "network"] => Self::MobileNetwork,
            ["network", "mobile", "apn"] => Self::MobileApns,
            ["network", "mobile", "apn-add"] => Self::MobileApn { name: None },
            ["network", "mobile", "apn", name] => Self::MobileApn {
                name: Some(name.to_string()),
            },
            ["network", "proxy"] => Self::Proxy,
            ["network", "usage", rest @ ..] => Self::Usage {
                scope: UsageScope::from_segments(rest)?,
            },
            ["network", "details", connection] => Self::Details {
                connection: connection.to_string(),
            },
            ["users", path] => Self::Account {
                path: path.to_string(),
                name: title_or(address, path.to_string()),
            },
            ["datetime", "timezone"] => Self::TimeZone,
            ["background", "font"] => Self::Font,
            ["display", index] => {
                let index: usize = index.parse().ok()?;
                Self::Display {
                    index,
                    name: title_or(address, format!("Display {}", index + 1)),
                }
            }
            ["keyboard", "shortcuts"] => Self::Shortcuts,
            ["keyboard", "input", kind, id] => Self::InputSource {
                source: keyboard::sources::Source {
                    kind: kind.to_string(),
                    id: id.to_string(),
                },
                name: title_or(address, id.to_string()),
            },
            _ => return None,
        };
        Some(route)
    }

    /// The element for this page.
    pub fn render(&self) -> Element {
        match self {
            Self::App { id, .. } => applications::AppPage { id: id.clone() }.into(),
            Self::DefaultApps => applications::DefaultAppsPage.into(),
            Self::DefaultApp { kind, .. } => applications::DefaultKindPage { kind: *kind }.into(),
            Self::Device { mac, .. } => bluetooth::BluetoothDevicePage { mac: mac.clone() }.into(),
            Self::Wifi => network::wifi::WifiPage.into(),
            Self::WifiSaved => network::wifi::SavedNetworksPage.into(),
            Self::WifiHotspot => network::wifi::HotspotPage.into(),
            Self::WifiNetwork { ssid } => {
                network::wifi::WifiNetworkPage { ssid: ssid.clone() }.into()
            }
            Self::Ethernet { device, .. } => network::ethernet::EthernetPage {
                device: device.clone(),
                name: self.title(),
            }
            .into(),
            Self::Profile { device, name } => network::ethernet::ProfilePage {
                device: device.clone(),
                name: name.clone(),
            }
            .into(),
            Self::Vpn { name } => network::vpn::VpnPage { name: name.clone() }.into(),
            Self::AddVpn { kind } => network::vpn_add::AddVpnPage { kind: kind.clone() }.into(),
            Self::Section { owner, section } => match section {
                Section::Tunnel => network::vpn_tunnel::TunnelPage {
                    owner: owner.clone(),
                }
                .into(),
                Section::Peer(index) => network::vpn_tunnel::PeerPage {
                    owner: owner.clone(),
                    index: *index,
                }
                .into(),
                other => network::profile::SectionPage {
                    owner: owner.clone(),
                    section: other.clone(),
                }
                .into(),
            },
            Self::Mobile => network::mobile::MobilePage.into(),
            Self::MobileNetwork => network::mobile::MobileNetworkPage.into(),
            Self::MobileApns => network::mobile::ApnListPage.into(),
            Self::MobileApn { name } => network::mobile::ApnPage { name: name.clone() }.into(),
            Self::Proxy => network::proxy::ProxyPage.into(),
            Self::Usage { scope } => network::usage::UsagePage {
                scope: scope.clone(),
            }
            .into(),
            Self::Details { connection } => network::details::DetailsPage {
                connection: connection.clone(),
            }
            .into(),
            Self::Account { path, .. } => users::AccountPage { path: path.clone() }.into(),
            Self::TimeZone => datetime::TimeZonePage.into(),
            Self::Font => appearance::FontPage.into(),
            Self::Display { index, .. } => displays::DisplayPage { index: *index }.into(),
            Self::Shortcuts => keyboard::ShortcutsPage.into(),
            Self::InputSource { source, .. } => keyboard::InputSourcePage {
                source: source.clone(),
            }
            .into(),
        }
    }
}

/// Moves the window. Every method sets the window's address, so the window
/// shows the same thing whichever control moved it.
#[derive(Clone, Copy)]
pub struct Navigator;

impl Navigator {
    /// Opens `route`, remembering its name for the header.
    pub fn push(&self, route: Route) {
        TITLES.with(|titles| {
            titles.borrow_mut().insert(route.address(), route.title());
        });
        navigate(route.address());
    }

    /// Goes back one level: to the parent address when it is a subpage, and
    /// otherwise to the top of the current page.
    pub fn pop(&self) {
        let Some(current) = current_address() else {
            return;
        };
        // An old-style address goes back to where its canonical form does.
        let mut parent = canonical_address(&current).parent();
        let target = loop {
            match parent {
                Some(address) if Route::from_address(&address).is_some() => break address,
                Some(address) => parent = address.parent(),
                None => match page_of(&current) {
                    Some(page) => break page.address(),
                    None => return,
                },
            }
        };
        navigate(target);
    }

    /// Opens sidebar page `target` at its top level, as if the sidebar chose it.
    pub fn goto(&self, target: PageId) {
        navigate(target.address());
    }
}

/// The navigator. It holds no state: the window's address is the state.
pub fn use_navigator() -> Navigator {
    Navigator
}
