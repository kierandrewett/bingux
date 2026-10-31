//! The Bingux icon set, embedded in the binary at compile time.
//!
//! The icons are the rounded Phosphor fork of the chat app (`@chat/icons`),
//! snapped to a 16 px pixel grid so they render as crisply as GNOME's symbolic
//! icons. The SVG files live in `icons/` and are generated from
//! `scripts/icons/src` by `make icons`; see `icons/COPYING` for the licence.
//! Each icon is one fill path, recoloured at draw time so it follows the
//! active theme. A level icon (Wi-Fi signal, battery) has a second path with
//! `fill-opacity` for the parts that are off; the tint keeps that opacity.

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
    CameraWeb,
    DisplayBrightness,
    NightLight,
    WeatherClearNight,
    PowerProfilePowerSaver,
    PowerProfileBalanced,
    PowerProfilePerformance,
    SystemShutdown,
    BatteryLevel0,
    BatteryLevel10,
    BatteryLevel20,
    BatteryLevel30,
    BatteryLevel40,
    BatteryLevel50,
    BatteryLevel60,
    BatteryLevel70,
    BatteryLevel80,
    BatteryLevel90,
    BatteryLevel100,
    ViewReveal,
    ViewConceal,
    DocumentEdit,
    /// A pencil: edit something (a path, a connection, an access point). `DocumentEdit` is only for editing a document.
    Pencil,
    UserInfo,
    Starred,
    NonStarred,
    ObjectRotateRight,
    /// Play.
    MediaPlaybackStart,
    /// Stop: the Files toolbar's Reload button while a folder loads.
    MediaPlaybackStop,
    /// Full screen.
    ViewFullscreen,
    /// Leave full screen.
    ViewRestore,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Fit to the window.
    ZoomFitBest,
    /// Rotate counter-clockwise.
    ObjectRotateLeft,
    /// Subtitles.
    MediaViewSubtitles,
    DialogPassword,
    SendTo,
    FolderDownload,
    InputTouchpad,
    PreferencesOther,
    ViewList,
    MediaRemovable,
    Printer,
    InputTablet,
    NetworkServer,
    AvatarDefault,
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
    DefaultAppText,
    DefaultAppDocuments,
    DragHandle,
    /// A PDF file.
    ApplicationPdf,
    /// A spreadsheet.
    Table,
    /// A presentation.
    Presentation,
    /// A font file.
    FontXGeneric,
    /// Source code or a script.
    TextXScript,
    /// A file of no known kind.
    ApplicationXGeneric,
    /// The badge on the icon of a symbolic link.
    EmblemSymbolicLink,
    /// Recording: the dot of a recording indicator.
    MediaRecord,
    /// Previous track.
    MediaSkipBackward,
    /// Next track.
    MediaSkipForward,
    /// Back 10 seconds.
    MediaSeekBackward,
    /// Forward 10 seconds.
    MediaSeekForward,
    /// Microphone muted.
    AudioInputMicrophoneMuted,
    /// Connected, kind of network unknown.
    NetworkTransmitReceive,
    /// Devices on one local network.
    NetworkWorkgroup,
    /// Screen sharing in progress.
    ScreenShared,
    /// Log out of the session.
    SystemLogOut,
    /// Restart the machine.
    SystemReboot,
    /// Switch to another user.
    SystemSwitchUser,
    /// A clear day: the sun.
    WeatherClear,
    /// A still camera.
    CameraPhoto,
    /// A numbered list.
    ViewListOrdered,
    /// A ticked box.
    CheckboxChecked,
    /// A paragraph of plain text.
    FormatTextPlaintext,
    /// Bold text.
    FormatTextBold,
    /// Italic text.
    FormatTextItalic,
    /// Struck-through text.
    FormatTextStrikethrough,
    /// Code, written as angle brackets.
    FormatCode,
    /// A quotation, written as quotation marks.
    FormatQuote,
    /// A link, written as a chain.
    InsertLink,
    /// Any icon of the chat fork at any weight, as the fork draws it (feature `chat-icons`).
    #[cfg(feature = "chat-icons")]
    Chat(crate::chat_icons::ChatIcon, crate::chat_icons::ChatWeight),
}

