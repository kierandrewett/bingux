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
const SETTINGS: &[(PageId, &str, &str, &str)] = &[
    (
        PageId::Network,
        "wi-fi",
        bingux_ui::i18n_id!("settings-setting-network-wi-fi"),
        "wireless internet on off radio",
    ),
    (
        PageId::Network,
        "airplane-mode",
        bingux_ui::i18n_id!("settings-setting-network-airplane-mode"),
        "flight offline radios disable wireless",
    ),
    (
        PageId::Network,
        "connect-to-hidden-network",
        bingux_ui::i18n_id!("settings-setting-network-connect-to-hidden-network"),
        "ssid hidden join add network",
    ),
    (
        PageId::Network,
        "saved-networks",
        bingux_ui::i18n_id!("settings-setting-network-saved-networks"),
        "remembered forget passwords known",
    ),
    (
        PageId::Network,
        "wi-fi-hotspot",
        bingux_ui::i18n_id!("settings-setting-network-wi-fi-hotspot"),
        "share tether access point internet sharing",
    ),
    (
        PageId::Network,
        "wired",
        bingux_ui::i18n_id!("settings-setting-network-wired"),
        "ethernet cable lan connection profile 802.1x",
    ),
    (
        PageId::Network,
        "add-vpn",
        bingux_ui::i18n_id!("settings-setting-network-add-vpn"),
        "wireguard openvpn import ovpn conf openconnect vpnc tunnel",
    ),
    (
        PageId::Network,
        "proxy",
        bingux_ui::i18n_id!("settings-setting-network-proxy"),
        "system proxy http https socks pac automatic manual ignore hosts",
    ),
    (
        PageId::Network,
        "data-usage",
        bingux_ui::i18n_id!("settings-setting-network-data-usage"),
        "bandwidth traffic downloaded uploaded limit billing cycle monthly warning metered",
    ),
    (
        PageId::Network,
        "mobile-network",
        bingux_ui::i18n_id!("settings-setting-network-mobile-network"),
        "cellular mobile data sim apn roaming carrier 4g 5g lte modem imei pin",
    ),
    (
        PageId::Network,
        "share-wi-fi-network",
        bingux_ui::i18n_id!("settings-setting-network-share-wi-fi-network"),
        "qr code password reveal copy share wifi",
    ),
    (
        PageId::Network,
        "technical-details",
        bingux_ui::i18n_id!("settings-setting-network-technical-details"),
        "connection details ip address gateway dns routes mac mtu speed uuid device advanced",
    ),
    (
        PageId::Network,
        "connect-automatically",
        bingux_ui::i18n_id!("settings-setting-network-connect-automatically"),
        "autoconnect join startup network connection",
    ),
    (
        PageId::Network,
        "wi-fi-password",
        bingux_ui::i18n_id!("settings-setting-network-wi-fi-password"),
        "show reveal copy saved key passphrase network",
    ),
    (
        PageId::Network,
        "priority",
        bingux_ui::i18n_id!("settings-setting-network-priority"),
        "autoconnect order higher first available other users permissions advanced",
    ),
    (
        PageId::Network,
        "wired-profiles",
        bingux_ui::i18n_id!("settings-setting-network-wired-profiles"),
        "ethernet profile switch add manage delete connection",
    ),
    (
        PageId::Network,
        "vpn",
        bingux_ui::i18n_id!("settings-setting-network-vpn"),
        "virtual private network tunnel connection",
    ),
    (
        PageId::Network,
        "mtu",
        bingux_ui::i18n_id!("settings-setting-network-mtu"),
        "packet size bytes maximum transmission unit automatic custom",
    ),
    (
        PageId::Network,
        "wake-on-lan",
        bingux_ui::i18n_id!("settings-setting-network-wake-on-lan"),
        "wake magic packet power ethernet wired",
    ),
    (
        PageId::Network,
        "cloned-mac-address",
        bingux_ui::i18n_id!("settings-setting-network-cloned-mac-address"),
        "mac hardware address spoof identity",
    ),
    (
        PageId::Network,
        "ipv4-method",
        bingux_ui::i18n_id!("settings-setting-network-ipv4-method"),
        "ip address dhcp automatic manual link-local disabled shared",
    ),
    (
        PageId::Network,
        "ipv6-method",
        bingux_ui::i18n_id!("settings-setting-network-ipv6-method"),
        "ip address dhcp automatic manual ignore disabled shared",
    ),
    (
        PageId::Network,
        "privacy-extensions",
        bingux_ui::i18n_id!("settings-setting-network-privacy-extensions"),
        "ipv6 temporary addresses privacy random",
    ),
    (
        PageId::Network,
        "automatic-dns",
        bingux_ui::i18n_id!("settings-setting-network-automatic-dns"),
        "dns servers ignore automatic name resolution",
    ),
    (
        PageId::Network,
        "dns-search-domains",
        bingux_ui::i18n_id!("settings-setting-network-dns-search-domains"),
        "dns search domain suffix",
    ),
    (
        PageId::Network,
        "use-this-connection-only-for-resources-on-its-network",
        bingux_ui::i18n_id!(
            "settings-setting-network-use-this-connection-only-for-resources-on-its-network"
        ),
        "never default route gateway",
    ),
    (
        PageId::Network,
        "random-mac-address",
        bingux_ui::i18n_id!("settings-setting-network-random-mac-address"),
        "mac hardware address randomise randomize privacy",
    ),
    (
        PageId::Bluetooth,
        "bluetooth",
        bingux_ui::i18n_id!("settings-setting-bluetooth-bluetooth"),
        "pair wireless devices on off headphones",
    ),
    (
        PageId::Bluetooth,
        "connects-automatically",
        bingux_ui::i18n_id!("settings-setting-bluetooth-connects-automatically"),
        "reconnect auto trusted",
    ),
    (
        PageId::Bluetooth,
        "forget-device",
        bingux_ui::i18n_id!("settings-setting-bluetooth-forget-device"),
        "remove unpair delete",
    ),
    (
        PageId::Bluetooth,
        "battery",
        bingux_ui::i18n_id!("settings-setting-bluetooth-battery"),
        "charge level device",
    ),
    (
        PageId::Bluetooth,
        "name",
        bingux_ui::i18n_id!("settings-setting-bluetooth-name"),
        "rename device alias",
    ),
    (
        PageId::Displays,
        "resolution",
        bingux_ui::i18n_id!("settings-setting-displays-resolution"),
        "size pixels sharpness monitor screen",
    ),
    (
        PageId::Displays,
        "refresh-rate",
        bingux_ui::i18n_id!("settings-setting-displays-refresh-rate"),
        "hertz hz frequency smooth",
    ),
    (
        PageId::Displays,
        "scale",
        bingux_ui::i18n_id!("settings-setting-displays-scale"),
        "zoom size text bigger smaller hidpi",
    ),
    (
        PageId::Displays,
        "orientation",
        bingux_ui::i18n_id!("settings-setting-displays-orientation"),
        "rotate portrait landscape flip",
    ),
    (
        PageId::Displays,
        "arrangement",
        bingux_ui::i18n_id!("settings-setting-displays-arrangement"),
        "layout position order monitors side by side",
    ),
    (
        PageId::Displays,
        "primary-display",
        bingux_ui::i18n_id!("settings-setting-displays-primary-display"),
        "main monitor default",
    ),
    (
        PageId::Displays,
        "enabled",
        bingux_ui::i18n_id!("settings-setting-displays-enabled"),
        "turn off monitor disable display",
    ),
    (
        PageId::Displays,
        "night-light",
        bingux_ui::i18n_id!("settings-setting-displays-night-light"),
        "blue light warm evening night mode",
    ),
    (
        PageId::Displays,
        "on-a-schedule",
        bingux_ui::i18n_id!("settings-setting-displays-on-a-schedule"),
        "night light time automatic sunset sunrise",
    ),
    (
        PageId::Displays,
        "colour-temperature",
        bingux_ui::i18n_id!("settings-setting-displays-colour-temperature"),
        "warmth warm cool tint color",
    ),
    (
        PageId::Sound,
        "volume",
        bingux_ui::i18n_id!("settings-setting-sound-volume"),
        "loudness speakers sound level quiet loud",
    ),
    (
        PageId::Sound,
        "output-device",
        bingux_ui::i18n_id!("settings-setting-sound-output-device"),
        "speakers headphones playback outputs",
    ),
    (
        PageId::Sound,
        "input-device",
        bingux_ui::i18n_id!("settings-setting-sound-input-device"),
        "microphone recording mic inputs",
    ),
    (
        PageId::Sound,
        "application-volumes",
        bingux_ui::i18n_id!("settings-setting-sound-application-volumes"),
        "applications app volume per app mute playing recording stream meter output",
    ),
    (
        PageId::Sound,
        "effects",
        bingux_ui::i18n_id!("settings-setting-sound-effects"),
        "equalizer eq compressor limiter reverb preset easyeffects plugin spectrum bass treble noise gate",
    ),
    (
        PageId::Power,
        "power-mode",
        bingux_ui::i18n_id!("settings-setting-power-power-mode"),
        "performance balanced saver battery energy",
    ),
    (
        PageId::Power,
        "battery",
        bingux_ui::i18n_id!("settings-setting-power-battery"),
        "charge percentage energy state",
    ),
    (
        PageId::Power,
        "automatic-screen-lock",
        bingux_ui::i18n_id!("settings-setting-power-automatic-screen-lock"),
        "lock password idle locked",
    ),
    (
        PageId::Power,
        "lock-after",
        bingux_ui::i18n_id!("settings-setting-power-lock-after"),
        "idle timeout password delay",
    ),
    (
        PageId::Power,
        "blank-screen-after",
        bingux_ui::i18n_id!("settings-setting-power-blank-screen-after"),
        "screen off display sleep idle timeout",
    ),
    (
        PageId::Power,
        "dim-screen-when-inactive",
        bingux_ui::i18n_id!("settings-setting-power-dim-screen-when-inactive"),
        "brightness idle dark",
    ),
    (
        PageId::Power,
        "suspend-when-inactive",
        bingux_ui::i18n_id!("settings-setting-power-suspend-when-inactive"),
        "sleep idle standby",
    ),
    (
        PageId::Power,
        "suspend-after",
        bingux_ui::i18n_id!("settings-setting-power-suspend-after"),
        "sleep idle timeout standby",
    ),
    (
        PageId::Power,
        "when-the-power-button-is-pressed",
        bingux_ui::i18n_id!("settings-setting-power-when-the-power-button-is-pressed"),
        "shutdown sleep suspend off",
    ),
    (
        PageId::Keyboard,
        "input-sources",
        bingux_ui::i18n_id!("settings-setting-keyboard-input-sources"),
        "layout language keyboard add input method",
    ),
    (
        PageId::Keyboard,
        "keyboard-layout",
        bingux_ui::i18n_id!("settings-setting-keyboard-keyboard-layout"),
        "language qwerty dvorak",
    ),
    (
        PageId::Keyboard,
        "input-method",
        bingux_ui::i18n_id!("settings-setting-keyboard-input-method"),
        "ime typing language",
    ),
    (
        PageId::Keyboard,
        "repeat-keys",
        bingux_ui::i18n_id!("settings-setting-keyboard-repeat-keys"),
        "auto repeat hold keys",
    ),
    (
        PageId::Keyboard,
        "repeat-speed",
        bingux_ui::i18n_id!("settings-setting-keyboard-repeat-speed"),
        "typematic rate fast slow",
    ),
    (
        PageId::Keyboard,
        "delay-before-repeat",
        bingux_ui::i18n_id!("settings-setting-keyboard-delay-before-repeat"),
        "hold delay wait",
    ),
    (
        PageId::Keyboard,
        "cursor-blinking",
        bingux_ui::i18n_id!("settings-setting-keyboard-cursor-blinking"),
        "caret cursor text insertion blink",
    ),
    (
        PageId::Keyboard,
        "blink-speed",
        bingux_ui::i18n_id!("settings-setting-keyboard-blink-speed"),
        "cursor blink time caret speed",
    ),
    (
        PageId::Keyboard,
        "caps-lock-key",
        bingux_ui::i18n_id!("settings-setting-keyboard-caps-lock-key"),
        "capslock shift caps uppercase",
    ),
    (
        PageId::Keyboard,
        "compose-key",
        bingux_ui::i18n_id!("settings-setting-keyboard-compose-key"),
        "accents special characters symbols",
    ),
    (
        PageId::Keyboard,
        "num-lock-on-at-sign-in",
        bingux_ui::i18n_id!("settings-setting-keyboard-num-lock-on-at-sign-in"),
        "numlock numbers startup login",
    ),
    (
        PageId::Keyboard,
        "remember-num-lock-state",
        bingux_ui::i18n_id!("settings-setting-keyboard-remember-num-lock-state"),
        "numlock memory keep",
    ),
    (
        PageId::Keyboard,
        "shortcuts",
        bingux_ui::i18n_id!("settings-setting-keyboard-shortcuts"),
        "keybindings hotkeys key combinations",
    ),
    (
        PageId::Keyboard,
        "try-it",
        bingux_ui::i18n_id!("settings-setting-keyboard-try-it"),
        "test repeat",
    ),
    (
        PageId::Mouse,
        "pointer-acceleration",
        bingux_ui::i18n_id!("settings-setting-mouse-pointer-acceleration"),
        "speed sensitivity mouse fast slow",
    ),
    (
        PageId::Mouse,
        "left-handed-buttons",
        bingux_ui::i18n_id!("settings-setting-mouse-left-handed-buttons"),
        "swap buttons left right mouse",
    ),
    (
        PageId::Mouse,
        "middle-click-paste",
        bingux_ui::i18n_id!("settings-setting-mouse-middle-click-paste"),
        "primary selection paste middle button",
    ),
    (
        PageId::Mouse,
        "natural-scrolling",
        bingux_ui::i18n_id!("settings-setting-mouse-natural-scrolling"),
        "scroll direction reverse touchpad mouse",
    ),
    (
        PageId::Mouse,
        "tap-to-click",
        bingux_ui::i18n_id!("settings-setting-mouse-tap-to-click"),
        "touchpad tap press",
    ),
    (
        PageId::Mouse,
        "two-finger-scrolling",
        bingux_ui::i18n_id!("settings-setting-mouse-two-finger-scrolling"),
        "touchpad scroll fingers",
    ),
    (
        PageId::Mouse,
        "scrolling-along-edges",
        bingux_ui::i18n_id!("settings-setting-mouse-scrolling-along-edges"),
        "touchpad edge scroll",
    ),
    (
        PageId::Mouse,
        "off-when-a-mouse-is-connected",
        bingux_ui::i18n_id!("settings-setting-mouse-off-when-a-mouse-is-connected"),
        "touchpad disable mouse plugged",
    ),
    (
        PageId::Mouse,
        "touchpad-pointer-acceleration",
        bingux_ui::i18n_id!("settings-setting-mouse-touchpad-pointer-acceleration"),
        "touchpad speed sensitivity fast slow",
    ),
    (
        PageId::Mouse,
        "primary-button",
        bingux_ui::i18n_id!("settings-setting-mouse-primary-button"),
        "touchpad swap buttons left right click",
    ),
    (
        PageId::Appearance,
        "wallpaper",
        bingux_ui::i18n_id!("settings-setting-appearance-wallpaper"),
        "background picture image desktop",
    ),
    (
        PageId::Appearance,
        "accent-colour",
        bingux_ui::i18n_id!("settings-setting-appearance-accent-colour"),
        "color highlight theme",
    ),
    (
        PageId::Appearance,
        "colour-scheme",
        bingux_ui::i18n_id!("settings-setting-appearance-colour-scheme"),
        "dark light mode theme color",
    ),
    (
        PageId::Appearance,
        "font",
        bingux_ui::i18n_id!("settings-setting-appearance-font"),
        "typeface text size window desktop",
    ),
    (
        PageId::Appearance,
        "text-size",
        bingux_ui::i18n_id!("settings-setting-appearance-text-size"),
        "scale larger smaller font accessibility",
    ),
    (
        PageId::Accessibility,
        "large-text",
        bingux_ui::i18n_id!("settings-setting-accessibility-large-text"),
        "bigger scaling accessibility",
    ),
    (
        PageId::Accessibility,
        "high-contrast",
        bingux_ui::i18n_id!("settings-setting-accessibility-high-contrast"),
        "contrast outline accessibility",
    ),
    (
        PageId::Accessibility,
        "reduce-animations",
        bingux_ui::i18n_id!("settings-setting-accessibility-reduce-animations"),
        "motion transitions accessibility",
    ),
    (
        PageId::Accessibility,
        "cursor-size",
        bingux_ui::i18n_id!("settings-setting-accessibility-cursor-size"),
        "pointer mouse big small accessibility",
    ),
    (
        PageId::Accessibility,
        "overlay-scrollbars",
        bingux_ui::i18n_id!("settings-setting-accessibility-overlay-scrollbars"),
        "scrollbar overlay hide scrolling",
    ),
    (
        PageId::Accessibility,
        "screen-reader",
        bingux_ui::i18n_id!("settings-setting-accessibility-screen-reader"),
        "speech read aloud narrator blind vision",
    ),
    (
        PageId::Accessibility,
        "on-screen-keyboard",
        bingux_ui::i18n_id!("settings-setting-accessibility-on-screen-keyboard"),
        "virtual keyboard typing pointer touch",
    ),
    (
        PageId::Accessibility,
        "sticky-keys",
        bingux_ui::i18n_id!("settings-setting-accessibility-sticky-keys"),
        "modifier shift control alt one at a time",
    ),
    (
        PageId::Accessibility,
        "slow-keys",
        bingux_ui::i18n_id!("settings-setting-accessibility-slow-keys"),
        "hold key delay typing accessibility",
    ),
    (
        PageId::Accessibility,
        "bounce-keys",
        bingux_ui::i18n_id!("settings-setting-accessibility-bounce-keys"),
        "repeat ignore double press typing",
    ),
    (
        PageId::Accessibility,
        "mouse-keys",
        bingux_ui::i18n_id!("settings-setting-accessibility-mouse-keys"),
        "number pad pointer move keyboard",
    ),
    (
        PageId::Accessibility,
        "locate-pointer",
        bingux_ui::i18n_id!("settings-setting-accessibility-locate-pointer"),
        "find cursor highlight ripple",
    ),
    (
        PageId::Appearance,
        "icons",
        bingux_ui::i18n_id!("settings-setting-appearance-icons"),
        "icon theme symbols",
    ),
    (
        PageId::Appearance,
        "cursor",
        bingux_ui::i18n_id!("settings-setting-appearance-cursor"),
        "pointer theme mouse",
    ),
    (
        PageId::Desktop,
        "show-dock",
        bingux_ui::i18n_id!("settings-setting-desktop-show-dock"),
        "dock bar launcher hide",
    ),
    (
        PageId::Desktop,
        "icon-size",
        bingux_ui::i18n_id!("settings-setting-desktop-icon-size"),
        "dock big small larger",
    ),
    (
        PageId::Desktop,
        "side-of-the-screen",
        bingux_ui::i18n_id!("settings-setting-desktop-side-of-the-screen"),
        "dock left right bottom position",
    ),
    (
        PageId::Desktop,
        "alignment",
        bingux_ui::i18n_id!("settings-setting-desktop-alignment"),
        "dock centre left right position",
    ),
    (
        PageId::Desktop,
        "dock-mouse-actions",
        bingux_ui::i18n_id!("settings-setting-desktop-dock-mouse-actions"),
        "click middle scroll dock mouse",
    ),
    (
        PageId::Desktop,
        "click",
        bingux_ui::i18n_id!("settings-setting-desktop-click"),
        "dock click press action",
    ),
    (
        PageId::Desktop,
        "middle-click",
        bingux_ui::i18n_id!("settings-setting-desktop-middle-click"),
        "dock middle button action",
    ),
    (
        PageId::Desktop,
        "scroll",
        bingux_ui::i18n_id!("settings-setting-desktop-scroll"),
        "dock scroll wheel action",
    ),
    (
        PageId::Desktop,
        "scroll-direction",
        bingux_ui::i18n_id!("settings-setting-desktop-scroll-direction"),
        "dock scroll wheel up down",
    ),
    (
        PageId::Desktop,
        "show-sidebar",
        bingux_ui::i18n_id!("settings-setting-desktop-show-sidebar"),
        "sidebar pages navigation",
    ),
    (
        PageId::Desktop,
        "show-seconds-in-the-clock",
        bingux_ui::i18n_id!("settings-setting-desktop-show-seconds-in-the-clock"),
        "top bar clock time",
    ),
    (
        PageId::Desktop,
        "show-system-usage",
        bingux_ui::i18n_id!("settings-setting-desktop-show-system-usage"),
        "control centre cpu memory graph",
    ),
    (
        PageId::Desktop,
        "show-my-public-ip-address",
        bingux_ui::i18n_id!("settings-setting-desktop-show-my-public-ip-address"),
        "privacy network address",
    ),
    (
        PageId::Desktop,
        "control-centre",
        bingux_ui::i18n_id!("settings-setting-desktop-control-centre"),
        "tiles quick settings panel",
    ),
    (
        PageId::Desktop,
        "top-bar",
        bingux_ui::i18n_id!("settings-setting-desktop-top-bar"),
        "panel clock status bar",
    ),
    (
        PageId::Search,
        "search-sources",
        bingux_ui::i18n_id!("settings-setting-search-search-sources"),
        "providers engines files web",
    ),
    (
        PageId::Search,
        "file-search",
        bingux_ui::i18n_id!("settings-setting-search-file-search"),
        "folders find files documents",
    ),
    (
        PageId::Search,
        "new-folder",
        bingux_ui::i18n_id!("settings-setting-search-new-folder"),
        "add choose folder index include directory",
    ),
    (
        PageId::Search,
        "search-subfolders",
        bingux_ui::i18n_id!("settings-setting-search-search-subfolders"),
        "recursive nested folders subdirectories",
    ),
    (
        PageId::Search,
        "previews",
        bingux_ui::i18n_id!("settings-setting-search-previews"),
        "thumbnails preview files images",
    ),
    (
        PageId::Search,
        "preview-files",
        bingux_ui::i18n_id!("settings-setting-search-preview-files"),
        "thumbnails pictures documents",
    ),
    (
        PageId::Search,
        "largest-file-to-preview",
        bingux_ui::i18n_id!("settings-setting-search-largest-file-to-preview"),
        "size limit thumbnails",
    ),
    (
        PageId::Search,
        "prepare-nearby-previews",
        bingux_ui::i18n_id!("settings-setting-search-prepare-nearby-previews"),
        "cache thumbnails background",
    ),
    (
        PageId::Search,
        "web-search",
        bingux_ui::i18n_id!("settings-setting-search-web-search"),
        "internet engine browser web",
    ),
    (
        PageId::Search,
        "default-engine",
        bingux_ui::i18n_id!("settings-setting-search-default-engine"),
        "search provider google duckduckgo",
    ),
    (
        PageId::Search,
        "new-search-engine",
        bingux_ui::i18n_id!("settings-setting-search-new-search-engine"),
        "add new search provider web address shortcut",
    ),
    (
        PageId::Search,
        "assistant",
        bingux_ui::i18n_id!("settings-setting-search-assistant"),
        "ai chat model helper answers",
    ),
    (
        PageId::Search,
        "assistant-program",
        bingux_ui::i18n_id!("settings-setting-search-assistant-program"),
        "ai model local provider",
    ),
    (
        PageId::Search,
        "model",
        bingux_ui::i18n_id!("settings-setting-search-model"),
        "ai language model assistant",
    ),
    (
        PageId::Applications,
        "default-applications",
        bingux_ui::i18n_id!("settings-setting-applications-default-applications"),
        "browser mail music video photos defaults",
    ),
    (
        PageId::Applications,
        "open-by-default",
        bingux_ui::i18n_id!("settings-setting-applications-open-by-default"),
        "file type link open with program",
    ),
    (
        PageId::Applications,
        "installed",
        bingux_ui::i18n_id!("settings-setting-applications-installed"),
        "apps programs uninstall list",
    ),
    (
        PageId::Applications,
        "start-at-login",
        bingux_ui::i18n_id!("settings-setting-applications-start-at-login"),
        "autostart startup login launch application",
    ),
    (
        PageId::Applications,
        "file-types",
        bingux_ui::i18n_id!("settings-setting-applications-file-types"),
        "mime type extension supported handle open",
    ),
    (
        PageId::Applications,
        "notifications",
        bingux_ui::i18n_id!("settings-setting-applications-notifications"),
        "banner alert lock screen sound notify",
    ),
    (
        PageId::DateTime,
        "set-automatically",
        bingux_ui::i18n_id!("settings-setting-datetime-set-automatically"),
        "ntp network time sync clock",
    ),
    (
        PageId::DateTime,
        "time-zone",
        bingux_ui::i18n_id!("settings-setting-datetime-time-zone"),
        "city region location timezone",
    ),
    (
        PageId::DateTime,
        "clock-format",
        bingux_ui::i18n_id!("settings-setting-datetime-clock-format"),
        "24 hour 12 am pm",
    ),
    (
        PageId::DateTime,
        "show-seconds",
        bingux_ui::i18n_id!("settings-setting-datetime-show-seconds"),
        "clock time seconds top bar",
    ),
    (
        PageId::DateTime,
        "show-date-in-the-top-bar",
        bingux_ui::i18n_id!("settings-setting-datetime-show-date-in-the-top-bar"),
        "date clock top bar",
    ),
    (
        PageId::DateTime,
        "show-weekday-in-the-top-bar",
        bingux_ui::i18n_id!("settings-setting-datetime-show-weekday-in-the-top-bar"),
        "day week clock top bar",
    ),
    (
        PageId::DateTime,
        "set-date-and-time",
        bingux_ui::i18n_id!("settings-setting-datetime-set-date-and-time"),
        "clock manual change time",
    ),
    (
        PageId::Users,
        "full-name",
        bingux_ui::i18n_id!("settings-setting-users-full-name"),
        "display name real name user",
    ),
    (
        PageId::Users,
        "user-name",
        bingux_ui::i18n_id!("settings-setting-users-user-name"),
        "login account username",
    ),
    (
        PageId::Users,
        "account-type",
        bingux_ui::i18n_id!("settings-setting-users-account-type"),
        "administrator standard admin permissions",
    ),
    (
        PageId::Users,
        "change-password",
        bingux_ui::i18n_id!("settings-setting-users-change-password"),
        "password login secret",
    ),
    (
        PageId::Users,
        "automatic-login",
        bingux_ui::i18n_id!("settings-setting-users-automatic-login"),
        "sign in autologin password skip",
    ),
    (
        PageId::Users,
        "account-locked",
        bingux_ui::i18n_id!("settings-setting-users-account-locked"),
        "disabled lock access",
    ),
    (
        PageId::Users,
        "language",
        bingux_ui::i18n_id!("settings-setting-users-language"),
        "locale region language format",
    ),
    (
        PageId::About,
        "device-name",
        bingux_ui::i18n_id!("settings-setting-about-device-name"),
        "hostname computer name machine",
    ),
    (
        PageId::About,
        "hardware-model",
        bingux_ui::i18n_id!("settings-setting-about-hardware-model"),
        "machine computer make model",
    ),
    (
        PageId::About,
        "processor",
        bingux_ui::i18n_id!("settings-setting-about-processor"),
        "cpu chip cores",
    ),
    (
        PageId::About,
        "memory",
        bingux_ui::i18n_id!("settings-setting-about-memory"),
        "ram size gigabytes",
    ),
    (
        PageId::About,
        "graphics",
        bingux_ui::i18n_id!("settings-setting-about-graphics"),
        "gpu video card driver",
    ),
    (
        PageId::About,
        "capacity",
        bingux_ui::i18n_id!("settings-setting-about-capacity"),
        "disk storage drive space",
    ),
    (
        PageId::About,
        "kernel",
        bingux_ui::i18n_id!("settings-setting-about-kernel"),
        "linux version system",
    ),
    (
        PageId::About,
        "operating-system",
        bingux_ui::i18n_id!("settings-setting-about-operating-system"),
        "bingux distribution name version",
    ),
    (
        PageId::About,
        "desktop",
        bingux_ui::i18n_id!("settings-setting-about-desktop"),
        "gnoblin shell version",
    ),
    (
        PageId::About,
        "display",
        bingux_ui::i18n_id!("settings-setting-about-display"),
        "display server wayland x11 graphics session",
    ),
    (
        PageId::About,
        "version",
        bingux_ui::i18n_id!("settings-setting-about-version"),
        "bingux release system version",
    ),
];

