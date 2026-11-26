//! The store keeps what changes in a window, and says when it cannot.
//!
//! Failures these guard against:
//!   1. a setting changed in the window is lost at the next start;
//!   2. a damaged or newer settings file is overwritten by this build;
//!   3. a write that fails is silent, or its state never clears after a
//!      later write succeeds;
//!   4. Start over is lost: the damaged file is deleted instead of kept.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use bingux_ui::search::SearchService;
use bingux_ui::web_view::test_double::TestDoubleBackend;

use super::files::{self, Problem, remove_tree, tests::temp};
use super::places::{Folders, Places};
use super::settings::Settings;
use super::store::{Store, keeping_of, not_saved};
use crate::browser::{Browser, Options, Profile};
use crate::model::keeping::{Keeping, NotSaved};

fn folders(root: &Path) -> Folders {
    Places::named_folders(root)
}

/// A window of the profile in `root`, as `main` opens it.
fn window(root: &Path) -> Browser {
    crate::locale::register();
    let folders = folders(root);
    let (settings, problem) = Settings::read(&folders.settings());
    let keeping = keeping_of(problem.as_ref());
    let store = Store::open(&folders, keeping.clone(), &SearchService::new()).unwrap();
    Browser::with_profile(
        TestDoubleBackend::new(),
        Options::default(),
        Profile {
            id: None,
            name: None,
            settings,
            settings_keeping: keeping,
            store: Some(store),
        },
    )
}

#[test]
fn a_search_engine_chosen_in_the_window_survives_a_new_window() {
    let root = temp();
    let mut browser = window(&root);
    assert_eq!(browser.settings_keeping, Keeping::Saved);
    browser.set_search_engine(2);
    browser.flush_store();
    assert_eq!(browser.settings_keeping, Keeping::Saved);
    drop(browser);
    let again = window(&root);
    assert_eq!(again.search().name, "Wikipedia");
    let mode = fs::metadata(folders(&root).settings())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    remove_tree(&root).unwrap();
}

#[test]
fn a_damaged_file_is_never_written_over_and_start_over_keeps_it_aside() {
    let root = temp();
    let path = folders(&root).settings();
    fs::write(&path, b"{ not json").unwrap();
    let mut browser = window(&root);
    assert!(browser.settings_keeping.can_start_over());
    assert!(browser.take_notices()[0].text.contains("could not be read"));
    browser.set_search_engine(3);
    browser.flush_store();
    assert_eq!(fs::read(&path).unwrap(), b"{ not json");
    browser.start_over_settings();
    browser.flush_store();
    assert_eq!(browser.settings_keeping, Keeping::Saved);
    assert_eq!(Settings::read(&path).0.search_engine, "google");
    let aside: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.json.damaged-")
        })
        .collect();
    assert_eq!(aside.len(), 1);
    assert_eq!(fs::read(aside[0].path()).unwrap(), b"{ not json");
    remove_tree(&root).unwrap();
}

#[test]
fn a_newer_browsers_file_is_left_as_it_is() {
    let root = temp();
    let path = folders(&root).settings();
    let newer = br#"{"version":9,"search_engine":"startpage"}"#;
    fs::write(&path, newer).unwrap();
    let mut browser = window(&root);
    assert!(matches!(
        browser.settings_keeping,
        Keeping::NotSaved(NotSaved::TooNew { found: 9, .. })
    ));
    browser.set_search_engine(2);
    browser.start_over_settings();
    browser.flush_store();
    assert_eq!(fs::read(&path).unwrap(), newer);
    remove_tree(&root).unwrap();
}

#[test]
fn a_read_only_folder_says_so_and_the_next_good_write_clears_it() {
    let root = temp();
    let mut browser = window(&root);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
    browser.set_search_engine(1);
    browser.flush_store();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        browser.settings_keeping,
        Keeping::NotSaved(NotSaved::ReadOnly)
    );
    assert!(
        browser
            .take_notices()
            .iter()
            .any(|notice| notice.text.contains("read-only"))
    );
    browser.set_search_engine(2);
    browser.flush_store();
    assert_eq!(browser.settings_keeping, Keeping::Saved);
    assert_eq!(
        Settings::read(&folders(&root).settings()).0.search_engine,
        "wikipedia"
    );
    remove_tree(&root).unwrap();
}

#[test]
fn a_full_disk_is_disk_full() {
    // A real ENOSPC: `/dev/full` refuses every write with it.
    let mut full = fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let error = files::write_synced(&mut full, b"{}").unwrap_err();
    assert_eq!(
        not_saved(&Problem::of(&error)),
        Keeping::NotSaved(NotSaved::DiskFull)
    );
}

#[test]
fn a_private_window_and_a_window_without_a_profile_say_nothing_in_a_toast() {
    let mut browser = Browser::new(TestDoubleBackend::new(), Options { private: true });
    assert!(browser.take_notices().is_empty());
    assert_eq!(
        browser.settings_keeping,
        Keeping::NotSaved(NotSaved::NoProfile)
    );
}
