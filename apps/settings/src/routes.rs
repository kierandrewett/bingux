//! Pages and subpages, declared as data.
//!
//! Settings' navigation tree is one [`RouteTable`] of URI patterns:
//! `bingux-settings://network/wifi/saved`, `bingux-settings://applications/default`.
//! Path segments go into the hierarchy, the query holds one-off state and the
//! fragment names a row (`#method`), as everywhere in bingux-ui. The sidebar,
//! rows, Back and its history menu, search, `xdg-open` and the command line
//! all move the window the same way, through the window's router
//! ([`bingux_ui::nav`]).
//!
//! Adding a page is one chained call in [`table`]. A row that opens it calls
//! `router.open(routes::name(..))`; the helpers below make the [`Target`]
//! (a location and the title the row already knows).

use bingux_ui::prelude::{Location, Params, RouteTable, Target};
use freya::prelude::*;

use crate::pages::network::{self, Owner, Section, UsageScope};
use crate::pages::{
    PageId, appearance, applications, bluetooth, datetime, displays, keyboard, sound, users,
};

/// The scheme of this application's locations.
pub const SCHEME: &str = "bingux-settings";

impl PageId {
    /// The location of this page's top level, such as `bingux-settings://network`.
    pub fn location(self) -> Location {
        Location::new(SCHEME).join(self.panel_name())
    }
}

fn network_root() -> Location {
    PageId::Network.location()
}

// ---- targets: where a row leads ------------------------------------------------

/// One application, by desktop id, with its name for the header.
pub fn app(id: &str, name: &str) -> Target {
    Target::from(PageId::Applications.location().join(id)).titled(name)
}

/// The default application for each kind of file and link.
pub fn default_apps() -> Target {
    PageId::Applications.location().join("default").into()
}

/// One kind of file or link, by its place in the default applications list,
/// with its name for the header.
pub fn default_app(kind: usize, name: &str) -> Target {
    let slug = applications::KINDS
        .get(kind)
        .map(|kind| kind.title.to_ascii_lowercase())
        .unwrap_or_default();
    Target::from(PageId::Applications.location().join("default").join(slug)).titled(name)
}

/// One Bluetooth device, by address, with its name for the header.
pub fn device(mac: &str, name: &str) -> Target {
    Target::from(PageId::Bluetooth.location().join(mac)).titled(name)
}

/// The Wi-Fi screen: the connected network and the networks in range.
pub fn wifi() -> Target {
    network_root().join("wifi").into()
}

/// The saved Wi-Fi networks.
pub fn wifi_saved() -> Target {
    network_root().join("wifi").join("saved").into()
}

/// Turning this computer into a Wi-Fi hotspot.
pub fn hotspot() -> Target {
    network_root().join("hotspot").into()
}

/// One Wi-Fi network, by name. `saved_list` says it was opened from the Saved
/// networks list.
pub fn wifi_network(ssid: &str, saved_list: bool) -> Target {
    Owner::Wifi {
        ssid: ssid.to_string(),
        saved_list,
    }
    .location()
    .into()
}

/// One wired network device, by interface name, with its connection name for
/// the header.
pub fn ethernet(device: &str, name: &str) -> Target {
    Target::from(network_root().join("ethernet").join(device)).titled(name)
}

/// The profiles of a wired device that has more than one.
pub fn ethernet_profiles(device: &str) -> Target {
    network_root()
        .join("ethernet")
        .join(device)
        .join("profile")
        .into()
}

/// One wired profile of a device that is not the one in use.
pub fn profile(device: &str, name: &str) -> Target {
    Owner::Wired {
        device: device.to_string(),
        profile: name.to_string(),
        in_use: false,
    }
    .location()
    .into()
}

/// One VPN profile, by connection name.
pub fn vpn(name: &str) -> Target {
    Owner::Vpn {
        name: name.to_string(),
    }
    .location()
    .into()
}

/// Adding a VPN: the chooser, or the form for one kind.
pub fn add_vpn(kind: Option<&str>) -> Target {
    let base = network_root().join("add-vpn");
    match kind {
        Some(kind) => base.join(kind).into(),
        None => base.into(),
    }
}

/// A section of one connection's settings: IPv4, DNS, Security and so on.
pub fn section(owner: &Owner, section: &Section) -> Target {
    section.location(owner.location()).into()
}

/// The mobile network (modem) screen.
pub fn mobile() -> Target {
    network_root().join("mobile").into()
}

