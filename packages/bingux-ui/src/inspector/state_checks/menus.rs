//! The menu keyboard walk (CK-16) and the open state of a menu button
//! (ST-06).
//!
//! Every dropdown and menu button of the frame is opened with the keyboard
//! (focus it, press Enter) and walked:
//!
//! - while it is open the button keeps the look it has under the pointer
//!   (`state-open`);
//! - Home highlights the first row that can be picked and End the last;
//!   Down and Up move one row (`menu-keys`);
//! - typing a row's first letters highlights a row that contains them
//!   (MN-02) and the query is still there after a pause (MN-03);
//! - the first Escape takes back the query and leaves the menu open, the next
//!   closes it and focus is back on the button (FC-11, MN-01);
//! - in a dropdown, Enter picks the highlighted row and closes the list.
//!
//! The highlight is read from the rows' own fills: a row a menu highlights is
//! painted, the others are not. A row that is painted both at Home and at End
//! is the selected one.

use std::time::Duration;

use freya::prelude::NamedKey;
use torin::prelude::Area;

use super::rig::Rig;
use super::view::{Target, text_in, visible_box};
use super::{Report, Rule};
use crate::inspector::snapshot::Snapshot as Tree;

/// A list this long or shorter is on screen whole, so its rows are known.
const WHOLE_LIST: usize = 12;

/// The most menus walked in one frame: each is a handful of renders.
const MAX_MENUS: usize = 6;

/// How long the type-ahead query is left alone before it is read again: longer
/// than the two seconds it used to time out after (MN-03).
const PAUSE: Duration = Duration::from_millis(2500);

/// One row of an open menu.
#[derive(Clone, Debug)]
struct Row {
    text: String,
    /// Not dimmed: it can be highlighted and picked.
    selectable: bool,
    /// Painted: highlighted, or the selected option.
    filled: bool,
}

/// The rows of every open menu or list in `tree`, in tree order: the text,
/// whether it can be picked, and its visible box.
fn listed(tree: &Tree, window: Area) -> Vec<(String, bool, Area)> {
    tree.nodes
        .iter()
        .filter(|node| node.a11y.role == "MenuItem" && !node.used.hidden)
        .filter(|node| matches!(node.a11y.component.as_deref(), Some("MenuRow" | "DropdownItem")))
        .filter_map(|node| {
            let text = text_in(tree, node)?;
            let props = node.props.as_ref()?;
            let visible = visible_box(tree, node, window)?;
            Some((text, props.opacity.is_none_or(|opacity| opacity > 0.9), visible))
        })
        .collect()
}

/// Whether a menu or list is open.
pub(super) fn is_open(rig: &mut Rig) -> bool {
    let window = rig.window;
    !listed(&rig.tree(), window).is_empty()
}

/// The rows of every open menu or list, with which are painted: a highlighted
/// row, or the selected option, has a fill that differs from the panel's.
///
/// A list takes its rows and its height over a few frames (the rows are built
/// for the part in view, then the height follows what was built, and a key
/// that scrolls it builds more), so the rows are read once two frames agree.
fn rows(rig: &mut Rig) -> Vec<Row> {
    let window = rig.window;
    let mut listed = listed(&rig.tree(), window);
    if listed.is_empty() {
        return Vec::new();
    }
    for _ in 0..8 {
        rig.settle(super::rig::SETTLE);
        let next = self::listed(&rig.tree(), window);
        let steady = next == listed;
        listed = next;
        if steady {
            break;
        }
    }
    let frame = rig.frame();
    if std::env::var_os("BINGUX_UI_STATE_TRACE").is_some() {
        for (text, selectable, area) in &listed {
            eprintln!(
                "[state] row {text:?} selectable {selectable} at {:.0},{:.0} {:.0}x{:.0} pixel {:?}",
                area.min_x(),
                area.min_y(),
                area.width(),
                area.height(),
                frame.pixel(area.min_x() + 3.0, area.center().y)
            );
        }
        eprintln!("[state] ---");
    }
    // A row is painted when its fill differs from the panel's own, which shows
    // in the 8 px padding beside it.
    listed
        .into_iter()
        .map(|(text, selectable, area)| {
            let y = area.center().y;
            let (row, panel) = (frame.pixel(area.min_x() + 3.0, y), frame.pixel(area.min_x() - 4.0, y));
            Row { text, selectable, filled: row.iter().zip(&panel).any(|(a, b)| a.abs_diff(*b) >= 4) }
        })
        .collect()
}

