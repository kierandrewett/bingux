//! Bingux Media: music and films from the person's own folders.
//!
//! The engine (`bingux-media`: mpv, the library index, MPRIS, Last.fm) starts
//! before the window. The window is a navigation split view over the library
//! (songs, albums, artists, films and the queue) with the now-playing bar
//! along the bottom, and the full player over everything when a film plays
//! or the bar is pressed. Every part of it is a bingux-ui component; this
//! crate holds the pages and the glue.

mod app;
mod engine;
mod pages;
mod settings_page;
#[cfg(feature = "snapshot")]
mod snapshot_mode;
mod song_list;
mod stage;
mod strings;

use std::ffi::OsStr;
use std::path::Path;

use bingux_ui::icons::Icon;
use bingux_ui::prelude::{Application, RouteTable};

/// The scheme of Media's locations.
pub const SCHEME: &str = "bingux-media";

const USAGE: &str = "Usage: bingux-media [--] [FILE | FOLDER | URI]...\n\
\n\
Plays each FILE, or the songs and films in each FOLDER, in Media, starting it\n\
if it is not running. With no argument the window opens on the library, or the\n\
running window is raised. An option other than --help is an error; `--` ends\n\
the options, for a name that starts with `-`.\n";

/// The address that plays `path`.
pub fn open_address(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut encoded = String::new();
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(*byte as char)
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    format!("{SCHEME}://open?path={encoded}")
}

/// A launch argument as a location: a file or folder plays; an address of
/// this app is itself; anything else is refused.
fn normalise(argument: &OsStr) -> String {
    let text = argument.to_string_lossy();
    if text.starts_with(&format!("{SCHEME}:")) {
        return text.into_owned();
    }
    let path = match text.strip_prefix("file://") {
        Some(rest) => {
            use std::os::unix::ffi::OsStringExt;
            std::path::PathBuf::from(std::ffi::OsString::from_vec(
                bingux_files::location::percent_decode(rest),
            ))
        }
        None => std::path::PathBuf::from(argument),
    };
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|dir| dir.join(&path))
            .unwrap_or(path)
    };
    if !path.exists() {
        eprintln!("bingux-media: {}: not found", path.display());
        return String::new();
    }
    open_address(&path)
}

/// Whether a Media window is running (its launch socket answers).
fn already_running() -> bool {
    let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return false;
    };
    std::os::unix::net::UnixStream::connect(Path::new(&dir).join("bingux-media.sock")).is_ok()
}

/// The navigation tree.
pub fn routes() -> RouteTable {
    RouteTable::new(SCHEME)
        .index("Media", |_| pages::index())
        .root("songs")
        .title("Songs")
        .icon(Icon::AudioXGeneric)
        .page(|_| pages::songs())
        .root("albums")
        .title("Albums")
        .icon(Icon::MediaOptical)
        .page(|_| pages::albums())
        .root("artists")
        .title("Artists")
        .icon(Icon::AvatarDefault)
        .page(|_| pages::artists())
        .root("videos")
        .title("Videos")
        .icon(Icon::VideoGeneric)
        .page(|_| pages::videos())
        .root("queue")
        .title("Up next")
        .icon(Icon::ViewListOrdered)
        .page(|_| pages::queue())
        .route("albums/:key")
        .title_with(|params| pages::album_title(&params.str("key")))
        .page(|params| pages::album(params.str("key").to_owned()))
        .route("artists/:name")
        .title_param("name")
        .page(|params| pages::artist(params.str("name").to_owned()))
        .route("open")
        .title("Opening")
        .query("path", "")
        .ephemeral()
        .page(|params| pages::open(params.query("path").unwrap_or_default().to_owned()))
}

fn main() {
    bingux_ui::i18n::register(bingux_ui::catalogue!("media"));
    #[cfg(feature = "snapshot")]
    if std::env::args().nth(1).as_deref() == Some("--snapshot") {
        let dir = std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/bingux-media-snapshots".to_string());
        engine::start();
        snapshot_mode::run(&dir, std::env::args().nth(3).as_deref());
        return;
    }
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--help" || arg == "-h")
    {
        print!("{USAGE}");
        return;
    }
    // A second launch only hands its addresses to the running window: it
    // needs no player of its own.
    if !already_running() {
        engine::start();
    }
    Application::new("bingux-media")
        .routes(routes())
        .title("Media")
        .size(1100.0, 760.0)
        .min_size(360.0, 360.0)
        .borderless()
        .normalise(normalise)
        .about(bingux_ui::app_info!(
            description = "Play music and films",
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
        bingux_ui::i18n::check::crate_catalogue(env!("CARGO_MANIFEST_DIR"), "media").assert_clean();
    }
}
