//! The Bingux icon set, embedded in the binary at compile time.
//!
//! The icons are the rounded Phosphor fork of the chat app (`@chat/icons`),
//! snapped to a 16 px pixel grid so they render as crisply as GNOME's symbolic
//! icons. The SVG files live in `icons/` and are generated from
//! `scripts/icons/src` by `make icons`; see `icons/COPYING` for the licence.
//! Each icon is one fill path, recoloured at draw time so it follows the
//! active theme.

use freya::prelude::*;

/// Every bundled icon. Names follow the Adwaita symbolic icon each one replaces,
/// so the QML shell and the Rust apps share one vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    GoPrevious,
    GoNext,
    SidebarShow,
    Computer,
    PreferencesSystem,
    EmblemSystem,
    Display,
    WindowClose,
    WindowMinimize,
    WindowMaximize,
    WindowRestore,
    Appearance,
    Desktop,
    Search,
    Filter,
    Network,
    Bluetooth,
    Sound,
    Power,
    Keyboard,
    Mouse,
    Accessibility,
    Clock,
    Users,
    About,
    Refresh,
    Delete,
    Check,
    ListAdd,
    ListRemove,
    PanDown,
    PanUp,
    PanEnd,
    EditClear,
    DialogInformation,
    DialogWarning,
    DialogError,
    EditCopy,
    Folder,
    DocumentOpen,
    OpenMenu,
    ViewMore,
    UserTrash,
    Application,
    Applications,
    Headphones,
    Speakers,
    Gamepad,
    Phone,
    Pause,
    Inspect,
    NetworkWirelessSignalNone,
    NetworkWirelessSignalWeak,
    NetworkWirelessSignalOk,
    NetworkWirelessSignalGood,
    NetworkWirelessSignalExcellent,
    NetworkWirelessEncrypted,
    NetworkWired,
    NetworkVpn,
    AirplaneMode,
    NetworkWirelessHotspot,
    NetworkWirelessDisabled,
    NetworkOffline,
    NetworkError,
    AudioInputMicrophone,
    AudioVolumeMuted,
    AudioVolumeLow,
    AudioVolumeMedium,
    AudioCard,
    CameraWeb,
    DisplayBrightness,
    NightLight,
    WeatherClearNight,
    PowerProfilePowerSaver,
    PowerProfileBalanced,
    PowerProfilePerformance,
    SystemShutdown,
    BatteryLevel50,
    BatteryLevel100,
    ViewReveal,
    ViewConceal,
    DocumentEdit,
    UserInfo,
    Starred,
    NonStarred,
    ObjectRotateRight,
    DialogPassword,
    SystemLockScreen,
    SendTo,
    FolderDownload,
    PreferencesDesktopKeyboardShortcuts,
    InputTouchpad,
    PreferencesOther,
    ViewList,
    MediaRemovable,
    Printer,
    InputTablet,
    NetworkServer,
    AvatarDefault,
    SecurityHigh,
    SecurityLow,
    FindLocation,
    MailUnread,
    ApplicationsMultimedia,
    AccessoriesCalculator,
    WebBrowser,
    XOfficeCalendar,
    HelpBrowser,
    ViewGrid,
    TopBar,
    NotificationsDisabled,
    ChangesPrevent,
    Bookmarks,
    AddOn,
    VideoGeneric,
    ImageGeneric,
    UserHome,
    FolderDocuments,
    FolderMusic,
    FolderPictures,
    FolderVideos,
    DriveHarddisk,
    TextXGeneric,
    AudioXGeneric,
    PackageXGeneric,
    FolderNew,
    MediaEject,
    MediaOptical,
    FolderTemplates,
    FolderPublicshare,
    FolderDesktop,
    WindowNew,
}

