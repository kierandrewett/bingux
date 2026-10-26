//! The list of settings pages and the launch-argument mapping.
//!
//! Each page is one module with a component whose `render` owns its state
//! and talks to its backend through `crate::system`. The sidebar order is
//! the order of [`PageId::ALL`].

use freya::prelude::*;

use crate::icons::Icon;

pub use crate::system::bingux_config;

pub mod about;
pub mod accessibility;
pub mod appearance;
pub mod applications;
pub mod bluetooth;
pub mod connection_editor;
pub mod datetime;
pub mod desktop;
pub mod displays;
pub mod gnoblin;
pub mod keyboard;
pub mod loading;
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
    Accessibility,
    Desktop,
    Search,
    Displays,
    Sound,
    Power,
    Keyboard,
    Mouse,
    DateTime,
    Users,
    Applications,
    About,
}

/// Load every page's data in the background, so each page is filled in by
/// the time it is opened.
pub fn prefetch_all() {
    about::prefetch();
    appearance::prefetch();
    accessibility::prefetch();
    applications::prefetch();
    bluetooth::prefetch();
    datetime::prefetch();
    desktop::prefetch();
    displays::prefetch();
    keyboard::prefetch();
    mouse::prefetch();
    network::prefetch();
    network::wifi::prefetch();
    power::prefetch();
    search::prefetch();
    sound::prefetch();
    users::prefetch();
}

