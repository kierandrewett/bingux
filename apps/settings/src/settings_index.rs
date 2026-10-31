//! What Settings registers with bingux-ui's search: one document per page
//! and per setting, and one per installed application.
//!
//! [`SETTINGS`] names, for each setting, the page it is on, its title as the
//! page shows it, and plain words a person might search for instead. Add a
//! row when a page gains a setting. The search itself (matching, typo
//! tolerance, ranking, highlights, Spotlight) is `bingux_ui::search`; this file
//! only describes the documents.
//!
//! A document's `uri` is where choosing it goes: the page, the subpage when
//! the setting lives on one, and a `#fragment` naming the row (the slug of its
//! title, which the row anchors itself to).

use std::sync::atomic::{AtomicBool, Ordering};

use bingux_ui::prelude::Location;
use bingux_ui::search::{self, Document, Schema, Searchable};

use crate::pages::PageId;

/// The index of pages and settings.
pub const INDEX: &str = "settings";
/// The index of installed applications.
pub const APPS_INDEX: &str = "settings-applications";


/// Page, visible title, and extra words to match.
const SETTINGS: &[(PageId, &str, &str)] = &[
    (PageId::Network, "Wi-Fi", "wireless internet on off radio"),
    (
        PageId::Network,
        "Airplane mode",
        "flight offline radios disable wireless",
    ),
    (
        PageId::Network,
        "Connect to hidden network",
        "ssid hidden join add network",
    ),
    (
        PageId::Network,
        "Saved networks",
        "remembered forget passwords known",
    ),
    (
        PageId::Network,
        "Wi-Fi hotspot",
        "share tether access point internet sharing",
    ),
    (
        PageId::Network,
        "Wired",
        "ethernet cable lan connection profile 802.1x",
    ),
    (
        PageId::Network,
        "Add VPN",
        "wireguard openvpn import ovpn conf openconnect vpnc tunnel",
    ),
    (
        PageId::Network,
        "Proxy",
        "system proxy http https socks pac automatic manual ignore hosts",
    ),
    (
        PageId::Network,
        "Data usage",
        "bandwidth traffic downloaded uploaded limit billing cycle monthly warning metered",
    ),
    (
        PageId::Network,
        "Mobile network",
        "cellular mobile data sim apn roaming carrier 4g 5g lte modem imei pin",
    ),
    (
        PageId::Network,
        "Share Wi-Fi network",
        "qr code password reveal copy share wifi",
    ),
    (
        PageId::Network,
        "Technical details",
        "connection details ip address gateway dns routes mac mtu speed uuid device advanced",
    ),
    (
        PageId::Network,
        "Connect automatically",
        "autoconnect join startup network connection",
    ),
    (
        PageId::Network,
        "Wi-Fi password",
        "show reveal copy saved key passphrase network",
    ),
    (
        PageId::Network,
        "Priority",
        "autoconnect order higher first available other users permissions advanced",
    ),
    (
        PageId::Network,
        "Wired profiles",
        "ethernet profile switch add manage delete connection",
    ),
    (
        PageId::Network,
        "VPN",
        "virtual private network tunnel connection",
    ),
    (
        PageId::Network,
        "MTU",
        "packet size bytes maximum transmission unit automatic custom",
    ),
    (
        PageId::Network,
        "Wake on LAN",
        "wake magic packet power ethernet wired",
    ),
    (
        PageId::Network,
        "Cloned MAC address",
        "mac hardware address spoof identity",
    ),
    (
        PageId::Network,
        "IPv4 method",
        "ip address dhcp automatic manual link-local disabled shared",
    ),
    (
        PageId::Network,
        "IPv6 method",
        "ip address dhcp automatic manual ignore disabled shared",
    ),
    (
        PageId::Network,
        "Privacy extensions",
        "ipv6 temporary addresses privacy random",
    ),
    (
        PageId::Network,
        "Automatic DNS",
        "dns servers ignore automatic name resolution",
    ),
    (
        PageId::Network,
        "DNS search domains",
        "dns search domain suffix",
    ),
    (
        PageId::Network,
        "Use this connection only for resources on its network",
        "never default route gateway",
    ),
    (
        PageId::Network,
        "Random MAC address",
        "mac hardware address randomise randomize privacy",
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
    (PageId::Bluetooth, "Name", "rename device alias"),
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
        "Application volumes",
        "applications app volume per app mute playing recording stream meter output",
    ),
    (
        PageId::Sound,
        "Effects",
        "equalizer eq compressor limiter reverb preset easyeffects plugin spectrum bass treble noise gate",
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
        "Cursor blinking",
        "caret cursor text insertion blink",
    ),
    (
        PageId::Keyboard,
        "Blink speed",
        "cursor blink time caret speed",
    ),
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
        "Middle click paste",
        "primary selection paste middle button",
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
        PageId::Mouse,
        "Touchpad pointer acceleration",
        "touchpad speed sensitivity fast slow",
    ),
    (
        PageId::Mouse,
        "Primary button",
        "touchpad swap buttons left right click",
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
    (
        PageId::Appearance,
        "Text size",
        "scale larger smaller font accessibility",
    ),
    (
        PageId::Accessibility,
        "Large text",
        "bigger scaling accessibility",
    ),
    (
        PageId::Accessibility,
        "High contrast",
        "contrast outline accessibility",
    ),
    (
        PageId::Accessibility,
        "Reduce animations",
        "motion transitions accessibility",
    ),
    (
        PageId::Accessibility,
        "Cursor size",
        "pointer mouse big small accessibility",
    ),
    (
        PageId::Accessibility,
        "Overlay scrollbars",
        "scrollbar overlay hide scrolling",
    ),
    (
        PageId::Accessibility,
        "Screen reader",
        "speech read aloud narrator blind vision",
    ),
    (
        PageId::Accessibility,
        "On-screen keyboard",
        "virtual keyboard typing pointer touch",
    ),
    (
        PageId::Accessibility,
        "Sticky keys",
        "modifier shift control alt one at a time",
    ),
    (
        PageId::Accessibility,
        "Slow keys",
        "hold key delay typing accessibility",
    ),
    (
        PageId::Accessibility,
        "Bounce keys",
        "repeat ignore double press typing",
    ),
    (
        PageId::Accessibility,
        "Mouse keys",
        "number pad pointer move keyboard",
    ),
    (
        PageId::Accessibility,
        "Locate pointer",
        "find cursor highlight ripple",
    ),
    (PageId::Appearance, "Icons", "icon theme symbols"),
    (PageId::Appearance, "Cursor", "pointer theme mouse"),
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
        "Search subfolders",
        "recursive nested folders subdirectories",
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
        PageId::Applications,
        "Start at login",
        "autostart startup login launch application",
    ),
    (
        PageId::Applications,
        "File types",
        "mime type extension supported handle open",
    ),
    (
        PageId::Applications,
        "Notifications",
        "banner alert lock screen sound notify",
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
    (PageId::Users, "Language", "locale region language format"),
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
    (PageId::About, "Graphics", "gpu video card driver"),
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

/// The subpage a setting lives on, when it is not on the page itself.
fn subpage(page: PageId, title: &str) -> Option<&'static str> {
    Some(match (page, title) {
        (PageId::Network, "Wi-Fi") => "wifi",
        (PageId::Network, "Saved networks") => "wifi/saved",
        (PageId::Network, "Wi-Fi hotspot") => "hotspot",
        (PageId::Network, "Add VPN") => "add-vpn",
        (PageId::Network, "Proxy") => "proxy",
        (PageId::Network, "Data usage") => "usage",
        (PageId::Network, "Mobile network") => "mobile",
        (PageId::Keyboard, "Shortcuts") => "shortcuts",
        (PageId::DateTime, "Time zone") => "timezone",
        (PageId::Appearance, "Font") => "font",
        (PageId::Sound, "Application volumes") => "applications",
        (PageId::Applications, "Default applications" | "Open by default") => "default",
        _ => return None,
    })
}