impl Icon {
    /// Every icon, in declaration order.
    pub const ALL: [Icon; 179] = [
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
        Self::CameraWeb,
        Self::DisplayBrightness,
        Self::NightLight,
        Self::WeatherClearNight,
        Self::PowerProfilePowerSaver,
        Self::PowerProfileBalanced,
        Self::PowerProfilePerformance,
        Self::SystemShutdown,
        Self::BatteryLevel0,
        Self::BatteryLevel10,
        Self::BatteryLevel20,
        Self::BatteryLevel30,
        Self::BatteryLevel40,
        Self::BatteryLevel50,
        Self::BatteryLevel60,
        Self::BatteryLevel70,
        Self::BatteryLevel80,
        Self::BatteryLevel90,
        Self::BatteryLevel100,
        Self::ViewReveal,
        Self::ViewConceal,
        Self::DocumentEdit,
        Self::Pencil,
        Self::UserInfo,
        Self::Starred,
        Self::NonStarred,
        Self::ObjectRotateRight,
        Self::MediaPlaybackStart,
        Self::MediaPlaybackStop,
        Self::ViewFullscreen,
        Self::ViewRestore,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ZoomFitBest,
        Self::ObjectRotateLeft,
        Self::MediaViewSubtitles,
        Self::DialogPassword,
        Self::SendTo,
        Self::FolderDownload,
        Self::InputTouchpad,
        Self::PreferencesOther,
        Self::ViewList,
        Self::MediaRemovable,
        Self::Printer,
        Self::InputTablet,
        Self::NetworkServer,
        Self::AvatarDefault,
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
        Self::DefaultAppText,
        Self::DefaultAppDocuments,
        Self::DragHandle,
        Self::ApplicationPdf,
        Self::Table,
        Self::Presentation,
        Self::FontXGeneric,
        Self::TextXScript,
        Self::ApplicationXGeneric,
        Self::EmblemSymbolicLink,
        Self::MediaRecord,
        Self::MediaSkipBackward,
        Self::MediaSkipForward,
        Self::MediaSeekBackward,
        Self::MediaSeekForward,
        Self::AudioInputMicrophoneMuted,
        Self::NetworkTransmitReceive,
        Self::NetworkWorkgroup,
        Self::ScreenShared,
        Self::SystemLogOut,
        Self::SystemReboot,
        Self::SystemSwitchUser,
        Self::WeatherClear,
        Self::CameraPhoto,
        Self::ViewListOrdered,
        Self::CheckboxChecked,
        Self::FormatTextPlaintext,
        Self::FormatTextBold,
        Self::FormatTextItalic,
        Self::FormatTextStrikethrough,
        Self::FormatCode,
        Self::FormatQuote,
        Self::InsertLink,
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
            Self::Desktop => include_bytes!("../icons/preferences-desktop-symbolic.svg"),
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
            Self::BatteryLevel0 => include_bytes!("../icons/battery-level-0-symbolic.svg"),
            Self::BatteryLevel10 => include_bytes!("../icons/battery-level-10-symbolic.svg"),
            Self::BatteryLevel20 => include_bytes!("../icons/battery-level-20-symbolic.svg"),
            Self::BatteryLevel30 => include_bytes!("../icons/battery-level-30-symbolic.svg"),
            Self::BatteryLevel40 => include_bytes!("../icons/battery-level-40-symbolic.svg"),
            Self::BatteryLevel50 => include_bytes!("../icons/battery-level-50-symbolic.svg"),
            Self::BatteryLevel60 => include_bytes!("../icons/battery-level-60-symbolic.svg"),
            Self::BatteryLevel70 => include_bytes!("../icons/battery-level-70-symbolic.svg"),
            Self::BatteryLevel80 => include_bytes!("../icons/battery-level-80-symbolic.svg"),
            Self::BatteryLevel90 => include_bytes!("../icons/battery-level-90-symbolic.svg"),
            Self::BatteryLevel100 => include_bytes!("../icons/battery-level-100-symbolic.svg"),
            Self::ViewReveal => include_bytes!("../icons/view-reveal-symbolic.svg"),
            Self::ViewConceal => include_bytes!("../icons/view-conceal-symbolic.svg"),
            Self::DocumentEdit => include_bytes!("../icons/document-edit-symbolic.svg"),
            Self::Pencil => include_bytes!("../icons/pencil-symbolic.svg"),
            Self::UserInfo => include_bytes!("../icons/user-info-symbolic.svg"),
            Self::Starred => include_bytes!("../icons/starred-symbolic.svg"),
            Self::NonStarred => include_bytes!("../icons/non-starred-symbolic.svg"),
            Self::ObjectRotateRight => include_bytes!("../icons/object-rotate-right-symbolic.svg"),
            Self::MediaPlaybackStart => include_bytes!("../icons/media-playback-start-symbolic.svg"),
            Self::MediaPlaybackStop => include_bytes!("../icons/media-playback-stop-symbolic.svg"),
            Self::ViewFullscreen => include_bytes!("../icons/view-fullscreen-symbolic.svg"),
            Self::ViewRestore => include_bytes!("../icons/view-restore-symbolic.svg"),
            Self::ZoomIn => include_bytes!("../icons/zoom-in-symbolic.svg"),
            Self::ZoomOut => include_bytes!("../icons/zoom-out-symbolic.svg"),
            Self::ZoomFitBest => include_bytes!("../icons/zoom-fit-best-symbolic.svg"),
            Self::ObjectRotateLeft => include_bytes!("../icons/object-rotate-left-symbolic.svg"),
            Self::MediaViewSubtitles => include_bytes!("../icons/media-view-subtitles-symbolic.svg"),
            Self::DialogPassword => include_bytes!("../icons/dialog-password-symbolic.svg"),
            Self::SendTo => include_bytes!("../icons/send-to-symbolic.svg"),
            Self::FolderDownload => include_bytes!("../icons/folder-download-symbolic.svg"),
            Self::InputTouchpad => include_bytes!("../icons/input-touchpad-symbolic.svg"),
            Self::PreferencesOther => include_bytes!("../icons/preferences-other-symbolic.svg"),
            Self::ViewList => include_bytes!("../icons/view-list-symbolic.svg"),
            Self::MediaRemovable => include_bytes!("../icons/media-removable-symbolic.svg"),
            Self::Printer => include_bytes!("../icons/printer-symbolic.svg"),
            Self::InputTablet => include_bytes!("../icons/input-tablet-symbolic.svg"),
            Self::NetworkServer => include_bytes!("../icons/network-server-symbolic.svg"),
            Self::AvatarDefault => include_bytes!("../icons/avatar-default-symbolic.svg"),
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
            Self::DefaultAppText => include_bytes!("../icons/default-app-text-symbolic.svg"),
            Self::DefaultAppDocuments => include_bytes!("../icons/default-app-documents-symbolic.svg"),
            Self::DragHandle => include_bytes!("../icons/list-drag-handle-symbolic.svg"),
            Self::ApplicationPdf => include_bytes!("../icons/application-pdf-symbolic.svg"),
            Self::Table => include_bytes!("../icons/table-symbolic.svg"),
            Self::Presentation => include_bytes!("../icons/presentation-symbolic.svg"),
            Self::FontXGeneric => include_bytes!("../icons/font-x-generic-symbolic.svg"),
            Self::TextXScript => include_bytes!("../icons/text-x-script-symbolic.svg"),
            Self::ApplicationXGeneric => include_bytes!("../icons/application-x-generic-symbolic.svg"),
            Self::EmblemSymbolicLink => include_bytes!("../icons/emblem-symbolic-link-symbolic.svg"),
            Self::MediaRecord => include_bytes!("../icons/media-record-symbolic.svg"),
            Self::MediaSkipBackward => include_bytes!("../icons/media-skip-backward-symbolic.svg"),
            Self::MediaSkipForward => include_bytes!("../icons/media-skip-forward-symbolic.svg"),
            Self::MediaSeekBackward => include_bytes!("../icons/media-seek-backward-symbolic.svg"),
            Self::MediaSeekForward => include_bytes!("../icons/media-seek-forward-symbolic.svg"),
            Self::AudioInputMicrophoneMuted => {
                include_bytes!("../icons/audio-input-microphone-muted-symbolic.svg")
            }
            Self::NetworkTransmitReceive => {
                include_bytes!("../icons/network-transmit-receive-symbolic.svg")
            }
            Self::NetworkWorkgroup => include_bytes!("../icons/network-workgroup-symbolic.svg"),
            Self::ScreenShared => include_bytes!("../icons/screen-shared-symbolic.svg"),
            Self::SystemLogOut => include_bytes!("../icons/system-log-out-symbolic.svg"),
            Self::SystemReboot => include_bytes!("../icons/system-reboot-symbolic.svg"),
            Self::SystemSwitchUser => include_bytes!("../icons/system-switch-user-symbolic.svg"),
            Self::WeatherClear => include_bytes!("../icons/weather-clear-symbolic.svg"),
            Self::CameraPhoto => include_bytes!("../icons/camera-photo-symbolic.svg"),
            Self::ViewListOrdered => include_bytes!("../icons/view-list-ordered-symbolic.svg"),
            Self::CheckboxChecked => include_bytes!("../icons/checkbox-checked-symbolic.svg"),
            Self::FormatTextPlaintext => {
                include_bytes!("../icons/format-text-plaintext-symbolic.svg")
            }
            Self::FormatTextBold => include_bytes!("../icons/format-text-bold-symbolic.svg"),
            Self::FormatTextItalic => include_bytes!("../icons/format-text-italic-symbolic.svg"),
            Self::FormatTextStrikethrough => {
                include_bytes!("../icons/format-text-strikethrough-symbolic.svg")
            }
            Self::FormatCode => include_bytes!("../icons/format-code-symbolic.svg"),
            Self::FormatQuote => include_bytes!("../icons/format-quote-symbolic.svg"),
            Self::InsertLink => include_bytes!("../icons/insert-link-symbolic.svg"),
            #[cfg(feature = "chat-icons")]
            // A weight the fork lacks is drawn in the regular weight, which every fork icon has.
            Self::Chat(icon, weight) => icon
                .bytes(weight)
                .or_else(|| icon.bytes(crate::chat_icons::ChatWeight::Regular))
                .expect("every fork icon has a regular weight"),
        }
    }
}

