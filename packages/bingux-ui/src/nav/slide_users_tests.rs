//! Every slide in bingux-ui is one motion in and out (CK-09, MO-01, MO-02).
//!
//! Kieran: "the subpage close animation is too fast tbh. the speed and easing needs
//! to be the saem for in and out". The curve itself is held by
//! `nav::gesture::tests`; the page push and pop by `shell::navigation::tests`
//! (`a_closing_page_retraces_the_path_of_an_opening_one`), the sheet by
//! `shell::sheet_motion::tests` (`closing_is_the_same_motion_as_opening`); the
//! frames of a real slide by `scripts/nav-slide.py`. Those each name one slide.
//! This holds them all at once, and the next one:
//!
//! * the curve every slide shares is its own mirror image, at the rule's
//!   tolerance of 0.05 and in fact exactly, and it takes the same time both ways;
//! * the files that draw a slide are the ones listed here: a file that starts to use
//!   [`slide`] or a `SLIDE_MS` that is not in [`USERS`] fails this test until it is added,
//!   which is the moment to give it a symmetry test of its own;
//! * no motion has a duration for going in that is not the one for going out: a pair of
//!   constants named `..._IN_MS` and `..._OUT_MS`, or `..._OPEN_MS` and `..._CLOSE_MS`,
//!   with two different values is a failure unless it is in [`ASYMMETRIC_ON_PURPOSE`] with
//!   its reason.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::gesture::{SLIDE_MS, slide};

/// What draws a slide, with how it opens and closes. The file that defines the curve is
/// first. A file here is checked to use the one curve; one that is not here is found by
/// [`every_slide_is_in_the_table`].
const USERS: &[(&str, &str)] = &[
    ("nav/gesture.rs", "the curve itself, SLIDE_MS 320, and the swipe release that leaves a slide at the finger's speed"),
    ("shell/navigation.rs", "a page pushed and popped; predictive back; Motion::ms is SLIDE_MS both ways"),
    ("shell/sheet_motion.rs", "a sheet opening and closing on one clock that runs backwards; SHEET_MS is SLIDE_MS"),
    ("files/status_line.rs", "the status line's message slides in and out with one tween of its own 150 ms"),
];

/// Constant pairs with two values that are meant to differ, and why.
const ASYMMETRIC_ON_PURPOSE: &[(&str, &str)] = &[(
    "FOCUS",
    "the focus ring blurs in over 160 ms and out over 120 ms (motion.rs FOCUS_IN_MS, FOCUS_OUT_MS): a ring, not something that slides, and it must not outstay the control that lost focus",
)];

fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out.sort();
    out
}

fn relative(path: &Path) -> String {
    path.strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The text of a file outside its test code, comments and doc lines.
fn code_of(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    let body = text.split("\n#[cfg(test)]").next().unwrap_or("");
    body.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_test_file(path: &str) -> bool {
    path.ends_with("_tests.rs") || path.ends_with("/tests.rs") || path == "tests.rs"
}

#[test]
fn the_curve_every_slide_shares_is_its_own_mirror_image() {
    for i in 0..=200 {
        let t = i as f32 / 200.0;
        // The rule: p_out(t) = 1 - p_in(1 - t) within 0.05 ...
        let out = slide(1.0 - t);
        let mirrored = 1.0 - slide(t);
        assert!((out - mirrored).abs() <= 0.05, "t {t}: {out} against {mirrored}");
        // ... and it is exact, so there is no slack to spend.
        assert!((out - mirrored).abs() < 1e-5, "t {t}: {out} against {mirrored}");
    }
    assert_eq!(slide(0.0), 0.0);
    assert_eq!(slide(1.0), 1.0);
    // It takes the time it takes both ways: one constant, not an in and an out.
    assert_eq!(SLIDE_MS, 320);
}

#[test]
fn every_slide_is_in_the_table() {
    let known: BTreeSet<&str> = USERS.iter().map(|(file, _)| *file).collect();
    let mut found = BTreeSet::new();
    for path in sources() {
        let name = relative(&path);
        if is_test_file(&name) {
            continue;
        }
        let code = code_of(&path);
        let draws = code.contains("SLIDE_MS") || code.contains("gesture::slide") || code.contains("slide,") && code.contains("nav::gesture");
        if draws {
            found.insert(name);
        }
    }
    let unlisted: Vec<_> = found.iter().filter(|file| !known.contains(file.as_str())).collect();
    assert!(
        unlisted.is_empty(),
        "these files draw a slide and are not in USERS of nav/slide_users_tests.rs: {unlisted:?}. \
         Add each with how it opens and closes, and a test that its close is its open mirrored and takes as long"
    );
    let gone: Vec<_> = known.iter().filter(|file| !found.contains(**file)).collect();
    assert!(gone.is_empty(), "these files are in USERS and no longer draw a slide: {gone:?}");
}

#[test]
fn a_slide_with_its_own_duration_has_one_for_both_directions() {
    // The status line keeps a 150 ms of its own. It must be one constant, used by the one tween
    // that goes both ways, not an open and a close.
    let text = code_of(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/files/status_line.rs"));
    let uses = text.matches("SLIDE_MS").count();
    assert!(uses >= 2, "the status line's SLIDE_MS is defined and used");
    for word in ["SLIDE_OPEN_MS", "SLIDE_CLOSE_MS", "SLIDE_IN_MS", "SLIDE_OUT_MS", "OPEN_MS", "CLOSE_MS"] {
        assert!(!text.contains(word), "status_line.rs has {word}: a slide goes in and out in the same time");
    }
}

#[test]
fn no_motion_goes_in_and_out_in_different_times() {
    // `const NAME_IN_MS: u64 = 160;` and `const NAME_OUT_MS: u64 = 120;` with different values.
    let mut found: Vec<(String, String, String, u64)> = Vec::new();
    for path in sources() {
        let name = relative(&path);
        if is_test_file(&name) {
            continue;
        }
        for line in code_of(&path).lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("pub const ").or_else(|| line.strip_prefix("pub(crate) const ")).or_else(|| line.strip_prefix("const ")) else {
                continue;
            };
            let Some((ident, value)) = rest.split_once(": u64 = ") else { continue };
            let Ok(ms) = value.trim_end_matches(';').replace('_', "").parse::<u64>() else { continue };
            for (a, b) in [("_IN_MS", "_OUT_MS"), ("_OPEN_MS", "_CLOSE_MS"), ("_SHOW_MS", "_HIDE_MS"), ("_ENTER_MS", "_EXIT_MS")] {
                if let Some(stem) = ident.strip_suffix(a) {
                    found.push((stem.to_owned(), a.to_owned(), b.to_owned(), ms));
                } else if let Some(stem) = ident.strip_suffix(b) {
                    found.push((stem.to_owned(), b.to_owned(), a.to_owned(), ms));
                }
            }
        }
    }
    let mut bad = Vec::new();
    for (stem, suffix, other, ms) in &found {
        if let Some((_, _, _, other_ms)) = found.iter().find(|(s, o, _, _)| s == stem && o == other && o != suffix) {
            if ms != other_ms && !ASYMMETRIC_ON_PURPOSE.iter().any(|(allowed, _)| stem.ends_with(allowed)) {
                bad.push(format!("{stem}{suffix} is {ms} ms and {stem}{other} is {other_ms} ms"));
            }
        }
    }
    bad.sort();
    bad.dedup();
    assert!(
        bad.is_empty(),
        "a motion that goes in and out in different times: {bad:?}. \
         Make them one constant, or list the pair in ASYMMETRIC_ON_PURPOSE with its reason"
    );
}
