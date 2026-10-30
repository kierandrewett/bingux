//! Bingux Files: the file manager of the Bingux desktop shell.
//!
//! The window is the file chooser's body (`bingux_ui::files::browse`) with no
//! question to answer, in tabs. A second launch hands its folders to the
//! running window, which opens each in a new tab, and exits.

mod app;
mod compositor;
#[cfg(test)]
mod fuzz;
mod tabset;
mod target;
mod tearoff;
#[cfg(feature = "snapshot")]
mod snapshot_mode;
mod window_size;
mod windows;

use bingux_ui::prelude::Application;

const USAGE: &str = "Usage: bingux-files [--] [FOLDER | FILE | URI]...\n\
\n\
Opens each FOLDER in a new tab of the Files window, starting it if it is not\n\
running. A FILE opens the folder that holds it, with the file selected. FOLDER\n\
may be a path, ~, a file:// URI, bingux-files://PATH, recent:/// for the recent\n\
files or trash:/// for the Trash. With no argument a new window opens on the\n\
home folder, or the running window is raised. A name that is not there is\n\
reported on stderr and opens nothing. An option other than --help is an error;\n\
`--` ends the options, for a folder whose name starts with `-`.\n";

fn main() {
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        let dir = std::env::args().nth(2).unwrap_or_else(|| "/tmp/bingux-files-snapshots".to_string());
        snapshot_mode::run(&dir, std::env::args().nth(3).as_deref());
        return;
    }
    let (width, height) = window_size::remembered();
    Application::new("bingux-files")
        .routes(bingux_ui::files::folder_table())
        .title("Files")
        .size(width, height)
        .borderless()
        // The window being carried by a tab torn off it is drawn faded over
        // the window it may join; see `tearoff`.
        .transparent()
        .normalise(target::normalise)
        .about(bingux_ui::app_info!(
            description = "Browse and organise files and folders",
            licence_text = include_str!("../../../COPYING"),
        ))
        .usage(USAGE)
        .run(|| app::App);
}
