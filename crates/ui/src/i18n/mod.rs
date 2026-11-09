//! Text in the person's language, from [Project Fluent](https://projectfluent.org)
//! catalogues: the one way a Bingux app localises its interface.
//!
//! Every string a person reads lives in a `.ftl` catalogue, one per app and
//! language, and code asks for it by ID:
//!
//! ```ignore
//! use bingux_ui::tr;
//!
//! let title = tr!("files-empty-title");                 // "Folder is empty"
//! let count = tr!("files-items", count = items.len());  // "1 item", "12 items"
//! let tip = tr!("files-new-folder.tooltip");            // an attribute
//! let ask = tr!("files-delete-confirm", name = &file);  // "Delete “notes.txt”?"
//! ```
//!
//! with the source catalogue `locale/en-US/files.ftl` next to the crate:
//!
//! ```ftl
//! files-empty-title = Folder is empty
//! files-items = { $count ->
//!     [one] { $count } item
//!    *[other] { $count } items
//! }
//! files-new-folder = New Folder
//!     .tooltip = Make a folder here
//! ```
//!
//! and one line at start-up, before the window opens:
//!
//! ```ignore
//! bingux_ui::i18n::register(bingux_ui::catalogue!("files"));
//! ```
//!
//! bingux-ui's own strings (dialogs, menus, the file chooser, the edit bar)
//! are the `bingux-ui` catalogue, registered already.
//!
//! # Which language
//!
//! The person's languages come from [`crate::locale::Locale::preferred`]
//! (`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`, in gettext's order) and are
//! negotiated against the languages that have a catalogue installed. Each
//! message is looked up along the resulting chain: the person's languages,
//! then `en-US`, the source language (Bingux is written in American English),
//! then `en`. A language that has translated half the messages shows its half
//! and English for the rest; a message ID never reaches the screen.
//!
//! The source catalogue is compiled into the program (`include_str!`), so a
//! missing install still shows every string. Other languages are read from,
//! first match wins:
//!
//! | Where                                            | When                          |
//! | ------------------------------------------------ | ----------------------------- |
//! | each directory of `BINGUX_LOCALE_DIR` (`:` list) | a translator testing a file   |
//! | `<crate>/locale/` (the catalogue's own)          | debug builds, from the source |
//! | `$XDG_DATA_HOME/bingux/locale/`                  | a per-user install            |
//! | each `$XDG_DATA_DIRS` entry + `/bingux/locale/`  | `/usr/share/bingux/locale/`   |
//!
//! each holding `<language>/<catalogue>.ftl`, such as `de/files.ftl`.
//!
//! # Changing language while running
//!
//! [`set_locale`] changes the language of every window: the window roots
//! follow [`use_language`] and are rebuilt (the location is kept). The
//! reading direction follows the language shown ([`direction`]), so a
//! catalogue in Arabic or Hebrew turns the window right to left and an Arabic
//! locale with no Arabic catalogue stays left to right, like its text.
//!
//! # Numbers, dates and bidirectional text
//!
//! A number placeable (`{ $count }`, `{ NUMBER($size, maximumFractionDigits: 1) }`)
//! is written by [`crate::format::Number`] in the catalogue's language
//! (`1,234.5` in English, `1.234,5` in German). A date passed as
//! [`crate::format::Moment`] is written by [`crate::format::Date`] and
//! `DATETIME($when, timeStyle: "short")` picks the parts. Plural variants
//! (`[one]`, `[few]`, `[other]`) follow the language's CLDR rules.
//!
//! A placeable is wrapped in Unicode isolation marks (FSI, U+2068, and PDI,
//! U+2069) where its direction could disturb the sentence around it: always
//! in a right-to-left catalogue, and in a left-to-right one when the value
//! holds right-to-left letters (a Hebrew file name in an English sentence).
//! The text engine reads them and draws nothing for them. See `design/rtl.md`.
//!
//! # Checks and pseudo-locales
//!
//! [`check`] fails a test when code asks for an ID the source catalogue does
//! not have, passes the wrong variables, or the catalogue holds a message
//! nothing uses; `scripts/ftl-check.py` checks every language's file.
//! `BINGUX_PSEUDO_LOCALE=accented` or `rtl` rewrites every catalogue message
//! ([`Pseudo`]), so a string that is not localised stands out as plain English.
//! The conventions for IDs, attributes, comments and plurals are in
//! `docs/bingux-ui.md`, "Localisation".

mod catalogue;
pub mod check;
mod localiser;
mod message;
mod pseudo;

use std::sync::atomic::{AtomicU64, Ordering};

