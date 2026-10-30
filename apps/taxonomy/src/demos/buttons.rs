//! Controls: buttons, toggles, sliders and steppers.

use bingux_ui::controls::scrub::scrub;
use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::{LONG, rows, with};
use crate::model::Spec;

// ---- Buttons ---------------------------------------------------------------

pub const BUTTON: &[Spec] = &[
    Spec::live("Default", || action_button("Action", |_| {}).into_element()),
    Spec::live("Suggested", || {
        primary_button("Save", |_| {}).into_element()
    }),
    Spec::live("Destructive", || {
        action_button("Delete", |_| {}).destructive().into_element()
    }),
    Spec::live("Flat", || {
        action_button("Skip", |_| {}).flat().into_element()
    }),
    Spec::forced("With an icon", || {
        action_button("Open", |_| {})
            .icon(Icon::Folder)
            .into_element()
    }),
    Spec::forced("Loading", || {
        action_button("Saving", |_| {}).loading(true).into_element()
    }),
    Spec::forced("Disabled", || {
        action_button("Action", |_| {})
            .enabled(false)
            .into_element()
    }),
    Spec::forced("Long label", || action_button(LONG, |_| {}).into_element()),
    Spec::live("Compact, inside a row", || {
        let mut on = use_state(|| true);
        rows(vec![
            with("Default", action_button("Action", |_| {}).compact()),
            with("Icon button", icon_button(Icon::Refresh, |_| {}).tooltip("Refresh")),
            with(
                "Segmented",
                segmented(
                    vec![("a".into(), "One".into()), ("b".into(), "Two".into())],
                    "a",
                    |_| {},
                ),
            ),
            with("Switch", switch(on(), move |next: bool| on.set(next))),
        ])
    }),
    Spec::live("Row button", || {
        rows(vec![
            with("Change", row_button("Change", |_| {})),
            with("Prominent", row_button("Add", |_| {}).prominent()),
        ])
    }),
];

pub const ICON_BUTTON: &[Spec] = &[
    Spec::live("Rest, hover, pressed", || {
        icon_button(Icon::Refresh, |_| {})
            .tooltip("Refresh")
            .into_element()
    }),
    Spec::live("Danger", || {
        icon_button(Icon::Delete, |_| {})
            .tooltip("Delete")
            .danger()
            .into_element()
    }),
    Spec::live("Filled", || {
        icon_button(Icon::ListAdd, |_| {})
            .tooltip("Add")
            .filled()
            .into_element()
    }),
    Spec::forced("Active", || {
        icon_button(Icon::Filter, |_| {})
            .tooltip("Filter")
            .active(true)
            .into_element()
    }),
    Spec::forced("Busy", || {
        icon_button(Icon::Refresh, |_| {})
            .tooltip("Refreshing")
            .busy(true)
            .into_element()
    }),
    // Press it: the reload arrows turn, shrink and fade as the stop square
    // turns in, and back. The name follows the state, not the motion.
    Spec::live("Morph: Reload and Stop", || {
        let mut stopping = use_state(|| false);
        icon_button(Icon::Refresh, move |_| stopping.set(!stopping()))
            .morph(Icon::MediaPlaybackStop, stopping())
            .tooltip(if stopping() { "Stop (Esc)" } else { "Refresh (F5)" })
            .into_element()
    }),
    Spec::forced("Morph, stopped", || {
        icon_button(Icon::Refresh, |_| {})
            .morph(Icon::MediaPlaybackStop, true)
            .tooltip("Stop (Esc)")
            .into_element()
    }),
    Spec::forced("Disabled", || {
        icon_button(Icon::OpenMenu, |_| {})
            .enabled(false)
            .into_element()
    }),
];

fn view_items(view: State<&'static str>) -> Vec<MenuItem> {
    let pick = |name: &'static str, label: &'static str| {
        let mut view = view;
        menu_item(label, move |_| view.set(name)).checked(view() == name)
    };
    vec![pick("list", "List"), pick("grid", "Grid")]
}

pub const SPLIT_BUTTON: &[Spec] = &[
    Spec::live("Press the icon to switch, the chevron to pick", || {
        let mut view = use_state(|| "list");
        split_button(if view() == "list" {
            Icon::ViewList
        } else {
            Icon::ViewGrid
        })
        .tooltip("Switch view")
        .menu_label("View options")
        .on_press(move |_| view.set(if view() == "list" { "grid" } else { "list" }))
        .menu(view_items(view))
        .into_element()
    })
    .script("click-name View options; wait 300; save {out}"),
    Spec::live("With a label", || {
        split_button(Icon::Folder)
            .label("Open")
            .menu_label("Open options")
            .menu(vec![
                menu_item("Open in new window", |_| {}),
                menu_item("Open as administrator", |_| {}),
            ])
            .into_element()
    }),
    Spec::forced("Disabled", || {
        split_button(Icon::ViewList)
            .tooltip("Switch view")
            .menu(vec![menu_item("List", |_| {})])
            .enabled(false)
            .into_element()
    }),
];

