//! Text Editor: the plain text and code editor of the Bingux desktop.
//!
//! The editing surface, find bar and Markdown view are `bingux_ui::text_editor`
//! and the text engine is `bingux-text`; this crate is the window around
//! them: tabs of documents, opening and saving files in any encoding, drafts
//! that keep unsaved text across restarts, and the menus.
//!
//! A second launch hands its files to the running window, which opens each
//! in a tab, and exits.

mod app;
mod content;
mod document;
mod launch;
mod prefs;
mod session;
#[cfg(feature = "snapshot")]
mod snapshot_mode;

use bingux_ui::prelude::{Application, Message, i18n_id, tr};

const USAGE: &str = "Usage: bingux-text-editor [--] [FILE | URI]...\n\
\n\
Opens each FILE in a tab of the Text Editor window, starting it if it is not\n\
running. FILE may be a path, ~/path or a file:// URI; a file that does not\n\
exist yet opens as a new document that is saved there. `new:` opens a new\n\
empty document. With no argument the window opens with the documents of the\n\
last session, or the running window is raised. An option other than --help\n\
is an error; `--` ends the options, for a file whose name starts with `-`.\n";

fn main() {
    bingux_ui::i18n::register(bingux_ui::catalogue!("text-editor"));
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        let dir = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/bingux-text-editor-snapshots".to_string());
        let files: Vec<String> = std::env::args().skip(3).collect();
        snapshot_mode::run(&dir, &files);
        return;
    }
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--bench-typing") {
        let file = std::env::args().nth(2).unwrap_or_default();
        snapshot_mode::bench_typing(&file);
        return;
    }
    // The grammars load on a worker while the window opens.
    std::thread::spawn(bingux_text::highlight::warm);
    app::prepare(prefs::load(), session::load_session());
    let (width, height) = app::window_size();
    Application::new("bingux-text-editor")
        .routes(launch::table())
        .title(Message::new(i18n_id!("text-editor-title")))
        .size(width, height)
        .min_size(360.0, 320.0)
        .borderless()
        // A window carried by a torn-off tab is drawn faded over the window it
        // may join.
        .transparent()
        .normalise(launch::normalise)
        .about(bingux_ui::app_info!(
            description = tr!("text-editor-description"),
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
        bingux_ui::i18n::check::crate_catalogue(env!("CARGO_MANIFEST_DIR"), "text-editor")
            .assert_clean();
    }
}
