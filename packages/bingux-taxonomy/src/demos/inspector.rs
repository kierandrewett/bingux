//! Inspector: the devtools window every bingux-ui application carries.
//!
//! The inspector is a window of its own, opened with a key chord over the
//! window it watches, so it cannot be embedded in a page. These specimens
//! describe each tool and what it acts on; open it over this window with
//! Ctrl+Alt+Shift+I to see it at work on the taxonomy itself.

use bingux_ui::prelude::*;
use freya::prelude::*;

use super::common::rows;
use crate::model::Spec;

fn key_row(title: &str, keys: &str, what: &str) -> Element {
    row(title).subtitle(what).mono_value(keys).into_element()
}

pub const OPEN: &[Spec] = &[
    Spec::forced("Keys", || {
        rows(vec![
            key_row(
                "Open or close the inspector",
                "Ctrl+Alt+Shift+I",
                "Over the window that has the keyboard focus, in any app",
            ),
            key_row(
                "Inspect mode",
                "Ctrl+Shift+C",
                "Point at an element to select it in the tree",
            ),
            key_row(
                "At start",
                "BINGUX_INSPECT_OPEN=1",
                "Opens an inspector over the first window, for tests",
            ),
        ])
    }),
    Spec::forced("Cost while closed", || {
        status(Status::Info(
            "No inspector, no cost: nothing is captured, timed or sampled.".into(),
        ))
        .into_element()
    }),
];

pub const TOOLS: &[Spec] = &[Spec::forced("The tools", || {
    rows(vec![
            row("Elements").subtitle("The element tree of the window, as a tree_view; hover to highlight, press to select").leading(Icon::Display).into_element(),
            row("Properties").subtitle("Computed style, box model and the token each value is; edit a prop and see it live").leading(Icon::PreferencesSystem).into_element(),
            row("Performance").subtitle("Frame times as a timeline chart and a sampled flame graph; long frames are named").leading(Icon::Clock).into_element(),
            row("Accessibility").subtitle("The accessibility tree with checks for missing names and roles").leading(Icon::Accessibility).into_element(),
            row("Phone preview").subtitle("Holds the watched window at 390 x 800, compact density, touch").leading(Icon::Desktop).into_element(),
        ])
})];
