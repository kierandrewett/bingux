//! The browser's window, built from bingux-ui and nothing else.

mod address;
mod app;
mod ctx;
mod find;
mod gate;
mod menus;
mod page;
mod pages;
mod phone;
mod popovers;
mod private;
mod prompts;
mod settings;
mod strip;
mod toolbar;
mod web;

pub use app::{App, Handoff};
pub use ctx::{Ctx, Engine};
pub use gate::{Gate, GateWindow};
#[cfg(feature = "snapshot")]
pub use phone::open_tabs;
