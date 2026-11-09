//! Keypad: a grid of keys, for a calculator, a PIN entry, a dialler or a
//! lock screen.
//!
//! A [`KeypadKey`] is a button with a short face label (`7`, `sin`, `⌫`), an
//! accessible name that says what it does ("Seven", "Sine", "Delete the last
//! character"), a tone and a span of columns. A [`Keypad`] lays keys out
//! row by row, `columns` to a row, every key sharing the width of its row and
//! every row sharing the height it is given. Each key is one Tab stop with
//! the focus ring, and Enter or Space presses it.
//!
//! A keypad usually types into something. `Keypad::refocus(id)` hands the
//! keyboard focus to that control (a text field's `a11y_id`) after each press,
//! so typing on the keyboard carries on where it was.

use freya::prelude::*;

use super::press::use_press;
use super::tooltip::hover_tip_sized;
use crate::component::ComponentName;
use crate::focus::{press_focus, use_focus_core};
use crate::icons::{self, Icon};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};
use crate::typography::{EMPHASIS, HEADING, Ink, TextStyle};

/// What a key is for, which decides how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyTone {
    /// A digit or a letter: the main keys, on the control fill.
    Digit,
    /// A function or a modifier: a quieter fill and a smaller label.
    Plain,
    /// An operator: the accent colour on a soft fill.
    Operator,
    /// The key that completes the entry (equals, Call, Enter): the accent fill.
    Suggested,
    /// A key that throws away what was entered (clear): the danger ink.
    Destructive,
}

/// One key of a [`Keypad`].
#[derive(Clone, PartialEq)]
pub struct KeypadKey {
    label: String,
    name: Option<String>,
    tone: KeyTone,
    span: u8,
    enabled: bool,
    active: bool,
    icon: Option<Icon>,
    tooltip: Option<String>,
    on_press: EventHandler<()>,
}

/// A key showing `label` that calls `on_press`.
pub fn keypad_key(label: impl Into<String>, on_press: impl Into<EventHandler<()>>) -> KeypadKey {
    KeypadKey {
        label: label.into(),
        name: None,
        tone: KeyTone::Digit,
        span: 1,
        enabled: true,
        active: false,
        icon: None,
        tooltip: None,
        on_press: on_press.into(),
    }
}

impl KeypadKey {
    /// What the key does, for a screen reader ("Divide" for
    /// `÷`). Without a name the label is the name, which suits a digit.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn tone(mut self, tone: KeyTone) -> Self {
        self.tone = tone;
        self
    }

    /// How many columns the key takes (a wide zero). At least 1.
    pub fn span(mut self, columns: u8) -> Self {
        self.span = columns.max(1);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A toggle that is on: drawn with the accent fill, and announced as
    /// pressed.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Draw `icon` in place of the label, for a key a glyph names better
    /// than letters (Backspace). The name is still what a screen reader says.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A tooltip for the key, shown after the pointer rests on it: the key's
    /// name and its shortcut ("Delete (Backspace)"). A key whose face says all
    /// of it (a digit) needs none.
    pub fn tooltip(mut self, text: impl Into<String>) -> Self {
        self.tooltip = Some(text.into());
        self
    }
}

/// A grid of keys. See the [module](self).
#[derive(Clone, PartialEq)]
pub struct Keypad {
    columns: u8,
    keys: Vec<KeypadKey>,
    refocus: Option<AccessibilityId>,
    dense: bool,
}

/// A keypad of `columns` columns. Keys fill the rows in order, and a key's
/// span counts as that many columns; a row ends when it is full.
pub fn keypad(columns: u8, keys: Vec<KeypadKey>) -> Keypad {
    Keypad {
        columns: columns.max(1),
        keys,
        refocus: None,
        dense: false,
    }
}

impl Keypad {
    /// After a key is pressed, give the keyboard focus to `id`.
    pub fn refocus(mut self, id: AccessibilityId) -> Self {
        self.refocus = Some(id);
        self
    }

    /// Tighter gaps and smaller labels, for a grid that has many keys (the
    /// bit view of a programmer's calculator).
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }

    /// The keys split into rows by their spans.
    fn rows(&self) -> Vec<Vec<usize>> {
        let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
        let mut used = 0u8;
        for (index, key) in self.keys.iter().enumerate() {
            let span = key.span.min(self.columns);
            if used + span > self.columns {
                rows.push(Vec::new());
                used = 0;
            }
            rows.last_mut().expect("one row").push(index);
            used += span;
            if used >= self.columns {
                rows.push(Vec::new());
                used = 0;
            }
        }
        if rows.last().is_some_and(Vec::is_empty) {
            rows.pop();
        }
        rows
    }
}

