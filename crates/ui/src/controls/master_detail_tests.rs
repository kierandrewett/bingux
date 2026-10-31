//! The effect stack in motion: a master list of reorderable rows (a switch and a delete button
//! each) beside a detail page, played one 16 ms frame at a time on a headless window.
//!
//! The invariants: the detail page never moves while the list is toggled, a row is deleted,
//! another row is selected or a row is dragged to a new place; a dragged row starts from where it
//! is drawn and, interrupted, carries on from where it is drawn.

use std::time::Duration;

use freya::prelude::*;
use freya_core::element::ElementExt;

use super::master_detail;
use crate::controls::{icon_button, sortable, switch};
use crate::icons::Icon;
use crate::rows::row;
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::surface::use_tokens;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

const FRAME: Duration = Duration::from_millis(16);
const DESKTOP: (f32, f32) = (1000.0, 560.0);
const FOOTER: &str = "Using Linux Studio Plugins";

#[derive(Clone, PartialEq)]
struct Stack;

impl Component for Stack {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut names = use_state(|| {
            vec!["Equalizer", "Compressor", "Limiter", "Reverb"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        });
        let mut selected = use_state(|| "Equalizer".to_string());
        let mut enabled = use_state(|| vec![true; 4]);
        let list = names.read().clone();
        let rows = list
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let (press, remove) = (name.clone(), name.clone());
                let tip = format!("Remove {name}");
                let on = enabled.read().get(index).copied().unwrap_or(true);
                row(name.clone())
                    .selected(*selected.read() == *name)
                    .trailing(
                        rect()
                            .tw(&tokens, &[flex_row(), gap(S1)])
                            .child(switch(on, move |next: bool| {
                                if let Some(slot) = enabled.write().get_mut(index) {
                                    *slot = next;
                                }
                            }))
                            .child(
                                icon_button(Icon::Delete, move |_| {
                                    let mut list = names.write();
                                    if let Some(at) = list.iter().position(|item| *item == remove) {
                                        list.remove(at);
                                        enabled.write().remove(at.min(3));
                                    }
                                })
                                .tooltip(&tip),
                            ),
                    )
                    .on_press(move |_| selected.set(press.clone()))
                    .into()
            })
            .collect();
        let master = sortable(rows, move |(from, to): (usize, usize)| {
            let mut list = names.write();
            let item = list.remove(from);
            list.insert(to, item);
        })
        .labels(list.clone());
        let detail = rect()
            .tw(&tokens, &[flex_col(), gap(S4), p(S4)])
            .child(crate::typography::styled_label(
                crate::typography::BODY,
                &format!("Page of {}", selected.read()),
            ))
            .child(crate::typography::styled_label(
                crate::typography::BODY,
                FOOTER,
            ));
        WindowHost::new(master_detail(master, detail, true, |()| {}))
    }
}

#[derive(Clone, PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        Stack
    }
}

fn start() -> Snapshot {
    let mut shot = Snapshot::new(|| Root.into_element(), DESKTOP);
    shot.settle(Duration::from_millis(400));
    shot
}

fn text_of(element: &dyn ElementExt) -> Option<String> {
    if let Some(label) = Label::try_downcast(element) {
        return Some(label.text.to_string());
    }
    Paragraph::try_downcast(element).map(|paragraph| {
        paragraph
            .spans
            .iter()
            .map(|span| span.text.to_string())
            .collect()
    })
}

fn label(shot: &mut Snapshot, text: &str) -> Option<torin::prelude::Area> {
    shot.runner()
        .find_many(|node, element| (text_of(element)? == text).then(|| node.layout().area))
        .into_iter()
        .next()
}

fn mid(area: &torin::prelude::Area) -> (f32, f32) {
    (
        area.min_x() + area.width() / 2.0,
        area.min_y() + area.height() / 2.0,
    )
}

fn named(shot: &mut Snapshot, name: &str) -> Option<(f32, f32)> {
    shot.runner()
        .find_many(|node, element| {
            (element.accessibility().builder.label() == Some(name))
                .then(|| mid(&node.layout().area))
        })
        .into_iter()
        .next()
}

fn footer(shot: &mut Snapshot) -> (f32, f32) {
    let area = label(shot, FOOTER).expect("the page's footer is drawn");
    (area.min_x(), area.min_y())
}