/// The image source of `icon`. Its id is the address of the embedded file, so
/// building one hashes a pointer and not the SVG: a page draws hundreds.
fn source(icon: Icon) -> ImageSource {
    let bytes = icon.bytes();
    ImageSource::from((bytes.as_ptr() as usize, bytes))
}

/// The viewer behind every icon.
///
/// Rendering an icon only looks its raster up in the asset cache (keyed by
/// the icon, the size in device pixels and the colour): the first draw of a
/// combination rasterises it on a worker, and until it lands the viewer is an
/// empty box of the same size, so nothing moves. `render` never rasterises on
/// the UI thread. The colour is set as both `fill` and `color` so the key does
/// not change when the viewer learns its inherited text colour after the first
/// draw, which would rasterise every icon twice.
pub(crate) fn glyph(icon: Icon, edge: f32, color: Color) -> SvgViewer {
    let viewer = off_thread(
        SvgViewer::new(source(icon))
            .width(Size::px(edge))
            .height(Size::px(edge))
            .fill(color),
        color,
    );
    if icon.mirrors_in_rtl() && crate::controls::direction::use_direction().is_rtl() {
        viewer.scale((-1.0, 1.0))
    } else {
        viewer
    }
}

/// Makes `viewer` rasterise on a worker, as an empty box of its size until it
/// lands, keyed on `color` from the first draw (as `fill` and `currentColor`)
/// so it does not rasterise a second time when it learns its inherited colour.
pub(crate) fn off_thread(viewer: SvgViewer, color: Color) -> SvgViewer {
    viewer
        .color(color)
        .async_rasterization(true)
        .show_loader(false)
}

