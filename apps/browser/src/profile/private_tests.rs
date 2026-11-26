//! A private window writes nothing (`docs/browser/profiles-and-store.md`,
//! "Isolation").
//!
//! Failures this guards against:
//!   1. a private window that records a visit, or opens the profile's
//!      database (which creates `-wal` and `-shm` even to read);
//!   2. a setting changed in a private window written to its profile;
//!   3. Clear history in a private window reaching the profile's history;
//!   4. opening a private window creating a lock, a folder or a registry.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use bingux_ui::web_view::test_double::TestDoubleBackend;

use super::files::{remove_tree, tests::temp};
use super::open::{Opened, open, open_private};
use super::places::Places;
use crate::model::browser::{Browser, Options, Profile};
use crate::model::keeping::{Keeping, NotSaved};
use crate::model::tabs::Place;

/// Path, size, modification time and mode of everything below `root`,
/// without following symbolic links.
fn walk(root: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u32)> {
    let mut found = BTreeMap::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            found.insert(
                path.clone(),
                (meta.size(), meta.mtime(), meta.mtime_nsec(), meta.mode()),
            );
            if meta.is_dir() {
                folders.push(path);
            }
        }
    }
    found
}

#[test]
fn a_private_window_leaves_every_file_as_it_was() {
    let home = temp();
    let variables: Vec<(&str, PathBuf)> = vec![
        ("XDG_DATA_HOME", home.join("data")),
        ("XDG_CACHE_HOME", home.join("cache")),
        ("XDG_STATE_HOME", home.join("state")),
        ("XDG_RUNTIME_DIR", home.join("run")),
    ];
    let places = Places::from_lookup(&|name| {
        variables
            .iter()
            .find(|(variable, _)| *variable == name)
            .map(|(_, path)| OsString::from(path))
    });
    // A profile with settings and history of its own, closed again.
    match open(&places, None) {
        Opened::Profile(opened) => drop(opened),
        other => panic!("the first start opens Personal, not {other:?}"),
    }
    let before = walk(&home);
    assert!(
        before.keys().any(|path| path.ends_with("profiles.json")),
        "the walk sees the profile"
    );

    let Opened::Private { settings, from } = open_private(&places, None) else {
        panic!("a private window opens");
    };
    assert!(from.is_some(), "it belongs to the default profile");
    let mut private = Browser::with_profile(
        TestDoubleBackend::new(),
        Options { private: true },
        Profile::private(from.map(|id| id.as_str().to_string()), settings),
    );
    assert_eq!(
        private.settings_keeping,
        Keeping::NotSaved(NotSaved::Private)
    );
    for url in ["https://example.com/", "https://example.org/a"] {
        private.open_tab(url, Place::End, true);
        private.poll();
    }
    private.set_search_engine(3);
    private.clear_history();
    private.flush_store();
    assert_eq!(private.search().name, "Google", "the change applies here");
    drop(private);

    assert_eq!(
        walk(&home),
        before,
        "nothing was created, changed or removed"
    );
    remove_tree(&home).unwrap();
}