/// The mobile network's advanced settings.
pub fn mobile_advanced() -> Target {
    network_root().join("mobile").join("advanced").into()
}

/// The mobile network's data usage.
pub fn mobile_usage() -> Target {
    network_root().join("mobile").join("usage").into()
}

/// Choosing the mobile carrier.
pub fn mobile_network() -> Target {
    mobile_advanced().location.join("network").into()
}

/// The access point names.
pub fn mobile_apns() -> Target {
    mobile_advanced().location.join("apn").into()
}

/// One access point name, or a new one when `name` is `None`.
pub fn mobile_apn(name: Option<&str>) -> Target {
    let base = mobile_advanced().location;
    match name {
        Some(name) => base.join("apn").join(name).into(),
        None => base.join("apn-add").into(),
    }
}

/// The system proxy.
pub fn proxy() -> Target {
    network_root().join("proxy").into()
}

/// Data usage, for everything, one interface or one connection.
pub fn usage(scope: &UsageScope) -> Target {
    scope.location(network_root().join("usage")).into()
}

/// A connection's details (they live on its Advanced page).
pub fn details(connection: &str) -> Target {
    network_root().join("details").join(connection).into()
}

/// One user account, by account path, with its full name for the header.
pub fn account(path: &str, name: &str) -> Target {
    Target::from(PageId::Users.location().join(path)).titled(name)
}

/// The time zone list.
pub fn time_zone() -> Target {
    PageId::DateTime.location().join("timezone").into()
}

/// The installed fonts, for the window and the desktop.
pub fn font() -> Target {
    PageId::Appearance.location().join("font").into()
}

/// One display on the Displays page, by its place in the list, with its label
/// for the header.
pub fn display(index: usize, name: &str) -> Target {
    Target::from(PageId::Displays.location().join(index.to_string())).titled(name)
}

/// The keyboard shortcuts, with a search box.
pub fn shortcuts() -> Target {
    PageId::Keyboard.location().join("shortcuts").into()
}

/// The volume, mute, output and level meter of each application that plays or
/// records sound.
pub fn sound_applications() -> Target {
    PageId::Sound.location().join("applications").into()
}


/// One input source, by its value in the configuration file, with its name
/// for the header.
pub fn input_source(source: &keyboard::sources::Source, name: &str) -> Target {
    Target::from(
        PageId::Keyboard
            .location()
            .join("input")
            .join(&source.kind)
            .join(&source.id),
    )
    .titled(name)
}

// ---- the table -----------------------------------------------------------------

/// The pages of the sections below a connection: the editors sit one level
/// down, below Advanced.
const SECTIONS: [(&str, Section); 9] = [
    ("advanced", Section::Advanced),
    ("advanced/ipv4", Section::Ipv4),
    ("advanced/ipv6", Section::Ipv6),
    ("advanced/dns", Section::Dns),
    ("advanced/security", Section::Security),
    ("advanced/identity", Section::Identity),
    ("advanced/proxy", Section::Proxy),
    ("advanced/options", Section::Options),
    ("usage", Section::Usage),
];

/// Settings' routes. Every page and subpage, with its title, icon and the
/// page it makes. See the module docs.
pub fn table() -> RouteTable {
    let mut table = RouteTable::new(SCHEME)
        .rewrite(legacy)
        .index("Settings", |_| rect().into());

    // The sidebar pages, in sidebar order, then the account page the profile opens.
    for page in PageId::ALL {
        table = root(table, page);
    }
    table = root(table, PageId::Users).unlisted();

    // Applications.
    table = table
        .route("applications/default")
        .title("Default applications")
        .page(|_| applications::DefaultAppsPage.into());
    for (kind, known) in applications::KINDS.iter().enumerate() {
        table = table
            .route(&format!("applications/default/{}", known.title.to_ascii_lowercase()))
            .title(known.title)
            .page(move |_| applications::DefaultKindPage { kind }.into());
    }
    table = table
        .route("applications/:id")
        .title_with(|p| crate::apps::cached_name(p.str("id")).unwrap_or_else(|| p.str("id").to_string()))
        .page(|p| applications::AppPage { id: p.str("id").to_string() }.into());

    // Bluetooth, Users, Date and time, Appearance, Displays, Keyboard, Sound.
    table = table
        .route("bluetooth/:mac")
        .title_param("mac")
        .page(|p| bluetooth::BluetoothDevicePage { mac: p.str("mac").to_string() }.into())
        .route("users/:path")
        .title_param("path")
        .page(|p| users::AccountPage { path: p.str("path").to_string() }.into())
        .route("datetime/timezone")
        .title("Time zone")
        .page(|_| datetime::TimeZonePage.into())
        .route("background/font")
        .title("Font")
        .page(|_| appearance::FontPage.into())
        .route("display/:index:int")
        .title_with(|p| format!("Display {}", p.int("index").unwrap_or(0) + 1))
        .page(|p| displays::DisplayPage { index: p.int("index").unwrap_or(0) as usize }.into())
        .route("keyboard/shortcuts")
        .title("Shortcuts")
        .page(|_| keyboard::ShortcutsPage.into())
        .route("keyboard/input/:kind/:id")
        .title_param("id")
        .page(|p| {
            keyboard::InputSourcePage {
                source: keyboard::sources::Source {
                    kind: p.str("kind").to_string(),
                    id: p.str("id").to_string(),
                },
            }
            .into()
        })
        .route("sound/applications")
        .title("Application volumes")
        .page(|_| sound::ApplicationsPage.into());

    network_routes(table)
}

