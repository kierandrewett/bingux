//! Bundled Adwaita symbolic icons, embedded in the binary at compile time.
//!
//! The SVG files live in `icons/` (see `icons/COPYING` for their licence).
//! Each icon is recoloured at draw time so it follows the active theme.

use freya::prelude::*;

use crate::theme::rgba;

/// Every icon the Settings window draws.
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
}

impl Icon {
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
            Self::Appearance => {
                include_bytes!("../icons/preferences-desktop-appearance-symbolic.svg")
            }
            Self::Search => include_bytes!("../icons/system-search-symbolic.svg"),
            Self::Network => include_bytes!("../icons/network-wireless-symbolic.svg"),
            Self::Bluetooth => include_bytes!("../icons/bluetooth-active-symbolic.svg"),
            Self::Sound => include_bytes!("../icons/audio-speakers-symbolic.svg"),
            Self::Power => include_bytes!("../icons/battery-symbolic.svg"),
            Self::Keyboard => include_bytes!("../icons/input-keyboard-symbolic.svg"),
            Self::Mouse => include_bytes!("../icons/input-mouse-symbolic.svg"),
            Self::Clock => include_bytes!("../icons/preferences-system-time-symbolic.svg"),
            Self::Users => include_bytes!("../icons/system-users-symbolic.svg"),
            Self::About => include_bytes!("../icons/help-about-symbolic.svg"),
            Self::Refresh => include_bytes!("../icons/view-refresh-symbolic.svg"),
            Self::Delete => include_bytes!("../icons/edit-delete-symbolic.svg"),
            Self::Check => include_bytes!("../icons/object-select-symbolic.svg"),
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

/// A rounded square in a category colour with a white symbolic icon, as in
/// macOS System Settings' sidebar.
pub fn icon_tile(glyph: Icon, rgb: [f32; 3], size: f32) -> Rect {
    let base = rgba(rgb[0], rgb[1], rgb[2], 1.0);
    rect()
        .width(Size::px(size))
        .height(Size::px(size))
        .corner_radius(size * 0.28)
        .background(base)
        .center()
        .child(
            rect()
                .expanded()
                .corner_radius(size * 0.28)
                .background(Color::WHITE.with_af32(0.10))
                .center()
                .child(icon(glyph, size * 0.56, Color::WHITE)),
        )
}