pub use catalogue::Catalogue;
pub use message::{Arg, Message};
pub use pseudo::Pseudo;

use crate::controls::direction::Direction;
use crate::locale::Locale;

/// Bumped by everything that changes which text a message gives: a catalogue
/// registered, the locale or the pseudo-locale set.
static REVISION: AtomicU64 = AtomicU64::new(0);

fn bump() {
    REVISION.fetch_add(1, Ordering::Relaxed);
    localiser::notify();
}

/// The message `id` (or `id.attribute`) in the person's language, with no
/// variables. [`tr!`](crate::tr) is the same with a literal ID, which the
/// checks can see.
pub fn text(id: &str) -> String {
    localiser::format(id, None)
}

/// Whether some catalogue in the chain has `id` (or `id.attribute`).
pub fn has(id: &str) -> bool {
    localiser::has(id)
}

/// Adds an app's catalogue. Call it once, before the first window and before
/// any string is asked for: `register(catalogue!("files"))`. Registering a
/// domain again replaces it.
pub fn register(catalogue: Catalogue) {
    catalogue::register(catalogue);
    bump();
}

/// Shows every window in `locale` (`None`: the environment's languages
/// again), and rebuilds their content. For a language switch in the app or
/// the taxonomy; the person's language otherwise comes from the session.
pub fn set_locale(locale: Option<Locale>) {
    Locale::set_current(locale);
    bump();
}

/// The language the interface is shown in: the first of the chain that has
/// a catalogue (`de`, `en-US`). A pseudo-locale names itself (`en-XA`,
/// `ar-XB`).
pub fn language() -> String {
    localiser::current().chain()[0].clone()
}

/// The languages messages are looked up in, in order: the negotiated ones,
/// then `en-US`, then `en` when it has a catalogue.
pub fn languages() -> Vec<String> {
    localiser::current().chain().to_vec()
}

/// The direction of the language shown: what a window reads in unless
/// `BINGUX_DIRECTION` says otherwise.
pub fn direction() -> Direction {
    localiser::current().direction()
}

/// A number that changes whenever the text of a message might: compare it to
/// know when to redraw.
pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

/// The pseudo-locale in force, if any.
pub fn pseudo() -> Option<Pseudo> {
    pseudo::current()
}

/// Turns a pseudo-locale on or off for every window and rebuilds them.
pub fn set_pseudo(pseudo: Option<Pseudo>) {
    pseudo::set(pseudo);
    bump();
}

/// For a window's root: the language revision, which changes when
/// [`set_locale`], [`set_pseudo`] or [`register`] does. Key the window's
/// content on it so every string is asked for again. bingux-ui's application
/// windows do this already.
pub fn use_language() -> u64 {
    localiser::use_language()
}

/// The message `id` in the person's language, as a `String`.
///
/// ```ignore
/// tr!("files-empty-title")
/// tr!("files-items", count = items.len())
/// tr!("files-delete-confirm", name = file.name(), count = 1)
/// tr!("files-new-folder.tooltip")
/// ```
///
/// The ID is a literal, so the catalogue check (`i18n::check`) can see every
/// use and its variables. Variable names are Rust identifiers and match the
/// message's `$names` (`count = n` is `$count`). A value is anything
/// [`Arg`] converts from: text, an integer, a float, a
/// [`Moment`](crate::format::Moment). For an ID made at run time use
/// [`Message`] or [`text`].
#[macro_export]
macro_rules! tr {
    ($id:literal $(,)?) => {
        $crate::i18n::text($id)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::Message::new($id)
            $(.arg(stringify!($name), $value))+
            .format()
    };
}

/// A message ID as a `&'static str`, for a table of messages that are asked
/// for later with [`text`] or [`Message`]: `const ITEMS: [&str; 2] =
/// [i18n_id!("files-sort-name"), i18n_id!("files-sort-size")]`. It does
/// nothing at run time; the catalogue check counts the ID as used.
#[macro_export]
macro_rules! i18n_id {
    ($id:literal) => {
        $id
    };
}

/// The catalogue `<domain>.ftl` of the crate this is written in: its source
/// (`locale/en-US/<domain>.ftl`) compiled in, and its `locale/` directory read
/// in debug builds so a translation can be tried without installing it.
#[macro_export]
macro_rules! catalogue {
    ($domain:literal) => {
        $crate::i18n::Catalogue::new(
            $domain,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/locale/en-US/",
                $domain,
                ".ftl"
            )),
        )
        .with_source_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/locale"))
    };
}

#[cfg(test)]
mod tests;
