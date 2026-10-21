//! The list of settings pages and the launch-argument mapping.
//!
//! Each page is one module with a component whose `render` owns its state
//! and talks to its backend through `crate::system`. The sidebar order is
//! the order of [`PageId::ALL`].

use freya::prelude::*;

use crate::icons::Icon;
use crate::system;

/// The `bingux-config` command every page uses: `BINGUX_CONFIG_BIN` when
/// set (the test harness convention), else PATH, else the binary built or
/// installed beside `bingux-settings`.
pub fn bingux_config() -> String {
    std::env::var("BINGUX_CONFIG_BIN")
        .ok()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| system::locate("bingux-config"))
}

pub mod about;
pub mod appearance;
pub mod bluetooth;
pub mod datetime;
pub mod desktop;
pub mod displays;
pub mod gnoblin;
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

    /// One line under the page title.
    pub fn description(self) -> &'static str {
        match self {
            Self::Network => "Wi-Fi, wired connections and airplane mode",
            Self::Bluetooth => "Pair, connect and remove devices",
            Self::Appearance => "Wallpaper, accent colour and colour scheme",
            Self::Desktop => "Dock, sidebar, top bar and Control Centre",
            Self::Search => "Providers, web engines, file locations and previews",
            Self::Displays => "Resolution, refresh rate, scale and night light",
            Self::Sound => "Output and input devices, volumes and applications",
            Self::Power => "Battery, power mode, blank screen and lock",
            Self::Keyboard => "Key repeat, input sources and shortcuts",
            Self::Mouse => "Pointer speed, scrolling and touchpad gestures",
            Self::DateTime => "Clock, time zone and automatic time",
            Self::Users => "Accounts on this computer",
            Self::About => "This computer, the system and versions",
        }
    }

    /// Words the sidebar search matches besides the title.
    pub fn keywords(self) -> &'static str {
        match self {
            Self::Network => "wifi wi-fi internet ethernet wired airplane vpn",
            Self::Bluetooth => "headphones devices pair",
            Self::Appearance => "wallpaper background accent colour color theme dark light",
            Self::Desktop => "dock sidebar bar clock control centre layout",
            Self::Search => "providers engines files previews assistant",
            Self::Displays => "monitor screen resolution scale night light",
            Self::Sound => "audio volume speakers microphone mute",
            Self::Power => "battery energy sleep lock screen suspend",
            Self::Keyboard => "layout input source repeat shortcuts",
            Self::Mouse => "touchpad trackpad pointer scrolling tap",
            Self::DateTime => "clock time zone ntp format",
            Self::Users => "accounts password login avatar",
            Self::About => "version kernel hardware memory disk gnoblin",
        }
    }

    /// Base colour of the sidebar icon tile, in the macOS System Settings
    /// spirit: one hue per category, not per page.
    pub fn tile_rgb(self) -> [f32; 3] {
        match self {
            Self::Network | Self::Bluetooth => [0.20, 0.52, 0.93],
            Self::Appearance => [0.84, 0.38, 0.60],
            Self::Desktop => [0.13, 0.57, 0.64],
            Self::Search => [0.50, 0.50, 0.55],
            Self::Displays => [0.23, 0.62, 0.84],
            Self::Sound => [0.90, 0.30, 0.30],
            Self::Power => [0.23, 0.58, 0.29],
            Self::Keyboard | Self::Mouse => [0.50, 0.50, 0.55],
            Self::DateTime => [0.93, 0.36, 0.00],
            Self::Users => [0.57, 0.26, 0.68],
            Self::About => [0.44, 0.51, 0.59],
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
