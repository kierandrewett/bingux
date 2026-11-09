//! Bingux Preview: the picture and document viewer of the Bingux desktop shell.
//!
//! The window is `bingux_ui::preview` (the viewers Quick Look uses) in
//! `Mode::Window` with a title line and a list of the files beside the one
//! that was opened. A second launch hands its file to the running window.
//!
//! Everything a viewer shares with other applications lives in `bingux-ui`
//! (the zoomable view, `bingux_ui::zoom_view`; the viewers and the helper that
//! decodes what Freya cannot, `bingux_ui::preview`; the "Open with" list and
//! the chooser, `bingux_ui::files`). This crate is the Preview window: the
//! list the arrows walk, the title line, Copy picture and Save a copy, and the
//! remembered window size and zoom.

mod app;
mod copy;
mod crop_mode;
mod edit;
mod launch;
mod playlist;
mod prefs;
#[cfg(feature = "snapshot")]
mod snapshot_mode;

use bingux_ui::nav::{Mode, RouteTable};
use bingux_ui::prelude::Application;

const USAGE: &str = "Usage: bingux-preview [--] [FILE | FOLDER | URI]...\n\
\n\
Opens a picture or a document in the Preview window, starting it if it is not\n\
running. One FILE is shown with the files of its folder that are of its kind,\n\
so Left and Right go through them. Several files are the list the arrows go\n\
through; a FOLDER is its pictures and documents. FILE may be a path, ~ or a\n\
file:// URI. With no argument an empty window opens, or the running window is\n\
raised. A name that is not there is reported on stderr and opens nothing. An\n\
option other than --help is an error; `--` ends the options, for a file whose\n\
name starts with `-`.\n";

fn main() {
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        let dir = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/bingux-preview-snapshots".to_string());
        snapshot_mode::run(&dir, std::env::args().nth(3).as_deref());
        return;
    }
    let (width, height) = prefs::window_size();
    // One route: every file is a location, `bingux-preview:///path/to/file`; no path is the empty window.
    let routes = RouteTable::new(launch::SCHEME)
        .rooted()
        .mode(Mode::Linear)
        .root("*path");
    Application::new("bingux-preview")
        .routes(routes)
        .title("Preview")
        .size(width, height)
        .borderless()
        .normalise(launch::normalise)
        .about(bingux_ui::app_info!(
            description = "View pictures and documents",
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(USAGE)
        .run(|| app::App);
}
