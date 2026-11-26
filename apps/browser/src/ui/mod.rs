//! The browser's window, built from bingux-ui and nothing else.

mod app;
mod ctx;
mod gate;
mod menus;
mod page;
mod pages;
mod phone;
mod popovers;
mod private;
mod strip;
mod toolbar;
mod web;

pub use app::{App, Handoff};
pub use ctx::Ctx;
pub use gate::{Gate, GateWindow};
#[cfg(feature = "snapshot")]
pub use phone::open_tabs;
