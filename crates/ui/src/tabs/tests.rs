//! The tab keys in a window, headless: one Ctrl+T opens one tab, though the
//! window's command router and the tab strip's own shortcut both see the key.
//!
//! Failure modes guarded: Ctrl+T running the new-tab action twice (the
//! command router still ran after the shortcut answered the key).

use std::cell::Cell;
use std::time::Duration;

use freya::prelude::*;

use super::*;
use crate::commands::use_window_commands;
use crate::controls::action_button;
use crate::shell::overlays::{OverlayHost, use_overlays};
use crate::snapshot::Snapshot;
use crate::theme::provide_tokens;
use crate::window::{WindowHost, use_window};

thread_local! {
    static NEW_TAB_CALLS: Cell<u32> = const { Cell::new(0) };
    static TAB_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[derive(Clone, PartialEq)]
struct Page(u32);

impl TabContent for Page {
    fn duplicate(&self) -> Self {
        Page(self.0)
    }
}
#[derive(PartialEq)]
struct Scene;

impl Component for Scene {
    fn render(&self) -> impl IntoElement {
        let set = use_state(|| TabSet::new(Page(0)));
        let tabs = Tabs::new(set, use_window(), use_overlays());
        TAB_COUNT.with(|count| count.set(set.read().len()));
        let new_tab = move |_: ()| {
            NEW_TAB_CALLS.with(|calls| calls.set(calls.get() + 1));
            let next = set.read().len() as u32;
            tabs.open(Page(next));
        };
        use_window_commands(tabs.commands(new_tab));
        rect()
            .expanded()
            .on_key_down(tabs.shortcuts(new_tab))
            .child(action_button("Inside", |_| {}))
    }
}

#[derive(PartialEq)]
struct Root;

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let _tokens = provide_tokens();
        WindowHost::new(OverlayHost::new(Scene))
    }
}

fn window() -> Snapshot {
    NEW_TAB_CALLS.with(|calls| calls.set(0));
    let mut shot = Snapshot::new(|| Root.into_element(), (800.0, 600.0));
    shot.settle(Duration::from_millis(300));
    assert!(shot.click_text("Inside"));
    shot.settle(Duration::from_millis(200));
    shot
}

fn chord(shot: &mut Snapshot, key: &str, modifiers: Modifiers) {
    shot.key_with(Key::Character(key.to_owned()), modifiers);
    shot.settle(Duration::from_millis(300));
}

#[test]
fn one_ctrl_t_opens_one_tab() {
    let mut shot = window();
    assert_eq!(TAB_COUNT.with(Cell::get), 1);
    chord(&mut shot, "t", Modifiers::CONTROL);
    assert_eq!(
        NEW_TAB_CALLS.with(Cell::get),
        1,
        "Ctrl+T runs the new-tab action once"
    );
    assert_eq!(TAB_COUNT.with(Cell::get), 2, "the tab count grows by one");
}

#[test]
fn ctrl_w_closes_one_tab() {
    let mut shot = window();
    chord(&mut shot, "t", Modifiers::CONTROL);
    assert_eq!(TAB_COUNT.with(Cell::get), 2);
    chord(&mut shot, "w", Modifiers::CONTROL);
    assert_eq!(TAB_COUNT.with(Cell::get), 1, "Ctrl+W closes one tab");
}