/// Draws `icon` as a square of `size` filled with `color`. Pass a token
/// colour: `Paint::Muted.color(&tokens)`.
pub fn icon(icon: Icon, size: crate::style::IconSize, color: Color) -> SvgViewer {
    glyph(icon, size.px(), color)
}

/// [`icon`] with its size scaled by `scale` and rounded to whole pixels, so a
/// zooming view keeps its glyph edges sharp. The size is never below 1 px.
pub fn icon_scaled(icon: Icon, size: crate::style::IconSize, scale: f32, color: Color) -> SvgViewer {
    glyph(icon, (size.px() * scale).round().max(1.0), color)
}

/// Rasterises `icon` at `size` in `color` on a worker, into the cache that
/// [`icon`] reads, so the first frame that draws it finds it there. Call from
/// a hook at start-up for the icons the first frame shows (the sidebar and
/// the header); anything else rasterises when it first appears. A combination
/// that is already cached or on its way costs nothing.
pub fn warm(icon: Icon, size: crate::style::IconSize, color: Color) {
    warm_svg(source(icon), size.px(), color);
}

impl Icon {
    /// Whether the icon is drawn flipped left to right when the interface reads
    /// right to left: what it draws has a direction in the reading order (a
    /// chevron or arrow, the side a sidebar toggle's panel is on, a list's
    /// bullets, a paper plane being sent). The set is `#mirror_rtl` in
    /// `scripts/icons/labels.tsv`, with the reasons in `design/rtl.md`, and a
    /// test keeps the two equal. [`icon`] and [`icon_scaled`] apply it.
    pub const fn mirrors_in_rtl(self) -> bool {
        matches!(
            self,
            Self::GoPrevious
                | Self::GoNext
                | Self::PanEnd
                | Self::SidebarShow
                | Self::SendTo
                | Self::ViewList
                | Self::HelpBrowser
                | Self::EmblemSymbolicLink
                | Self::SystemLogOut
        )
    }

