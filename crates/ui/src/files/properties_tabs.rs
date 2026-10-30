//! The tabs of the Properties dialog: which exist for what is selected, which
//! one opens, and the keys that switch between them.
//!
//! The model is plain data so it is tested without a window. The tab chosen
//! last is remembered for the session, so Properties opens where the person
//! left it: someone who checks permissions checks them again.

use std::cell::Cell;

use freya::prelude::*;

/// One page of the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    /// What it is and where: picture, name, type, size, location, drive,
    /// times, and the applications that open it.
    Basic,
    /// Who may read, write and execute it, and who owns it.
    Permissions,
    /// The facts of a picture or recording, the checksum, the extended
    /// attributes, and the numbers the file system keeps.
    Details,
    /// A folder's item count, size and largest items.
    Contents,
}

impl Tab {
    /// The key a segmented control tells apart tabs by.
    pub(super) fn key(self) -> &'static str {
        match self {
            Tab::Basic => "basic",
            Tab::Permissions => "permissions",
            Tab::Details => "details",
            Tab::Contents => "contents",
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Tab::Basic => "Basic",
            Tab::Permissions => "Permissions",
            Tab::Details => "Details",
            Tab::Contents => "Contents",
        }
    }

    pub(super) fn from_key(key: &str) -> Option<Tab> {
        [Tab::Basic, Tab::Permissions, Tab::Details, Tab::Contents]
            .into_iter()
            .find(|tab| tab.key() == key)
    }
}

/// The tabs for a selection, in order. Several items have Basic and
/// Permissions, which apply to all of them. One item adds Details, and one
/// folder adds Contents.
pub(super) fn available(single: bool, is_folder: bool) -> Vec<Tab> {
    let mut tabs = vec![Tab::Basic, Tab::Permissions];
    if single {
        tabs.push(Tab::Details);
        if is_folder {
            tabs.push(Tab::Contents);
        }
    }
    tabs
}

/// The tab to open: the one remembered, or Basic when this selection has no
/// such tab (a file after a folder's Contents).
pub(super) fn pick(remembered: Tab, tabs: &[Tab]) -> Tab {
    if tabs.contains(&remembered) {
        remembered
    } else {
        Tab::Basic
    }
}

thread_local! {
    /// The tab a person chose last, for the life of the process.
    static REMEMBERED: Cell<Tab> = const { Cell::new(Tab::Basic) };
}

pub(super) fn remembered() -> Tab {
    REMEMBERED.with(Cell::get)
}

/// Remember `tab`, which a person chose.
pub(super) fn remember(tab: Tab) {
    REMEMBERED.with(|cell| cell.set(tab));
}

/// Whether a tab bar of `count` equal segments fits in `width` px when its
/// widest title is `widest` px: each segment is its title between two
/// `pad`s, with `gap` between them and `gap` around them all. A bar that fits
/// fills the card; one that does not scrolls sideways.
pub(super) fn fits_filled(widest: f32, count: usize, width: f32, pad: f32, gap: f32) -> bool {
    let each = widest + 2.0 * pad;
    each * count as f32 + gap * (count.saturating_sub(1) + 2) as f32 <= width
}