impl Icon {
    /// Every icon, in declaration order.
    pub const ALL: [Icon; 132] = [
        Self::GoPrevious,
        Self::GoNext,
        Self::SidebarShow,
        Self::Computer,
        Self::PreferencesSystem,
        Self::EmblemSystem,
        Self::Display,
        Self::WindowClose,
        Self::WindowMinimize,
        Self::WindowMaximize,
        Self::WindowRestore,
        Self::Appearance,
        Self::Desktop,
        Self::Search,
        Self::Filter,
        Self::Network,
        Self::Bluetooth,
        Self::Sound,
        Self::Power,
        Self::Keyboard,
        Self::Mouse,
        Self::Accessibility,
        Self::Clock,
        Self::Users,
        Self::About,
        Self::Refresh,
        Self::Delete,
        Self::Check,
        Self::ListAdd,
        Self::ListRemove,
        Self::PanDown,
        Self::PanUp,
        Self::PanEnd,
        Self::EditClear,
        Self::DialogInformation,
        Self::DialogWarning,
        Self::DialogError,
        Self::EditCopy,
        Self::Folder,
        Self::DocumentOpen,
        Self::OpenMenu,
        Self::ViewMore,
        Self::UserTrash,
        Self::Application,
        Self::Applications,
        Self::Headphones,
        Self::Speakers,
        Self::Gamepad,
        Self::Phone,
        Self::Pause,
        Self::Inspect,
        Self::NetworkWirelessSignalNone,
        Self::NetworkWirelessSignalWeak,
        Self::NetworkWirelessSignalOk,
        Self::NetworkWirelessSignalGood,
        Self::NetworkWirelessSignalExcellent,
        Self::NetworkWirelessEncrypted,
        Self::NetworkWired,
        Self::NetworkVpn,
        Self::AirplaneMode,
        Self::NetworkWirelessHotspot,
        Self::NetworkWirelessDisabled,
        Self::NetworkOffline,
        Self::NetworkError,
        Self::AudioInputMicrophone,
        Self::AudioVolumeMuted,
        Self::AudioVolumeLow,
        Self::AudioVolumeMedium,
        Self::AudioCard,
        Self::CameraWeb,
        Self::DisplayBrightness,
        Self::NightLight,
        Self::WeatherClearNight,
        Self::PowerProfilePowerSaver,
        Self::PowerProfileBalanced,
        Self::PowerProfilePerformance,
        Self::SystemShutdown,
        Self::BatteryLevel50,
        Self::BatteryLevel100,
        Self::ViewReveal,
        Self::ViewConceal,
        Self::DocumentEdit,
        Self::UserInfo,
        Self::Starred,
        Self::NonStarred,
        Self::ObjectRotateRight,
        Self::DialogPassword,
        Self::SystemLockScreen,
        Self::SendTo,
        Self::FolderDownload,
        Self::PreferencesDesktopKeyboardShortcuts,
        Self::InputTouchpad,
        Self::PreferencesOther,
        Self::ViewList,
        Self::MediaRemovable,
        Self::Printer,
        Self::InputTablet,
        Self::NetworkServer,
        Self::AvatarDefault,
        Self::SecurityHigh,
        Self::SecurityLow,
        Self::FindLocation,
        Self::MailUnread,
        Self::ApplicationsMultimedia,
        Self::AccessoriesCalculator,
        Self::WebBrowser,
        Self::XOfficeCalendar,
        Self::HelpBrowser,
        Self::ViewGrid,
        Self::TopBar,
        Self::NotificationsDisabled,
        Self::ChangesPrevent,
        Self::Bookmarks,
        Self::AddOn,
        Self::VideoGeneric,
        Self::ImageGeneric,
        Self::UserHome,
        Self::FolderDocuments,
        Self::FolderMusic,
        Self::FolderPictures,
        Self::FolderVideos,
        Self::DriveHarddisk,
        Self::TextXGeneric,
        Self::AudioXGeneric,
        Self::PackageXGeneric,
        Self::FolderNew,
        Self::MediaEject,
        Self::MediaOptical,
        Self::FolderTemplates,
        Self::FolderPublicshare,
        Self::FolderDesktop,
        Self::WindowNew,
    ];

