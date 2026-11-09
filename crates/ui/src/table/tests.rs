//! What a table must keep doing, read from the laid-out frame: the rows follow
//! the sort, only the rows in view are built, and the keys move and extend the
//! selection.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use freya::prelude::*;

use super::{Column, SortDirection, data_table};
use crate::selection::Selection;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;

#[derive(Clone, Debug, PartialEq)]
struct Item {
    id: u32,
    name: String,
    size: u64,
}

fn items(count: u32) -> Vec<Item> {
    (0..count)
        .map(|id| Item {
            id,
            name: format!("Item {id:05}"),
            size: u64::from(id * 7919 % 1000),
        })
        .collect()
}

type Seen = Rc<RefCell<Vec<u32>>>;

struct Scene {
    count: u32,
    seen: Seen,
}

impl PartialEq for Scene {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count
    }
}

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let count = self.count;
        let rows = use_hook(move || Rc::new(items(count)));
        let columns = use_hook(|| {
            Rc::new(vec![
                Column::text("name", "Name", |item: &Item| item.name.clone())
                    .flex()
                    .priority(0)
                    .sort_by_text(),
                Column::text("size", "Size", |item: &Item| format!("{} KB", item.size))
                    .width(100.0)
                    .numeric()
                    .sort_by_key(|item: &Item| item.size),
            ])
        });
        let selection = use_state(Selection::<u32>::new);
        let mut ids: Vec<u32> = selection.read().iter().copied().collect();
        ids.sort_unstable();
        *self.seen.borrow_mut() = ids;
        rect().expanded().tw(&tokens, &[p(S4)]).child(
            data_table(&rows, columns, |item: &Item| item.id)
                .selection(selection)
                .sorted_by("name", SortDirection::Ascending),
        )
    }
}

fn open(count: u32) -> (Snapshot, Seen) {
    let seen: Seen = Rc::default();
    let shared = seen.clone();
    let mut shot = Snapshot::new(
        move || {
            Scene {
                count,
                seen: shared.clone(),
            }
            .into_element()
        },
        (640.0, 480.0),
    );
    shot.settle(Duration::from_millis(400));
    (shot, seen)
}

/// The text of every label, top to bottom as drawn.
fn texts(shot: &mut Snapshot) -> Vec<String> {
    let mut found: Vec<(f32, f32, String)> = shot.runner().find_many(|node, element| {
        let label = Label::try_downcast(element)?;
        let area = node.layout().area;
        Some((area.min_y(), area.min_x(), label.text.to_string()))
    });
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    found.into_iter().map(|(.., text)| text).collect()
}

/// The names of the rows in view, top to bottom.
fn names(shot: &mut Snapshot) -> Vec<String> {
    texts(shot)
        .into_iter()
        .filter(|text| text.starts_with("Item "))
        .collect()
}

#[test]
fn the_rows_follow_the_sort() {
    let (mut shot, _) = open(30);
    // Sorted by name to begin with.
    assert_eq!(
        names(&mut shot)[..3],
        ["Item 00000", "Item 00001", "Item 00002"]
    );

    // Pressing Size sorts by it, smallest first; the rows are drawn again, not
    // left showing what they showed before.
    assert!(shot.click_text("Size"));
    shot.settle(Duration::from_millis(300));
    let mut expected = items(30);
    expected.sort_by_key(|item| (item.size, item.id));
    let wanted: Vec<String> = expected
        .iter()
        .take(5)
        .map(|item| item.name.clone())
        .collect();
    assert_eq!(names(&mut shot)[..5], wanted[..]);

    // Pressing it again turns the sort round.
    assert!(shot.click_text("Size"));
    shot.settle(Duration::from_millis(300));
    expected.sort_by_key(|item| (std::cmp::Reverse(item.size), item.id));
    let wanted: Vec<String> = expected
        .iter()
        .take(5)
        .map(|item| item.name.clone())
        .collect();
    assert_eq!(names(&mut shot)[..5], wanted[..]);
}

#[test]
fn only_the_rows_in_view_are_built() {
    let (mut shot, _) = open(100_000);
    let built = names(&mut shot).len();
    assert!(
        (5..60).contains(&built),
        "a hundred thousand rows built {built} rows"
    );
}

#[test]
fn the_arrows_select_and_shift_extends_the_range() {
    let (mut shot, seen) = open(30);
    assert!(shot.click_text("Item 00002"));
    shot.settle(Duration::from_millis(200));
    assert_eq!(*seen.borrow(), [2]);

    shot.key(Key::Named(NamedKey::ArrowDown));
    shot.settle(Duration::from_millis(200));
    assert_eq!(*seen.borrow(), [3], "an arrow moves the selection");

    shot.key_with(Key::Named(NamedKey::ArrowDown), Modifiers::SHIFT);
    shot.key_with(Key::Named(NamedKey::ArrowDown), Modifiers::SHIFT);
    shot.settle(Duration::from_millis(200));
    assert_eq!(*seen.borrow(), [3, 4, 5], "Shift extends from the anchor");

    shot.key_with(Key::Character("a".into()), Modifiers::CONTROL);
    shot.settle(Duration::from_millis(200));
    assert_eq!(seen.borrow().len(), 30, "Ctrl+A selects every row");

    shot.key(Key::Named(NamedKey::Escape));
    shot.settle(Duration::from_millis(200));
    assert!(seen.borrow().is_empty(), "Escape clears the selection");
}

#[test]
fn the_selection_keeps_to_its_rows_through_a_sort() {
    let (mut shot, seen) = open(30);
    assert!(shot.click_text("Item 00004"));
    shot.settle(Duration::from_millis(200));
    assert!(shot.click_text("Size"));
    shot.settle(Duration::from_millis(300));
    assert_eq!(
        *seen.borrow(),
        [4],
        "sorting does not change what is selected"
    );
}