/// A sidebar page: its title, icon, group and search words from [`PageId`],
/// and its page with the Related group under it.
fn root(table: RouteTable, page: PageId) -> RouteTable {
    table
        .root(page.panel_name())
        .title(page.title())
        .icon(page.icon())
        .group(page.group())
        .keywords(page.keywords())
        .page(move |_| crate::app::RootPage { page }.into())
}

/// The Network page and everything below it.
fn network_routes(table: RouteTable) -> RouteTable {
    let mut table = table
        .route("network/wifi")
        .title("Wi-Fi")
        .page(|_| network::wifi::WifiPage.into())
        .route("network/wifi/saved")
        .title("Saved networks")
        .page(|_| network::wifi::SavedNetworksPage.into())
        .route("network/hotspot")
        .title("Wi-Fi hotspot")
        .page(|_| network::wifi::HotspotPage.into())
        .route("network/wifi/network/:ssid")
        .title_param("ssid")
        .page(|p| wifi_network_page(p, false))
        .route("network/wifi/saved/:ssid")
        .title_param("ssid")
        .page(|p| wifi_network_page(p, true))
        .route("network/ethernet/:device")
        .title_with(|p| {
            network::cached_ethernet_title(p.str("device")).unwrap_or_else(|| p.str("device").to_string())
        })
        .page(|p| network::ethernet::EthernetPage { device: p.str("device").to_string() }.into())
        .route("network/ethernet/:device/profile")
        .title("Profiles")
        .page(|p| network::ethernet::ProfilesPage { device: p.str("device").to_string() }.into())
        .route("network/ethernet/:device/profile/:profile")
        .title_param("profile")
        .page(|p| {
            network::ethernet::ProfilePage {
                device: p.str("device").to_string(),
                name: p.str("profile").to_string(),
            }
            .into()
        })
        .route("network/vpn/:name")
        .title_param("name")
        .page(|p| network::vpn::VpnPage { name: p.str("name").to_string() }.into())
        .route("network/add-vpn")
        .title_with(|_| network::add_vpn_title(None))
        .page(|_| network::vpn_add::AddVpnPage { kind: None }.into())
        .route("network/add-vpn/:kind")
        .title_with(|p| network::add_vpn_title(Some(p.str("kind"))))
        .page(|p| network::vpn_add::AddVpnPage { kind: Some(p.str("kind").to_string()) }.into())
        .route("network/mobile")
        .title("Mobile network")
        .page(|_| network::mobile::MobilePage.into())
        .route("network/mobile/advanced")
        .title("Advanced")
        .page(|_| network::mobile::MobileAdvancedPage.into())
        .route("network/mobile/usage")
        .title("Data usage")
        .page(|_| network::mobile::MobileUsagePage.into())
        .route("network/mobile/advanced/network")
        .title("Network")
        .page(|_| network::mobile::MobileNetworkPage.into())
        .route("network/mobile/advanced/apn")
        .title("Access point names")
        .page(|_| network::mobile::ApnListPage.into())
        .route("network/mobile/advanced/apn-add")
        .title("New access point")
        .page(|_| network::mobile::ApnPage { name: None }.into())
        .route("network/mobile/advanced/apn/:name")
        .title_param("name")
        .page(|p| network::mobile::ApnPage { name: Some(p.str("name").to_string()) }.into())
        .route("network/proxy")
        .title("Proxy")
        .page(|_| network::proxy::ProxyPage.into())
        .route("network/usage")
        .title("Data usage")
        .page(|_| network::usage::UsagePage { scope: UsageScope::All }.into())
        .route("network/usage/interface/:name")
        .title_param("name")
        .page(|p| {
            network::usage::UsagePage { scope: UsageScope::Interface(p.str("name").to_string()) }.into()
        })
        .route("network/usage/connection/:name")
        .title_param("name")
        .page(|p| {
            network::usage::UsagePage { scope: UsageScope::Connection(p.str("name").to_string()) }.into()
        })
        .route("network/details/:connection")
        .title_param("connection")
        .ephemeral()
        .page(|p| network::details::DetailsPage { connection: p.str("connection").to_string() }.into());

    // The pages below each kind of connection: Advanced and its editors, the
    // connection's data usage, and a VPN's tunnel and peers.
    let owners: [(&str, fn(&Params) -> Owner); 5] = [
        ("network/wifi/network/:ssid", |p| wifi_owner(p, false)),
        ("network/wifi/saved/:ssid", |p| wifi_owner(p, true)),
        ("network/ethernet/:device", |p| Owner::Wired {
            device: p.str("device").to_string(),
            profile: String::new(),
            in_use: true,
        }),
        ("network/ethernet/:device/profile/:profile", |p| Owner::Wired {
            device: p.str("device").to_string(),
            profile: p.str("profile").to_string(),
            in_use: false,
        }),
        ("network/vpn/:name", |p| Owner::Vpn { name: p.str("name").to_string() }),
    ];
    for (base, owner) in owners {
        for (suffix, section) in SECTIONS {
            let titled = section.clone();
            table = table
                .route(&format!("{base}/{suffix}"))
                .title_with(move |_| titled.title())
                .page(move |p| section_page(owner(p), section.clone()));
        }
    }
    table
        .route("network/vpn/:name/tunnel")
        .title("Tunnel")
        .page(|p| section_page(Owner::Vpn { name: p.str("name").to_string() }, Section::Tunnel))
        .route("network/vpn/:name/tunnel/peer/:index:int")
        .title_with(|p| format!("Peer {}", p.int("index").unwrap_or(0) + 1))
        .page(|p| {
            section_page(
                Owner::Vpn { name: p.str("name").to_string() },
                Section::Peer(p.int("index").unwrap_or(0) as usize),
            )
        })
}

