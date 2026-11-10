//! The design boundaries (`design/boundaries.md`), checked over the apps'
//! source where the types cannot see them. An app builds its window with
//! `Screen` or `NavigationSplitView` and paints no structural tier itself, and
//! a segmented control in an app picks a value: a control that switches what
//! the content shows is a `view_switcher` in the header.

use crate::source_scan::workspace_code;

/// Apps that still break a rule below, with what is left. Each is removed when
/// its migration lands; the test fails when a listed file is clean.
const PENDING: &[(&str, &str)] = &[];

/// The problems one file's code has with the boundaries.
fn violations(path: &str, code: &str) -> Vec<String> {
    let mut found = Vec::new();
    let lines: Vec<&str> = code.lines().collect();
    for (at, line) in lines.iter().enumerate() {
        let line_no = at + 1;
        for paint in ["Sidebar", "Strip", "Frosted"] {
            if line.contains(&format!("bg(Paint::{paint})")) {
                found.push(format!(
                    "line {line_no}: paints Paint::{paint} by hand; the shell paints the chrome \
                     (Screen, NavigationSplitView)"
                ));
            }
        }
        // A snapshot harness hosts the app; it is not the app's window.
        let window_root = line.contains("bg(Paint::Bg)")
            && line.contains("w_full()")
            && line.contains("h_full()");
        if window_root && !path.ends_with("snapshot_mode.rs") {
            found.push(format!(
                "line {line_no}: paints a window-sized Paint::Bg by hand; Screen paints the content"
            ));
        }
        // A function that builds the header for a screen elsewhere is fine.
        if line.contains("HeaderBar::new(")
            && !code.contains("Screen::new(")
            && !code.contains("-> HeaderBar")
        {
            // A dialog's or a sheet's bar is not a window header.
            let dialog = lines[at..lines.len().min(at + 4)]
                .iter()
                .any(|next| next.contains(".draggable(false)"));
            if !dialog {
                found.push(format!(
                    "line {line_no}: builds a window header outside Screen; give it to Screen::new"
                ));
            }
        }
        if line.contains("segmented(") && !line.contains("fn segmented") {
            let previous = lines[..at]
                .iter()
                .rev()
                .find(|text| !text.trim().is_empty());
            let in_row = line.contains(".trailing(segmented(")
                || previous.is_some_and(|text| text.trim_end().ends_with(".trailing("));
            let declared = previous.is_some_and(|text| text.contains("// boundary: value"));
            if !in_row && !declared {
                found.push(format!(
                    "line {line_no}: a segmented control outside a row. If it switches what the \
                     content shows, use view_switcher in the header; if it picks a value, put it \
                     in a row or say why on the line above (// boundary: value, ...)"
                ));
            }
        }
    }
    found
}

#[test]
fn the_checks_catch_the_banned_forms() {
    let app = "apps/x/src/app.rs";
    assert!(!violations(app, "rect().tw(&t, &[w(Dim::Sidebar), bg(Paint::Sidebar)])").is_empty());
    assert!(!violations(app, "rect().tw(&t, &[w_full(), h_full(), bg(Paint::Bg)])").is_empty());
    assert!(!violations(app, "let bar = HeaderBar::new(title)\n    .trailing(menu);").is_empty());
    assert!(!violations(app, "let s = segmented(modes, key, pick);").is_empty());

    assert!(
        violations(
            "apps/x/src/snapshot_mode.rs",
            "rect().tw(&t, &[w_full(), h_full(), bg(Paint::Bg)])"
        )
        .is_empty()
    );
    assert!(violations(app, "Screen::new(HeaderBar::new(title)).content(body)").is_empty());
    assert!(violations(app, "HeaderBar::new(title)\n    .draggable(false)").is_empty());
    assert!(violations(app, "row(title).trailing(segmented(options, key, pick))").is_empty());
    assert!(
        violations(
            app,
            "row(title)\n    .trailing(\n        segmented(options, key, pick),"
        )
        .is_empty()
    );
    assert!(
        violations(
            app,
            "// boundary: value, the keypad's base\nlet s = segmented(o, k, p);"
        )
        .is_empty()
    );
}

#[test]
fn apps_keep_their_controls_on_the_right_side_of_the_boundary() {
    let mut problems = Vec::new();
    let mut scanned = Vec::new();
    for (relative, code) in workspace_code() {
        // The taxonomy shows components in specimen frames; it is not an app
        // window.
        if !relative.starts_with("apps/") || relative.starts_with("apps/taxonomy/") {
            continue;
        }
        scanned.push(relative.clone());
        let found = violations(&relative, &code);
        if PENDING.iter().any(|(file, _)| relative == *file) {
            if found.is_empty() {
                problems.push(format!("{relative}: clean now; remove it from PENDING"));
            }
        } else {
            problems.extend(found.iter().map(|problem| format!("{relative}: {problem}")));
        }
    }
    for (file, _) in PENDING {
        if !scanned.iter().any(|relative| relative.as_str() == *file) {
            problems.push(format!("{file}: listed in PENDING but not found"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
