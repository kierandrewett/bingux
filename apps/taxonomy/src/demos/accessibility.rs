//! Foundations: what assistive technology hears, and the accessibility
//! preferences every component follows.

use bingux_ui::access::Access;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::rows;
use crate::model::Spec;

pub const SEMANTICS: &[Spec] = &[
    Spec::live("Controls named by their row", || {
        let mut on = use_state(|| true);
        let mut level = use_state(|| 60.0);
        rows(vec![
            switch_row("Wi-Fi", on(), move |next| on.set(next))
                .subtitle("A screen reader says: Wi-Fi, switch, on, Connected to Home")
                .into_element(),
            row("Volume")
                .trailing(slider(level(), move |next: f64| level.set(next)))
                .into_element(),
        ])
    }),
    Spec::live("Semantics on any element", || {
        rect()
            .semantics(
                Semantics::new()
                    .group("Battery")
                    .value("82 %, charging")
                    .hint("About 5 hours left"),
            )
            .child(label().text("82 % · charging"))
            .into_element()
    }),
    Spec::live("Announce without moving focus", || {
        let announcer = use_announcer();
        action_button("Copy link", move |_| announcer.announce("Link copied")).into_element()
    }),
];

pub const PREFERENCES: &[Spec] = &[Spec::forced("In force now", || {
    let access = Access::current();
    let word = |on: bool| if on { "On" } else { "Off" };
    rows(vec![
        row("Reduce motion")
            .value(word(access.reduced_motion))
            .into_element(),
        row("Increase contrast")
            .value(word(access.high_contrast))
            .into_element(),
        row("Reduce transparency")
            .value(word(access.reduce_transparency))
            .into_element(),
        row("Text size")
            .value(format!("{} %", access.text_percent()))
            .into_element(),
        row("Header menus")
            .value("Contrast and Text")
            .into_element(),
    ])
})];
