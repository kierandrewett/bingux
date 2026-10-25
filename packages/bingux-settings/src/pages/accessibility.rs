//! Accessibility: the settings for seeing, typing and pointing. Each switch
//! shows its new value at once and writes its GNOME key in the background; a
//! failed write puts the old value back and shows the error as a toast. The
//! keys live in the `a11y` and `interface` schemas, so GTK applications and the
//! compositor follow them.

use std::collections::BTreeMap;

use bingux_ui::runtime::{optimistic, optimistic_then};
use freya::prelude::*;

use super::loading::veiled;
use crate::system::{self, CommandResult, background};
use crate::ui::{
    Group, Overlays, PageHeader, page_column, row, segmented, switch_row, use_overlays,
    use_page_header,
};

const APPLICATIONS: &str = "org.gnome.desktop.a11y.applications";
const KEYBOARD: &str = "org.gnome.desktop.a11y.keyboard";
const A11Y_INTERFACE: &str = "org.gnome.desktop.a11y.interface";
const INTERFACE: &str = "org.gnome.desktop.interface";

/// What "Large text" sets, as GNOME Settings does.
const LARGE_TEXT: f64 = 1.25;
/// The cursor sizes GNOME Settings offers.
const CURSOR_SIZES: [u32; 5] = [24, 32, 48, 64, 96];
/// The key of the cursor size choice in `Picks`.
const CURSOR_KEY: &str = "cursor-size";

/// The values as the system has them. `None` means the key could not be read
/// and its row is hidden.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Model {
    screen_reader: Option<bool>,
    screen_keyboard: Option<bool>,
    large_text: Option<bool>,
    high_contrast: Option<bool>,
    reduce_animations: Option<bool>,
    cursor_size: Option<u32>,
    overlay_scrollbars: Option<bool>,
    sticky_keys: Option<bool>,
    slow_keys: Option<bool>,
    bounce_keys: Option<bool>,
    mouse_keys: Option<bool>,
    locate_pointer: Option<bool>,
}

fn flag(schema: &str, key: &str) -> Option<bool> {
    system::gsettings_get(schema, key)
        .ok()
        .map(|raw| raw.trim() == "true")
}

fn load() -> Model {
    Model {
        screen_reader: flag(APPLICATIONS, "screen-reader-enabled"),
        screen_keyboard: flag(APPLICATIONS, "screen-keyboard-enabled"),
        large_text: system::gsettings_get(INTERFACE, "text-scaling-factor")
            .ok()
            .and_then(|raw| raw.trim().parse::<f64>().ok())
            .map(|scale| scale >= LARGE_TEXT),
        high_contrast: flag(A11Y_INTERFACE, "high-contrast"),
        reduce_animations: flag(INTERFACE, "enable-animations").map(|on| !on),
        cursor_size: system::gsettings_get(INTERFACE, "cursor-size")
            .ok()
            .and_then(|raw| raw.trim().parse().ok()),
        overlay_scrollbars: flag(INTERFACE, "overlay-scrolling"),
        sticky_keys: flag(KEYBOARD, "stickykeys-enable"),
        slow_keys: flag(KEYBOARD, "slowkeys-enable"),
        bounce_keys: flag(KEYBOARD, "bouncekeys-enable"),
        mouse_keys: flag(KEYBOARD, "mousekeys-enable"),
        locate_pointer: flag(INTERFACE, "locate-pointer"),
    }
}

/// The shape of a loaded page with placeholder values, drawn under the veil
/// while the first load runs, so the values fill in without anything moving.
fn placeholder() -> Model {
    Model {
        screen_reader: Some(false),
        screen_keyboard: Some(false),
        large_text: Some(false),
        high_contrast: Some(false),
        reduce_animations: Some(false),
        cursor_size: Some(CURSOR_SIZES[1]),
        overlay_scrollbars: Some(false),
        sticky_keys: Some(false),
        slow_keys: Some(false),
        bounce_keys: Some(false),
        mouse_keys: Some(false),
        locate_pointer: Some(false),
    }
}

