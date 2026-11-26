//! What the location field of the file views offers for the text typed, as
//! plain data: no window, so every rule is a test.
//!
//! The field takes a place and a search in one text. Whether the text is one
//! or the other is decided by how it begins ([`is_path`]); what is offered for
//! each is here, and the pieces the component puts on screen
//! ([`omnibox`](super::omnibox)) are made from it:
//!
//! | Text | Offered |
//! | ---- | ------- |
//! | `/usr/sh`, `~/Do`, `../x`, `file:///`, `trash:`, `Pictures/20` | the folders and files that complete it, segment by segment |
//! | `report`, `type:image beach` | the matching entries of this folder, the places by that name, and a search of the scope with the filters |
//!
//! The scope chips say where a search looks, the filter chips what it keeps.
//! Both are the same engine as typing the operator (`type:image`, `in:~/x`):
//! [`Find`](super::search::Find) holds them, and a token typed in the field is
//! lifted into them the moment a space ends it.

use std::ops::Range;
use std::path::{Path, PathBuf};

use bingux_files::find::{FilterKey, Filters, Hidden, Kind, Modified, NameMode, Scope, SizeCond};
use bingux_files::{PlaceKind, Places};

use super::display;

use super::address_complete::find_match;

/// Whether the text is a place being typed rather than a search: it begins
/// like a path (`/`, `~`, `./`, `../`), is an address (`file:`, `trash:`,
/// `recent:`), or names a path below a folder (`Pictures/20`). A search for
/// words never has a slash in it.
pub(super) fn is_path(text: &str) -> bool {
    let text = text.trim_start();
    if text.starts_with(['/', '~']) || text == "." || text == ".." {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    if ["file:", "trash:", "recent:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        return true;
    }
    // `type:image` is an operator, not an address; a slash is what makes a path.
    text.contains('/') && !text.contains(char::is_whitespace)
}

// ---------------------------------------------------------------------------
// Where to look
// ---------------------------------------------------------------------------

/// One scope chip: its words and what it stands for.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ScopeChoice {
    pub label: String,
    pub scope: Scope,
}

/// The scopes that need no choosing: the folder on show and everything in it,
/// the home folder, and every place the person's files are (Everywhere).
pub(super) fn scopes() -> Vec<ScopeChoice> {
    vec![
        ScopeChoice {
            label: crate::tr!("ui-files-omni-scope-folder"),
            scope: Scope::Subfolders,
        },
        ScopeChoice {
            label: crate::tr!("ui-files-omni-scope-home"),
            scope: Scope::Home,
        },
        ScopeChoice {
            label: crate::tr!("ui-files-omni-scope-everywhere"),
            scope: Scope::Everywhere,
        },
    ]
}

/// The places a search can be limited to: the user folders and the
/// bookmarks, with the folder of `current` added when it is none of them (a
/// bookmark since removed).
pub(super) fn place_choices(places: &Places, home: &Path, current: &Scope) -> Vec<ScopeChoice> {
    let mut choices: Vec<ScopeChoice> = Vec::new();
    let mut seen: Vec<&Path> = vec![home];
    for place in places.standard.iter().chain(&places.bookmarks) {
        let Some(path) = place.path.as_deref() else {
            continue;
        };
        let searchable = matches!(
            place.kind,
            PlaceKind::Documents
                | PlaceKind::Downloads
                | PlaceKind::Pictures
                | PlaceKind::Music
                | PlaceKind::Videos
                | PlaceKind::Desktop
                | PlaceKind::Bookmark
        );
        if !searchable || seen.contains(&path) {
            continue;
        }
        seen.push(path);
        choices.push(ScopeChoice {
            label: place.name.clone(),
            scope: Scope::Place(path.to_path_buf()),
        });
    }
    if let Scope::Place(path) = current
        && !seen.contains(&path.as_path())
    {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        choices.push(ScopeChoice {
            label: name,
            scope: current.clone(),
        });
    }
    choices
}

/// Where the search looks, in words: `Documents`, `Home`, `everywhere`.
pub(super) fn place_phrase(scope: &Scope, folder: &str, choices: &[ScopeChoice]) -> String {
    match scope {
        Scope::Subfolders => folder.to_string(),
        Scope::Home => crate::tr!("ui-files-omni-phrase-home"),
        Scope::Everywhere => crate::tr!("ui-files-omni-phrase-everywhere"),
        Scope::Place(path) => choices
            .iter()
            .find(|choice| choice.scope == *scope)
            .map_or_else(
                || {
                    path.file_name().map_or_else(
                        || folder.to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    )
                },
                |choice| choice.label.clone(),
            ),
    }
}

