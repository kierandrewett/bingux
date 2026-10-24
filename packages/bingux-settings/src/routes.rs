//! Subpages: a page can open a deeper page and the header's Back button
//! returns to it.
//!
//! The stack belongs to one sidebar page. When the sidebar selection
//! changes, the stack that was recorded for the old page is ignored, so a
//! page always opens at its top level.

use freya::prelude::*;

use crate::pages::{
    PageId, appearance, applications, bluetooth, datetime, displays, keyboard, network, users, wifi,
};

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
            Self::Wired { name, .. } => name.clone(),
            Self::Vpn { name } => name.clone(),
            Self::Account { name, .. } => name.clone(),
            Self::TimeZone => "Time zone".to_string(),
            Self::Font => "Font".to_string(),
            Self::Shortcuts => "Shortcuts".to_string(),
            Self::InputSource { name, .. } => name.clone(),
        }
    }

    /// The subpage a command-line name stands for: `default` or the desktop
    /// id of an installed application. `None` when the name matches neither.
    pub fn from_name(name: &str) -> Option<Self> {
        if name == "default" {
            return Some(Self::DefaultApps);
        }
        if name == "shortcuts" {
            return Some(Self::Shortcuts);
        }
        if name == "timezone" {
            return Some(Self::TimeZone);
        }
        crate::apps::installed()
            .into_iter()
            .find(|app| app.id == name)
            .map(|app| Self::App {
                id: app.id,
                name: app.name,
            })
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
            Self::Wired { device, name } => network::WiredPage {
                device: device.clone(),
                name: name.clone(),
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

/// The subpage stack, shared with every page below the window root.
#[derive(Clone, Copy)]
pub struct Navigator {
    page: State<PageId>,
    stack: State<(PageId, Vec<Route>)>,
}

impl Navigator {
    /// The open subpages of the current sidebar page, oldest first.
    pub fn routes(&self) -> Vec<Route> {
        let (owner, routes) = &*self.stack.read();
        if *owner == *self.page.read() {
            routes.clone()
        } else {
            Vec::new()
        }
    }

    pub fn push(&self, route: Route) {
        let mut routes = self.routes();
        routes.push(route);
        let mut stack = self.stack;
        stack.set((*self.page.read(), routes));
    }

    pub fn pop(&self) {
        let mut routes = self.routes();
        routes.pop();
        let mut stack = self.stack;
        stack.set((*self.page.read(), routes));
    }

    /// Opens sidebar page `target` at its top level, as if the sidebar chose it.
    /// Subpages of the page left behind are dropped.
    pub fn goto(&self, target: PageId) {
        let mut page = self.page;
        page.set(target);
    }
}

/// Provide the navigator to everything below the window root.
pub fn provide_navigator(page: State<PageId>, initial: Vec<Route>) -> Navigator {
    use_provide_root_context(move || Navigator {
        page,
        stack: State::create((*page.peek(), initial)),
    })
}

/// The navigator the window root provided.
pub fn use_navigator() -> Navigator {
    use_consume::<Navigator>()
}
