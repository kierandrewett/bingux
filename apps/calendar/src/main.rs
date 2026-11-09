//! Bingux Calendar: the calendar of the Bingux desktop shell.
//!
//! Calendars are folders of `.ics` files under `~/.local/share/bingux-calendar`,
//! so the app needs no account, no service and no setup. The model, the
//! files, repeating events and reminders are `bingux-calendar-core`;
//! the views and gestures are `bingux_ui::calendar`. This crate is the window:
//! its header, sidebar, event editor and what it does with a launch argument.
//!
//! `bingux-calendar --reminders` runs no window and shows reminders as they
//! come due, for a session that has the calendar closed.

mod app;
mod calendars;
mod convert;
mod editor;
mod header;
mod model;
mod sidebar;
#[cfg(feature = "snapshot")]
mod snapshot_mode;
mod views;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use bingux_ui::prelude::{Application, i18n_id};

const USAGE: &str = "Usage: bingux-calendar [--reminders] [FILE.ics | YYYY-MM-DD | today]...\n\
\n\
Opens the calendar, starting it if it is not running. A FILE.ics offers to\n\
import its events; a date, or `today`, shows that day. A name that is not\n\
there is reported on stderr and opens nothing.\n\
\n\
--reminders  Run with no window and show reminders as they come due.\n";

/// Turns a launch argument into the address the window reads: a file to
/// import, or a day to show.
fn normalise(argument: &OsStr) -> String {
    let text = argument.to_string_lossy();
    if text == "today" || text.len() == 10 && text.as_bytes().get(4) == Some(&b'-') {
        return format!("bingux-calendar://date/{text}");
    }
    let path = text.strip_prefix("file://").unwrap_or(&text);
    let absolute = if path.starts_with('/') {
        std::path::PathBuf::from(argument)
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut encoded = String::new();
    for byte in absolute.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(char::from(*byte));
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    format!("bingux-calendar://open?file={encoded}")
}

fn main() {
    let first = std::env::args().nth(1);
    if first.as_deref() == Some("--reminders") {
        app::run_reminders();
        return;
    }
    #[cfg(feature = "snapshot")]
    if first.as_deref() == Some("--snapshot") {
        let dir = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/bingux-calendar-snapshots".to_string());
        snapshot_mode::run(&dir, std::env::args().nth(3).as_deref());
        return;
    }
    bingux_ui::i18n::register(bingux_ui::catalogue!("calendar"));
    Application::new("bingux-calendar")
        .title(bingux_ui::prelude::Message::new(i18n_id!("calendar-title")))
        .size(1100.0, 760.0)
        .borderless()
        .normalise(normalise)
        .about(bingux_ui::app_info!(
            description = bingux_ui::prelude::tr!("calendar-description"),
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(USAGE)
        .run(|| app::App);
}

#[cfg(test)]
mod catalogue_tests {
    /// Every `tr!` of the app has its message, with the right variables, and
    /// the catalogue holds nothing the code does not use.
    #[test]
    fn the_catalogue_matches_the_code() {
        bingux_ui::i18n::check::crate_catalogue(env!("CARGO_MANIFEST_DIR"), "calendar")
            .assert_clean();
    }
}