/// The subpage a setting lives on, when it is not on the page itself. Keyed
/// on the setting's id (its English slug).
fn subpage(page: PageId, id: &str) -> Option<&'static str> {
    Some(match (page, id) {
        (PageId::Network, "wi-fi") => "wifi",
        (PageId::Network, "saved-networks") => "wifi/saved",
        (PageId::Network, "wi-fi-hotspot") => "hotspot",
        (PageId::Network, "add-vpn") => "add-vpn",
        (PageId::Network, "proxy") => "proxy",
        (PageId::Network, "data-usage") => "usage",
        (PageId::Network, "mobile-network") => "mobile",
        (PageId::Keyboard, "shortcuts") => "shortcuts",
        (PageId::DateTime, "time-zone") => "timezone",
        (PageId::Appearance, "font") => "font",
        (PageId::Sound, "application-volumes") => "applications",
        (PageId::Applications, "default-applications" | "open-by-default") => "default",
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
fn uri(page: PageId, id: &str) -> String {
    let mut at: Location = page.location();
    if let Some(path) = subpage(page, id) {
        for segment in path.split('/') {
            at = at.join(segment);
        }
    }
    at.with_fragment(slug(id)).to_string()
}

fn page_document(page: PageId) -> Document {
    Document::new(page.panel_name())
        .title(&page.title())
        .keywords(page.keywords())
        .breadcrumb([page.title()])
        .category(page.group())
        .field("page", page.panel_name())
        .field("kind", "page")
        .icon("bingux-settings")
        .uri(&page.location().to_string())
}

fn setting_document(page: PageId, id: &str, title_key: &str, keywords: &str) -> Document {
    Document::new(format!("{}/{}", page.panel_name(), slug(id)))
        .title(&bingux_ui::i18n::text(title_key))
        .keywords(keywords)
        .breadcrumb([page.title()])
        .category(page.group())
        .field("page", page.panel_name())
        .field("kind", "setting")
        .icon("bingux-settings")
        .uri(&uri(page, id))
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
            .displayed([
                "title",
                "breadcrumb",
                "icon",
                "uri",
                "page",
                "category",
                "kind",
            ])
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
                .chain(SETTINGS.iter().map(|(page, id, title_key, keywords)| {
                    setting_document(*page, id, title_key, keywords)
                })),
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
            .displayed([
                "title",
                "breadcrumb",
                "icon",
                "uri",
                "page",
                "category",
                "kind",
            ])
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
    for result in [
        search::register(SettingsIndex),
        search::register(ApplicationsIndex),
    ] {
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
        crate::register_catalogue_for_tests();
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
            .map(|h| {
                (
                    h.title().unwrap_or_default().to_string(),
                    h.uri().unwrap_or_default().to_string(),
                )
            })
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
            let location =
                Location::parse(uri).unwrap_or_else(|| panic!("{uri} is not an address"));
            assert_eq!(location.scheme(), SCHEME, "{uri}");
            assert!(
                table.resolve(&location).is_some(),
                "`{}` has the address {uri}, which no route opens",
                doc.id
            );
        }
    }

    /// The fragments the rows had before they had ids: each English title's
    /// slug is the row's id, so every deep link is unchanged.
    #[test]
    fn ids_match_the_fragments_the_rows_had_before_localisation() {
        const BEFORE: &[(&str, &str, &str)] = &[
            ("Network", "Wi-Fi", "wi-fi"),
            ("Network", "Airplane mode", "airplane-mode"),
            (
                "Network",
                "Connect to hidden network",
                "connect-to-hidden-network",
            ),
            ("Network", "Saved networks", "saved-networks"),
            ("Network", "Wi-Fi hotspot", "wi-fi-hotspot"),
            ("Network", "Wired", "wired"),
            ("Network", "Add VPN", "add-vpn"),
            ("Network", "Proxy", "proxy"),
            ("Network", "Data usage", "data-usage"),
            ("Network", "Mobile network", "mobile-network"),
            ("Network", "Share Wi-Fi network", "share-wi-fi-network"),
            ("Network", "Technical details", "technical-details"),
            ("Network", "Connect automatically", "connect-automatically"),
            ("Network", "Wi-Fi password", "wi-fi-password"),
            ("Network", "Priority", "priority"),
            ("Network", "Wired profiles", "wired-profiles"),
            ("Network", "VPN", "vpn"),
            ("Network", "MTU", "mtu"),
            ("Network", "Wake on LAN", "wake-on-lan"),
            ("Network", "Cloned MAC address", "cloned-mac-address"),
            ("Network", "IPv4 method", "ipv4-method"),
            ("Network", "IPv6 method", "ipv6-method"),
            ("Network", "Privacy extensions", "privacy-extensions"),
            ("Network", "Automatic DNS", "automatic-dns"),
            ("Network", "DNS search domains", "dns-search-domains"),
            (
                "Network",
                "Use this connection only for resources on its network",
                "use-this-connection-only-for-resources-on-its-network",
            ),
            ("Network", "Random MAC address", "random-mac-address"),
            ("Bluetooth", "Bluetooth", "bluetooth"),
            (
                "Bluetooth",
                "Connects automatically",
                "connects-automatically",
            ),
            ("Bluetooth", "Forget device", "forget-device"),
            ("Bluetooth", "Battery", "battery"),
            ("Bluetooth", "Name", "name"),
            ("Displays", "Resolution", "resolution"),
            ("Displays", "Refresh rate", "refresh-rate"),
            ("Displays", "Scale", "scale"),
            ("Displays", "Orientation", "orientation"),
            ("Displays", "Arrangement", "arrangement"),
            ("Displays", "Primary display", "primary-display"),
            ("Displays", "Enabled", "enabled"),
            ("Displays", "Night light", "night-light"),
            ("Displays", "On a schedule", "on-a-schedule"),
            ("Displays", "Colour temperature", "colour-temperature"),
            ("Sound", "Volume", "volume"),
            ("Sound", "Output device", "output-device"),
            ("Sound", "Input device", "input-device"),
            ("Sound", "Application volumes", "application-volumes"),
            ("Sound", "Effects", "effects"),
            ("Power", "Power mode", "power-mode"),
            ("Power", "Battery", "battery"),
            ("Power", "Automatic screen lock", "automatic-screen-lock"),
            ("Power", "Lock after", "lock-after"),
            ("Power", "Blank screen after", "blank-screen-after"),
            (
                "Power",
                "Dim screen when inactive",
                "dim-screen-when-inactive",
            ),
            ("Power", "Suspend when inactive", "suspend-when-inactive"),
            ("Power", "Suspend after", "suspend-after"),
            (
                "Power",
                "When the power button is pressed",
                "when-the-power-button-is-pressed",
            ),
            ("Keyboard", "Input sources", "input-sources"),
            ("Keyboard", "Keyboard layout", "keyboard-layout"),
            ("Keyboard", "Input method", "input-method"),
            ("Keyboard", "Repeat keys", "repeat-keys"),
            ("Keyboard", "Repeat speed", "repeat-speed"),
            ("Keyboard", "Delay before repeat", "delay-before-repeat"),
            ("Keyboard", "Cursor blinking", "cursor-blinking"),
            ("Keyboard", "Blink speed", "blink-speed"),
            ("Keyboard", "Caps Lock key", "caps-lock-key"),
            ("Keyboard", "Compose key", "compose-key"),
            (
                "Keyboard",
                "Num Lock on at sign-in",
                "num-lock-on-at-sign-in",
            ),
            (
                "Keyboard",
                "Remember Num Lock state",
                "remember-num-lock-state",
            ),
            ("Keyboard", "Shortcuts", "shortcuts"),
            ("Keyboard", "Try it", "try-it"),
            ("Mouse", "Pointer acceleration", "pointer-acceleration"),
            ("Mouse", "Left-handed buttons", "left-handed-buttons"),
            ("Mouse", "Middle click paste", "middle-click-paste"),
            ("Mouse", "Natural scrolling", "natural-scrolling"),
            ("Mouse", "Tap to click", "tap-to-click"),
            ("Mouse", "Two-finger scrolling", "two-finger-scrolling"),
            ("Mouse", "Scrolling along edges", "scrolling-along-edges"),
            (
                "Mouse",
                "Off when a mouse is connected",
                "off-when-a-mouse-is-connected",
            ),
            (
                "Mouse",
                "Touchpad pointer acceleration",
                "touchpad-pointer-acceleration",
            ),
            ("Mouse", "Primary button", "primary-button"),
            ("Appearance", "Wallpaper", "wallpaper"),
            ("Appearance", "Accent colour", "accent-colour"),
            ("Appearance", "Colour scheme", "colour-scheme"),
            ("Appearance", "Font", "font"),
            ("Appearance", "Text size", "text-size"),
            ("Accessibility", "Large text", "large-text"),
            ("Accessibility", "High contrast", "high-contrast"),
            ("Accessibility", "Reduce animations", "reduce-animations"),
            ("Accessibility", "Cursor size", "cursor-size"),
            ("Accessibility", "Overlay scrollbars", "overlay-scrollbars"),
            ("Accessibility", "Screen reader", "screen-reader"),
            ("Accessibility", "On-screen keyboard", "on-screen-keyboard"),
            ("Accessibility", "Sticky keys", "sticky-keys"),
            ("Accessibility", "Slow keys", "slow-keys"),
            ("Accessibility", "Bounce keys", "bounce-keys"),
            ("Accessibility", "Mouse keys", "mouse-keys"),
            ("Accessibility", "Locate pointer", "locate-pointer"),
            ("Appearance", "Icons", "icons"),
            ("Appearance", "Cursor", "cursor"),
            ("Desktop", "Show dock", "show-dock"),
            ("Desktop", "Icon size", "icon-size"),
            ("Desktop", "Side of the screen", "side-of-the-screen"),
            ("Desktop", "Alignment", "alignment"),
            ("Desktop", "Dock mouse actions", "dock-mouse-actions"),
            ("Desktop", "Click", "click"),
            ("Desktop", "Middle click", "middle-click"),
            ("Desktop", "Scroll", "scroll"),
            ("Desktop", "Scroll direction", "scroll-direction"),
            ("Desktop", "Show sidebar", "show-sidebar"),
            (
                "Desktop",
                "Show seconds in the clock",
                "show-seconds-in-the-clock",
            ),
            ("Desktop", "Show system usage", "show-system-usage"),
            (
                "Desktop",
                "Show my public IP address",
                "show-my-public-ip-address",
            ),
            ("Desktop", "Control Centre", "control-centre"),
            ("Desktop", "Top bar", "top-bar"),
            ("Search", "Search sources", "search-sources"),
            ("Search", "File search", "file-search"),
            ("Search", "New folder", "new-folder"),
            ("Search", "Search subfolders", "search-subfolders"),
            ("Search", "Previews", "previews"),
            ("Search", "Preview files", "preview-files"),
            (
                "Search",
                "Largest file to preview",
                "largest-file-to-preview",
            ),
            (
                "Search",
                "Prepare nearby previews",
                "prepare-nearby-previews",
            ),
            ("Search", "Web search", "web-search"),
            ("Search", "Default engine", "default-engine"),
            ("Search", "New search engine", "new-search-engine"),
            ("Search", "Assistant", "assistant"),
            ("Search", "Assistant program", "assistant-program"),
            ("Search", "Model", "model"),
            (
                "Applications",
                "Default applications",
                "default-applications",
            ),
            ("Applications", "Open by default", "open-by-default"),
            ("Applications", "Installed", "installed"),
            ("Applications", "Start at login", "start-at-login"),
            ("Applications", "File types", "file-types"),
            ("Applications", "Notifications", "notifications"),
            ("DateTime", "Set automatically", "set-automatically"),
            ("DateTime", "Time zone", "time-zone"),
            ("DateTime", "Clock format", "clock-format"),
            ("DateTime", "Show seconds", "show-seconds"),
            (
                "DateTime",
                "Show date in the top bar",
                "show-date-in-the-top-bar",
            ),
            (
                "DateTime",
                "Show weekday in the top bar",
                "show-weekday-in-the-top-bar",
            ),
            ("DateTime", "Set date and time", "set-date-and-time"),
            ("Users", "Full name", "full-name"),
            ("Users", "User name", "user-name"),
            ("Users", "Account type", "account-type"),
            ("Users", "Change password", "change-password"),
            ("Users", "Automatic login", "automatic-login"),
            ("Users", "Account locked", "account-locked"),
            ("Users", "Language", "language"),
            ("About", "Device name", "device-name"),
            ("About", "Hardware model", "hardware-model"),
            ("About", "Processor", "processor"),
            ("About", "Memory", "memory"),
            ("About", "Graphics", "graphics"),
            ("About", "Capacity", "capacity"),
            ("About", "Kernel", "kernel"),
            ("About", "Operating system", "operating-system"),
            ("About", "Desktop", "desktop"),
            ("About", "Display", "display"),
            ("About", "Version", "version"),
        ];
        assert_eq!(BEFORE.len(), SETTINGS.len());
        for ((page, title, fragment), (row_page, id, _, _)) in BEFORE.iter().zip(SETTINGS) {
            assert_eq!(format!("{row_page:?}"), *page);
            assert_eq!(slug(title), *fragment, "{title}");
            assert_eq!(id, fragment, "{title}");
        }
    }

    /// An id names a row within its page. Two pages may share one: Battery is
    /// on Bluetooth and on Power, and each keeps its own fragment.
    #[test]
    fn ids_are_unique_within_a_page() {
        for (i, (page, id, _, _)) in SETTINGS.iter().enumerate() {
            assert!(
                SETTINGS[..i]
                    .iter()
                    .all(|(p, other, _, _)| p != page || other != id),
                "{page:?} has two settings with the id {id}"
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
        assert_eq!(
            slug("Use this connection only for resources on its network"),
            "use-this-connection-only-for-resources-on-its-network"
        );
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
        assert_eq!(
            top(&index, "accent colour")[0].1,
            "bingux-settings://background#accent-colour"
        );
        // The American spelling finds the British one through a synonym.
        assert!(top(&index, "color").iter().any(|h| h.0 == "Accent colour"));
        // A synonym.
        assert!(
            top(&index, "monitor")
                .iter()
                .any(|h| h.1.starts_with("bingux-settings://display"))
        );
        // A page name finds the page before its settings.
        assert_eq!(top(&index, "keyboard")[0].0, "Keyboard");
    }

    #[test]
    fn the_index_is_external_and_private_data_is_not_in_it() {
        assert!(
            SettingsIndex.schema().external,
            "Spotlight may search the settings"
        );
        assert!(
            !ApplicationsIndex.schema().external,
            "the application list is the person's"
        );
        for doc in SettingsIndex.documents() {
            assert_ne!(doc.external, Some(true));
        }
    }
}
