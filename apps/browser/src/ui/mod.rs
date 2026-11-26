//! The browser's window, built from bingux-ui and nothing else.

mod app;
mod ctx;
mod menus;
mod page;
mod pages;
mod phone;
mod popovers;
mod strip;
mod toolbar;
mod web;

pub use app::{App, Handoff};
pub use ctx::Ctx;
#[cfg(feature = "snapshot")]
pub use phone::open_tabs;
