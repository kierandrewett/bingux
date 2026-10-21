//! The list of settings pages and the launch-argument mapping.
//!
//! Each page is one module with a component whose `render` owns its state
//! and talks to its backend through `crate::system`. The sidebar order is
//! the order of [`PageId::ALL`].

use freya::prelude::*;

use crate::icons::Icon;

pub mod about;
pub mod appearance;
pub mod bluetooth;
pub mod datetime;
pub mod desktop;
pub mod displays;
pub mod keyboard;
pub mod mouse;
pub mod network;
pub mod power;
pub mod search;
pub mod sound;
pub mod users;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageId {
    Network,
    Bluetooth,
    Appearance,
    Desktop,
    Search,
    Displays,
    Sound,
    Power,
    Keyboard,
    Mouse,
    DateTime,
    Users,
    About,
}

impl PageId {
    /// Sidebar order.
    pub const ALL: [PageId; 13] = [
        PageId::Network,
        PageId::Bluetooth,
        PageId::Appearance,
        PageId::Desktop,
        PageId::Search,
        PageId::Displays,
        PageId::Sound,
        PageId::Power,
        PageId::Keyboard,
        PageId::Mouse,
        PageId::DateTime,
        PageId::Users,
        PageId::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Appearance => "Appearance",
            Self::Desktop => "Desktop",
            Self::Search => "Search",
            Self::Displays => "Displays",
            Self::Sound => "Sound",
            Self::Power => "Power",
            Self::Keyboard => "Keyboard",
            Self::Mouse => "Mouse and Touchpad",
            Self::DateTime => "Date and Time",
            Self::Users => "Users",
            Self::About => "About",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Network => Icon::Network,
            Self::Bluetooth => Icon::Bluetooth,
            Self::Appearance => Icon::Appearance,
            Self::Desktop => Icon::PreferencesSystem,
            Self::Search => Icon::Search,
            Self::Displays => Icon::Display,
            Self::Sound => Icon::Sound,
            Self::Power => Icon::Power,
            Self::Keyboard => Icon::Keyboard,
            Self::Mouse => Icon::Mouse,
            Self::DateTime => Icon::Clock,
            Self::Users => Icon::Users,
            Self::About => Icon::About,
        }
    }

    /// The canonical panel name, as accepted on the command line.
    pub fn panel_name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Appearance => "background",
            Self::Desktop => "desktop",
            Self::Search => "search",
            Self::Displays => "display",
            Self::Sound => "sound",
            Self::Power => "power",
            Self::Keyboard => "keyboard",
            Self::Mouse => "mouse",
            Self::DateTime => "datetime",
            Self::Users => "users",
            Self::About => "about",
        }
    }

    /// The panel name the shell passes on the command line.
    ///
    /// `background` and `display` are the names GNOME Settings used, which
    /// the shell's launch sites already pass.
    pub fn from_panel(name: &str) -> Option<Self> {
        Some(match name {
            "network" | "wifi" => Self::Network,
            "bluetooth" => Self::Bluetooth,
            "background" | "appearance" => Self::Appearance,
            "desktop" => Self::Desktop,
            "search" => Self::Search,
            "display" | "displays" => Self::Displays,
            "sound" => Self::Sound,
            "power" => Self::Power,
            "keyboard" => Self::Keyboard,
            "mouse" => Self::Mouse,
            "datetime" => Self::DateTime,
            "users" | "user-accounts" => Self::Users,
            "about" | "info-overview" => Self::About,
            _ => return None,
        })
    }

    /// The element for this page. Each page is a distinct component type so
    /// its hook state is dropped when the user switches pages.
    pub fn render(self) -> Element {
        match self {
            Self::Network => network::NetworkPage.into(),
            Self::Bluetooth => bluetooth::BluetoothPage.into(),
            Self::Appearance => appearance::AppearancePage.into(),
            Self::Desktop => desktop::DesktopPage.into(),
            Self::Search => search::SearchPage.into(),
            Self::Displays => displays::DisplaysPage.into(),
            Self::Sound => sound::SoundPage.into(),
            Self::Power => power::PowerPage.into(),
            Self::Keyboard => keyboard::KeyboardPage.into(),
            Self::Mouse => mouse::MousePage.into(),
            Self::DateTime => datetime::DateTimePage.into(),
            Self::Users => users::UsersPage.into(),
            Self::About => about::AboutPage.into(),
        }
    }
}