impl PageId {
    /// Sidebar order.
    pub const ALL: [PageId; 14] = [
        PageId::Network,
        PageId::Bluetooth,
        PageId::Displays,
        PageId::Sound,
        PageId::Power,
        PageId::Keyboard,
        PageId::Mouse,
        PageId::Appearance,
        PageId::Desktop,
        PageId::Search,
        PageId::Applications,
        PageId::Accessibility,
        PageId::DateTime,
        PageId::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Network => "Network",
            Self::Bluetooth => "Bluetooth",
            Self::Appearance => "Appearance",
            Self::Accessibility => "Accessibility",
            Self::Desktop => "Desktop and dock",
            Self::Search => "Search",
            Self::Displays => "Displays",
            Self::Sound => "Sound",
            Self::Power => "Power",
            Self::Keyboard => "Keyboard",
            Self::Mouse => "Mouse and touchpad",
            Self::DateTime => "Date and time",
            Self::Users => "Users",
            Self::Applications => "Applications",
            Self::About => "About",
        }
    }

    /// The pages a person is likely to want next from this one. Shown under
    /// the page as the Related group. Never includes the page itself.
    pub fn related(self) -> &'static [PageId] {
        match self {
            Self::Network => &[PageId::Bluetooth, PageId::Displays],
            Self::Bluetooth => &[PageId::Sound, PageId::Mouse],
            Self::Displays => &[PageId::Appearance, PageId::Power],
            Self::Sound => &[PageId::Bluetooth, PageId::Displays],
            Self::Power => &[PageId::Displays, PageId::Sound],
            Self::Keyboard => &[PageId::Mouse, PageId::Applications],
            Self::Mouse => &[PageId::Keyboard, PageId::Bluetooth],
            Self::Appearance => &[PageId::Desktop, PageId::Displays],
            Self::Desktop => &[PageId::Appearance, PageId::Search],
            Self::Search => &[PageId::Applications, PageId::Desktop],
            Self::Applications => &[PageId::Search, PageId::Keyboard],
            Self::DateTime => &[PageId::Power, PageId::About],
            Self::Users => &[PageId::About, PageId::DateTime],
            Self::About => &[PageId::Displays, PageId::Users],
            Self::Accessibility => &[PageId::Keyboard, PageId::Mouse],
        }
    }

    /// The sidebar section the page is listed under. Sections are divided by
    /// a hairline in the sidebar, not by a heading.
    pub fn group(self) -> &'static str {
        match self {
            Self::Network | Self::Bluetooth => "connections",
            Self::Displays | Self::Sound | Self::Power | Self::Keyboard | Self::Mouse => "devices",
            Self::Appearance | Self::Desktop | Self::Search | Self::Applications => "desktop",
            Self::Accessibility | Self::DateTime | Self::Users | Self::About => "system",
        }
    }

    /// Words the sidebar search matches besides the title.
    pub fn keywords(self) -> &'static str {
        match self {
            Self::Network => {
                "network wifi wi-fi wireless ethernet wired cable internet vpn wireguard openvpn proxy hotspot hidden saved networks airplane mobile cellular wwan modem sim apn roaming data usage dns ip address metered captive portal"
            }
            Self::Bluetooth => "headphones devices pair",
            Self::Appearance => "wallpaper background accent colour color theme dark light",
            Self::Accessibility => {
                "accessibility screen reader keyboard sticky slow bounce mouse keys pointer contrast large text cursor animations scrollbars"
            }
            Self::Desktop => "dock sidebar bar clock control centre layout",
            Self::Search => "providers engines files previews assistant",
            Self::Displays => "monitor screen resolution scale night light",
            Self::Sound => "audio volume speakers microphone mute",
            Self::Power => "battery energy sleep lock screen suspend",
            Self::Keyboard => "layout input source repeat shortcuts",
            Self::Mouse => "touchpad trackpad pointer scrolling tap",
            Self::DateTime => "clock time zone ntp format",
            Self::Users => "accounts password login avatar",
            Self::Applications => "apps programs default browser mail open with launch",
            Self::About => "version kernel hardware memory disk gnoblin",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::Network => Icon::Network,
            Self::Bluetooth => Icon::Bluetooth,
            Self::Appearance => Icon::Appearance,
            Self::Accessibility => Icon::Accessibility,
            Self::Desktop => Icon::Desktop,
            Self::Search => Icon::Search,
            Self::Displays => Icon::Display,
            Self::Sound => Icon::Sound,
            Self::Power => Icon::Power,
            Self::Keyboard => Icon::Keyboard,
            Self::Mouse => Icon::Mouse,
            Self::DateTime => Icon::Clock,
            Self::Users => Icon::Users,
            Self::Applications => Icon::Applications,
            Self::About => Icon::About,
        }
    }

    /// The canonical panel name, as accepted on the command line.
    pub fn panel_name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Appearance => "background",
            Self::Accessibility => "accessibility",
            Self::Desktop => "desktop",
            Self::Search => "search",
            Self::Displays => "display",
            Self::Sound => "sound",
            Self::Power => "power",
            Self::Keyboard => "keyboard",
            Self::Mouse => "mouse",
            Self::DateTime => "datetime",
            Self::Users => "users",
            Self::Applications => "applications",
            Self::About => "about",
        }
    }

    /// The panel name the shell passes on the command line.
    ///
    /// `background` and `display` are the names GNOME Settings used, which
    /// the shell's launch sites already pass.
    pub fn from_panel(name: &str) -> Option<Self> {
        Some(match name {
            "network" | "wifi" | "wi-fi" | "wired" | "ethernet" | "vpn" | "mobile" | "wwan"
            | "cellular" | "proxy" | "usage" | "data-usage" => Self::Network,
            "bluetooth" => Self::Bluetooth,
            "background" | "appearance" => Self::Appearance,
            "accessibility" | "a11y" => Self::Accessibility,
            "desktop" => Self::Desktop,
            "search" => Self::Search,
            "display" | "displays" => Self::Displays,
            "sound" => Self::Sound,
            "power" => Self::Power,
            "keyboard" => Self::Keyboard,
            "mouse" => Self::Mouse,
            "datetime" => Self::DateTime,
            "users" | "user-accounts" => Self::Users,
            "applications" | "apps" | "default-apps" => Self::Applications,
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
            Self::Accessibility => accessibility::AccessibilityPage.into(),
            Self::Desktop => desktop::DesktopPage.into(),
            Self::Search => search::SearchPage.into(),
            Self::Displays => displays::DisplaysPage.into(),
            Self::Sound => sound::SoundPage.into(),
            Self::Power => power::PowerPage.into(),
            Self::Keyboard => keyboard::KeyboardPage.into(),
            Self::Mouse => mouse::MousePage.into(),
            Self::DateTime => datetime::DateTimePage.into(),
            Self::Users => users::UsersPage.into(),
            Self::Applications => applications::ApplicationsPage.into(),
            Self::About => about::AboutPage.into(),
        }
    }
}