    /// The battery icon filled to `percent` (0 to 100, more is 100), rounded to
    /// the nearest ten: one of the `battery-level-N` files, which draw the
    /// charge as the filled part of the body and the rest dimmed.
    pub const fn battery_level(percent: u8) -> Self {
        match (percent.saturating_add(5) / 10) * 10 {
            0 => Self::BatteryLevel0,
            10 => Self::BatteryLevel10,
            20 => Self::BatteryLevel20,
            30 => Self::BatteryLevel30,
            40 => Self::BatteryLevel40,
            50 => Self::BatteryLevel50,
            60 => Self::BatteryLevel60,
            70 => Self::BatteryLevel70,
            80 => Self::BatteryLevel80,
            90 => Self::BatteryLevel90,
            _ => Self::BatteryLevel100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The files `#mirror_rtl` names in `scripts/icons/labels.tsv`.
    fn labelled() -> Vec<&'static str> {
        include_str!("../../../scripts/icons/labels.tsv")
            .lines()
            .find_map(|line| line.strip_prefix("#mirror_rtl:"))
            .expect("labels.tsv has a #mirror_rtl directive")
            .split_whitespace()
            .collect()
    }

    #[test]
    fn mirrored_icons_are_the_ones_labels_tsv_names() {
        let labelled = labelled();
        // Some files are one drawing under two names (pan-end is go-next), so an
        // icon is every file whose bytes it is.
        let files_of = |icon: Icon| -> Vec<String> {
            let bytes = icon.bytes();
            std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/icons"))
                .expect("icons/ is readable")
                .filter_map(Result::ok)
                .filter(|entry| std::fs::read(entry.path()).is_ok_and(|file| file == bytes))
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter_map(|name| name.strip_suffix("-symbolic.svg").map(str::to_string))
                .collect()
        };
        for icon in Icon::ALL {
            let files = files_of(icon);
            assert!(!files.is_empty(), "{icon:?} is a file in icons/");
            assert_eq!(
                icon.mirrors_in_rtl(),
                files.iter().any(|file| labelled.contains(&file.as_str())),
                "{icon:?} ({files:?}): Icon::mirrors_in_rtl and `#mirror_rtl` in labels.tsv disagree"
            );
        }
        for file in labelled {
            assert!(
                Icon::ALL.iter().any(|icon| files_of(*icon).iter().any(|name| name == file)),
                "`#mirror_rtl` names {file}, which no Icon draws"
            );
        }
    }

    #[test]
    fn battery_level_picks_the_nearest_ten_and_never_goes_past_full() {
        assert_eq!(Icon::battery_level(0), Icon::BatteryLevel0);
        assert_eq!(Icon::battery_level(4), Icon::BatteryLevel0);
        assert_eq!(Icon::battery_level(5), Icon::BatteryLevel10);
        assert_eq!(Icon::battery_level(49), Icon::BatteryLevel50);
        assert_eq!(Icon::battery_level(85), Icon::BatteryLevel90);
        assert_eq!(Icon::battery_level(100), Icon::BatteryLevel100);
        assert_eq!(Icon::battery_level(255), Icon::BatteryLevel100);
    }
}
