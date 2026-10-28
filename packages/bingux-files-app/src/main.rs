//! Bingux Files: the file manager of the Bingux desktop shell.
//!
//! The window is the file chooser's body (`bingux_ui::files::browse`) with no
//! question to answer, in tabs. A second launch hands its folder to the
//! running window, which opens it in a new tab, and exits.

mod app;
mod target;
mod window_size;

use bingux_ui::prelude::Application;

const USAGE: &str = "Usage: bingux-files [FOLDER | FILE | URI]\n\
\n\
Opens FOLDER in a new tab of the Files window, starting it if it is not\n\
running. A FILE opens the folder that holds it. FOLDER may be a path, ~, a\n\
file:// URI, bingux-files://PATH or recent:/// for the recent files. With no\n\
argument a new window opens on the home folder, or the running window is\n\
raised.\n";

fn main() {
    let (width, height) = window_size::remembered();
    Application::new("bingux-files")
        .routes(bingux_ui::files::folder_table())
        .title("Files")
        .size(width, height)
        .borderless()
        .normalise(target::normalise)
        .usage(USAGE)
        .run(|| app::App);
}
