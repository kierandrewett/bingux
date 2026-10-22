//! Bundled Adwaita symbolic icons, embedded in the binary at compile time.
//!
//! The SVG files live in `icons/` (see `icons/COPYING` for their licence).
//! Each icon is recoloured at draw time so it follows the active theme.

use freya::prelude::*;

/// Every bundled icon. Names follow the Adwaita symbolic icon they embed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    GoPrevious,
    GoNext,
    SidebarShow,
    Computer,
    PreferencesSystem,
    Display,
    WindowClose,
    WindowMinimize,
    WindowMaximize,
    WindowRestore,
    Appearance,
    Desktop,
    Search,
    Network,
    Bluetooth,
    Sound,
    Power,
    Keyboard,
    Mouse,
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
}

impl Icon {
    /// Every icon, in declaration order.
    pub const ALL: [Icon; 46] = [
        Self::GoPrevious,
        Self::GoNext,
        Self::SidebarShow,
        Self::Computer,
        Self::PreferencesSystem,
        Self::Display,
        Self::WindowClose,
        Self::WindowMinimize,
        Self::WindowMaximize,
        Self::WindowRestore,
        Self::Appearance,
        Self::Desktop,
        Self::Search,
        Self::Network,
        Self::Bluetooth,
        Self::Sound,
        Self::Power,
        Self::Keyboard,
        Self::Mouse,
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
    ];

    /// The embedded SVG source for this icon.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::GoPrevious => include_bytes!("../icons/go-previous-symbolic.svg"),
            Self::GoNext => include_bytes!("../icons/go-next-symbolic.svg"),
            Self::SidebarShow => include_bytes!("../icons/sidebar-show-symbolic.svg"),
            Self::Computer => include_bytes!("../icons/computer-symbolic.svg"),
            Self::PreferencesSystem => include_bytes!("../icons/preferences-system-symbolic.svg"),
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
            Self::Network => include_bytes!("../icons/network-wireless-symbolic.svg"),
            Self::Bluetooth => include_bytes!("../icons/bluetooth-active-symbolic.svg"),
            Self::Sound => include_bytes!("../icons/audio-volume-high-symbolic.svg"),
            Self::Power => include_bytes!("../icons/battery-level-100-symbolic.svg"),
            Self::Keyboard => include_bytes!("../icons/input-keyboard-symbolic.svg"),
            Self::Mouse => include_bytes!("../icons/input-mouse-symbolic.svg"),
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
        }
    }
}

/// Draws `icon` as a square of `size` logical pixels filled with `color`.
pub fn icon(icon: Icon, size: f32, color: Color) -> SvgViewer {
    SvgViewer::new(icon.bytes())
        .width(Size::px(size))
        .height(Size::px(size))
        .fill(color)
}