impl Component for Keypad {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let space = if self.dense { S1 } else { S2 };
        let columns = self.columns;
        let rows = self.rows();
        let mut grid = rect()
            .component("Keypad")
            .width(Size::fill())
            .height(Size::fill())
            .tw(&tokens, &[flex_col(), gap(space), grow_children()]);
        for row in rows {
            let mut line = rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .tw(&tokens, &[flex_row(), gap(space), grow_children()]);
            let mut used = 0u8;
            for index in row {
                let key = &self.keys[index];
                let span = key.span.min(columns);
                used += span;
                line = line.child(KeyView {
                    key: key.clone(),
                    refocus: self.refocus,
                    dense: self.dense,
                });
            }
            // A short last row leaves its right side empty, as a dialler's
            // bottom row does, rather than stretching its keys.
            if used < columns {
                let rest = (columns - used) as f32;
                line = line.child(rect().width(Size::flex(rest)).height(Size::fill()));
            }
            grid = grid.child(line);
        }
        grid
    }
}

/// The label of a digit or an operator: the heading size, tabular figures.
const KEY_LARGE: TextStyle = TextStyle {
    tabular: true,
    ..HEADING
};

/// The label of a function key.
const KEY_SMALL: TextStyle = EMPHASIS;

#[derive(PartialEq)]
struct KeyView {
    key: KeypadKey,
    refocus: Option<AccessibilityId>,
    dense: bool,
}

impl Component for KeyView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let core = use_focus_core();
        let focus = use_focus(a11y_id);
        let press = use_press();
        let enabled = use_enabled(self.key.enabled);
        let on_press = self.key.on_press.clone();
        let on_key = on_press.clone();
        let focus_after = self.refocus.unwrap_or(a11y_id);
        let key = &self.key;

        let strong = key.active || key.tone == KeyTone::Suggested;
        let base = if strong {
            Paint::Accent
        } else {
            match key.tone {
                KeyTone::Digit => Paint::Control,
                KeyTone::Plain | KeyTone::Destructive => Paint::Card,
                KeyTone::Operator => Paint::AccentSoft,
                KeyTone::Suggested => Paint::Accent,
            }
        };
        let ink = if !enabled {
            Ink::Muted
        } else if strong {
            Ink::OnAccent
        } else {
            match key.tone {
                KeyTone::Operator => Ink::Accent,
                KeyTone::Destructive => Ink::Danger,
                _ => Ink::Text,
            }
        };
        let style = if self.dense {
            KEY_SMALL
        } else if matches!(key.tone, KeyTone::Plain | KeyTone::Destructive)
            && key.label.chars().count() > 1
        {
            KEY_SMALL
        } else {
            KEY_LARGE
        };
        let name = key.name.clone().unwrap_or_else(|| key.label.clone());

        let face = rect()
            .component("KeypadKey")
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(name)
            .width(Size::flex(1.0))
            .height(Size::fill())
            .tw(
                &tokens,
                &[
                    flex_row(),
                    center(),
                    min_h(if self.dense { Dim::Box7 } else { Dim::Control }),
                    rounded(Radius::Control),
                    surface(base, hovering(), pressing(), false),
                ],
            )
            .maybe(!enabled, |face| face.opacity(0.5))
            .maybe(enabled, |face| {
                face.on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        pressing.set(true);
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_global_pointer_up(move |_| {
                        press.release();
                        pressing.set_if_modified(false);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        if !press.tap(&event) {
                            return;
                        }
                        on_press.call(());
                        press_focus(&core, focus_after);
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            on_key.call(());
                        }
                    })
            })
            .maybe_child(
                key.icon
                    .map(|icon| icons::icon(icon, IconSize::Control, ink.color(&tokens))),
            )
            .maybe_child(key.icon.is_none().then(|| {
                style
                    .in_ink(ink)
                    .label(&tokens, key.label.clone())
                    .max_lines(1)
            }))
            .focusable(focus())
            .tw(&tokens, &[grow_children()])
            .width(Size::flex(key.span as f32))
            .height(Size::fill());
        match &key.tooltip {
            // The tip's box takes the key's flex size: a box of its own
            // would shrink the key to its label and the row would not fill.
            Some(text) => hover_tip_sized(
                face,
                text.clone(),
                Size::flex(key.span as f32),
                Size::fill(),
            ),
            None => face.into_element(),
        }
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn pad(columns: u8, spans: &[u8]) -> Keypad {
        keypad(
            columns,
            spans
                .iter()
                .map(|s| keypad_key("k", |_| {}).span(*s))
                .collect(),
        )
    }

    #[test]
    fn keys_fill_rows_by_their_spans() {
        let rows = pad(4, &[1, 1, 1, 1, 1, 1, 1, 1, 2, 1, 1]).rows();
        assert_eq!(
            rows,
            vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7], vec![8, 9, 10]]
        );
    }

    #[test]
    fn a_wide_key_that_does_not_fit_starts_the_next_row() {
        let rows = pad(4, &[3, 2, 1]).rows();
        assert_eq!(rows, vec![vec![0], vec![1, 2]]);
    }

    #[test]
    fn a_span_wider_than_the_grid_is_the_grid() {
        let rows = pad(3, &[9, 1]).rows();
        assert_eq!(rows, vec![vec![0], vec![1]]);
    }

    #[test]
    fn no_keys_no_rows() {
        assert!(pad(4, &[]).rows().is_empty());
    }
}

