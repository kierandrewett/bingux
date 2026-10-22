//! Subpages: a page can open a deeper page and the header's Back button
//! returns to it.
//!
//! The stack belongs to one sidebar page. When the sidebar selection
//! changes, the stack that was recorded for the old page is ignored, so a
//! page always opens at its top level.

use freya::prelude::*;

use crate::pages::{PageId, appearance, applications, bluetooth, datetime, users, wifi};

/// A page below a sidebar page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// One application, by desktop id, with its name for the header.
    App { id: String, name: String },
    /// The default application for each kind of file and link.
    DefaultApps,
    /// One Bluetooth device, by address, with its name for the header.
    Device { mac: String, name: String },
    /// One Wi-Fi network, by name.
    WifiNetwork { ssid: String },
    /// One user account, by account path, with its full name for the header.
    Account { path: String, name: String },
    /// The time zone list.
    TimeZone,
    /// Setting the date and time by hand.
    SetTime,
    /// The installed fonts, for the window and the desktop.
    Font,
}

impl Route {
    pub fn title(&self) -> String {
        match self {
            Self::App { name, .. } => name.clone(),
            Self::DefaultApps => "Default applications".to_string(),
            Self::Device { name, .. } => name.clone(),
            Self::WifiNetwork { ssid } => ssid.clone(),
            Self::Account { name, .. } => name.clone(),
            Self::TimeZone => "Time zone".to_string(),
            Self::SetTime => "Set date and time".to_string(),
            Self::Font => "Font".to_string(),
        }
    }

    /// The subpage a command-line name stands for: `default` or the desktop
    /// id of an installed application. `None` when the name matches neither.
    pub fn from_name(name: &str) -> Option<Self> {
        if name == "default" {
            return Some(Self::DefaultApps);
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
            Self::Device { mac, .. } => bluetooth::BluetoothDevicePage { mac: mac.clone() }.into(),
            Self::WifiNetwork { ssid } => wifi::NetworkDetailsPage { ssid: ssid.clone() }.into(),
            Self::Account { path, .. } => users::AccountPage { path: path.clone() }.into(),
            Self::TimeZone => datetime::TimeZonePage.into(),
            Self::SetTime => datetime::SetTimePage.into(),
            Self::Font => appearance::FontPage.into(),
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
