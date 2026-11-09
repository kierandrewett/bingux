//! What a Bingux application asks of the desktop it runs on, in one typed
//! place: how windows should look, opening a link or a file, a notification,
//! keeping the screen awake.
//!
//! bingux-ui makes no assumption about the session. It runs under Gnoblin's
//! compositor, under sway, or under GNOME's Mutter, with or without
//! xdg-desktop-portal, `gsettings`, GVfs or a notification daemon. Each call
//! asks the standard freedesktop interface first, falls back to files and
//! programs the XDG specifications describe, and when nothing answers it fails
//! with a reason the caller can show, in no more than a few seconds and usually
//! at once. An application never waits on a service that is not there.
//!
//! | Module                   | Asks                                                               | Falls back to                                  |
//! | ------------------------ | ------------------------------------------------------------------ | ---------------------------------------------- |
//! | [`appearance`]           | the Settings portal                                                | `bingux.toml`, GTK settings files, gsettings   |
//! | [`open`]                 | the OpenURI portal                                                 | the default desktop entry, then `xdg-open`     |
//! | [`notify`]               | `org.freedesktop.Notifications`                                    | an error: no daemon, no notification           |
//! | [`inhibit`]              | the Inhibit portal                                                 | `org.freedesktop.ScreenSaver`, then logind     |
//! | [`apps`]                 | desktop entries and `mimeapps.list` in the XDG directories         | nothing: it reads files                        |
//! | [`services()`]           | the session and system buses                                       | all `false` without a bus                      |
//!
//! Every call that can wait is blocking and asserts that it is not on the UI
//! thread, like [`crate::runtime`]; use it inside
//! [`crate::runtime::background_then`], or the `_then` form it comes with.
//! `BINGUX_PORTAL=0` keeps a process off the session bus entirely (for a
//! capture or a test on a desktop that has one) and `BINGUX_PORTAL=1` forces it
//! on.
//!
//! Icon lookup is [`crate::app_icon`]: the XDG icon theme specification, read
//! from the files, with the theme named by [`appearance::icon_theme`].

pub mod appearance;
pub mod apps;
mod bus;
pub mod inhibit;
pub mod notify;
pub mod open;
mod services;

pub use bus::BusError;
pub use services::{Services, services};

#[cfg(test)]
pub(crate) mod testing;