/// The tab a key press switches to, from `current` among `tabs`: Ctrl+1 to
/// Ctrl+9 for the first to the ninth, Ctrl+Tab for the next and
/// Ctrl+Shift+Tab for the previous, both wrapping round. A digit with no tab
/// switches nothing.
pub(super) fn shortcut(key: &Key, modifiers: Modifiers, tabs: &[Tab], current: Tab) -> Option<Tab> {
    if !modifiers.contains(Modifiers::CONTROL) || modifiers.intersects(Modifiers::ALT | Modifiers::META) {
        return None;
    }
    let index = tabs.iter().position(|tab| *tab == current)?;
    match key {
        Key::Named(NamedKey::Tab) => {
            let count = tabs.len();
            let next = if modifiers.contains(Modifiers::SHIFT) {
                (index + count - 1) % count
            } else {
                (index + 1) % count
            };
            Some(tabs[next])
        }
        Key::Character(text) => {
            let digit = text.chars().next().filter(|_| text.chars().count() == 1)?.to_digit(10)?;
            tabs.get((digit as usize).checked_sub(1)?).copied()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(text: &str) -> Key {
        Key::Character(text.into())
    }

    #[test]
    fn a_selection_has_the_tabs_that_apply_to_it() {
        assert_eq!(available(false, false), [Tab::Basic, Tab::Permissions]);
        assert_eq!(available(false, true), [Tab::Basic, Tab::Permissions], "several items have no Contents");
        assert_eq!(available(true, false), [Tab::Basic, Tab::Permissions, Tab::Details]);
        assert_eq!(
            available(true, true),
            [Tab::Basic, Tab::Permissions, Tab::Details, Tab::Contents]
        );
    }

    #[test]
    fn a_bar_fills_the_card_only_when_equal_segments_fit() {
        // Four segments of a 90 px title with 8 px of padding each side and
        // 4 px gaps: 4 * 106 + 4 * 5 = 444.
        assert!(fits_filled(90.0, 4, 444.0, 8.0, 4.0));
        assert!(!fits_filled(90.0, 4, 443.0, 8.0, 4.0));
        // Three fit where four do not.
        assert!(fits_filled(90.0, 3, 340.0, 8.0, 4.0));
        assert!(!fits_filled(90.0, 4, 340.0, 8.0, 4.0));
    }

    #[test]
    fn a_missing_tab_falls_back_to_basic() {
        let file = available(true, false);
        assert_eq!(pick(Tab::Permissions, &file), Tab::Permissions);
        assert_eq!(pick(Tab::Contents, &file), Tab::Basic);
        assert_eq!(pick(Tab::Details, &available(false, false)), Tab::Basic);
    }

    #[test]
    fn keys_round_trip_and_the_choice_is_remembered() {
        for tab in [Tab::Basic, Tab::Permissions, Tab::Details, Tab::Contents] {
            assert_eq!(Tab::from_key(tab.key()), Some(tab));
        }
        assert_eq!(Tab::from_key("nope"), None);
        remember(Tab::Details);
        assert_eq!(remembered(), Tab::Details);
        remember(Tab::Basic);
    }

    #[test]
    fn control_and_a_digit_pick_a_tab_by_place() {
        let tabs = available(true, true);
        let ctrl = Modifiers::CONTROL;
        assert_eq!(shortcut(&press("1"), ctrl, &tabs, Tab::Details), Some(Tab::Basic));
        assert_eq!(shortcut(&press("4"), ctrl, &tabs, Tab::Basic), Some(Tab::Contents));
        assert_eq!(shortcut(&press("5"), ctrl, &tabs, Tab::Basic), None, "no fifth tab");
        assert_eq!(shortcut(&press("0"), ctrl, &tabs, Tab::Basic), None);
        let two = available(false, false);
        assert_eq!(shortcut(&press("3"), ctrl, &two, Tab::Basic), None, "no Details for several items");
    }

    #[test]
    fn control_tab_wraps_in_both_directions() {
        let tabs = available(true, true);
        let next = Modifiers::CONTROL;
        let back = Modifiers::CONTROL | Modifiers::SHIFT;
        let tab = Key::Named(NamedKey::Tab);
        assert_eq!(shortcut(&tab, next, &tabs, Tab::Basic), Some(Tab::Permissions));
        assert_eq!(shortcut(&tab, next, &tabs, Tab::Contents), Some(Tab::Basic));
        assert_eq!(shortcut(&tab, back, &tabs, Tab::Basic), Some(Tab::Contents));
        assert_eq!(shortcut(&tab, back, &tabs, Tab::Details), Some(Tab::Permissions));
    }

    #[test]
    fn other_keys_switch_nothing() {
        let tabs = available(true, true);
        assert_eq!(shortcut(&press("2"), Modifiers::empty(), &tabs, Tab::Basic), None);
        assert_eq!(shortcut(&press("2"), Modifiers::ALT, &tabs, Tab::Basic), None);
        assert_eq!(shortcut(&press("2"), Modifiers::CONTROL | Modifiers::ALT, &tabs, Tab::Basic), None);
        assert_eq!(shortcut(&Key::Named(NamedKey::Tab), Modifiers::empty(), &tabs, Tab::Basic), None);
        assert_eq!(shortcut(&press("a"), Modifiers::CONTROL, &tabs, Tab::Basic), None);
        assert_eq!(shortcut(&press("12"), Modifiers::CONTROL, &tabs, Tab::Basic), None);
    }
}