fn wifi_owner(params: &Params, saved_list: bool) -> Owner {
    Owner::Wifi {
        ssid: params.str("ssid").to_string(),
        saved_list,
    }
}

/// A Wi-Fi network's own page.
fn wifi_network_page(params: &Params, saved_list: bool) -> Element {
    network::wifi::WifiNetworkPage {
        ssid: params.str("ssid").to_string(),
        saved_list,
    }
    .into()
}

/// A page below a connection's page.
fn section_page(owner: Owner, section: Section) -> Element {
    match (&section, &owner) {
        (Section::Tunnel, _) => network::vpn_tunnel::TunnelPage { owner }.into(),
        (Section::Peer(index), _) => network::vpn_tunnel::PeerPage {
            index: *index,
            owner,
        }
        .into(),
        // A wired connection may have to work out which profile it is first.
        (_, Owner::Wired { .. }) => network::ethernet::WiredSection { owner, section }.into(),
        (Section::Usage, _) => network::usage::UsagePage {
            scope: UsageScope::Connection(owner.connection_name().to_string()),
        }
        .into(),
        (_, _) => network::profile::SectionPage {
            owner,
            section,
            extra: None,
        }
        .into(),
    }
}

// ---- links that used to open a screen -------------------------------------------

/// `location` with an old-style link rewritten to where it lives now, so every
/// link that ever opened a Settings screen still does.
///
/// Network: first segments `wifi`, `wifi/saved`, `wifi/network/<ssid>`,
/// `wired/<device>`, `wired/vpn/<name>`, `ethernet/<device>`, `vpn/<name>`,
/// `mobile`, `wwan`, `cellular`, `proxy` and `usage` are below `network/`.
/// Then `network/wifi/hotspot` is `network/hotspot`, the mobile carrier and
/// access point pages are below `network/mobile/advanced`, and a connection's
/// `ipv4`, `ipv6`, `dns`, `security`, `identity` and `proxy` pages are below
/// its `advanced` page. Other pages: the names GNOME Settings used
/// (`appearance`, `displays`, `user-accounts`...) become the panel names.
pub fn legacy(location: &Location) -> Location {
    let segments: Vec<&str> = location.segments().iter().map(String::as_str).collect();
    let under_network: Option<Vec<&str>> = match segments.as_slice() {
        ["network", ..] => Some(segments.clone()),
        ["wifi" | "wi-fi", rest @ ..] => Some(["network", "wifi"].into_iter().chain(rest.iter().copied()).collect()),
        ["wired" | "ethernet" | "vpn", "vpn", name] => Some(vec!["network", "vpn", name]),
        ["wired" | "ethernet", device] => Some(vec!["network", "ethernet", device]),
        ["vpn", name] => Some(vec!["network", "vpn", name]),
        ["wired" | "ethernet" | "vpn"] => Some(vec!["network"]),
        ["mobile" | "wwan" | "cellular", rest @ ..] => {
            Some(["network", "mobile"].into_iter().chain(rest.iter().copied()).collect())
        }
        ["proxy"] => Some(vec!["network", "proxy"]),
        ["usage" | "data-usage", rest @ ..] => {
            Some(["network", "usage"].into_iter().chain(rest.iter().copied()).collect())
        }
        _ => None,
    };
    let mapped: Vec<&str> = match under_network {
        Some(path) => upgrade_network_path(path),
        None => match segments.split_first() {
            Some((first, rest)) => match PageId::from_panel(first) {
                Some(page) if page.panel_name() != *first => {
                    std::iter::once(page.panel_name()).chain(rest.iter().copied()).collect()
                }
                _ => segments.clone(),
            },
            None => segments.clone(),
        },
    };
    if mapped == segments {
        return location.clone();
    }
    let mut moved = Location::new(location.scheme());
    for segment in mapped {
        moved = moved.join(segment);
    }
    for (key, value) in location.query_pairs() {
        moved = moved.with_query(key.clone(), value.clone());
    }
    match location.fragment() {
        Some(fragment) => moved.with_fragment(fragment),
        None => moved,
    }
}

