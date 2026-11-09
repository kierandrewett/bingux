// ftl-check: skip-file (the tests below pass IDs inside source fixtures, not UI strings)
//! The commands convention, checked over the workspace source. An app lists
//! its commands with `use_window_commands`, and its main menu with `app_menu`
//! (which adds the window's commands and About). It builds no shortcut sheet
//! of its own: the palette, the shortcuts sheet and the main menu already list
//! every command with its key, so a second list would drift from them.

use crate::source_scan::workspace_code;

/// Apps that still have a hand-built main menu or a shortcut sheet. Each is
/// removed when its visual rework lands; the test fails when one is clean.
const PENDING: &[(&str, &str)] = &[
    ("apps/files/src/app.rs", "a hand-built main menu"),
    (
        "apps/preview/src/app.rs",
        "a hand-built main menu and a shortcut sheet",
    ),
    (
        "apps/calendar/src/header.rs",
        "a hand-built main menu and a shortcut sheet",
    ),
];

/// The problems one file's code has with the convention.
fn violations(code: &str) -> Vec<&'static str> {
    let mut found = Vec::new();
    if code.contains("about::app_menu_items") {
        found.push("adds About by hand; app_menu already ends with it, so drop this call");
    }
    let sheet = code
        .lines()
        .any(|line| line.contains("dialog(") && line.to_ascii_lowercase().contains("shortcut"));
    if sheet {
        found.push("builds a shortcut sheet; the commands list their keys already");
    }
    found
}

#[test]
fn the_checks_catch_the_banned_forms() {
    assert!(!violations("items.extend(about::app_menu_items(overlays));").is_empty());
    assert!(!violations("show_dialog(dialog(title, shortcuts_text()))").is_empty());
    assert!(
        !violations(r#"show_dialog(dialog(tr!("calendar-keyboard-shortcuts"), SHORTCUTS))"#)
            .is_empty()
    );
    assert!(violations("app_menu(main_menu(overlays))").is_empty());
    assert!(violations(r#"show_dialog(dialog(tr!("settings-title"), body))"#).is_empty());
}

#[test]
fn apps_use_the_shared_commands_and_menu() {
    let mut problems = Vec::new();
    let mut scanned = Vec::new();
    for (relative, code) in workspace_code() {
        // The taxonomy keeps the API catalogue as text, which is not an app.
        if !relative.starts_with("apps/") || relative.starts_with("apps/taxonomy/") {
            continue;
        }
        scanned.push(relative.clone());
        let found = violations(&code);
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