/// Whether the control is one that opens a menu or a list.
fn opens_menu(target: &Target) -> bool {
    target.role == "ComboBox"
        || matches!(target.component.as_str(), "MenuButton" | "IconMenuButton" | "SplitButtonMenu")
}

fn dropdown(target: &Target) -> bool {
    target.role == "ComboBox"
}

/// The highlighted rows: painted, and not painted at both Home and End.
fn lit(rows: &[Row], selected: &[String]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.filled && !selected.contains(&row.text))
        .map(|row| row.text.clone())
        .collect()
}

pub(super) fn run(rig: &mut Rig, ids: &[u64], report: &mut Report) {
    let targets = rig.targets();
    let mut walked = 0;
    let mut paused = false;
    for target in targets.iter().filter(|target| opens_menu(target) && target.whole()) {
        if walked >= MAX_MENUS {
            report.note(format!("menu walk stopped after {MAX_MENUS} menus; more are in the frame"));
            break;
        }
        rig.park();
        // The button under the pointer, nothing open: the look the open one keeps.
        rig.hover(target.centre());
        let hovered = rig.frame();
        rig.park();
        // Open it from the keyboard.
        rig.focus(target.focus);
        rig.named(NamedKey::Enter);
        let mut open = is_open(rig);
        if !open {
            rig.named(NamedKey::ArrowDown);
            open = is_open(rig);
        }
        if !open {
            report.note(format!("{} did not open from Enter or Down", target.label()));
            rig.restore(ids, 3);
            continue;
        }
        walked += 1;
        report.menus += 1;

        // ST-06: the button holds its hover look while its menu is open.
        report.measure(Rule::Open);
        let opened = rig.frame();
        // The fill beside the label, where no glyph or ring is.
        let at = (target.visible.min_x() + 3.0, target.visible.center().y);
        let (under_pointer, while_open) = (hovered.pixel(at.0, at.1), opened.pixel(at.0, at.1));
        let apart = under_pointer.iter().zip(&while_open).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        if apart >= super::frame::VISIBLE_DELTA && !rig.compact() {
            opened.keep(&format!("open-{}", target.label()));
            hovered.keep(&format!("hovered-{}", target.label()));
            report.push(
                Rule::Open,
                target.label(),
                format!(
                    "while its menu is open the button is {while_open:?}, not the {under_pointer:?} it is under the pointer: it loses its hover fill"
                ),
                target.rect(),
            );
        }

        walk(rig, target, &mut paused, report);
        rig.park();
        if !rig.restore(ids, 3) {
            report.note(format!("stopped after walking {}: the window changed", target.label()));
            return;
        }
    }
}