/// Switches shown at once, by key. A value here wins over the loaded one until
/// the page is left.
type Pending = State<BTreeMap<String, bool>>;
/// Choices shown at once, by key, as for switches.
type Picks = State<BTreeMap<String, String>>;

fn shown(pending: Pending, key: &str, loaded: bool) -> bool {
    pending.read().get(key).copied().unwrap_or(loaded)
}

/// Flip a switch at once and run its write in the background. A failure
/// reverts the switch and shows the error as a toast.
fn set_flag(
    pending: Pending,
    overlays: Overlays,
    key: &str,
    on: bool,
    work: impl FnOnce() -> CommandResult + Send + 'static,
) {
    let previous = pending.read().clone();
    let mut value = previous.clone();
    value.insert(key.to_string(), on);
    optimistic(pending, Some(overlays), previous, value, move || {
        work().map(|_| ())
    });
}

/// Show a cursor size at once and write it in the background. A failure
/// reverts it and shows the error as a toast; success reloads the model.
fn choose_cursor_size(picks: Picks, loaded: State<Option<Model>>, overlays: Overlays, size: u32) {
    let previous = picks.read().clone();
    let mut next = previous.clone();
    next.insert(CURSOR_KEY.to_string(), size.to_string());
    optimistic_then(
        picks,
        Some(overlays),
        previous,
        next,
        move || system::gsettings_set(INTERFACE, "cursor-size", &size.to_string()).map(|_| ()),
        move || background(loaded, move || system::load_saved("accessibility", load)),
    );
}

fn flag_text(on: bool) -> &'static str {
    if on { "true" } else { "false" }
}

/// One switch row, or nothing when the key could not be read.
fn flag_row(
    pending: Pending,
    overlays: Overlays,
    key: &'static str,
    label: &str,
    subtitle: &str,
    loaded: Option<bool>,
    write: fn(bool) -> CommandResult,
) -> Option<Element> {
    let on = shown(pending, key, loaded?);
    Some(
        switch_row(label, on, move |next: bool| {
            set_flag(pending, overlays, key, next, move || write(next))
        })
        .subtitle(subtitle)
        .into(),
    )
}

/// A titled group of rows, or nothing when no row applies.
fn section(title: &str, rows: Vec<Option<Element>>) -> Option<Group> {
    let rows: Vec<Element> = rows.into_iter().flatten().collect();
    if rows.is_empty() {
        return None;
    }
    let mut group = Group::titled(title);
    for row in rows {
        group = group.row(row);
    }
    Some(group)
}

/// "Cursor size": the sizes as a segmented choice. A size the system already
/// uses is kept in the choice even when it is not one GNOME offers.
fn cursor_row(
    cursor: Option<u32>,
    picks: Picks,
    loaded: State<Option<Model>>,
    overlays: Overlays,
) -> Option<Element> {
    let current = cursor?;
    let mut sizes = CURSOR_SIZES.to_vec();
    if !sizes.contains(&current) {
        sizes.push(current);
    }
    let options: Vec<(String, String)> = sizes
        .into_iter()
        .map(|size| (size.to_string(), size.to_string()))
        .collect();
    Some(
        row("Cursor size")
            .trailing(segmented(
                options,
                &current.to_string(),
                move |key: String| {
                    if let Ok(size) = key.parse() {
                        choose_cursor_size(picks, loaded, overlays, size);
                    }
                },
            ))
            .into(),
    )
}

#[derive(PartialEq)]
pub struct AccessibilityPage;

