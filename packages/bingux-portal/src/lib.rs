//! `xdg-desktop-portal-bingux`: the portal backend that enforces Bingux portal permissions.
//!
//! The frontend calls this backend directly, so the rules live here and not in
//! the compositor.

pub mod binguxctl;
pub mod identity;
pub mod policy;
pub mod portal;
pub mod screenshot;
pub mod store;
