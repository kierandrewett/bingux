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
    Hotspot,
    /// One Wi-Fi network, by name. `saved_list` says it was opened from the
    /// Saved networks list, which Back returns to.
    WifiNetwork { ssid: String, saved_list: bool },
    /// One wired network device, by interface name, with its connection name
    /// for the header. The page is the connection page of the profile the
    /// device uses.
    Ethernet { device: String, name: String },
    /// The profiles of a wired device that has more than one.
    EthernetProfiles { device: String },
    /// One wired profile of a device that is not the one in use, by profile name.
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
    /// The mobile network's advanced settings: carrier, access points, SIM, modem.
    MobileAdvanced,
    /// The mobile network's data usage.
    MobileUsage,
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
    /// An old address: a connection's details. It opens the Advanced page of
    /// the connection, where the technical details are.
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

/// `address` with an old-style address rewritten to where it lives now, so
/// every address that ever opened a Network screen still does.
///
/// First segments: `wifi`, `wifi/saved`, `wifi/network/<ssid>`,
/// `wired/<device>`, `wired/vpn/<name>`, `ethernet/<device>`, `vpn/<name>`,
/// `mobile`, `wwan`, `cellular`, `proxy` and `usage` are below `network/`.
/// Then `network/wifi/hotspot` is `network/hotspot`, the mobile carrier and
/// access point pages are below `network/mobile/advanced`, and a connection's
/// `ipv4`, `ipv6`, `dns`, `security`, `identity` and `proxy` pages are below
/// its `advanced` page. Other addresses come back unchanged.
pub fn canonical_address(address: &Address) -> Address {
    let segments: Vec<&str> = address.segments().iter().map(String::as_str).collect();
    let under_network: Vec<&str> = match segments.as_slice() {
        ["network", ..] => segments.clone(),
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
    let mapped = upgrade_network_address(under_network);
    if mapped == segments {
        return address.clone();
    }
    let mut canonical = Address::new(address.scheme());
    for segment in mapped {
        canonical = canonical.join(segment);
    }
    canonical
}

/// The editors that were pages of their own below a connection before
/// Advanced held them.
const EDITORS: [&str; 6] = ["ipv4", "ipv6", "dns", "security", "identity", "proxy"];

/// Moves an address below `network/` to where its page is now.
fn upgrade_network_address(segments: Vec<&str>) -> Vec<&str> {
    let at = |index: usize| index.min(segments.len());
    match segments.as_slice() {
        ["network", "wifi", "hotspot"] => vec!["network", "hotspot"],
        ["network", "mobile", "network" | "apn" | "apn-add", ..] => {
            let mut mapped = segments[..at(2)].to_vec();
            mapped.push("advanced");
            mapped.extend_from_slice(&segments[at(2)..]);
            mapped
        }
        ["network", "wifi", "network" | "saved", _, editor]
        | ["network", "ethernet", _, "profile", _, editor]
        | ["network", "vpn", _, editor]
            if EDITORS.contains(editor) =>
        {
            let mut mapped = segments[..segments.len() - 1].to_vec();
            mapped.push("advanced");
            mapped.push(editor);
            mapped
        }
        _ => segments,
    }
}

impl Route {
    /// The route for a connection's own address plus the segments below it:
    /// the connection's page, or one of the pages below it.
    fn owner_route(owner: Owner, rest: &[&str]) -> Option<Self> {
        if rest.is_empty() {
            return Some(match owner {
                Owner::Wifi { ssid, saved_list } => Self::WifiNetwork { ssid, saved_list },
                Owner::Wired {
                    device,
                    in_use: true,
                    ..
                } => Self::Ethernet {
                    name: device.clone(),
                    device,
                },
                Owner::Wired {
                    device, profile, ..
                } => Self::Profile {
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
            Self::Hotspot => "Wi-Fi hotspot".to_string(),
            Self::WifiNetwork { ssid, .. } => ssid.clone(),
            Self::Ethernet { device, name } => {
                network::cached_ethernet_title(device).unwrap_or_else(|| name.clone())
            }
            Self::EthernetProfiles { .. } => "Profiles".to_string(),
            Self::Profile { name, .. } => name.clone(),
            Self::Vpn { name } => name.clone(),
            Self::AddVpn { kind } => network::add_vpn_title(kind.as_deref()),
            Self::Section { section, .. } => section.title(),
            Self::Mobile => "Mobile network".to_string(),
            Self::MobileAdvanced => "Advanced".to_string(),
            Self::MobileUsage => "Data usage".to_string(),
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
            Self::Hotspot => network_root().join("hotspot"),
            Self::WifiNetwork { ssid, saved_list } => Owner::Wifi {
                ssid: ssid.clone(),
                saved_list: *saved_list,
            }
            .address(),
            Self::Ethernet { device, .. } => network_root().join("ethernet").join(device),
            Self::EthernetProfiles { device } => network_root()
                .join("ethernet")
                .join(device)
                .join("profile"),
            Self::Profile { device, name } => Owner::Wired {
                device: device.clone(),
                profile: name.clone(),
                in_use: false,
            }
            .address(),
            Self::Vpn { name } => Owner::Vpn { name: name.clone() }.address(),
            Self::AddVpn { kind } => match kind {
                Some(kind) => network_root().join("add-vpn").join(kind),
                None => network_root().join("add-vpn"),
            },
            Self::Section { owner, section } => section.address(owner.address()),
            Self::Mobile => network_root().join("mobile"),
            Self::MobileAdvanced => network_root().join("mobile").join("advanced"),
            Self::MobileUsage => network_root().join("mobile").join("usage"),
            Self::MobileNetwork => network_root()
                .join("mobile")
                .join("advanced")
                .join("network"),
            Self::MobileApns => network_root().join("mobile").join("advanced").join("apn"),
            Self::MobileApn { name } => match name {
                Some(name) => network_root()
                    .join("mobile")
                    .join("advanced")
                    .join("apn")
                    .join(name),
                None => network_root()
                    .join("mobile")
                    .join("advanced")
                    .join("apn-add"),
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
            ["network", "hotspot"] => Self::Hotspot,
            ["network", "ethernet", device, "profile"] => Self::EthernetProfiles {
                device: device.to_string(),
            },
            ["network", "ethernet", device, "profile", name, rest @ ..] => Self::owner_route(
                Owner::Wired {
                    device: device.to_string(),
                    profile: name.to_string(),
                    in_use: false,
                },
                rest,
            )?,
            ["network", "ethernet", device] => Self::Ethernet {
                device: device.to_string(),
                name: title_or(address, device.to_string()),
            },
            ["network", "ethernet", device, rest @ ..] => Self::owner_route(
                Owner::Wired {
                    device: device.to_string(),
                    profile: String::new(),
                    in_use: true,
                },
                rest,
            )?,
            ["network", "wifi", kind @ ("network" | "saved"), ssid, rest @ ..] => {
                Self::owner_route(
                    Owner::Wifi {
                        ssid: ssid.to_string(),
                        saved_list: *kind == "saved",
                    },
                    rest,
                )?
            }
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
            ["network", "mobile", "advanced"] => Self::MobileAdvanced,
            ["network", "mobile", "usage"] => Self::MobileUsage,
            ["network", "mobile", "advanced", "network"] => Self::MobileNetwork,
            ["network", "mobile", "advanced", "apn"] => Self::MobileApns,
            ["network", "mobile", "advanced", "apn-add"] => Self::MobileApn { name: None },
            ["network", "mobile", "advanced", "apn", name] => Self::MobileApn {
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
            Self::Hotspot => network::wifi::HotspotPage.into(),
            Self::WifiNetwork { ssid, saved_list } => network::wifi::WifiNetworkPage {
                ssid: ssid.clone(),
                saved_list: *saved_list,
            }
            .into(),
            Self::Ethernet { device, .. } => network::ethernet::EthernetPage {
                device: device.clone(),
            }
            .into(),
            Self::EthernetProfiles { device } => network::ethernet::ProfilesPage {
                device: device.clone(),
            }
            .into(),
            Self::Profile { device, name } => network::ethernet::ProfilePage {
                device: device.clone(),
                name: name.clone(),
            }
            .into(),
            Self::Vpn { name } => network::vpn::VpnPage { name: name.clone() }.into(),
            Self::AddVpn { kind } => network::vpn_add::AddVpnPage { kind: kind.clone() }.into(),
            Self::Section { owner, section } => match (section, owner) {
                (Section::Tunnel, _) => network::vpn_tunnel::TunnelPage {
                    owner: owner.clone(),
                }
                .into(),
                (Section::Peer(index), _) => network::vpn_tunnel::PeerPage {
                    owner: owner.clone(),
                    index: *index,
                }
                .into(),
                // A wired connection may have to work out which profile it is first.
                (other, Owner::Wired { .. }) => network::ethernet::WiredSection {
                    owner: owner.clone(),
                    section: other.clone(),
                }
                .into(),
                (Section::Usage, _) => network::usage::UsagePage {
                    scope: UsageScope::Connection(owner.connection_name().to_string()),
                }
                .into(),
                (other, _) => network::profile::SectionPage {
                    owner: owner.clone(),
                    section: other.clone(),
                    extra: None,
                }
                .into(),
            },
            Self::Mobile => network::mobile::MobilePage.into(),
            Self::MobileAdvanced => network::mobile::MobileAdvancedPage.into(),
            Self::MobileUsage => network::mobile::MobileUsagePage.into(),
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
        if let Some(target) = back_target(&current) {
            navigate(target);
        }
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

/// Where Back goes from `current`: the nearest parent address that is a page, else the top of the
/// sidebar page it is on. An old-style address goes back to where its canonical form does.
pub fn back_target(current: &Address) -> Option<Address> {
    let mut parent = canonical_address(current).parent();
    loop {
        match parent {
            Some(address) if Route::from_address(&address).is_some() => return Some(address),
            Some(address) => parent = address.parent(),
            None => return page_of(current).map(PageId::address),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(text: &str) -> Address {
        Address::parse(&format!("{SCHEME}://{text}")).unwrap()
    }

    fn canonical(text: &str) -> String {
        canonical_address(&address(text)).segments().join("/")
    }

    fn back(text: &str) -> String {
        back_target(&address(text)).unwrap().segments().join("/")
    }

    #[test]
    fn old_editor_addresses_move_below_advanced() {
        assert_eq!(
            canonical("network/ethernet/enp42s0/profile/Office/ipv4"),
            "network/ethernet/enp42s0/profile/Office/advanced/ipv4"
        );
        assert_eq!(
            canonical("network/wifi/network/Home/security"),
            "network/wifi/network/Home/advanced/security"
        );
        assert_eq!(
            canonical("network/vpn/Work/dns"),
            "network/vpn/Work/advanced/dns"
        );
        assert_eq!(
            canonical("wifi/network/Home/proxy"),
            "network/wifi/network/Home/advanced/proxy"
        );
        // Not an editor of a connection: unchanged.
        assert_eq!(canonical("network/vpn/Work/tunnel"), "network/vpn/Work/tunnel");
        assert_eq!(
            canonical("network/wifi/network/Home/advanced"),
            "network/wifi/network/Home/advanced"
        );
    }

    #[test]
    fn old_pages_move_to_where_they_are_now() {
        assert_eq!(canonical("network/wifi/hotspot"), "network/hotspot");
        assert_eq!(canonical("wifi/hotspot"), "network/hotspot");
        assert_eq!(
            canonical("mobile/apn/Vodafone"),
            "network/mobile/advanced/apn/Vodafone"
        );
        assert_eq!(canonical("cellular/network"), "network/mobile/advanced/network");
        assert_eq!(canonical("network/mobile"), "network/mobile");
        assert_eq!(canonical("wired/enp42s0"), "network/ethernet/enp42s0");
    }

    #[test]
    fn every_network_address_names_a_page() {
        for text in [
            "network/wifi",
            "network/wifi/saved",
            "network/wifi/saved/Home",
            "network/wifi/network/Home",
            "network/wifi/network/Home/advanced",
            "network/wifi/network/Home/advanced/ipv4",
            "network/wifi/network/Home/usage",
            "network/hotspot",
            "network/ethernet/enp42s0",
            "network/ethernet/enp42s0/advanced",
            "network/ethernet/enp42s0/advanced/identity",
            "network/ethernet/enp42s0/usage",
            "network/ethernet/enp42s0/profile",
            "network/ethernet/enp42s0/profile/Office",
            "network/ethernet/enp42s0/profile/Office/advanced/options",
            "network/vpn/Work",
            "network/vpn/Work/tunnel",
            "network/vpn/Work/tunnel/peer/1",
            "network/vpn/Work/advanced",
            "network/add-vpn/wireguard",
            "network/mobile",
            "network/mobile/advanced",
            "network/mobile/advanced/apn/Vodafone",
            "network/mobile/usage",
            "network/proxy",
            "network/usage/connection/Home",
            "network/details/Home",
        ] {
            assert!(
                Route::from_address(&address(text)).is_some(),
                "{text} names no page"
            );
        }
    }

    #[test]
    fn a_page_opens_at_the_address_it_was_made_from() {
        for text in [
            "network/wifi/saved/Home/advanced/dns",
            "network/ethernet/enp42s0/advanced/ipv6",
            "network/ethernet/enp42s0/profile/Office/usage",
            "network/vpn/Work/usage",
            "network/mobile/advanced/apn-add",
            "network/hotspot",
        ] {
            let route = Route::from_address(&address(text)).unwrap();
            assert_eq!(route.address().segments().join("/"), text);
        }
    }

    #[test]
    fn back_always_goes_to_the_page_that_holds_the_row() {
        // Network > Ethernet > Advanced > IPv4, and back out of it.
        assert_eq!(
            back("network/ethernet/enp42s0/advanced/ipv4"),
            "network/ethernet/enp42s0/advanced"
        );
        assert_eq!(
            back("network/ethernet/enp42s0/advanced"),
            "network/ethernet/enp42s0"
        );
        assert_eq!(back("network/ethernet/enp42s0"), "network");
        // A profile opens from the list of profiles, which sits below the device.
        assert_eq!(
            back("network/ethernet/enp42s0/profile/Office"),
            "network/ethernet/enp42s0/profile"
        );
        assert_eq!(
            back("network/ethernet/enp42s0/profile"),
            "network/ethernet/enp42s0"
        );
        // A Wi-Fi network opens from Wi-Fi, or from Saved networks.
        assert_eq!(back("network/wifi/network/Home"), "network/wifi");
        assert_eq!(back("network/wifi/saved/Home"), "network/wifi/saved");
        assert_eq!(back("network/wifi"), "network");
        // The data usage of one connection returns to the connection.
        assert_eq!(back("network/vpn/Work/usage"), "network/vpn/Work");
        assert_eq!(back("network/vpn/Work/tunnel/peer/0"), "network/vpn/Work/tunnel");
        assert_eq!(back("network/hotspot"), "network");
        assert_eq!(back("network/mobile/advanced/apn/Vodafone"), "network/mobile/advanced/apn");
        // An old address goes back to where its new one does.
        assert_eq!(back("network/wifi/hotspot"), "network");
        assert_eq!(back("wifi/network/Home/ipv4"), "network/wifi/network/Home/advanced");
    }
}