    /// The embedded SVG source for this icon.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::GoPrevious => include_bytes!("../icons/go-previous-symbolic.svg"),
            Self::GoNext => include_bytes!("../icons/go-next-symbolic.svg"),
            Self::SidebarShow => include_bytes!("../icons/sidebar-show-symbolic.svg"),
            Self::Computer => include_bytes!("../icons/computer-symbolic.svg"),
            Self::PreferencesSystem => include_bytes!("../icons/preferences-system-symbolic.svg"),
            Self::EmblemSystem => include_bytes!("../icons/emblem-system-symbolic.svg"),
            Self::Display => include_bytes!("../icons/video-display-symbolic.svg"),
            Self::WindowClose => include_bytes!("../icons/window-close-symbolic.svg"),
            Self::WindowMinimize => include_bytes!("../icons/window-minimize-symbolic.svg"),
            Self::WindowMaximize => include_bytes!("../icons/window-maximize-symbolic.svg"),
            Self::WindowRestore => include_bytes!("../icons/window-restore-symbolic.svg"),
            Self::Appearance => include_bytes!("../icons/applications-graphics-symbolic.svg"),
            Self::Desktop => {
                include_bytes!("../icons/preferences-desktop-wallpaper-symbolic.svg")
            }
            Self::Search => include_bytes!("../icons/system-search-symbolic.svg"),
            Self::Filter => include_bytes!("../icons/view-filter-symbolic.svg"),
            Self::Network => include_bytes!("../icons/network-wireless-symbolic.svg"),
            Self::Bluetooth => include_bytes!("../icons/bluetooth-active-symbolic.svg"),
            Self::Sound => include_bytes!("../icons/audio-volume-high-symbolic.svg"),
            Self::Power => include_bytes!("../icons/battery-level-100-symbolic.svg"),
            Self::Keyboard => include_bytes!("../icons/input-keyboard-symbolic.svg"),
            Self::Mouse => include_bytes!("../icons/input-mouse-symbolic.svg"),
            Self::Accessibility => {
                include_bytes!("../icons/preferences-desktop-accessibility-symbolic.svg")
            }
            Self::Clock => include_bytes!("../icons/preferences-system-time-symbolic.svg"),
            Self::Users => include_bytes!("../icons/system-users-symbolic.svg"),
            Self::About => include_bytes!("../icons/help-about-symbolic.svg"),
            Self::Refresh => include_bytes!("../icons/view-refresh-symbolic.svg"),
            Self::Delete => include_bytes!("../icons/edit-delete-symbolic.svg"),
            Self::Check => include_bytes!("../icons/object-select-symbolic.svg"),
            Self::ListAdd => include_bytes!("../icons/list-add-symbolic.svg"),
            Self::ListRemove => include_bytes!("../icons/list-remove-symbolic.svg"),
            Self::PanDown => include_bytes!("../icons/pan-down-symbolic.svg"),
            Self::PanUp => include_bytes!("../icons/pan-up-symbolic.svg"),
            Self::PanEnd => include_bytes!("../icons/pan-end-symbolic.svg"),
            Self::EditClear => include_bytes!("../icons/edit-clear-symbolic.svg"),
            Self::DialogInformation => include_bytes!("../icons/dialog-information-symbolic.svg"),
            Self::DialogWarning => include_bytes!("../icons/dialog-warning-symbolic.svg"),
            Self::DialogError => include_bytes!("../icons/dialog-error-symbolic.svg"),
            Self::EditCopy => include_bytes!("../icons/edit-copy-symbolic.svg"),
            Self::Folder => include_bytes!("../icons/folder-symbolic.svg"),
            Self::DocumentOpen => include_bytes!("../icons/document-open-symbolic.svg"),
            Self::OpenMenu => include_bytes!("../icons/open-menu-symbolic.svg"),
            Self::ViewMore => include_bytes!("../icons/view-more-symbolic.svg"),
            Self::UserTrash => include_bytes!("../icons/user-trash-symbolic.svg"),
            Self::Application => include_bytes!("../icons/application-x-executable-symbolic.svg"),
            Self::Applications => include_bytes!("../icons/view-app-grid-symbolic.svg"),
            Self::Headphones => include_bytes!("../icons/audio-headphones-symbolic.svg"),
            Self::Speakers => include_bytes!("../icons/audio-speakers-symbolic.svg"),
            Self::Gamepad => include_bytes!("../icons/input-gaming-symbolic.svg"),
            Self::Phone => include_bytes!("../icons/phone-symbolic.svg"),
            Self::Pause => include_bytes!("../icons/media-playback-pause-symbolic.svg"),
            // The element picker, as GTK's own inspector uses.
            Self::Inspect => include_bytes!("../icons/find-location-symbolic.svg"),
            Self::NetworkWirelessSignalNone => {
                include_bytes!("../icons/network-wireless-signal-none-symbolic.svg")
            }
            Self::NetworkWirelessSignalWeak => {
                include_bytes!("../icons/network-wireless-signal-weak-symbolic.svg")
            }
            Self::NetworkWirelessSignalOk => {
                include_bytes!("../icons/network-wireless-signal-ok-symbolic.svg")
            }
            Self::NetworkWirelessSignalGood => {
                include_bytes!("../icons/network-wireless-signal-good-symbolic.svg")
            }
            Self::NetworkWirelessSignalExcellent => {
                include_bytes!("../icons/network-wireless-signal-excellent-symbolic.svg")
            }
            Self::NetworkWirelessEncrypted => {
                include_bytes!("../icons/network-wireless-encrypted-symbolic.svg")
            }
            Self::NetworkWired => include_bytes!("../icons/network-wired-symbolic.svg"),
            Self::NetworkVpn => include_bytes!("../icons/network-vpn-symbolic.svg"),
            Self::AirplaneMode => include_bytes!("../icons/airplane-mode-symbolic.svg"),
            Self::NetworkWirelessHotspot => {
                include_bytes!("../icons/network-wireless-hotspot-symbolic.svg")
            }
            Self::NetworkWirelessDisabled => {
                include_bytes!("../icons/network-wireless-disabled-symbolic.svg")
            }
            Self::NetworkOffline => include_bytes!("../icons/network-offline-symbolic.svg"),
            Self::NetworkError => include_bytes!("../icons/network-error-symbolic.svg"),
            Self::AudioInputMicrophone => {
                include_bytes!("../icons/audio-input-microphone-symbolic.svg")
            }
            Self::AudioVolumeMuted => include_bytes!("../icons/audio-volume-muted-symbolic.svg"),
            Self::AudioVolumeLow => include_bytes!("../icons/audio-volume-low-symbolic.svg"),
            Self::AudioVolumeMedium => include_bytes!("../icons/audio-volume-medium-symbolic.svg"),
            Self::AudioCard => include_bytes!("../icons/audio-card-symbolic.svg"),
            Self::CameraWeb => include_bytes!("../icons/camera-web-symbolic.svg"),
            Self::DisplayBrightness => include_bytes!("../icons/display-brightness-symbolic.svg"),
            Self::NightLight => include_bytes!("../icons/night-light-symbolic.svg"),
            Self::WeatherClearNight => include_bytes!("../icons/weather-clear-night-symbolic.svg"),
            Self::PowerProfilePowerSaver => {
                include_bytes!("../icons/power-profile-power-saver-symbolic.svg")
            }
            Self::PowerProfileBalanced => {
                include_bytes!("../icons/power-profile-balanced-symbolic.svg")
            }
            Self::PowerProfilePerformance => {
                include_bytes!("../icons/power-profile-performance-symbolic.svg")
            }
            Self::SystemShutdown => include_bytes!("../icons/system-shutdown-symbolic.svg"),
            Self::BatteryLevel50 => include_bytes!("../icons/battery-level-50-symbolic.svg"),
            Self::BatteryLevel100 => include_bytes!("../icons/battery-level-100-symbolic.svg"),
            Self::ViewReveal => include_bytes!("../icons/view-reveal-symbolic.svg"),
            Self::ViewConceal => include_bytes!("../icons/view-conceal-symbolic.svg"),
            Self::DocumentEdit => include_bytes!("../icons/document-edit-symbolic.svg"),
            Self::UserInfo => include_bytes!("../icons/user-info-symbolic.svg"),
            Self::Starred => include_bytes!("../icons/starred-symbolic.svg"),
            Self::NonStarred => include_bytes!("../icons/non-starred-symbolic.svg"),
            Self::ObjectRotateRight => include_bytes!("../icons/object-rotate-right-symbolic.svg"),
            Self::DialogPassword => include_bytes!("../icons/dialog-password-symbolic.svg"),
            Self::SystemLockScreen => include_bytes!("../icons/system-lock-screen-symbolic.svg"),
            Self::SendTo => include_bytes!("../icons/send-to-symbolic.svg"),
            Self::FolderDownload => include_bytes!("../icons/folder-download-symbolic.svg"),
            Self::PreferencesDesktopKeyboardShortcuts => {
                include_bytes!("../icons/preferences-desktop-keyboard-shortcuts-symbolic.svg")
            }
            Self::InputTouchpad => include_bytes!("../icons/input-touchpad-symbolic.svg"),
            Self::PreferencesOther => include_bytes!("../icons/preferences-other-symbolic.svg"),
            Self::ViewList => include_bytes!("../icons/view-list-symbolic.svg"),
            Self::MediaRemovable => include_bytes!("../icons/media-removable-symbolic.svg"),
            Self::Printer => include_bytes!("../icons/printer-symbolic.svg"),
            Self::InputTablet => include_bytes!("../icons/input-tablet-symbolic.svg"),
            Self::NetworkServer => include_bytes!("../icons/network-server-symbolic.svg"),
            Self::AvatarDefault => include_bytes!("../icons/avatar-default-symbolic.svg"),
            Self::SecurityHigh => include_bytes!("../icons/security-high-symbolic.svg"),
            Self::SecurityLow => include_bytes!("../icons/security-low-symbolic.svg"),
            Self::FindLocation => include_bytes!("../icons/find-location-symbolic.svg"),
            Self::MailUnread => include_bytes!("../icons/mail-unread-symbolic.svg"),
            Self::ApplicationsMultimedia => {
                include_bytes!("../icons/applications-multimedia-symbolic.svg")
            }
            Self::AccessoriesCalculator => {
                include_bytes!("../icons/accessories-calculator-symbolic.svg")
            }
            Self::WebBrowser => include_bytes!("../icons/web-browser-symbolic.svg"),
            Self::XOfficeCalendar => include_bytes!("../icons/x-office-calendar-symbolic.svg"),
            Self::HelpBrowser => include_bytes!("../icons/help-browser-symbolic.svg"),
            Self::ViewGrid => include_bytes!("../icons/view-grid-symbolic.svg"),
            Self::TopBar => include_bytes!("../icons/focus-top-bar-symbolic.svg"),
            Self::NotificationsDisabled => {
                include_bytes!("../icons/notifications-disabled-symbolic.svg")
            }
            Self::ChangesPrevent => include_bytes!("../icons/changes-prevent-symbolic.svg"),
            Self::Bookmarks => include_bytes!("../icons/bookmarks-symbolic.svg"),
            Self::AddOn => include_bytes!("../icons/addon-symbolic.svg"),
            Self::VideoGeneric => include_bytes!("../icons/video-generic-symbolic.svg"),
            Self::ImageGeneric => include_bytes!("../icons/image-generic-symbolic.svg"),
            Self::UserHome => include_bytes!("../icons/user-home-symbolic.svg"),
            Self::FolderDocuments => include_bytes!("../icons/folder-documents-symbolic.svg"),
            Self::FolderMusic => include_bytes!("../icons/folder-music-symbolic.svg"),
            Self::FolderPictures => include_bytes!("../icons/folder-pictures-symbolic.svg"),
            Self::FolderVideos => include_bytes!("../icons/folder-videos-symbolic.svg"),
            Self::DriveHarddisk => include_bytes!("../icons/drive-harddisk-symbolic.svg"),
            Self::TextXGeneric => include_bytes!("../icons/text-x-generic-symbolic.svg"),
            Self::AudioXGeneric => include_bytes!("../icons/audio-x-generic-symbolic.svg"),
            Self::PackageXGeneric => include_bytes!("../icons/package-x-generic-symbolic.svg"),
            Self::FolderNew => include_bytes!("../icons/folder-new-symbolic.svg"),
            Self::MediaEject => include_bytes!("../icons/media-eject-symbolic.svg"),
            Self::MediaOptical => include_bytes!("../icons/media-optical-symbolic.svg"),
            Self::FolderTemplates => include_bytes!("../icons/folder-templates-symbolic.svg"),
            Self::FolderPublicshare => include_bytes!("../icons/folder-publicshare-symbolic.svg"),
            Self::FolderDesktop => include_bytes!("../icons/folder-desktop-symbolic.svg"),
            Self::WindowNew => include_bytes!("../icons/window-new-symbolic.svg"),
        }
    }
}

/// Draws `icon` as a square of `size` filled with `color`. Pass a token
/// colour: `Paint::Muted.color(&tokens)`.
pub fn icon(icon: Icon, size: crate::style::IconSize, color: Color) -> SvgViewer {
    SvgViewer::new(icon.bytes())
        .width(Size::px(size.px()))
        .height(Size::px(size.px()))
        .fill(color)
}

/// [`icon`] with its size scaled by `scale` and rounded to whole pixels, so a
/// zooming view keeps its glyph edges sharp. The size is never below 1 px.
pub fn icon_scaled(icon: Icon, size: crate::style::IconSize, scale: f32, color: Color) -> SvgViewer {
    let edge = (size.px() * scale).round().max(1.0);
    SvgViewer::new(icon.bytes())
        .width(Size::px(edge))
        .height(Size::px(edge))
        .fill(color)
}