/// A key with a tooltip lives in a box of its own (`hover_tip_sized`). That
/// box has to take the key's flex share of the row, or the key shrinks to its
/// label and the row no longer fills.
#[cfg(all(test, feature = "testing"))]
mod tooltip_layout_tests {
    use freya::prelude::*;

    use super::*;
    use crate::shell::overlays::OverlayHost;
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    #[derive(PartialEq, Clone)]
    struct Page {
        /// Which of the keys carry a tooltip.
        tips: [bool; 4],
        /// Key labels: a short one beside long ones shows a shrunken box.
        labels: [&'static str; 4],
    }

    impl Component for Page {
        fn render(&self) -> impl IntoElement {
            let keys = self
                .labels
                .iter()
                .zip(self.tips)
                .map(|(label, tip)| {
                    let key = keypad_key(*label, |_: ()| {});
                    if tip { key.tooltip("A tooltip") } else { key }
                })
                .collect();
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .child(keypad(4, keys))
        }
    }

    #[derive(PartialEq, Clone)]
    struct Root(Page);

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _tokens = provide_tokens();
            WindowHost::new(OverlayHost::new(self.0.clone()))
        }
    }

    /// The width of every key, left to right, as drawn.
    fn key_widths(page: Page) -> Vec<f32> {
        let labels = page.labels;
        let mut shot = Snapshot::new(move || Root(page.clone()).into_element(), (400.0, 120.0));
        shot.settle(std::time::Duration::from_millis(300));
        let tree = crate::inspector::snapshot::Snapshot::capture_headless(shot.runner(), 1.0);
        let faces: Vec<_> = tree
            .nodes
            .iter()
            .filter(|node| node.a11y.component.as_deref() == Some("KeypadKey"))
            .filter_map(|node| node.area)
            .collect();
        assert_eq!(faces.len(), labels.len(), "every key is drawn");
        let mut widths: Vec<(f32, f32)> = faces
            .iter()
            .map(|area| (area.min_x(), area.width()))
            .collect();
        widths.sort_by(|a, b| a.0.total_cmp(&b.0));
        widths.into_iter().map(|(_, width)| width).collect()
    }

    #[test]
    fn keys_with_and_without_a_tooltip_share_the_row_alike() {
        let labels = ["7", "8", "sinh", "="];
        let plain = key_widths(Page {
            tips: [false; 4],
            labels,
        });
        for tips in [
            [true; 4],
            [false, true, false, true],
            [true, false, false, false],
        ] {
            let widths = key_widths(Page { tips, labels });
            for (plain, with) in plain.iter().zip(&widths) {
                assert!(
                    (plain - with).abs() < 1.0,
                    "tips {tips:?}: key widths {widths:?} differ from {plain:?}"
                );
            }
        }
        let first = plain[0];
        assert!(
            plain.iter().all(|w| (w - first).abs() < 1.0),
            "equal columns: {plain:?}"
        );
    }
}