// ---------------------------------------------------------------------------
// What to keep
// ---------------------------------------------------------------------------

/// The file types the chips offer.
pub(super) const KIND_CHIPS: [Kind; 5] = [
    Kind::Document,
    Kind::Image,
    Kind::Audio,
    Kind::Video,
    Kind::Folder,
];

/// The words of a file type.
pub(super) fn kind_name(kind: Kind) -> String {
    match kind {
        Kind::Folder => crate::tr!("ui-files-omni-kind-folders"),
        Kind::Image => crate::tr!("ui-files-omni-kind-images"),
        Kind::Video => crate::tr!("ui-files-omni-kind-video"),
        Kind::Audio => crate::tr!("ui-files-omni-kind-audio"),
        Kind::Document => crate::tr!("ui-files-omni-kind-documents"),
        Kind::Archive => crate::tr!("ui-files-omni-kind-archives"),
        Kind::Code => crate::tr!("ui-files-omni-kind-code"),
    }
}

/// Add `kind` to the types kept, or take it away when it is there.
pub(super) fn toggle_kind(filters: &mut Filters, kind: Kind) {
    match filters.kinds.iter().position(|have| *have == kind) {
        Some(at) => {
            filters.kinds.remove(at);
        }
        None => {
            filters.kinds.push(kind);
            filters.kinds.sort();
        }
    }
}

/// The ages the chips offer, each with the ID of the message that names it.
pub(super) const WHEN_CHIPS: [(&str, Modified); 3] = [
    (crate::i18n_id!("ui-files-omni-when-today"), Modified::Today),
    (
        crate::i18n_id!("ui-files-omni-when-week"),
        Modified::Days(7),
    ),
    (
        crate::i18n_id!("ui-files-omni-when-year"),
        Modified::ThisYear,
    ),
];

/// Keep what changed in `when`; again, and every age is kept.
pub(super) fn toggle_modified(filters: &mut Filters, when: Modified) {
    filters.modified = if filters.modified == Some(when) {
        None
    } else {
        Some(when)
    };
}

const MB: u64 = 1_000_000;

/// The sizes the chips offer, each with the ID of the message that names it.
pub(super) const SIZE_CHIPS: [(&str, SizeCond); 2] = [
    (
        crate::i18n_id!("ui-files-omni-size-small"),
        SizeCond::Lt(MB),
    ),
    (
        crate::i18n_id!("ui-files-omni-size-large"),
        SizeCond::Gt(100 * MB),
    ),
];

/// Keep the files of `size`; again, and every size is kept.
pub(super) fn toggle_size(filters: &mut Filters, size: SizeCond) {
    filters.size = if filters.size == Some(size) {
        None
    } else {
        Some(size)
    };
}

/// The filters set that no chip shows as one of its own, each with its words:
/// an extension, a name pattern, a folder (`in:`), hidden files, case, and
/// a type, age or size that is not one a chip offers (`modified:2024`).
pub(super) fn other_filters(filters: &Filters) -> Vec<(FilterKey, String)> {
    let mut out = Vec::new();
    for key in filters.active() {
        let own = match key {
            FilterKey::Kind => filters.kinds.iter().all(|kind| KIND_CHIPS.contains(kind)),
            FilterKey::Modified => filters
                .modified
                .is_some_and(|when| WHEN_CHIPS.iter().any(|(_, chip)| *chip == when)),
            FilterKey::Size => filters
                .size
                .is_some_and(|size| SIZE_CHIPS.iter().any(|(_, chip)| *chip == size)),
            _ => false,
        };
        if !own {
            out.push((key, token_label(key, filters)));
        }
    }
    out
}