/// A title as a fragment: lower case words joined with `-`.
pub fn slug(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut gap = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.extend(c.to_lowercase());
        } else {
            gap = true;
        }
    }
    out
}

/// The address of a setting: its page and subpage, and its row as the fragment.
fn uri(page: PageId, title: &str) -> String {
    let mut at: Location = page.location();
    if let Some(path) = subpage(page, title) {
        for segment in path.split('/') {
            at = at.join(segment);
        }
    }
    at.with_fragment(slug(title)).to_string()
}

fn page_document(page: PageId) -> Document {
    Document::new(page.panel_name())
        .title(page.title())
        .keywords(page.keywords())
        .breadcrumb([page.title()])
        .category(page.group())
        .field("page", page.panel_name())
        .field("kind", "page")
        .icon("bingux-settings")
        .uri(&page.location().to_string())
}

fn setting_document(page: PageId, title: &str, keywords: &str) -> Document {
    Document::new(format!("{}/{}", page.panel_name(), slug(title)))
        .title(title)
        .keywords(keywords)
        .breadcrumb([page.title()])
        .category(page.group())
        .field("page", page.panel_name())
        .field("kind", "setting")
        .icon("bingux-settings")
        .uri(&uri(page, title))
}

/// The pages and settings of Bingux Settings. Exposed to Spotlight: the
/// documents are static and say nothing about the person or the machine.
pub struct SettingsIndex;