impl Component for AccessibilityPage {
    fn render(&self) -> impl IntoElement {
        let loaded = system::use_cached_saved("accessibility", load);
        let overlays = use_overlays();
        let pending: Pending = use_state(BTreeMap::new);
        let picks: Picks = use_state(BTreeMap::new);
        use_page_header(PageHeader::default());

        let (model, loading) = match loaded.read().clone() {
            Some(model) => (model, false),
            None => (placeholder(), true),
        };
        let cursor = picks
            .read()
            .get(CURSOR_KEY)
            .and_then(|value| value.parse().ok())
            .or(model.cursor_size);

        let seeing = section(
            "Seeing",
            vec![
                flag_row(
                    pending,
                    overlays,
                    "screen-reader",
                    "Screen reader",
                    "Reads out what is on screen.",
                    model.screen_reader,
                    |on| {
                        system::gsettings_set(APPLICATIONS, "screen-reader-enabled", flag_text(on))
                    },
                ),
                flag_row(
                    pending,
                    overlays,
                    "large-text",
                    "Large text",
                    "Makes the text 25 % larger.",
                    model.large_text,
                    |on| {
                        let scale = if on { LARGE_TEXT } else { 1.0 };
                        system::gsettings_set(
                            INTERFACE,
                            "text-scaling-factor",
                            &format!("{scale:.2}"),
                        )
                    },
                ),
                flag_row(
                    pending,
                    overlays,
                    "high-contrast",
                    "High contrast",
                    "Stronger outlines and colours, so controls stand out.",
                    model.high_contrast,
                    |on| system::gsettings_set(A11Y_INTERFACE, "high-contrast", flag_text(on)),
                ),
                flag_row(
                    pending,
                    overlays,
                    "reduce-animations",
                    "Reduce animations",
                    "Windows and menus appear without movement.",
                    model.reduce_animations,
                    |on| system::gsettings_set(INTERFACE, "enable-animations", flag_text(!on)),
                ),
                cursor_row(cursor, picks, loaded, overlays),
                flag_row(
                    pending,
                    overlays,
                    "overlay-scrollbars",
                    "Overlay scrollbars",
                    "Scrollbars appear only while scrolling.",
                    model.overlay_scrollbars,
                    |on| system::gsettings_set(INTERFACE, "overlay-scrolling", flag_text(on)),
                ),
            ],
        );

        let typing = section(
            "Typing",
            vec![
                flag_row(
                    pending,
                    overlays,
                    "on-screen-keyboard",
                    "On-screen keyboard",
                    "Shows a keyboard on screen for typing with a pointer.",
                    model.screen_keyboard,
                    |on| {
                        system::gsettings_set(
                            APPLICATIONS,
                            "screen-keyboard-enabled",
                            flag_text(on),
                        )
                    },
                ),
                flag_row(
                    pending,
                    overlays,
                    "sticky-keys",
                    "Sticky keys",
                    "Press Shift, Control or Alt once, then the next key.",
                    model.sticky_keys,
                    |on| system::gsettings_set(KEYBOARD, "stickykeys-enable", flag_text(on)),
                ),
                flag_row(
                    pending,
                    overlays,
                    "slow-keys",
                    "Slow keys",
                    "A key counts only when it is held down for a moment.",
                    model.slow_keys,
                    |on| system::gsettings_set(KEYBOARD, "slowkeys-enable", flag_text(on)),
                ),
                flag_row(
                    pending,
                    overlays,
                    "bounce-keys",
                    "Bounce keys",
                    "Ignores quick repeated presses of the same key.",
                    model.bounce_keys,
                    |on| system::gsettings_set(KEYBOARD, "bouncekeys-enable", flag_text(on)),
                ),
            ],
        );

        let pointing = section(
            "Pointing",
            vec![
                flag_row(
                    pending,
                    overlays,
                    "mouse-keys",
                    "Mouse keys",
                    "Moves the pointer with the number pad.",
                    model.mouse_keys,
                    |on| system::gsettings_set(KEYBOARD, "mousekeys-enable", flag_text(on)),
                ),
                flag_row(
                    pending,
                    overlays,
                    "locate-pointer",
                    "Locate pointer",
                    "Shows a ripple around the pointer when Control is pressed.",
                    model.locate_pointer,
                    |on| system::gsettings_set(INTERFACE, "locate-pointer", flag_text(on)),
                ),
            ],
        );

        let page = page_column()
            .maybe_child(seeing)
            .maybe_child(typing)
            .maybe_child(pointing);
        veiled(page, loading)
    }
}

/// Start loading this page's data before it is opened.
pub fn prefetch() {
    system::prefetch_saved("accessibility", load);
}
