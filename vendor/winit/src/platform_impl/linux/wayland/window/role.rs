//! The role of a Wayland window surface: an `xdg_toplevel` or an `xdg_popup`.
//!
//! Bingux patch. Upstream winit only makes toplevels. A popup is a window whose
//! compositor places it against its parent (anchor, gravity, constraint
//! adjustment), draws it above the parent and may let it extend past the
//! parent's edges; it is how a menu leaves its window. [`Role`] gives
//! [`WindowState`](super::WindowState) one set of calls for both: the requests
//! that only a toplevel understands (title, maximise, drag, decoration mode)
//! do nothing on a popup.

use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::Proxy;
use sctk::shell::xdg::XdgSurface as _;
use sctk::reexports::client::protocol::wl_output::WlOutput;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::protocols::xdg::shell::client::xdg_surface::XdgSurface;
use sctk::reexports::protocols::xdg::shell::client::xdg_toplevel::{ResizeEdge, XdgToplevel};
use sctk::shell::xdg::popup::Popup as SctkPopup;
use sctk::shell::xdg::window::{DecorationMode, Window as SctkWindow};
use sctk::shell::WaylandSurface;

/// The toplevel or popup behind a window.
#[derive(Debug, Clone)]
pub enum Role {
    Toplevel(SctkWindow),
    Popup(SctkPopup),
}

impl Role {
    pub fn wl_surface(&self) -> &WlSurface {
        match self {
            Self::Toplevel(window) => window.wl_surface(),
            Self::Popup(popup) => popup.wl_surface(),
        }
    }

    pub fn xdg_surface(&self) -> &XdgSurface {
        match self {
            Self::Toplevel(window) => window.xdg_surface(),
            Self::Popup(popup) => popup.xdg_surface(),
        }
    }

    pub fn toplevel(&self) -> Option<&SctkWindow> {
        match self {
            Self::Toplevel(window) => Some(window),
            Self::Popup(_) => None,
        }
    }

    pub fn popup(&self) -> Option<&SctkPopup> {
        match self {
            Self::Toplevel(_) => None,
            Self::Popup(popup) => Some(popup),
        }
    }

    pub fn xdg_toplevel(&self) -> Option<&XdgToplevel> {
        self.toplevel().map(SctkWindow::xdg_toplevel)
    }

    pub fn commit(&self) {
        self.wl_surface().commit();
    }

    pub fn set_buffer_scale(&self, scale: u32) -> Result<(), sctk::shell::Unsupported> {
        let surface = self.wl_surface();
        if surface.version() < 3 {
            return Err(sctk::shell::Unsupported);
        }
        surface.set_buffer_scale(scale as i32);
        Ok(())
    }

    pub fn set_title(&self, title: &str) {
        if let Some(window) = self.toplevel() {
            window.set_title(title);
        }
    }

    pub fn set_app_id(&self, app_id: impl Into<String>) {
        if let Some(window) = self.toplevel() {
            window.set_app_id(app_id);
        }
    }

    pub fn set_parent(&self, parent: Option<&SctkWindow>) {
        if let Some(window) = self.toplevel() {
            window.set_parent(parent);
        }
    }

    pub fn set_minimized(&self) {
        if let Some(window) = self.toplevel() {
            window.set_minimized();
        }
    }

    pub fn set_maximized(&self) {
        if let Some(window) = self.toplevel() {
            window.set_maximized();
        }
    }

    pub fn unset_maximized(&self) {
        if let Some(window) = self.toplevel() {
            window.unset_maximized();
        }
    }

    pub fn set_fullscreen(&self, output: Option<&WlOutput>) {
        if let Some(window) = self.toplevel() {
            window.set_fullscreen(output);
        }
    }

    pub fn unset_fullscreen(&self) {
        if let Some(window) = self.toplevel() {
            window.unset_fullscreen();
        }
    }

    pub fn move_(&self, seat: &WlSeat, serial: u32) {
        if let Some(window) = self.toplevel() {
            window.move_(seat, serial);
        }
    }

    pub fn resize(&self, seat: &WlSeat, serial: u32, edges: ResizeEdge) {
        if let Some(window) = self.toplevel() {
            window.resize(seat, serial, edges);
        }
    }

    pub fn show_window_menu(&self, seat: &WlSeat, serial: u32, position: (i32, i32)) {
        if let Some(window) = self.toplevel() {
            window.show_window_menu(seat, serial, position);
        }
    }

    pub fn set_min_size(&self, size: Option<(u32, u32)>) {
        if let Some(window) = self.toplevel() {
            window.set_min_size(size);
        }
    }

    pub fn set_max_size(&self, size: Option<(u32, u32)>) {
        if let Some(window) = self.toplevel() {
            window.set_max_size(size);
        }
    }

    pub fn request_decoration_mode(&self, mode: Option<DecorationMode>) {
        if let Some(window) = self.toplevel() {
            window.request_decoration_mode(mode);
        }
    }
}