impl Searchable for SettingsIndex {
    fn name(&self) -> &str {
        INDEX
    }

    fn schema(&self) -> Schema {
        Schema::new()
            .title("Settings")
            .searchable("title", 10.0)
            .searchable("keywords", 5.0)
            .searchable("breadcrumb", 3.0)
            .filterable(["page", "category", "kind"])
            .displayed(["title", "breadcrumb", "icon", "uri", "page", "category", "kind"])
            .synonym_group(&["colour", "color"])
            .synonym_group(&["wallpaper", "background"])
            .synonym_group(&["display", "monitor", "screen"])
            .synonym_group(&["sound", "audio"])
            .external(true)
    }

    fn documents(&self) -> Box<dyn Iterator<Item = Document> + Send + '_> {
        Box::new(
            PageId::ALL
                .iter()
                .copied()
                .chain(std::iter::once(PageId::Users))
                .map(page_document)
                .chain(
                    SETTINGS
                        .iter()
                        .map(|(page, title, keywords)| setting_document(*page, title, keywords)),
                ),
        )
    }
}

/// The installed applications, each opening its page. Private: Spotlight has
/// its own application search, and the list is the person's.
pub struct ApplicationsIndex;

impl Searchable for ApplicationsIndex {
    fn name(&self) -> &str {
        APPS_INDEX
    }

    fn schema(&self) -> Schema {
        Schema::new()
            .title("Installed applications")
            .searchable("title", 10.0)
            .searchable("keywords", 5.0)
            .searchable("breadcrumb", 3.0)
            .filterable(["page", "kind"])
            .displayed(["title", "breadcrumb", "icon", "uri", "page", "category", "kind"])
    }

    fn documents(&self) -> Box<dyn Iterator<Item = Document> + Send + '_> {
        let location = PageId::Applications.location();
        Box::new(crate::apps::installed().into_iter().map(move |app| {
            Document::new(app.id.clone())
                .title(&app.name)
                .keywords(&format!("{} {}", app.comment, app.category))
                .breadcrumb([PageId::Applications.title()])
                .category(PageId::Applications.group())
                .field("page", PageId::Applications.panel_name())
                .field("kind", "application")
                .icon(&app.icon)
                .uri(&location.clone().join(app.id.as_str()).to_string())
        }))
    }
}

