//! Platform: what the desktop tells bingux-ui, and what an application asks of
//! it. The pages read the real session, so they show what this machine has:
//! with no portal, no bus and no `gsettings` the rows say so and the window
//! still looks right.

use bingux_ui::platform::notify::Notification;
use bingux_ui::platform::{self, appearance, notify, open};
use bingux_ui::prelude::*;
use bingux_ui::runtime;
use freya::prelude::*;

use super::common::{rows, with};
use crate::model::Spec;

/// "Yes" or "No" for a row's value.
fn yes_no(present: bool) -> &'static str {
    if present { "Yes" } else { "No" }
}

pub const APPEARANCE_SOURCES: &[Spec] = &[Spec::forced("What the desktop says", || {
    let found = use_state(|| None::<appearance::Appearance>);
    use_hook(move || runtime::background(found, appearance::current));
    let mut group = Group::titled("Appearance");
    match found.read().as_ref() {
        None => {
            group = group.row(property_row("Reading", "Asking the desktop"));
        }
        Some(now) => {
            let from = &now.from;
            let line =
                |value: String, source: appearance::Source| format!("{value} ({})", source.name());
            group = group
                .row(property_row(
                    "Colour scheme",
                    line(format!("{:?}", now.scheme), from.scheme),
                ))
                .row(property_row(
                    "Accent",
                    line(now.accent.to_owned(), from.accent),
                ))
                .row(property_row(
                    "Higher contrast",
                    line(yes_no(now.high_contrast).to_owned(), from.high_contrast),
                ))
                .row(property_row(
                    "Reduced motion",
                    line(yes_no(now.reduced_motion).to_owned(), from.reduced_motion),
                ))
                .row(property_row(
                    "Text size",
                    line(format!("{:.0}%", now.text_scale * 100.0), from.text_scale),
                ))
                .row(property_row(
                    "Icon theme",
                    line(
                        now.icon_theme
                            .clone()
                            .unwrap_or_else(|| appearance::DEFAULT_ICON_THEME.to_owned()),
                        from.icon_theme,
                    ),
                ));
        }
    }
    group.into_element()
})];

pub const SERVICES: &[Spec] = &[Spec::forced("What the session offers", || {
    let found = use_state(|| None::<platform::Services>);
    use_hook(move || runtime::background(found, platform::services));
    let mut group = Group::titled("Services");
    match *found.read() {
        None => {
            group = group.row(property_row("Reading", "Asking the session bus"));
        }
        Some(offered) => {
            group = group
                .row(property_row("Session bus", yes_no(offered.bus)))
                .row(property_row("Portal", yes_no(offered.portal)))
                .row(property_row("Notifications", yes_no(offered.notifications)))
                .row(property_row("Screensaver", yes_no(offered.screensaver)))
                .row(property_row("Logind", yes_no(offered.logind)));
        }
    }
    group.into_element()
})];

pub const ASKING: &[Spec] = &[Spec::live("Open a link, show a notification", || {
    let mut said = use_state(|| "Nothing asked yet".to_owned());
    rows(vec![
        with(
            "Open a link",
            action_button("Open", move |_| {
                open::open_uri_then("https://example.org".to_owned(), move |result| {
                    said.set(match result {
                        Ok(via) => format!("Opened: {via:?}"),
                        Err(error) => error.to_string(),
                    });
                });
            }),
        ),
        with(
            "Notify",
            action_button("Show", move |_| {
                notify::send_then(
                    Notification::new("Taxonomy")
                        .body("A notification from bingux-ui.")
                        .transient(true),
                    move |result| {
                        said.set(match result {
                            Ok(sent) => format!("Shown as notification {}", sent.0),
                            Err(error) => error.to_string(),
                        });
                    },
                );
            }),
        ),
        property_row("Answer", said.read().clone()).into_element(),
    ])
})];