pub const ACTION_STRIP: &[Spec] = &[Spec::live("Three actions, one disabled", || {
    action_strip(vec![
        strip_action(Icon::Delete, "Forget", |_| {}),
        strip_action(Icon::WindowClose, "Disconnect", |_| {}),
        strip_action(Icon::EmblemSystem, "Settings", |_| {}).enabled(false),
    ])
    .into_element()
})];

// ---- Toggles ---------------------------------------------------------------

pub const SWITCH: &[Spec] = &[
    Spec::live("On, hover, pressed", || {
        let mut on = use_state(|| true);
        switch(on(), move |next| on.set(next)).into_element()
    }),
    Spec::live("Off", || {
        let mut on = use_state(|| false);
        switch(on(), move |next| on.set(next)).into_element()
    }),
    Spec::forced("Disabled on", || {
        switch(true, |_| {}).enabled(false).into_element()
    }),
    Spec::forced("Disabled off", || {
        switch(false, |_| {}).enabled(false).into_element()
    }),
    Spec::live("As a row", || {
        let mut on = use_state(|| true);
        rows(vec![
            switch_row("Enabled", on(), move |next| on.set(next)).into_element(),
            switch_row(LONG, true, |_| {})
                .subtitle("A subtitle under a long title")
                .into_element(),
        ])
    }),
];

pub const CHECKBOX: &[Spec] = &[
    Spec::live("Unchecked, hover, pressed", || {
        let mut check = use_state(|| CheckState::Off);
        checkbox(check(), move |next: bool| check.set(next.into()))
            .text("Remember")
            .into_element()
    }),
    Spec::live("Checked", || {
        let mut check = use_state(|| CheckState::On);
        checkbox(check(), move |next: bool| check.set(next.into()))
            .text("Remember")
            .into_element()
    }),
    Spec::forced("Mixed", || {
        checkbox(CheckState::Mixed, |_| {})
            .text("Some selected")
            .into_element()
    }),
    Spec::forced("Disabled", || {
        checkbox(true, |_| {})
            .text("Locked")
            .enabled(false)
            .into_element()
    }),
    Spec::forced("Long label", || {
        checkbox(false, |_| {}).text(LONG).into_element()
    }),
];

pub const RADIO: &[Spec] = &[
    Spec::live("Radio rows", || {
        let mut radio = use_state(|| "medium".to_string());
        radio_rows(
            Some("Text size".to_string()),
            vec![
                radio_row("Small", radio.read().as_str() == "small", move |_: ()| {
                    radio.set("small".to_string())
                })
                .subtitle("Compact"),
                radio_row("Medium", radio.read().as_str() == "medium", move |_: ()| {
                    radio.set("medium".to_string())
                }),
                radio_row("Large", radio.read().as_str() == "large", move |_: ()| {
                    radio.set("large".to_string())
                })
                .subtitle("Easier to read"),
            ],
        )
        .into_element()
    }),
    Spec::forced("Disabled", || {
        radio_rows(
            None,
            vec![
                radio_row("Unavailable", false, |_: ()| {}).enabled(false),
                radio_row("Selected but unavailable", true, |_: ()| {}).enabled(false),
            ],
        )
        .into_element()
    }),
];

// ---- Sliders and steppers --------------------------------------------------

pub const SLIDER: &[Spec] = &[
    Spec::live("Rest, hover, drag", || {
        let mut volume = use_state(|| 62.0_f64);
        slider(volume(), move |next| volume.set(next))
            .fill()
            .into_element()
    }),
    Spec::live("Stepped, with its value", || {
        let mut stepped = use_state(|| 40.0_f64);
        slider(stepped(), move |next| stepped.set(next))
            .step(10.0)
            .show_value()
            .fill()
            .into_element()
    }),
    Spec::forced("Mark and danger zone", || {
        slider(85.0, |_| {})
            .mark(100.0)
            .danger_from(90.0)
            .fill()
            .into_element()
    }),
    Spec::forced("Disabled", || {
        slider(30.0, |_| {}).enabled(false).fill().into_element()
    }),
    Spec::live("As a row", || {
        let mut volume = use_state(|| 62.0_f64);
        rows(vec![
            slider_row("Volume", volume(), move |next| volume.set(next)).into_element(),
        ])
    }),
];

pub const STEPPER: &[Spec] = &[
    Spec::live("Rest, hover, pressed", || {
        let mut count = use_state(|| 3_i64);
        stepper(count(), move |next| count.set(next))
            .range(0, 10)
            .suffix(" items")
            .into_element()
    }),
    Spec::forced("At its limit", || {
        stepper(10, |_| {})
            .range(0, 10)
            .suffix(" items")
            .into_element()
    }),
    Spec::forced("Disabled", || {
        stepper(4, |_| {}).enabled(false).into_element()
    }),
];

pub const SCRUB: &[Spec] = &[
    Spec::live("Drag the label to change the value", || {
        let mut value = use_state(|| 50.0_f64);
        scrub(format!("{:.0}", value()), move |delta| {
            value.set(value() + delta)
        })
        .into_element()
    }),
    Spec::forced("Disabled", || {
        scrub("50", |_| {}).enabled(false).into_element()
    }),
];
