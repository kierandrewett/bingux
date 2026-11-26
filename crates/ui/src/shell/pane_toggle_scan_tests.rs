//! No code builds a sidebar or pane button by hand (`shell::pane_toggle`): a
//! toggle built from `icon_button` with a sidebar glyph could go in a slot of the
//! app's choosing, on the wrong side of its panel. Scanned over the workspace's
//! source, as `boundary_scan_tests` does for the design boundaries.

use crate::source_scan::workspace_code;

/// Code that may name a sidebar glyph: the icon table, the toggle that chooses
/// it, the taxonomy's category glyph (a picture of "navigation", not a
/// button) and the tests of the rule.
const ALLOWED: &[&str] = &[
    "crates/ui/src/icons.rs",
    "crates/ui/src/shell/pane_toggle.rs",
    "apps/taxonomy/src/model.rs",
    // The tests that say which glyph a side has, and this scan's own fixtures.
    "crates/ui/src/shell/pane_toggle_tests.rs",
    "crates/ui/src/shell/pane_toggle_scan_tests.rs",
];

/// The lines of `code` that name a sidebar glyph.
fn naming_a_sidebar_glyph(path: &str, code: &str) -> Vec<String> {
    if ALLOWED.contains(&path) {
        return Vec::new();
    }
    code.lines()
        .enumerate()
        .filter(|(_, line)| line.contains("Icon::SidebarShow"))
        .map(|(at, _)| {
            format!(
                "{path}:{}: names a sidebar glyph; build the button with `pane_toggle(side, ..)` and \
                 give it to `HeaderBar::pane_toggle` (or `pane_toggle_item`), so it is on its panel's side",
                at + 1
            )
        })
        .collect()
}

#[test]
fn no_code_builds_a_sidebar_button_by_hand() {
    let found: Vec<String> = workspace_code()
        .iter()
        .flat_map(|(path, code)| naming_a_sidebar_glyph(path, code))
        .collect();
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_scan_catches_a_hand_built_toggle() {
    let by_hand = "header.leading(icon_button(Icon::SidebarShow, f))";
    assert!(!naming_a_sidebar_glyph("apps/x/src/header.rs", by_hand).is_empty());
    let right = "icon_button(Icon::SidebarShowRight, f)";
    assert!(!naming_a_sidebar_glyph("crates/ui/src/files/chooser.rs", right).is_empty());
    assert!(naming_a_sidebar_glyph("crates/ui/src/shell/pane_toggle.rs", by_hand).is_empty());
    assert!(
        naming_a_sidebar_glyph("apps/x/src/header.rs", "pane_toggle(PanelSide::Start, f)")
            .is_empty()
    );
}
