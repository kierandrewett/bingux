//! The browser's window, built from bingux-ui and nothing else.

mod address;
mod app;
mod ctx;
mod find;
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

pub use app::App;
pub use ctx::{Ctx, Engine};
#[cfg(feature = "snapshot")]
pub use phone::open_tabs;
