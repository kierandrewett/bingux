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

use crate::pages::{
    PageId, appearance, applications, bluetooth, datetime, displays, keyboard, network, users, wifi,
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
/// name, so `wifi/saved` is on the Wi-Fi page. `None` when it names no page.
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
    /// The saved Wi-Fi networks.
    WifiSaved,
    /// One Wi-Fi network, by name.
    Network { ssid: String },
    /// One wired network device, by interface name, with its connection name for the header.
    Wired { device: String, name: String },
    /// One VPN profile, by connection name.
    Vpn { name: String },
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

impl Route {
    pub fn title(&self) -> String {
        match self {
            Self::App { name, .. } => name.clone(),
            Self::Display { name, .. } => name.clone(),
            Self::DefaultApps => "Default applications".to_string(),
            Self::DefaultApp { name, .. } => name.clone(),
            Self::Device { name, .. } => name.clone(),
            Self::WifiSaved => "Saved networks".to_string(),
            Self::Network { ssid } => ssid.clone(),
            Self::Wired { device, name } => {
                network::cached_wired_title(device).unwrap_or_else(|| name.clone())
            }
            Self::Vpn { name } => name.clone(),
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
            Self::WifiSaved => PageId::WiFi.address().join("saved"),
            Self::Network { ssid } => PageId::WiFi.address().join("network").join(ssid),
            Self::Wired { device, .. } => PageId::Network.address().join(device),
            Self::Vpn { name } => PageId::Network.address().join("vpn").join(name),
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
            ["wifi", "saved"] => Self::WifiSaved,
            ["wifi", "network", ssid] => Self::Network {
                ssid: ssid.to_string(),
            },
            ["wired", "vpn", name] => Self::Vpn {
                name: name.to_string(),
            },
            ["wired", device] => Self::Wired {
                device: device.to_string(),
                name: title_or(address, device.to_string()),
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
            Self::WifiSaved => wifi::SavedNetworksPage.into(),
            Self::Network { ssid } => wifi::NetworkPage { ssid: ssid.clone() }.into(),
            Self::Wired { device, .. } => network::WiredPage {
                device: device.clone(),
                name: self.title(),
            }
            .into(),
            Self::Vpn { name } => network::VpnPage { name: name.clone() }.into(),
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
        let target = match current.parent() {
            Some(parent) if Route::from_address(&parent).is_some() => parent,
            _ => match page_of(&current) {
                Some(page) => page.address(),
                None => return,
            },
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