/// Starts search in the window's process: registers the indexes and shares the
/// external one with Spotlight. Called by the window root, so a second launch,
/// which only hands its address to the running copy, never takes the socket.
pub fn start() {
    register();
    #[cfg(not(feature = "snapshot"))]
    if let Err(error) = search::serve_external(search::ExternalConfig {
        app_id: "bingux-settings".into(),
        app_version: env!("CARGO_PKG_VERSION").into(),
    }) {
        eprintln!("bingux-settings: search is not shared with Spotlight: {error}");
    }
}

/// Registers what Settings searches. The documents load on a worker.
pub fn register() {
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.swap(true, Ordering::AcqRel) {
        return;
    }
    for result in [search::register(SettingsIndex), search::register(ApplicationsIndex)] {
        if let Err(error) = result {
            eprintln!("bingux-settings: search index did not register: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::SCHEME;
    use bingux_ui::search::{Cancel, Index, Search};

    fn index() -> Index {
        let index = Index::new(INDEX, SettingsIndex.schema()).unwrap();
        index.upsert(SettingsIndex.documents()).unwrap();
        index
    }

    fn top(index: &Index, text: &str) -> Vec<(String, String)> {
        let found = index
            .search(&Search::new(text).limit(15), &Cancel::new())
            .unwrap();
        found
            .hits
            .iter()
            .map(|h| (h.title().unwrap_or_default().to_string(), h.uri().unwrap_or_default().to_string()))
            .collect()
    }

    #[test]
    fn every_document_opens_a_route_of_the_window() {
        let table = crate::routes::table();
        let index = index();
        let docs = index.export(false);
        assert!(docs.len() > 150, "{} documents", docs.len());
        for doc in docs {
            let uri = doc.fields["uri"].as_str().expect("uri");
            let location = Location::parse(uri).unwrap_or_else(|| panic!("{uri} is not an address"));
            assert_eq!(location.scheme(), SCHEME, "{uri}");
            assert!(
                table.resolve(&location).is_some(),
                "`{}` has the address {uri}, which no route opens",
                doc.id
            );
        }
    }

    #[test]
    fn ids_are_unique_and_slugs_are_fragments() {
        let docs = index().export(false);
        let mut ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "two settings share an id");
        assert_eq!(slug("Use this connection only for resources on its network"), "use-this-connection-only-for-resources-on-its-network");
        assert_eq!(slug("Wi-Fi"), "wi-fi");
    }

    #[test]
    fn typing_finds_the_setting_and_its_subpage() {
        let index = index();
        let hits = top(&index, "proxy");
        assert_eq!(hits[0].0, "Proxy");
        assert_eq!(hits[0].1, "bingux-settings://network/proxy#proxy");
        // Typos, prefixes and compounds, as a person types them.
        assert_eq!(top(&index, "blutooth")[0].0, "Bluetooth");
        assert_eq!(top(&index, "prxoy")[0].0, "Proxy");
        assert_eq!(top(&index, "wifi")[0].0, "Wi-Fi");
        assert_eq!(top(&index, "time zo")[0].0, "Time zone");
        assert_eq!(top(&index, "accent colour")[0].1, "bingux-settings://background#accent-colour");
        // The American spelling finds the British one through a synonym.
        assert!(top(&index, "color").iter().any(|h| h.0 == "Accent colour"));
        // A synonym.
        assert!(top(&index, "monitor").iter().any(|h| h.1.starts_with("bingux-settings://display")));
        // A page name finds the page before its settings.
        assert_eq!(top(&index, "keyboard")[0].0, "Keyboard");
    }

    #[test]
    fn the_index_is_external_and_private_data_is_not_in_it() {
        assert!(SettingsIndex.schema().external, "Spotlight may search the settings");
        assert!(!ApplicationsIndex.schema().external, "the application list is the person's");
        for doc in SettingsIndex.documents() {
            assert_ne!(doc.external, Some(true));
        }
    }
}