/// The keyboard walk of the menu that `target` has open.
fn walk(rig: &mut Rig, target: &Target, paused: &mut bool, report: &mut Report) {
    let label = target.label();
    let fail = |report: &mut Report, what: String| {
        report.push(Rule::MenuKeys, label.clone(), what, target.rect());
    };
    let current = |rig: &mut Rig| rows(rig);

    let all = current(rig);
    let pickable: Vec<&Row> = all.iter().filter(|row| row.selectable).collect();
    if pickable.len() < 2 {
        report.note(format!("{label}: fewer than two rows to walk"));
        rig.named(NamedKey::Escape);
        return;
    }
    report.measure(Rule::MenuKeys);
    // On a phone a menu is a sheet, whose rows take no keyboard highlight
    // fill (a finger has no cursor): only the open and close are held.
    if rig.compact() {
        return finish(rig, target, report, label.clone());
    }

    // Home and End move the highlight, to the two ends. Which row is lit is
    // read from the fills: a list the window shows whole (a dozen rows or
    // fewer) is held to the exact row, a longer one scrolls and is held to
    // the highlight moving.
    rig.named(NamedKey::Home);
    let at_home = current(rig);
    rig.named(NamedKey::End);
    let at_end = current(rig);
    let selected: Vec<String> = at_home
        .iter()
        .filter(|row| row.filled && at_end.iter().any(|other| other.filled && other.text == row.text))
        .map(|row| row.text.clone())
        .collect();
    let first = at_home.iter().find(|row| row.selectable).map(|row| row.text.clone());
    let last = at_end.iter().rev().find(|row| row.selectable).map(|row| row.text.clone());
    let lit_home = lit(&at_home, &selected);
    let lit_end = lit(&at_end, &selected);
    // A highlight on the selected row paints nothing more than the selection
    // does, so it reads as no extra row lit.
    let holds = |lit: &[String], want: &Option<String>| {
        want.as_ref().is_none_or(|want| lit.contains(want) || (lit.is_empty() && selected.contains(want)))
    };
    let whole = at_home.len() <= WHOLE_LIST;
    if lit_home == lit_end {
        fail(report, "Home and End highlight the same row: they do not move the highlight".to_owned());
    } else if whole {
        if !holds(&lit_home, &first) {
            rig.frame().keep(&format!("menu-home-{label}"));
            fail(report, format!("Home highlights {lit_home:?}, expected the first pickable row {first:?}"));
        }
        if !holds(&lit_end, &last) {
            rig.frame().keep(&format!("menu-end-{label}"));
            fail(report, format!("End highlights {lit_end:?}, expected the last pickable row {last:?}"));
        }
    }

    // Down from Home moves the highlight on and Up brings it back.
    rig.named(NamedKey::Home);
    let home_again = lit(&current(rig), &selected);
    rig.named(NamedKey::ArrowDown);
    let after_down = current(rig);
    let down = lit(&after_down, &selected);
    if down == home_again {
        fail(report, "Down from the first row does not move the highlight".to_owned());
    } else if whole {
        let second = after_down.iter().filter(|row| row.selectable).nth(1).map(|row| row.text.clone());
        if !holds(&down, &second) {
            fail(report, format!("Down from the first row highlights {down:?}, expected the second {second:?}"));
        }
    }
    rig.named(NamedKey::ArrowUp);
    let up = lit(&current(rig), &selected);
    if up != home_again {
        fail(report, format!("Up from the second row highlights {up:?}, not the first row's {home_again:?}"));
    }

    // Type-ahead: the first two letters of a later row, then a pause.
    let probe = pickable.iter().rev().find(|row| !selected.contains(&row.text)).map(|row| row.text.clone());
    if let Some(probe) = probe {
        let query: String = probe.chars().filter(|c| c.is_alphanumeric()).take(2).collect::<String>().to_lowercase();
        if query.chars().count() == 2 {
            rig.named(NamedKey::Home);
            rig.type_text(&query);
            let found = lit(&current(rig), &selected);
            let matches = |text: &str| text.to_lowercase().contains(&query);
            // A highlight on the selected row paints nothing more than the
            // selection, so it reads as no extra row lit.
            let on_selected = found.is_empty() && selected.iter().any(|text| matches(text));
            if !on_selected && !found.iter().any(|text| matches(text)) {
                fail(report, format!("typing {query:?} highlights {found:?}: no row that contains it"));
            } else if !*paused {
                *paused = true;
                rig.shot.runner().poll(Duration::from_millis(16), PAUSE);
                let later = lit(&current(rig), &selected);
                let kept = later.iter().any(|text| matches(text)) || (later.is_empty() && selected.iter().any(|text| matches(text)));
                if !kept && is_open(rig) {
                    fail(report, format!("the query {query:?} is gone after a {:?} pause (MN-03): the highlight went to {later:?}", PAUSE));
                }
            }
            // First Escape takes the query back and leaves the menu open.
            rig.named(NamedKey::Escape);
            if !is_open(rig) {
                fail(report, "the first Escape after typing closes the menu: it should take back the query first".to_owned());
                return finish(rig, target, report, label.clone());
            }
        }
    }

    // Enter on a dropdown picks the highlighted row and closes the list. A
    // dropdown in a header is a window setting (theme, density, scale) whose
    // pick rebuilds the window, so it is not picked.
    if dropdown(target) && !target.path.contains("HeaderBar") {
        rig.named(NamedKey::Home);
        rig.named(NamedKey::ArrowDown);
        rig.named(NamedKey::Enter);
        rig.settle(super::rig::SETTLE * 2);
        if is_open(rig) {
            fail(report, "Enter on a highlighted row does not close the list".to_owned());
            rig.named(NamedKey::Escape);
        } else if rig.focused() != target.focus {
            fail(report, "after Enter the focus is not back on the dropdown button".to_owned());
        }
        return;
    }
    // Escape closes it, and focus is back on the button.
    finish(rig, target, report, label);
}

/// Escape closes the menu and gives the focus back to its button.
fn finish(rig: &mut Rig, target: &Target, report: &mut Report, label: String) {
    rig.named(NamedKey::Escape);
    rig.settle(super::rig::SETTLE * 2);
    if is_open(rig) {
        report.push(Rule::MenuKeys, label, "Escape does not close the menu".to_owned(), target.rect());
        return;
    }
    if rig.focused() != target.focus {
        report.push(
            Rule::MenuKeys,
            label,
            "after Escape the focus is not back on the button that opened the menu".to_owned(),
            target.rect(),
        );
    }
}