/// What a token of the active filters says.
pub(super) fn token_label(key: FilterKey, filters: &Filters) -> String {
    match key {
        FilterKey::Kind => crate::tr!(
            "ui-files-omni-token-type",
            kinds = filters
                .kinds
                .iter()
                .map(|kind| kind_name(*kind))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        FilterKey::Ext => crate::tr!(
            "ui-files-omni-token-extension",
            extensions = filters
                .exts
                .iter()
                .map(|ext| format!(".{ext}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        FilterKey::Size => match filters.size {
            Some(SizeCond::Gt(n)) => {
                crate::tr!("ui-files-omni-token-size-over", size = display::size(n))
            }
            Some(SizeCond::Ge(n)) => {
                crate::tr!("ui-files-omni-token-size-at-least", size = display::size(n))
            }
            Some(SizeCond::Lt(n)) => {
                crate::tr!("ui-files-omni-token-size-under", size = display::size(n))
            }
            Some(SizeCond::Le(n)) => {
                crate::tr!("ui-files-omni-token-size-at-most", size = display::size(n))
            }
            Some(SizeCond::Eq(n)) => {
                crate::tr!("ui-files-omni-token-size-exactly", size = display::size(n))
            }
            Some(SizeCond::Between(low, high)) => crate::tr!(
                "ui-files-omni-token-size-between",
                low = display::size(low),
                high = display::size(high)
            ),
            None => String::new(),
        },
        FilterKey::Modified => match filters.modified {
            Some(Modified::Today) => crate::tr!("ui-files-omni-token-modified-today"),
            Some(Modified::Yesterday) => crate::tr!("ui-files-omni-token-modified-yesterday"),
            Some(Modified::Days(n)) => crate::tr!("ui-files-omni-token-modified-days", days = n),
            Some(Modified::ThisYear) => crate::tr!("ui-files-omni-token-modified-year"),
            Some(Modified::On(moment)) => crate::tr!(
                "ui-files-omni-token-modified-on",
                moment = moment.to_string()
            ),
            Some(Modified::Since(moment)) => crate::tr!(
                "ui-files-omni-token-modified-since",
                moment = moment.to_string()
            ),
            Some(Modified::Before(moment)) => crate::tr!(
                "ui-files-omni-token-modified-before",
                moment = moment.to_string()
            ),
            Some(Modified::Between(from, to)) => crate::tr!(
                "ui-files-omni-token-modified-between",
                from = from.to_string(),
                to = to.to_string()
            ),
            None => String::new(),
        },
        FilterKey::Name => {
            let mode = match filters.name_mode {
                NameMode::Contains => String::new(),
                NameMode::Starts => crate::tr!("ui-files-omni-name-starts"),
                NameMode::Exact => crate::tr!("ui-files-omni-name-exact"),
                NameMode::Regex => crate::tr!("ui-files-omni-name-regex"),
            };
            let globs = filters.globs.join(", ");
            match (mode.is_empty(), globs.is_empty()) {
                (false, false) => crate::tr!(
                    "ui-files-omni-token-name-mode-globs",
                    mode = mode,
                    globs = globs
                ),
                (false, true) => crate::tr!("ui-files-omni-token-name-mode", mode = mode),
                (true, _) => crate::tr!("ui-files-omni-token-name-globs", globs = globs),
            }
        }
        FilterKey::Within => crate::tr!(
            "ui-files-omni-token-within",
            folder = filters.within.clone().unwrap_or_default()
        ),
        FilterKey::Hidden => match filters.hidden {
            Hidden::Include => crate::tr!("ui-files-omni-token-hidden-include"),
            Hidden::Only => crate::tr!("ui-files-omni-token-hidden-only"),
            Hidden::Exclude => crate::tr!("ui-files-omni-token-hidden-exclude"),
            Hidden::Follow => String::new(),
        },
        FilterKey::Case => crate::tr!("ui-files-omni-token-case"),
    }
}

// ---------------------------------------------------------------------------
// Places
// ---------------------------------------------------------------------------

/// A place that answers the text: a user folder, a bookmark, or a folder the
/// person has been in lately.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PlaceMatch {
    pub name: String,
    pub path: PathBuf,
    pub matched: Range<usize>,
    pub bookmark: bool,
}

/// The most places the field offers for a text.
pub(super) const MOST_PLACES: usize = 3;

/// The places whose name has `text` in it, ignoring case: the user folders and
/// the bookmarks first, then the folders of `recent` (newest first), each path
/// once, at most [`MOST_PLACES`]. The home folder and the trash answer to
/// their names like any other. A blank text matches nothing.
pub(super) fn places_matching(text: &str, places: &Places, recent: &[PathBuf]) -> Vec<PlaceMatch> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<PlaceMatch> = Vec::new();
    let mut push = |name: String, path: &Path, bookmark: bool| {
        if out.len() >= MOST_PLACES || out.iter().any(|seen| seen.path == path) {
            return;
        }
        let matched = find_match(&name, text);
        if matched.is_empty() {
            return;
        }
        out.push(PlaceMatch {
            name,
            path: path.to_path_buf(),
            matched,
            bookmark,
        });
    };
    for place in places.bookmarks.iter().chain(&places.standard) {
        if let Some(path) = &place.path {
            push(place.name.clone(), path, place.kind == PlaceKind::Bookmark);
        }
    }
    for path in recent {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        push(name, path, false);
    }
    out
}

// ---------------------------------------------------------------------------
// What the search says of itself
// ---------------------------------------------------------------------------

/// How a search of folders is going, for the heading over its results.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SearchStatus {
    pub place: String,
    pub running: bool,
    pub found: usize,
    pub folders: usize,
    pub truncated: bool,
    pub unreadable: usize,
}

