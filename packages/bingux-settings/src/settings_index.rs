//! The settings the search finds. Each entry names the page the setting is
//! on, its title as the page shows it, and plain words a person might search
//! for instead. Add a row when a page gains a setting.

use crate::pages::PageId;

/// Page, visible title, and extra words to match.
const SETTINGS: &[(PageId, &str, &str)] = &[
    (PageId::WiFi, "Wi-Fi", "wireless internet on off radio"),
    (
        PageId::WiFi,
        "Airplane mode",
        "flight offline radios disable wireless",
    ),
    (
        PageId::WiFi,
        "Connect to hidden network",
        "ssid hidden join add network",
    ),
    (
        PageId::WiFi,
        "Saved networks",
        "remembered forget passwords known",
    ),
    (
        PageId::WiFi,
        "Wi-Fi hotspot",
        "share tether access point internet sharing",
    ),
    (PageId::Network, "Wired", "ethernet cable lan connection"),
    (
        PageId::Network,
        "VPN",
        "virtual private network tunnel connection",
    ),
    (
        PageId::Bluetooth,
        "Bluetooth",
        "pair wireless devices on off headphones",
    ),
    (
        PageId::Bluetooth,
        "Connects automatically",
        "reconnect auto trusted",
    ),
    (PageId::Bluetooth, "Forget device", "remove unpair delete"),
    (PageId::Bluetooth, "Battery", "charge level device"),
    (
        PageId::Displays,
        "Resolution",
        "size pixels sharpness monitor screen",
    ),
    (
        PageId::Displays,
        "Refresh rate",
        "hertz hz frequency smooth",
    ),
    (
        PageId::Displays,
        "Scale",
        "zoom size text bigger smaller hidpi",
    ),
    (
        PageId::Displays,
        "Orientation",
        "rotate portrait landscape flip",
    ),
    (
        PageId::Displays,
        "Arrangement",
        "layout position order monitors side by side",
    ),
    (PageId::Displays, "Primary display", "main monitor default"),
    (
        PageId::Displays,
        "Enabled",
        "turn off monitor disable display",
    ),
    (
        PageId::Displays,
        "Night light",
        "blue light warm evening night mode",
    ),
    (
        PageId::Displays,
        "On a schedule",
        "night light time automatic sunset sunrise",
    ),
    (
        PageId::Displays,
        "Colour temperature",
        "warmth warm cool tint color",
    ),
    (
        PageId::Sound,
        "Volume",
        "loudness speakers sound level quiet loud",
    ),
    (
        PageId::Sound,
        "Output device",
        "speakers headphones playback outputs",
    ),
    (
        PageId::Sound,
        "Input device",
        "microphone recording mic inputs",
    ),
    (
        PageId::Sound,
        "Applications",
        "app volume per app mute playing",
    ),
    (
        PageId::Power,
        "Power mode",
        "performance balanced saver battery energy",
    ),
    (PageId::Power, "Battery", "charge percentage energy state"),
    (
        PageId::Power,
        "Automatic screen lock",
        "lock password idle locked",
    ),
    (PageId::Power, "Lock after", "idle timeout password delay"),
    (
        PageId::Power,
        "Blank screen after",
        "screen off display sleep idle timeout",
    ),
    (
        PageId::Power,
        "Dim screen when inactive",
        "brightness idle dark",
    ),
    (PageId::Power, "Suspend when inactive", "sleep idle standby"),
    (PageId::Power, "Suspend after", "sleep idle timeout standby"),
    (
        PageId::Power,
        "When the power button is pressed",
        "shutdown sleep suspend off",
    ),
    (
        PageId::Keyboard,
        "Input sources",
        "layout language keyboard add input method",
    ),
    (
        PageId::Keyboard,
        "Keyboard layout",
        "language qwerty dvorak",
    ),
    (PageId::Keyboard, "Input method", "ime typing language"),
    (PageId::Keyboard, "Repeat keys", "auto repeat hold keys"),
    (PageId::Keyboard, "Repeat speed", "typematic rate fast slow"),
    (PageId::Keyboard, "Delay before repeat", "hold delay wait"),
    (
        PageId::Keyboard,
        "Caps Lock key",
        "capslock shift caps uppercase",
    ),
    (
        PageId::Keyboard,
        "Compose key",
        "accents special characters symbols",
    ),
    (
        PageId::Keyboard,
        "Num Lock on at sign-in",
        "numlock numbers startup login",
    ),
    (
        PageId::Keyboard,
        "Remember Num Lock state",
        "numlock memory keep",
    ),
    (
        PageId::Keyboard,
        "Shortcuts",
        "keybindings hotkeys key combinations",
    ),
    (PageId::Keyboard, "Try it", "test repeat"),
    (
        PageId::Mouse,
        "Pointer acceleration",
        "speed sensitivity mouse fast slow",
    ),
    (
        PageId::Mouse,
        "Left-handed buttons",
        "swap buttons left right mouse",
    ),
    (
        PageId::Mouse,
        "Natural scrolling",
        "scroll direction reverse touchpad mouse",
    ),
    (PageId::Mouse, "Tap to click", "touchpad tap press"),
    (
        PageId::Mouse,
        "Two-finger scrolling",
        "touchpad scroll fingers",
    ),
    (
        PageId::Mouse,
        "Scrolling along edges",
        "touchpad edge scroll",
    ),
    (
        PageId::Mouse,
        "Off when a mouse is connected",
        "touchpad disable mouse plugged",
    ),
    (
        PageId::Appearance,
        "Wallpaper",
        "background picture image desktop",
    ),
    (PageId::Appearance, "Accent colour", "color highlight theme"),
    (
        PageId::Appearance,
        "Colour scheme",
        "dark light mode theme color",
    ),
    (
        PageId::Appearance,
        "Font",
        "typeface text size window desktop",
    ),
    (PageId::Desktop, "Show dock", "dock bar launcher hide"),
    (PageId::Desktop, "Icon size", "dock big small larger"),
    (
        PageId::Desktop,
        "Side of the screen",
        "dock left right bottom position",
    ),
    (
        PageId::Desktop,
        "Alignment",
        "dock centre left right position",
    ),
    (
        PageId::Desktop,
        "Dock mouse actions",
        "click middle scroll dock mouse",
    ),
    (PageId::Desktop, "Click", "dock click press action"),
    (PageId::Desktop, "Middle click", "dock middle button action"),
    (PageId::Desktop, "Scroll", "dock scroll wheel action"),
    (
        PageId::Desktop,
        "Scroll direction",
        "dock scroll wheel up down",
    ),
    (PageId::Desktop, "Show sidebar", "sidebar pages navigation"),
    (
        PageId::Desktop,
        "Show seconds in the clock",
        "top bar clock time",
    ),
    (
        PageId::Desktop,
        "Show system usage",
        "control centre cpu memory graph",
    ),
    (
        PageId::Desktop,
        "Show my public IP address",
        "privacy network address",
    ),
    (
        PageId::Desktop,
        "Control Centre",
        "tiles quick settings panel",
    ),
    (PageId::Desktop, "Top bar", "panel clock status bar"),
    (
        PageId::Search,
        "Search sources",
        "providers engines files web",
    ),
    (
        PageId::Search,
        "File search",
        "folders find files documents",
    ),
    (
        PageId::Search,
        "Add a folder",
        "folder index include directory",
    ),
    (
        PageId::Search,
        "Previews",
        "thumbnails preview files images",
    ),
    (
        PageId::Search,
        "Preview files",
        "thumbnails pictures documents",
    ),
    (
        PageId::Search,
        "Largest file to preview",
        "size limit thumbnails",
    ),
    (
        PageId::Search,
        "Prepare nearby previews",
        "cache thumbnails background",
    ),
    (PageId::Search, "Web search", "internet engine browser web"),
    (
        PageId::Search,
        "Default engine",
        "search provider google duckduckgo",
    ),
    (
        PageId::Search,
        "Add engine",
        "new search provider web address shortcut",
    ),
    (PageId::Search, "Assistant", "ai chat model helper answers"),
    (
        PageId::Search,
        "Assistant program",
        "ai model local provider",
    ),
    (PageId::Search, "Model", "ai language model assistant"),
    (
        PageId::Applications,
        "Default applications",
        "browser mail music video photos defaults",
    ),
    (
        PageId::Applications,
        "Open by default",
        "file type link open with program",
    ),
    (
        PageId::Applications,
        "Installed",
        "apps programs uninstall list",
    ),
    (
        PageId::DateTime,
        "Set automatically",
        "ntp network time sync clock",
    ),
    (
        PageId::DateTime,
        "Time zone",
        "city region location timezone",
    ),
    (PageId::DateTime, "Clock format", "24 hour 12 am pm"),
    (
        PageId::DateTime,
        "Show seconds",
        "clock time seconds top bar",
    ),
    (
        PageId::DateTime,
        "Show date in the top bar",
        "date clock top bar",
    ),
    (
        PageId::DateTime,
        "Show weekday in the top bar",
        "day week clock top bar",
    ),
    (
        PageId::DateTime,
        "Set date and time",
        "clock manual change time",
    ),
    (PageId::Users, "Full name", "display name real name user"),
    (PageId::Users, "User name", "login account username"),
    (
        PageId::Users,
        "Account type",
        "administrator standard admin permissions",
    ),
    (PageId::Users, "Change password", "password login secret"),
    (
        PageId::Users,
        "Automatic login",
        "sign in autologin password skip",
    ),
    (PageId::Users, "Account locked", "disabled lock access"),
    (
        PageId::About,
        "Device name",
        "hostname computer name machine",
    ),
    (
        PageId::About,
        "Hardware model",
        "machine computer make model",
    ),
    (PageId::About, "Processor", "cpu chip cores"),
    (PageId::About, "Memory", "ram size gigabytes"),
    (PageId::About, "Disk", "storage drive space capacity"),
    (PageId::About, "Kernel", "linux version system"),
    (
        PageId::About,
        "Operating system",
        "bingux distribution name version",
    ),
    (PageId::About, "Desktop version", "gnoblin shell version"),
    (
        PageId::About,
        "Display server",
        "wayland x11 graphics session",
    ),
    (PageId::About, "Bingux version", "release system version"),
];

