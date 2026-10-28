//! Checks on the registry and on what it shows.
//!
//! `registry_covers_bingux_ui` is the enforceable requirement: it runs
//! `scripts/taxonomy-check.py`, which fails when bingux-ui exports a component,
//! control builder, `Icon` variant or style token that no entry shows and
//! `coverage-allowlist.txt` does not excuse. The rest check that the registry
//! is well formed and that every page really renders.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use bingux_ui::icons::Icon;

use crate::icon_table::ICON_FILES;
use crate::model::{Body, Category, PageId, pages};
use crate::registry::ENTRIES;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn registry_covers_bingux_ui() {
    let script = repo().join("scripts/taxonomy-check.py");
    let output = Command::new("python3")
        .arg(&script)
        .output()
        .expect("run python3 scripts/taxonomy-check.py");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn entries_are_well_formed() {
    let mut names = HashSet::new();
    for entry in ENTRIES {
        assert!(
            names.insert(entry.name),
            "two entries are called {:?}",
            entry.name
        );
        assert!(!entry.api.is_empty(), "{} has no API line", entry.name);
        assert!(!entry.doc.is_empty(), "{} has no doc anchor", entry.name);
        assert!(!entry.page.is_empty(), "{} has no page", entry.name);
        if let Body::States(specs) = entry.body {
            assert!(!specs.is_empty(), "{} shows no state", entry.name);
            let mut labels = HashSet::new();
            for spec in specs {
                assert!(
                    labels.insert(spec.label),
                    "{} shows {:?} twice",
                    entry.name,
                    spec.label
                );
            }
        }
    }
}

#[test]
fn every_category_has_a_page() {
    for category in Category::ALL {
        assert!(
            pages().iter().any(|page| page.category == category),
            "{} has no page",
            category.title()
        );
    }
}

#[test]
fn page_addresses_round_trip() {
    let mut seen = HashSet::new();
    for id in PageId::all() {
        let segments = id.segments();
        assert!(
            seen.insert(segments.clone()),
            "two pages share {segments:?}"
        );
        assert_eq!(PageId::from_segments(&segments), Some(id));
    }
}

#[test]
fn every_icon_file_is_listed() {
    let dir = repo().join("packages/bingux-ui/icons");
    let files = std::fs::read_dir(&dir)
        .expect("read icons")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with("-symbolic.svg")
        })
        .count();
    assert_eq!(
        ICON_FILES.len(),
        files,
        "the icon table and the icons directory differ"
    );
    // Two variants may name one file (`Power` is the battery glyph in the
    // sidebar); the table names one of them, so a variant is listed when its
    // file is.
    for icon in Icon::ALL {
        assert!(
            ICON_FILES.iter().any(|file| file.svg == icon.bytes()),
            "Icon::{icon:?} has no row in the icon table"
        );
    }
    for file in ICON_FILES {
        assert!(
            !file.place.is_empty(),
            "{} has no row in scripts/icons/labels.tsv",
            file.file
        );
    }
}

/// Every page renders at desktop width and at phone width without a panic,
/// and the capture is a real image.
#[test]
fn every_page_renders() {
    let dir = std::env::temp_dir().join(format!("bingux-taxonomy-test-{}", std::process::id()));
    for id in PageId::all() {
        for (suffix, size) in [("desktop", (1320.0, 900.0)), ("phone", (390.0, 800.0))] {
            let path = dir.join(format!("{}-{suffix}.png", id.segments().join("_")));
            let mut shot = crate::snapshot_mode::open(id, size);
            shot.settle(Duration::from_millis(250)).save(&path);
            let length = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            assert!(length > 1000, "{} rendered {length} bytes", path.display());
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Moving from every page to every other page in one window must not panic.
/// A specimen's hooks are its own: if the window reused one specimen's
/// component for another's, Freya would fail with "Hook functions must follow
/// these rules" (the Switches page did when opened from another page). Each
/// page is visited from the page before it, at desktop width and at phone
/// width, so every entry is built in both densities after a different page.
#[test]
fn navigating_between_every_page_does_not_panic() {
    let ids = PageId::all();
    for size in [(1320.0, 900.0), (390.0, 800.0)] {
        let mut shot = crate::snapshot_mode::open(ids[0], size);
        shot.settle(Duration::from_millis(200));
        // Forward, then backward, so each page follows two different pages.
        for id in ids.iter().skip(1).chain(ids.iter().rev().skip(1)) {
            shot.navigate(crate::app::location_of(*id));
            shot.settle(Duration::from_millis(200));
        }
    }
}