/// The muted line at the end of the results' heading: what is going on, or
/// what happened.
pub(super) fn status_text(status: &SearchStatus) -> String {
    if status.running {
        return if status.found == 0 {
            crate::tr!("ui-files-omni-status-searching")
        } else {
            crate::tr!("ui-files-omni-status-found-searching", found = status.found)
        };
    }
    crate::tr!(
        "ui-files-omni-status-found",
        found = status.found,
        limit = u8::from(status.truncated),
        unreadable = status.unreadable
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_files::Place;
    use bingux_files::find::Query;

    fn places() -> Places {
        let place = |kind, name: &str, path: &str| Place {
            kind,
            name: name.into(),
            path: Some(PathBuf::from(path)),
        };
        Places {
            standard: vec![
                place(PlaceKind::Home, "Home", "/home/me"),
                place(PlaceKind::Documents, "Documents", "/home/me/Documents"),
                place(PlaceKind::Pictures, "Pictures", "/home/me/Pictures"),
                place(PlaceKind::Trash, "Trash", "/home/me/.trash"),
            ],
            devices: vec![place(PlaceKind::Volume, "USB", "/run/media/me/USB")],
            bookmarks: vec![place(PlaceKind::Bookmark, "Work docs", "/srv/work")],
        }
    }

    #[test]
    fn text_that_begins_like_a_path_is_a_place_and_words_are_a_search() {
        for path in [
            "/usr/sh",
            "/",
            "~",
            "~/Do",
            "../x",
            "..",
            ".",
            "file:///tmp",
            "trash:",
            "Recent:///",
            "Pictures/20",
        ] {
            assert!(is_path(path), "{path}");
        }
        for words in [
            "report",
            "tax return",
            "type:image beach",
            "size:>1MB",
            "in:~/Music song",
            "a b/c d",
            "",
        ] {
            assert!(!is_path(words), "{words}");
        }
    }

    #[test]
    fn the_scope_chips_are_the_folder_home_and_everywhere_and_the_places_are_chosen() {
        let labels: Vec<_> = scopes().into_iter().map(|choice| choice.label).collect();
        assert_eq!(labels, ["This folder", "Home", "Everywhere"]);
        let choices = place_choices(&places(), Path::new("/home/me"), &Scope::Subfolders);
        let names: Vec<_> = choices.iter().map(|choice| choice.label.as_str()).collect();
        assert_eq!(
            names,
            ["Documents", "Pictures", "Work docs"],
            "user folders and bookmarks, not Home, the Trash or drives"
        );
        // A scope that is none of them (a bookmark since removed) is kept, so the chip can show.
        let gone = Scope::Place(PathBuf::from("/srv/gone"));
        let choices = place_choices(&places(), Path::new("/home/me"), &gone);
        assert_eq!(
            choices.last().map(|choice| choice.label.as_str()),
            Some("gone")
        );
        assert_eq!(place_phrase(&gone, "Docs", &choices), "gone");
        assert_eq!(place_phrase(&Scope::Subfolders, "Docs", &choices), "Docs");
        assert_eq!(place_phrase(&Scope::Home, "Docs", &choices), "Home");
        assert_eq!(
            place_phrase(&Scope::Everywhere, "Docs", &choices),
            "everywhere"
        );
    }

    #[test]
    fn a_filter_chip_toggles_and_chips_of_one_row_add_up_or_replace_as_the_engine_reads_them() {
        let mut filters = Filters::default();
        toggle_kind(&mut filters, Kind::Image);
        toggle_kind(&mut filters, Kind::Audio);
        assert_eq!(
            filters.kinds,
            [Kind::Image, Kind::Audio],
            "types are any-of, kept in the engine's order"
        );
        toggle_kind(&mut filters, Kind::Image);
        assert_eq!(filters.kinds, [Kind::Audio]);
        toggle_modified(&mut filters, Modified::Today);
        assert_eq!(filters.modified, Some(Modified::Today));
        toggle_modified(&mut filters, Modified::ThisYear);
        assert_eq!(
            filters.modified,
            Some(Modified::ThisYear),
            "one age at a time"
        );
        toggle_modified(&mut filters, Modified::ThisYear);
        assert_eq!(filters.modified, None);
        toggle_size(&mut filters, SIZE_CHIPS[1].1);
        assert_eq!(filters.size, Some(SizeCond::Gt(100 * MB)));
        toggle_size(&mut filters, SIZE_CHIPS[1].1);
        assert_eq!(filters.size, None);
    }

    #[test]
    fn what_the_chips_do_not_show_is_listed_with_its_words() {
        let filters = Query::parse(
            "type:image,video ext:png size:>10MB modified:today in:Docs hidden:only case:yes",
        )
        .filters;
        let others: Vec<_> = other_filters(&filters)
            .into_iter()
            .map(|(key, text)| (key, text))
            .collect();
        assert_eq!(
            others,
            [
                (FilterKey::Size, "Size over 10\u{a0}MB".to_string()),
                (FilterKey::Ext, "Extension: .png".to_string()),
                (FilterKey::Within, "In Docs".to_string()),
                (FilterKey::Hidden, "Only hidden files".to_string()),
                (FilterKey::Case, "Match case".to_string()),
            ]
        );
        // A type the chips have is theirs; one they have not (archives) is listed.
        let archives = Query::parse("type:archive").filters;
        assert_eq!(
            other_filters(&archives),
            [(FilterKey::Kind, "Type: Archives".to_string())]
        );
        assert!(
            other_filters(&Query::parse("type:image modified:week size:<1MB").filters).is_empty()
        );
    }

    #[test]
    fn a_token_says_its_filter_in_words() {
        let filters =
            Query::parse("type:image,video size:>10MB modified:week ext:png name:*.x case:yes hidden:only in:Docs match:regex").filters;
        let label = |key| token_label(key, &filters);
        assert_eq!(label(FilterKey::Kind), "Type: Images, Video");
        assert_eq!(label(FilterKey::Size), "Size over 10\u{a0}MB");
        assert_eq!(label(FilterKey::Modified), "Modified in the last 7 days");
        assert_eq!(label(FilterKey::Ext), "Extension: .png");
        assert_eq!(label(FilterKey::Name), "Name regular expression, *.x");
        assert_eq!(label(FilterKey::Case), "Match case");
        assert_eq!(label(FilterKey::Hidden), "Only hidden files");
        assert_eq!(label(FilterKey::Within), "In Docs");
    }

    #[test]
    fn places_answer_to_their_names_bookmarks_and_user_folders_before_recent_folders() {
        let recent = [
            PathBuf::from("/srv/data/Documents-old"),
            PathBuf::from("/home/me/Documents"),
        ];
        let found = places_matching("doc", &places(), &recent);
        let names: Vec<_> = found
            .iter()
            .map(|place| (place.name.as_str(), place.bookmark))
            .collect();
        assert_eq!(
            names,
            [
                ("Work docs", true),
                ("Documents", false),
                ("Documents-old", false)
            ]
        );
        assert_eq!(found[0].matched, 5..8, "the match is marked in the name");
        // A folder that is a place and was visited is listed once.
        assert_eq!(
            found
                .iter()
                .filter(|place| place.path == Path::new("/home/me/Documents"))
                .count(),
            1
        );
        assert!(places_matching("", &places(), &recent).is_empty());
        assert!(places_matching("zzz", &places(), &recent).is_empty());
        let many: Vec<PathBuf> = (0..9)
            .map(|n| PathBuf::from(format!("/x/doc{n}")))
            .collect();
        assert_eq!(places_matching("doc", &places(), &many).len(), MOST_PLACES);
    }

    #[test]
    fn the_search_says_how_it_is_going() {
        let mut status = SearchStatus {
            place: "Pictures".into(),
            running: true,
            ..SearchStatus::default()
        };
        assert_eq!(status_text(&status), "Searching…");
        status.found = 1204;
        assert_eq!(status_text(&status), "1,204 found, searching…");
        status.running = false;
        assert_eq!(status_text(&status), "1,204 found");
        status.unreadable = 3;
        assert_eq!(
            status_text(&status),
            "1,204 found · 3 folders could not be read"
        );
        status.unreadable = 1;
        assert_eq!(
            status_text(&status),
            "1,204 found · 1 folder could not be read"
        );
        status.unreadable = 3;
        status.truncated = true;
        assert_eq!(
            status_text(&status),
            "1,204 found · limit reached, narrow the search"
        );
        assert_eq!(
            status_text(&SearchStatus {
                found: 10_000,
                ..SearchStatus::default()
            }),
            "10,000 found"
        );
    }
}