/// The editors that were pages of their own below a connection before
/// Advanced held them.
const EDITORS: [&str; 6] = ["ipv4", "ipv6", "dns", "security", "identity", "proxy"];

/// Moves a path below `network/` to where its page is now.
fn upgrade_network_path(segments: Vec<&str>) -> Vec<&str> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn location(text: &str) -> Location {
        Location::parse(&format!("{SCHEME}://{text}")).unwrap()
    }

    fn canonical(text: &str) -> String {
        legacy(&location(text)).segments().join("/")
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
        assert_eq!(canonical("network/vpn/Work/dns"), "network/vpn/Work/advanced/dns");
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
    fn the_names_gnome_settings_used_become_the_panel_names() {
        assert_eq!(canonical("appearance"), "background");
        assert_eq!(canonical("displays/1"), "display/1");
        assert_eq!(canonical("user-accounts"), "users");
        assert_eq!(canonical("apps/default"), "applications/default");
        assert_eq!(legacy(&location("appearance?x=1#frag")).to_string(), format!("{SCHEME}://background?x=1#frag"));
    }

    /// Every page a person can reach has a route, and every route's page opens
    /// at the location it is declared for.
    #[test]
    fn every_page_names_a_route() {
        let table = table();
        for text in [
            "",
            "network",
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
            "network/add-vpn",
            "network/add-vpn/wireguard",
            "network/mobile",
            "network/mobile/advanced",
            "network/mobile/advanced/apn",
            "network/mobile/advanced/apn-add",
            "network/mobile/advanced/apn/Vodafone",
            "network/mobile/advanced/network",
            "network/mobile/usage",
            "network/proxy",
            "network/usage",
            "network/usage/interface/enp42s0",
            "network/usage/connection/Home",
            "network/details/Home",
            "applications/default",
            "applications/default/web",
            "applications/org.gnome.Foo",
            "bluetooth/AA:BB:CC:DD:EE:FF",
            "users/%2Forg%2Ffreedesktop%2FAccounts%2FUser1000",
            "datetime/timezone",
            "background/font",
            "display/0",
            "keyboard/shortcuts",
            "keyboard/input/xkb/us",
            "sound/applications",
        ] {
            let place = table.canonical(&location(text));
            assert!(table.resolve(&place).is_some(), "{text} names no page");
        }
        // Not a page: the table says so, and the view shows a page that offers Up.
        assert!(table.resolve(&location("network/nowhere")).is_none());
        assert!(table.resolve(&location("display/second")).is_none());
    }

    #[test]
    fn the_sidebar_lists_the_pages_in_order_and_not_the_account_page() {
        let table = table();
        let listed: Vec<String> = table.sidebar().map(|(_, route)| route.id().to_string()).collect();
        let expected: Vec<&str> = PageId::ALL.iter().map(|page| page.panel_name()).collect();
        assert_eq!(listed, expected);
        assert!(table.roots().any(|(_, route)| route.id() == "users"));
    }

    #[test]
    fn a_subpage_titles_itself_from_its_route() {
        let table = table();
        let title = |text: &str| table.title_of(&table.canonical(&location(text)));
        assert_eq!(title("network/wifi/saved"), "Saved networks");
        assert_eq!(title("network/wifi/network/Home%20Net"), "Home Net");
        assert_eq!(title("network/wifi/network/Home/advanced/ipv4"), "IPv4");
        assert_eq!(title("network/vpn/Work/tunnel/peer/0"), "Peer 1");
        assert_eq!(title("network/add-vpn"), "Add VPN");
        assert_eq!(title("sound/applications"), "Application volumes");
        assert_eq!(title("display/1"), "Display 2");
        assert_eq!(title("wifi/network/Home/proxy"), "Proxy");
    }

    #[test]
    fn up_goes_to_the_page_that_holds_the_row() {
        let table = table();
        let up = |text: &str| {
            table
                .up(&table.canonical(&location(text)))
                .unwrap()
                .segments()
                .join("/")
        };
        // Network > Ethernet > Advanced > IPv4, and back out of it.
        assert_eq!(
            up("network/ethernet/enp42s0/advanced/ipv4"),
            "network/ethernet/enp42s0/advanced"
        );
        assert_eq!(up("network/ethernet/enp42s0/advanced"), "network/ethernet/enp42s0");
        assert_eq!(up("network/ethernet/enp42s0"), "network");
        // A profile opens from the list of profiles, which sits below the device.
        assert_eq!(
            up("network/ethernet/enp42s0/profile/Office"),
            "network/ethernet/enp42s0/profile"
        );
        assert_eq!(up("network/ethernet/enp42s0/profile"), "network/ethernet/enp42s0");
        // A Wi-Fi network opens from Wi-Fi, or from Saved networks.
        assert_eq!(up("network/wifi/network/Home"), "network/wifi");
        assert_eq!(up("network/wifi/saved/Home"), "network/wifi/saved");
        assert_eq!(up("network/wifi"), "network");
        // The data usage of one connection returns to the connection.
        assert_eq!(up("network/vpn/Work/usage"), "network/vpn/Work");
        assert_eq!(up("network/vpn/Work/tunnel/peer/0"), "network/vpn/Work/tunnel");
        assert_eq!(up("network/hotspot"), "network");
        assert_eq!(
            up("network/mobile/advanced/apn/Vodafone"),
            "network/mobile/advanced/apn"
        );
        // An old link goes up to where its new form does.
        assert_eq!(up("network/wifi/hotspot"), "network");
        assert_eq!(up("wifi/network/Home/ipv4"), "network/wifi/network/Home/advanced");
    }

    #[test]
    fn a_row_opens_the_location_its_route_is_declared_for() {
        let table = table();
        for target in [
            wifi_network("Home", false),
            wifi_network("Home", true),
            profile("enp42s0", "Office"),
            vpn("Work"),
            section(&Owner::Vpn { name: "Work".into() }, &Section::Peer(2)),
            section(
                &Owner::Wired { device: "enp42s0".into(), profile: String::new(), in_use: true },
                &Section::Ipv6,
            ),
            mobile_apn(None),
            mobile_apn(Some("Vodafone")),
            usage(&UsageScope::Connection("Home".into())),
            default_app(0, "Web"),
            input_source(&keyboard::sources::Source { kind: "xkb".into(), id: "us".into() }, "English"),
            display(1, "Display 2"),
        ] {
            let found = table.resolve(&target.location);
            assert!(found.is_some(), "{} names no page", target.location);
            assert_eq!(table.canonical(&target.location), target.location, "{}", target.location);
        }
    }
}