fn frames(shot: &mut Snapshot, count: u32) {
    shot.runner().poll_n(FRAME, count);
}

/// Play `frames` frames and return the footer's place in each.
fn watch(shot: &mut Snapshot, count: u32) -> Vec<(f32, f32)> {
    (0..count)
        .map(|_| {
            frames(shot, 1);
            footer(shot)
        })
        .collect()
}

fn assert_still(places: &[(f32, f32)], home: (f32, f32), what: &str) {
    assert!(
        places
            .iter()
            .all(|place| (place.0 - home.0).abs() < 0.01 && (place.1 - home.1).abs() < 0.01),
        "{what}: the page moved: {places:?}"
    );
}

#[test]
fn switching_an_effect_off_moves_nothing_else() {
    let mut shot = start();
    let home = footer(&mut shot);
    let rows_before = label(&mut shot, "Compressor").expect("a row").min_y();
    let at = shot.runner().find_many(|node, element| {
        (element.accessibility().builder.role() == accesskit::Role::Switch)
            .then(|| mid(&node.layout().area))
    });
    let first = at
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a switch");
    shot.click(first.0, first.1);
    let seen = watch(&mut shot, 14);
    assert_still(&seen, home, "toggle");
    assert!(
        (label(&mut shot, "Compressor").expect("a row").min_y() - rows_before).abs() < 0.01,
        "a row moved on a toggle"
    );
}

#[test]
fn selecting_another_effect_changes_the_page_and_moves_nothing() {
    let mut shot = start();
    let home = footer(&mut shot);
    let target = label(&mut shot, "Limiter").expect("a row");
    let at = mid(&target);
    shot.click(at.0, at.1);
    let seen = watch(&mut shot, 14);
    assert_still(&seen, home, "select");
    assert!(
        label(&mut shot, "Page of Limiter").is_some(),
        "the page shows the selected effect"
    );
}

#[test]
fn deleting_an_effect_closes_its_row_and_leaves_the_page_where_it_is() {
    let mut shot = start();
    let home = footer(&mut shot);
    let button = named(&mut shot, "Remove Compressor").expect("a delete button");
    shot.click(button.0, button.1);
    let seen = watch(&mut shot, 20);
    assert_still(&seen, home, "delete");
    assert!(label(&mut shot, "Compressor").is_none(), "the row is gone");
    assert!(label(&mut shot, "Limiter").is_some() && label(&mut shot, "Reverb").is_some());
}

#[test]
fn dragging_a_row_starts_where_it_is_drawn_and_an_interruption_carries_on_from_there() {
    let mut shot = start();
    let home = footer(&mut shot);
    let first = label(&mut shot, "Equalizer").expect("a row").min_y();
    let grip = named(&mut shot, "Reorder Equalizer").expect("a grip");
    shot.press(grip);
    let mut held = Vec::new();
    let mut page = Vec::new();
    for step in 1..=12 {
        shot.move_to((grip.0, grip.1 + 70.0 * step as f32 / 12.0));
        frames(&mut shot, 1);
        held.push(label(&mut shot, "Equalizer").expect("the held row").min_y());
        page.push(footer(&mut shot));
    }
    assert_still(&page, home, "drag");
    assert!(
        (held[0] - first).abs() < 8.0,
        "the first frame of the drag starts at the row's place: {held:?}"
    );
    assert!(
        held.windows(2).all(|pair| pair[1] >= pair[0] - 0.01),
        "the held row follows the pointer down: {held:?}"
    );
    // Escape in the middle: the row goes home from where it is drawn, in steps, never jumping.
    let at = *held.last().expect("frames");
    shot.key(Key::Named(NamedKey::Escape));
    let mut back = vec![at];
    for _ in 0..24 {
        frames(&mut shot, 1);
        back.push(label(&mut shot, "Equalizer").expect("the row").min_y());
    }
    assert!(
        back.windows(2).all(|pair| (pair[1] - pair[0]).abs() < 40.0),
        "the row jumped on the way home: {back:?}"
    );
    assert!(
        (back.last().unwrap() - first).abs() < 1.0,
        "it settles at home: {back:?}"
    );
    assert_still(&watch(&mut shot, 4), home, "after the interruption");
}