/// The index with each entry's title and words folded to lower case once, so
/// a keystroke only folds the query.
static FOLDED: std::sync::LazyLock<Vec<(PageId, &'static str, String)>> =
    std::sync::LazyLock::new(|| {
        SETTINGS
            .iter()
            .map(|(page, title, keywords)| {
                (*page, *title, format!("{title} {keywords}").to_lowercase())
            })
            .collect()
    });

/// The words of `query`, folded to lower case.
fn query_words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// Whether `haystack` contains every word of `words`.
fn contains_all(haystack: &str, words: &[String]) -> bool {
    words.iter().all(|word| haystack.contains(word.as_str()))
}

/// The settings whose title or words contain every word of `query`, ignoring
/// case, in index order. An empty query matches nothing.
pub fn matching(query: &str) -> Vec<(PageId, &'static str)> {
    let words = query_words(query);
    if words.is_empty() {
        return Vec::new();
    }
    FOLDED
        .iter()
        .filter(|(_, _, haystack)| contains_all(haystack, &words))
        .map(|(page, title, _)| (*page, *title))
        .collect()
}

/// Each page's title and sidebar words folded to lower case once.
static PAGES_FOLDED: std::sync::LazyLock<Vec<(PageId, String)>> = std::sync::LazyLock::new(|| {
    PageId::ALL
        .iter()
        .map(|page| {
            (
                *page,
                format!("{} {}", page.title(), page.keywords()).to_lowercase(),
            )
        })
        .collect()
});

/// The pages whose own title or words contain every word of `query`, ignoring
/// case. An empty query matches nothing.
pub fn matching_pages(query: &str) -> Vec<PageId> {
    let words = query_words(query);
    if words.is_empty() {
        return Vec::new();
    }
    PAGES_FOLDED
        .iter()
        .filter(|(_, haystack)| contains_all(haystack, &words))
        .map(|(page, _)| *page)
        .collect()
}
