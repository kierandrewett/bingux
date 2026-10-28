//! Alert dialogs (`AdwAlertDialog`, SwiftUI `.alert`): the question, its
//! responses, and the keyboard contract that goes with them.
//!
//! # Layout
//!
//! The card has one padding on all four sides, [`DIALOG_INSET`] (12 px at
//! every density). The buttons sit inside it, so their radius is the card's
//! less the inset ([`Radius::Inner`], concentric with the card's corners). The
//! text sits one inset further in, so the title, the message and a field all
//! start 24 px from the edge and the buttons 12 px: the text block is the
//! padded part, the button row is the card's own content.
//!
//! Down the card the gaps grow with the distance between the things they
//! separate: icon to title [`S2`], title to message [`S2`], message to buttons
//! twice [`DIALOG_INSET`]. The card is as wide as its text, between
//! [`Dim::DialogMin`] and [`Dim::Dialog`], and never wider than the window
//! less a margin. A long title wraps. Text is centred, for every dialog.
//!
//! Buttons are one row of equal widths on a desktop, Cancel first and the
//! affirmative response last (the order `response` was called in). At compact
//! density they stack full width, each a [`Dim::Button`] tall (48 px, above the
//! 44 px touch minimum) with the order reversed: the affirmative response is
//! on top and Cancel at the bottom, nearest the thumb, the order of iOS and of
//! libadwaita's narrow alerts. Tab follows what is on screen.
//!
//! # Keyboard
//!
//! Every dialog has a *default action*, which Enter runs, and a *cancel
//! action*, which Escape, Back and a press on the scrim run. Both name one of
//! the dialog's responses; neither is "close without a result".
//!
//! - [`Dialog::default_action`] names the default response. Without it the
//!   default is the suggested response, else the cancel response, else the
//!   first response that is not destructive. So a dialog that loses data
//!   defaults to its safe answer unless it says otherwise.
//! - [`Dialog::cancel_action`] names the cancel response. Without it, it is
//!   the response added by [`Dialog::cancel`].
//! - The default button carries a key cap with the Return glyph at all times
//!   (see [`crate::controls::default_mark`]), wherever the focus is. The
//!   keyboard focus ring follows Tab and is the only ring: on another button
//!   it is a ring beside the key cap, on the default button itself the ring
//!   and the key cap are both shown. A phone shows the focus ring alone.
//! - The dialog opens with the keyboard focus on the default button, unless
//!   that button is destructive: a destructive default is marked but never
//!   focused, so a stray Enter that opens the dialog cannot also confirm it.
//! - Enter presses the button that has the focus, otherwise the default
//!   action; Space presses the focused button; Enter in a single-line field
//!   runs the default action, and in a multi-line field it adds a line.

use freya::prelude::*;

use crate::controls::button::{ActionButton, action_button, primary_button};
use crate::focus::trap_inline;
use crate::icons::{self, Icon};
use crate::shell::overlays::Overlays;
use crate::shell::panel::Panel;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink, SECTION};
use crate::controls::path_label::text_width;

/// How a dialog response is styled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseKind {
    /// A plain elevated button.
    Default,
    /// The accent-filled button for the action the user most likely wants.
    Suggested,
    /// The danger-filled button for actions that lose data.
    Destructive,
}

/// The id [`Dialog::cancel`] gives its response.
pub const CANCEL_ID: &str = "cancel";

/// One button at the bottom of a dialog.
#[derive(Clone, PartialEq)]
pub struct DialogResponse {
    /// Names the response for [`Dialog::default_action`] and
    /// [`Dialog::cancel_action`].
    pub id: String,
    pub label: String,
    pub kind: ResponseKind,
    pub on_press: EventHandler<()>,
}

/// A glyph above an alert's title, for a question that needs more than words:
/// a warning before data is lost, an error that stopped something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogIcon {
    Info,
    Warning,
    Error,
}

impl DialogIcon {
    fn glyph(self) -> Icon {
        match self {
            DialogIcon::Info => Icon::DialogInformation,
            DialogIcon::Warning => Icon::DialogWarning,
            DialogIcon::Error => Icon::DialogError,
        }
    }

    fn ink(self) -> Ink {
        match self {
            DialogIcon::Info => Ink::Accent,
            DialogIcon::Warning => Ink::Warning,
            DialogIcon::Error => Ink::Danger,
        }
    }
}

/// An alert: title, body and one to three responses, with the default and
/// cancel actions that Enter and Escape run (see the module docs).
#[derive(Clone, PartialEq)]
pub struct Dialog {
    pub title: String,
    pub body: String,
    pub responses: Vec<DialogResponse>,
    /// Extra content between the body and the responses, such as a field.
    pub content: Option<Element>,
    /// Set by [`framed_dialog`]: the card is the content alone, at this size.
    pub frame: Option<DialogFrame>,
    /// The id of the response Enter runs, or `None` for the implicit default.
    pub default: Option<String>,
    /// The id of the response Escape runs, or `None` for [`CANCEL_ID`].
    pub cancel: Option<String>,
    /// A glyph above the title.
    pub icon: Option<DialogIcon>,
    /// Runs once the dialog has faded out and is gone. A dialog opened from
    /// the keyboard gives the keyboard back to its owner here.
    pub on_closed: Option<EventHandler<()>>,
}

/// The size of a [`framed_dialog`], in logical pixels. The card never grows
/// past the window: it keeps a margin on a phone and a small window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DialogFrame {
    pub width: f32,
    pub height: f32,
    /// The card fills the whole window, square-cornered to the window's own
    /// radius, with no margin: a sheet on a phone, a viewer in full screen.
    pub fill: bool,
}

pub fn dialog(title: impl Into<String>, body: impl Into<String>) -> Dialog {
    Dialog {
        title: title.into(),
        body: body.into(),
        responses: Vec::new(),
        content: None,
        frame: None,
        default: None,
        cancel: None,
        icon: None,
        on_closed: None,
    }
}

/// A dialog that is only `content`: no title, text or buttons of its own, on
/// the same card, scrim and motion as an alert, `width` by `height` px (cut
/// down to fit the window). Escape and a press on the scrim close it. The
/// content draws its own header, scrolls itself and handles its own keys,
/// with `title` for assistive technology.
///
/// This is for a dialog whose body is the point: a Quick Look preview, a
/// properties sheet, an image viewer. An alert with a question uses
/// [`dialog`].
pub fn framed_dialog(
    title: impl Into<String>,
    content: impl Into<Element>,
    width: f32,
    height: f32,
) -> Dialog {
    Dialog {
        content: Some(content.into()),
        frame: Some(DialogFrame { width, height, fill: false }),
        ..dialog(title, "")
    }
}

impl Dialog {
    /// Make a [`framed_dialog`] fill the whole window instead of floating as a
    /// card: a full-screen viewer, or a sheet on a phone. It keeps its scrim
    /// behaviour (Escape closes it) and the window's corner radius.
    pub fn fill_window(mut self) -> Self {
        if let Some(frame) = self.frame.as_mut() {
            frame.fill = true;
        }
        self
    }

    /// Call `handler` when the dialog has closed and gone, whichever way it
    /// closed. Use it to put the keyboard focus back where it was.
    pub fn on_closed(mut self, handler: impl Into<EventHandler<()>>) -> Self {
        self.on_closed = Some(handler.into());
        self
    }

    /// Add a response named after its label (`"Keep both"` is `keep_both`).
    /// Use [`action`](Self::action) when the label may change or repeat.
    pub fn response(
        self,
        label: impl Into<String>,
        kind: ResponseKind,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        let label = label.into();
        self.action(slug(&label), label, kind, on_press)
    }

    /// Add a response with an explicit `id`, for
    /// [`default_action`](Self::default_action) and
    /// [`cancel_action`](Self::cancel_action) to name.
    pub fn action(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        kind: ResponseKind,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.responses.push(DialogResponse {
            id: id.into(),
            label: label.into(),
            kind,
            on_press: on_press.into(),
        });
        self
    }

    /// A plain "Cancel"-style response that only closes the dialog. It is the
    /// cancel action unless [`cancel_action`](Self::cancel_action) names
    /// another.
    pub fn cancel(self, label: impl Into<String>) -> Self {
        self.action(CANCEL_ID, label, ResponseKind::Default, |_| {})
    }

    /// The response that Enter runs when no button has the keyboard focus.
    /// It is marked, and has the focus when the dialog opens, unless it is
    /// destructive (see the module docs). A dialog that loses data defaults to
    /// its cancel response without saying so; name a destructive response
    /// only when Enter should confirm it.
    pub fn default_action(mut self, id: impl Into<String>) -> Self {
        self.default = Some(id.into());
        self
    }

    /// The response that Escape, Back and a press on the scrim run.
    pub fn cancel_action(mut self, id: impl Into<String>) -> Self {
        self.cancel = Some(id.into());
        self
    }

    /// A glyph above the title.
    pub fn icon(mut self, icon: DialogIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn content(mut self, content: impl Into<Element>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Resolve the keyboard contract of this dialog.
    pub(crate) fn plan(&self) -> Plan {
        let responses: Vec<(&str, ResponseKind)> = self
            .responses
            .iter()
            .map(|response| (response.id.as_str(), response.kind))
            .collect();
        plan(&responses, self.default.as_deref(), self.cancel.as_deref())
    }
}

/// A response's id from its label: lower case, anything else an underscore.
fn slug(label: &str) -> String {
    let mut id = String::new();
    for c in label.chars() {
        if c.is_alphanumeric() {
            id.extend(c.to_lowercase());
        } else if !id.ends_with('_') && !id.is_empty() {
            id.push('_');
        }
    }
    id.trim_end_matches('_').to_string()
}

/// What Enter, Escape and the first focus do, resolved from a dialog's
/// responses. Indices are into [`Dialog::responses`], in the order they were
/// added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    /// What Enter runs when no button has the focus; marked on screen.
    pub default: Option<usize>,
    /// What Escape, Back and the scrim run.
    pub cancel: Option<usize>,
    /// The button that has the keyboard focus when the dialog opens.
    pub initial: Option<usize>,
}

/// Resolve what Enter, Escape and the first focus do for responses that are
/// each an `(id, kind)`, in order, with the ids a dialog named (or `None`).
pub(crate) fn plan(
    responses: &[(&str, ResponseKind)],
    default: Option<&str>,
    cancel: Option<&str>,
) -> Plan {
    let find = |id: &str| responses.iter().position(|(own, _)| *own == id);
    let cancel = cancel.and_then(find).or_else(|| find(CANCEL_ID));
    // The answer a person most likely wants, then the safe one, then any
    // answer that does not lose data. A destructive response is the default
    // only when the dialog says so.
    let default = default.and_then(find).or_else(|| {
        responses
            .iter()
            .position(|(_, kind)| *kind == ResponseKind::Suggested)
            .or(cancel)
            .or_else(|| {
                responses
                    .iter()
                    .position(|(_, kind)| *kind != ResponseKind::Destructive)
            })
    });
    let initial = default.filter(|&index| responses[index].1 != ResponseKind::Destructive);
    Plan {
        default,
        cancel,
        initial,
    }
}

impl Plan {
    /// The response Enter runs: the focused button's own, else the default.
    /// `focused` is the button that has the keyboard focus, if one has.
    pub(crate) fn enter(&self, focused: Option<usize>) -> Option<usize> {
        focused.or(self.default)
    }

    /// The response Escape runs.
    pub(crate) fn escape(&self) -> Option<usize> {
        self.cancel
    }

    /// Whether the button at `index` carries the default mark. It does at all
    /// times, wherever the focus is: Tab moves the focus ring, not the mark.
    pub(crate) fn marks(&self, index: usize) -> bool {
        self.default == Some(index)
    }

    /// The order buttons are laid out and tabbed in: as added in a row; at
    /// compact density reversed, so the affirmative response is on top.
    pub(crate) fn order(count: usize, compact: bool) -> Vec<usize> {
        if compact {
            (0..count).rev().collect()
        } else {
            (0..count).collect()
        }
    }
}

/// The widest a dialog gets, as a share of the window width, so it keeps a
/// margin on a phone.
const DIALOG_MAX_WIDTH_PERCENT: f32 = 94.0;
/// The tallest a framed dialog gets, as a share of the window height.
const DIALOG_MAX_HEIGHT_PERCENT: f32 = 90.0;

/// How wide a [`framed_dialog`] asked to be `width` px wide really is in this
/// window: it keeps its margin on a phone. Content that chooses its layout by
/// the width it has, such as a tab bar that fills the card or scrolls, asks
/// here. Read it while rendering, so it follows the window.
pub(crate) fn frame_width(width: f32) -> f32 {
    let window = Platform::get().current_window().root_size.read().width;
    width.min(window * DIALOG_MAX_WIDTH_PERCENT / 100.0)
}

/// The card of a [`framed_dialog`]: the alert card without its padding and
/// gap, clipped so the content's corners follow the card's.
const DIALOG_FRAME: Recipe = &[
    flex_col(),
    rounded(Radius::Overlay),
    bg(Paint::Bg),
    border(Line::Hairline),
    shadow(Elevation::Floating),
    clip(),
];

/// The card of a [`Dialog::fill_window`]: the window's own corner radius and
/// nothing floating.
const DIALOG_FILL: Recipe = &[flex_col(), rounded(Radius::Window), bg(Paint::Bg), clip()];

/// The card of an alert: [`DIALOG_INSET`] all round and between its sections.
/// The message to the buttons is then the text block's own bottom padding plus
/// this gap, twice the inset.
const ALERT_CARD: Recipe = &[
    flex_col(),
    p(DIALOG_INSET),
    gap(DIALOG_INSET),
    rounded(Radius::Overlay),
    bg(Paint::Bg),
    border(Line::Hairline),
    shadow(Elevation::Floating),
];

/// The icon, title and message: padded by one more inset, so the text starts
/// twice the card's padding from its edge.
const ALERT_TEXT: Recipe = &[
    flex_col(),
    w_full(),
    items(Align::Center),
    p(DIALOG_INSET),
    gap(S2),
];

/// A field or other content between the message and the buttons.
const ALERT_CONTENT: Recipe = &[w_full(), px(DIALOG_INSET)];

/// The responses: a row of equal widths, [`S2`] apart.
const ALERT_ROW: Recipe = &[w_full(), flex_row(), grow_children(), gap(S2)];

/// The responses at compact density: a full-width column.
const ALERT_COLUMN: Recipe = &[w_full(), flex_col(), gap(S2)];

/// How wide an alert's card is: as wide as its text and buttons want, no
/// narrower than [`Dim::DialogMin`] and no wider than [`Dim::Dialog`] or the
/// window less its margin. The text is measured on one line, so a long title
/// or message asks for the maximum and wraps there. A dialog with content
/// (a field) is the maximum, so the field has room.
fn card_width(dialog: &Dialog, compact: bool) -> f32 {
    let window = Platform::get().current_window().root_size.read().width;
    let most = Dim::Dialog.px().min(window * DIALOG_MAX_WIDTH_PERCENT / 100.0);
    let least = Dim::DialogMin.px().min(most);
    if dialog.content.is_some() {
        return most;
    }
    let inset = DIALOG_INSET.px();
    let line = |text: &str, style| {
        text.split('\n')
            .map(|line| text_width(line, style))
            .fold(0.0, f32::max)
    };
    // The text sits two insets in from each edge.
    let text = line(&dialog.title, SECTION).max(line(&dialog.body, BODY)) + 4.0 * inset;
    // A row of buttons, all as wide as the widest: its label and padding, and
    // the key cap on the default one, with the card's inset at both ends.
    let buttons = if compact {
        0.0
    } else {
        let default = dialog.plan().default;
        let cap = text_width("↵", CAPTION) + 2.0 * S1.px() + S2.px();
        let widest = dialog
            .responses
            .iter()
            .enumerate()
            .map(|(index, response)| {
                let mark = if default == Some(index) { cap } else { 0.0 };
                (text_width(&response.label, BODY) + mark + 2.0 * S3.px()).max(Dim::Box9.px())
            })
            .fold(0.0, f32::max);
        let count = dialog.responses.len();
        let gaps = count.saturating_sub(1) as f32 * S2.px();
        widest * count as f32 + gaps + 2.0 * inset
    };
    text.max(buttons).clamp(least, most)
}

#[derive(PartialEq)]
pub(crate) struct DialogLayer {
    pub(crate) overlays: Overlays,
    pub(crate) dialog: Dialog,
    pub(crate) closing: bool,
    pub(crate) layer: Layer,
}

impl Component for DialogLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let a11y_id = use_a11y();
        let plan = self.dialog.plan();
        // The card is one modal focus scope (see `focus::trap_inline`): Tab
        // cycles inside it, the page behind is inert, and focus returns to the
        // opener when it closes. A scope opens on its start unless a response
        // was asked to take the first focus (`keep`).
        let scope = move |card: Rect, keep: bool| -> Element {
            let trap = trap_inline(card);
            let trap = if keep { trap.keep_initial_focus() } else { trap };
            trap.into_element()
        };

        let responses = self.dialog.responses.clone();
        // One press runs the response and closes the dialog, once: a second
        // Enter or click while it fades out does nothing.
        let run = {
            let responses = responses.clone();
            move |index: usize| {
                if overlays.dialog_closing() {
                    return;
                }
                responses[index].on_press.call(());
                overlays.dismiss_dialog();
            }
        };
        // Escape, Back and the scrim: the cancel response, then close.
        let cancel = {
            let run = run.clone();
            move |_: ()| match plan.escape() {
                Some(index) => run(index),
                None => {
                    // No cancel response: just close.
                    if !overlays.dialog_closing() {
                        overlays.dismiss_dialog();
                    }
                }
            }
        };

        let compact = is_compact();
        let inset = DIALOG_INSET.px();
        let button_radius = Radius::Overlay.nested(inset);
        let buttons = Plan::order(responses.len(), compact).into_iter().map(|index| {
            let response = &responses[index];
            let press = {
                let run = run.clone();
                move |_: ()| run(index)
            };
            let button: ActionButton = match response.kind {
                ResponseKind::Suggested => primary_button(response.label.clone(), press),
                ResponseKind::Destructive => {
                    action_button(response.label.clone(), press).destructive()
                }
                ResponseKind::Default => action_button(response.label.clone(), press),
            }
            .radius(button_radius);
            let button = if plan.marks(index) { button.default_action() } else { button };
            let button = if plan.initial == Some(index) { button.autofocus() } else { button };
            Element::from(if compact { button.fill_width() } else { button.grow() })
        });

        if let Some(frame) = self.dialog.frame {
            let (width, height, max_width, max_height) = if frame.fill {
                (Size::window_percent(100.0), Size::window_percent(100.0), Size::window_percent(100.0), Size::window_percent(100.0))
            } else {
                (
                    Size::px(frame.width),
                    Size::px(frame.height),
                    Size::window_percent(DIALOG_MAX_WIDTH_PERCENT),
                    Size::window_percent(DIALOG_MAX_HEIGHT_PERCENT),
                )
            };
            let card = rect()
                .a11y_id(a11y_id)
                .a11y_focusable(false)
                .a11y_role(AccessibilityRole::Dialog)
                .a11y_alt(self.dialog.title.clone())
                .tw(&tokens, if frame.fill { DIALOG_FILL } else { DIALOG_FRAME })
                .width(width)
                .height(height)
                .max_width(max_width)
                .max_height(max_height)
                // A press on the card never reaches the panel's scrim.
                .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
                .maybe_child(self.dialog.content.clone().map(|content| {
                    crate::resilience::error_boundary(self.dialog.title.clone(), content)
                }));
            return Element::from(
                Panel::dialog(!self.closing, self.layer, vec![scope(card, plan.initial.is_some())])
                    .on_dismiss(EventHandler::new(cancel))
                    .on_closed(EventHandler::new(move |_: ()| overlays.finish_dialog())),
            );
        }

        let width = card_width(&self.dialog, compact);
        let icon = self.dialog.icon;
        let on_enter = run.clone();
        let card = rect()
            .a11y_id(a11y_id)
            // The scope's sentinels take the focus, so the card is no Tab stop
            // of its own: a press that landed on it would show no ring.
            .a11y_focusable(false)
            .a11y_role(AccessibilityRole::AlertDialog)
            .a11y_alt(self.dialog.title.clone())
            .tw(&tokens, ALERT_CARD)
            .width(Size::px(width))
            .max_width(Size::window_percent(DIALOG_MAX_WIDTH_PERCENT))
            // A press on the card never reaches the panel's scrim.
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .child(
                rect()
                    .tw(&tokens, ALERT_TEXT)
                    .maybe_child(icon.map(|icon| {
                        icons::icon(icon.glyph(), IconSize::Hero, icon.ink().color(&tokens))
                    }))
                    .child(
                        SECTION
                            .paragraph(&tokens, self.dialog.title.clone())
                            .width(Size::fill())
                            .text_align(TextAlign::Center),
                    )
                    .maybe_child((!self.dialog.body.is_empty()).then(|| {
                        BODY.in_ink(Ink::Muted)
                            .paragraph(&tokens, self.dialog.body.clone())
                            .width(Size::fill())
                            .text_align(TextAlign::Center)
                    })),
            )
            .maybe_child(self.dialog.content.clone().map(|content| {
                rect()
                    .tw(&tokens, ALERT_CONTENT)
                    .child(crate::resilience::error_boundary(
                        self.dialog.title.clone(),
                        content,
                    ))
            }))
            .child(
                rect()
                    .tw(&tokens, if compact { ALERT_COLUMN } else { ALERT_ROW })
                    .children(buttons),
            );

        // Enter reaches here from anything inside the scope that does not take
        // it: the card, the focus sentinels (which listen to keys so that a
        // key they receive bubbles this far), a single-line field. A button
        // takes its own Enter, so a focused button is never overruled by the
        // default.
        let keys = rect().on_key_down(move |event: Event<KeyboardEventData>| {
            if event.key == Key::Named(NamedKey::Enter)
                && !event.modifiers.intersects(Modifiers::ALT | Modifiers::CONTROL | Modifiers::META)
                && let Some(index) = plan.enter(None)
            {
                event.stop_propagation();
                on_enter(index);
            }
        });

        Element::from(
            Panel::dialog(
                !self.closing,
                self.layer,
                vec![keys.child(scope(card, plan.initial.is_some())).into()],
            )
            .on_dismiss(EventHandler::new(cancel))
            .on_closed(EventHandler::new(move |_: ()| overlays.finish_dialog())),
        )
    }
}

/// The default, cancel and first-focus rules, with no window.
#[cfg(test)]
mod logic_tests {
    use super::ResponseKind::{Default, Destructive, Suggested};
    use super::*;

    const CONFIRM: &[(&str, ResponseKind)] = &[("cancel", Default), ("forget", Destructive)];
    const RESTART: &[(&str, ResponseKind)] = &[("cancel", Default), ("restart", Suggested)];
    const CLOSE: &[(&str, ResponseKind)] = &[("cancel", Default)];
    const CONFLICT: &[(&str, ResponseKind)] =
        &[("skip", Default), ("keep_both", Default), ("replace", Destructive)];

    #[test]
    fn a_destructive_dialog_defaults_to_its_safe_answer() {
        let plan = plan(CONFIRM, None, None);
        assert_eq!(plan.default, Some(0), "Enter keeps the data");
        assert_eq!(plan.cancel, Some(0));
        assert_eq!(plan.initial, Some(0), "the safe button has the focus when it opens");
        assert!(plan.marks(0) && !plan.marks(1), "the mark is on Cancel, not on Forget");
    }

    #[test]
    fn a_suggested_response_is_the_default_and_has_the_focus() {
        let plan = plan(RESTART, None, None);
        assert_eq!((plan.default, plan.initial, plan.cancel), (Some(1), Some(1), Some(0)));
    }

    #[test]
    fn a_lone_close_response_is_the_default_and_the_cancel() {
        let plan = plan(CLOSE, None, None);
        assert_eq!((plan.default, plan.initial, plan.cancel), (Some(0), Some(0), Some(0)));
    }

    #[test]
    fn a_destructive_default_is_marked_but_never_focused() {
        let plan = plan(CONFIRM, Some("forget"), None);
        assert_eq!(plan.default, Some(1), "the dialog said Enter confirms");
        assert_eq!(plan.initial, None, "the destructive button never takes the first focus");
        assert!(plan.marks(1));
        assert_eq!(plan.enter(None), Some(1));
    }

    #[test]
    fn the_cancel_action_can_be_any_response() {
        let plan = plan(CONFLICT, Some("keep_both"), Some("skip"));
        assert_eq!((plan.default, plan.cancel, plan.initial), (Some(1), Some(0), Some(1)));
        assert_eq!(plan.escape(), Some(0));
    }

    #[test]
    fn without_a_cancel_response_escape_has_no_response_to_run() {
        let plan = plan(CONFLICT, None, None);
        assert_eq!(plan.escape(), None);
        assert_eq!(plan.default, Some(0), "the first answer that loses nothing");
    }

    #[test]
    fn an_unknown_id_falls_back_to_the_implicit_rules() {
        let plan = plan(CONFIRM, Some("nope"), Some("nada"));
        assert_eq!((plan.default, plan.cancel), (Some(0), Some(0)));
    }

    #[test]
    fn a_dialog_of_only_destructive_answers_has_no_default() {
        let plan = plan(&[("delete", Destructive)], None, None);
        assert_eq!((plan.default, plan.initial, plan.enter(None)), (None, None, None));
    }

    /// Enter presses the focused button, else the default; Tab moves the
    /// focus across every button while the mark stays on the default.
    #[test]
    fn enter_follows_the_focus_and_the_mark_never_moves() {
        let plan = plan(CONFIRM, None, None);
        assert_eq!(plan.enter(None), Some(0), "nothing focused: the default");
        assert_eq!(plan.enter(Some(0)), Some(0), "the default itself focused");
        assert_eq!(plan.enter(Some(1)), Some(1), "Tab to Forget, then Enter: Forget");
        for focused in [None, Some(0), Some(1)] {
            assert!(plan.marks(0), "the mark stays on Cancel with focus on {focused:?}");
            assert!(!plan.marks(1));
        }
    }

    #[test]
    fn stacked_buttons_put_the_affirmative_response_on_top() {
        assert_eq!(Plan::order(3, false), vec![0, 1, 2]);
        assert_eq!(Plan::order(3, true), vec![2, 1, 0]);
    }

    #[test]
    fn response_ids_come_from_labels() {
        assert_eq!(slug("Keep both"), "keep_both");
        assert_eq!(slug("Delete permanently"), "delete_permanently");
        assert_eq!(slug("  Skip! "), "skip");
    }
}

/// The card's layout and keys in a headless window, at desktop and phone size.
#[cfg(all(test, feature = "testing"))]
mod window_tests {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::time::Duration;

    use torin::prelude::Area;

    use super::*;
    use crate::shell::overlays::{OverlayHost, use_overlays};
    use crate::snapshot::Snapshot;
    use crate::theme::provide_tokens;
    use crate::window::WindowHost;

    const DESKTOP: (f32, f32) = (900.0, 600.0);
    const PHONE: (f32, f32) = (390.0, 800.0);

    /// What the responses did, in order.
    #[derive(Clone, Default)]
    struct Log(Rc<RefCell<Vec<String>>>);

    impl PartialEq for Log {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }
    }

    impl Log {
        fn recorder(&self, name: &'static str) -> EventHandler<()> {
            let log = self.0.clone();
            EventHandler::new(move |_: ()| log.borrow_mut().push(name.to_string()))
        }

        fn take(&self) -> Vec<String> {
            std::mem::take(&mut self.0.borrow_mut())
        }
    }

    #[derive(PartialEq)]
    struct Open {
        make: fn(&Log) -> Dialog,
        log: Log,
    }

    impl Component for Open {
        fn render(&self) -> impl IntoElement {
            let overlays = use_overlays();
            let (make, log) = (self.make, self.log.clone());
            use_hook(move || overlays.show_dialog(make(&log)));
            rect()
        }
    }

    /// The window: tokens, then the window host and the overlays over `child`.
    #[derive(PartialEq)]
    struct Root {
        child: Element,
    }

    impl Component for Root {
        fn render(&self) -> impl IntoElement {
            let _tokens = provide_tokens();
            WindowHost::new(OverlayHost::new(self.child.clone()))
        }
    }

    fn forget(log: &Log) -> Dialog {
        dialog("Forget this network?", "Reconnecting will need the password again.")
            .action("cancel", "Cancel", ResponseKind::Default, log.recorder("cancel"))
            .action("forget", "Forget", ResponseKind::Destructive, log.recorder("forget"))
    }

    fn restart(log: &Log) -> Dialog {
        dialog("Restart now?", "The update needs a restart to finish.")
            .action("cancel", "Later", ResponseKind::Default, log.recorder("later"))
            .action("restart", "Restart", ResponseKind::Suggested, log.recorder("restart"))
    }

    fn delete_default(log: &Log) -> Dialog {
        forget(log).default_action("forget")
    }

    fn long(log: &Log) -> Dialog {
        dialog(
            "Stop searching “Documents/Projects/Quarterly reports 2024/Archive”?",
            "Files in this folder no longer appear in search results. Files that are already open are not changed, and you can add the folder again whenever you like.",
        )
        .action("cancel", "Cancel", ResponseKind::Default, log.recorder("cancel"))
        .action("remove", "Remove", ResponseKind::Destructive, log.recorder("remove"))
    }

    fn info(log: &Log) -> Dialog {
        dialog("Saved", "").action("cancel", "OK", ResponseKind::Default, log.recorder("ok"))
    }

    fn warning(log: &Log) -> Dialog {
        forget(log).icon(DialogIcon::Warning)
    }

    fn conflict(log: &Log) -> Dialog {
        dialog("“Report.pdf” already exists", "Something with this name is already in “Documents”.")
            .action("skip", "Skip", ResponseKind::Default, log.recorder("skip"))
            .action("keep_both", "Keep both", ResponseKind::Default, log.recorder("keep_both"))
            .action("replace", "Replace", ResponseKind::Destructive, log.recorder("replace"))
            .default_action("keep_both")
            .cancel_action("skip")
    }

    #[derive(PartialEq)]
    struct Field {
        text: State<String>,
    }

    impl Component for Field {
        fn render(&self) -> impl IntoElement {
            crate::controls::text_field(self.text, |_: String| {}).autofocus()
        }
    }

    #[derive(PartialEq)]
    struct WithField {
        log: Log,
    }

    impl Component for WithField {
        fn render(&self) -> impl IntoElement {
            let overlays = use_overlays();
            let text = use_state(|| "Holiday".to_string());
            let log = self.log.clone();
            use_hook(move || {
                overlays.show_dialog(
                    dialog("Rename preset", "")
                        .content(Element::from(Field { text }))
                        .action("cancel", "Cancel", ResponseKind::Default, log.recorder("cancel"))
                        .action("rename", "Rename", ResponseKind::Suggested, log.recorder("rename")),
                )
            });
            rect()
        }
    }

    /// A button that opens the Restart dialog, as a page does.
    #[derive(PartialEq)]
    struct FromButton {
        log: Log,
    }

    impl Component for FromButton {
        fn render(&self) -> impl IntoElement {
            let overlays = use_overlays();
            let log = self.log.clone();
            action_button("Open dialog", move |_: ()| overlays.show_dialog(restart(&log)))
        }
    }

    fn open_with(root: impl Fn() -> Element + 'static, size: (f32, f32)) -> Snapshot {
        let mut shot = Snapshot::new(root, size);
        shot.settle(Duration::from_millis(700));
        shot
    }

    fn open(make: fn(&Log) -> Dialog, size: (f32, f32)) -> (Snapshot, Log) {
        let log = Log::default();
        let seen = log.clone();
        let shot = open_with(
            move || {
                Root { child: Open { make, log: seen.clone() }.into() }.into_element()
            },
            size,
        );
        (shot, log)
    }

    /// The boxes of an open alert, in logical pixels.
    #[derive(Clone, Debug)]
    struct Found {
        card: Area,
        card_radius: f32,
        /// The icon, title and message block, and what is in it.
        text: Area,
        text_parts: Vec<Area>,
        /// Everything between the text block and the button row.
        content: Vec<Area>,
        row: Area,
        buttons: Vec<Area>,
        button_radius: f32,
    }

    fn found(shot: &mut Snapshot) -> Option<Found> {
        shot.runner()
            .find_many(|node, element| {
                if element.accessibility().builder.role() != AccessibilityRole::AlertDialog {
                    return None;
                }
                let kids = node.children();
                let area = |node: &freya_testing::TestingNode| node.layout().area;
                let (text, rest) = kids.split_first()?;
                let (row, content) = rest.split_last()?;
                let buttons = row.children();
                let face_radius = buttons
                    .first()?
                    .children()
                    .first()?
                    .element()
                    .style()
                    .corner_radius
                    .top_left();
                Some(Found {
                    card: area(&node),
                    card_radius: element.style().corner_radius.top_left(),
                    text: area(text),
                    text_parts: text.children().iter().map(area).collect(),
                    content: content.iter().map(area).collect(),
                    row: area(row),
                    buttons: buttons.iter().map(area).collect(),
                    button_radius: face_radius,
                })
            })
            .into_iter()
            .next()
    }

    fn is_open(shot: &mut Snapshot) -> bool {
        found(shot).is_some()
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.51
    }

    fn key(shot: &mut Snapshot, key: Key) {
        shot.key(key);
        shot.settle(Duration::from_millis(400));
    }

    fn enter() -> Key {
        Key::Named(NamedKey::Enter)
    }

    fn tab() -> Key {
        Key::Named(NamedKey::Tab)
    }

    /// Padding is one inset on all four sides of the card: the buttons start
    /// an inset from the card's left, right and bottom edges, the text block
    /// an inset from its left, right and top, and the text a second inset in.
    #[test]
    fn the_card_has_one_padding_on_every_side() {
        let inset = DIALOG_INSET.fixed_px();
        for (name, size) in [("desktop", DESKTOP), ("phone", PHONE)] {
            for (label, make) in [("forget", forget as fn(&Log) -> Dialog), ("long", long), ("warning", warning)] {
                let (mut shot, _) = open(make, size);
                let f = found(&mut shot).unwrap_or_else(|| panic!("{name} {label}: no card"));
                let at = format!("{name} {label}: {f:#?}");
                assert!(near(f.row.min_x() - f.card.min_x(), inset), "row left {at}");
                assert!(near(f.card.max_x() - f.row.max_x(), inset), "row right {at}");
                assert!(near(f.card.max_y() - f.row.max_y(), inset), "row bottom {at}");
                assert!(near(f.text.min_x() - f.card.min_x(), inset), "text left {at}");
                assert!(near(f.card.max_x() - f.text.max_x(), inset), "text right {at}");
                assert!(near(f.text.min_y() - f.card.min_y(), inset), "text top {at}");
                // The title and message are one more inset in, on both sides.
                for part in &f.text_parts {
                    if part.width() > 100.0 {
                        assert!(near(part.min_x() - f.card.min_x(), 2.0 * inset), "part left {at}");
                        assert!(near(f.card.max_x() - part.max_x(), 2.0 * inset), "part right {at}");
                    }
                }
            }
        }
    }

    /// The gaps grow with what they separate: title to message, then message
    /// to buttons.
    #[test]
    fn gaps_grow_from_title_to_message_to_buttons() {
        for (name, size) in [("desktop", DESKTOP), ("phone", PHONE)] {
            let (mut shot, _) = open(forget, size);
            let f = found(&mut shot).expect("card");
            let [title, body] = f.text_parts[..] else {
                panic!("{name}: a title and a message, got {:?}", f.text_parts)
            };
            let title_to_message = body.min_y() - title.max_y();
            let message_to_buttons = f.row.min_y() - body.max_y();
            assert!(near(title_to_message, S2.px()), "{name}: title to message is {title_to_message}");
            assert!(
                near(message_to_buttons, 2.0 * DIALOG_INSET.px()),
                "{name}: message to buttons is {message_to_buttons}"
            );
            assert!(title_to_message < message_to_buttons, "{name}");
        }
    }

    /// Buttons: one row of equal widths a gap apart at desktop; a full-width
    /// column at least 44 px tall on a phone; both with the radius nested in
    /// the card's.
    #[test]
    fn buttons_are_equal_nested_and_stacked_on_a_phone() {
        let gap = S2.px();
        for (label, make) in [("two", forget as fn(&Log) -> Dialog), ("three", conflict)] {
            let (mut shot, _) = open(make, DESKTOP);
            let f = found(&mut shot).expect("card");
            assert!(near(f.card_radius, Radius::Overlay.px()), "{label}: card radius {}", f.card_radius);
            assert!(
                near(f.button_radius, f.card_radius - DIALOG_INSET.px()),
                "{label}: button radius {} is not the card's {} less the inset",
                f.button_radius,
                f.card_radius
            );
            let first = f.buttons[0];
            for pair in f.buttons.windows(2) {
                assert!(near(pair[0].width(), pair[1].width()), "{label}: widths {:?}", f.buttons);
                assert!(near(pair[1].min_x() - pair[0].max_x(), gap), "{label}: gap {:?}", f.buttons);
                assert!(near(pair[0].min_y(), pair[1].min_y()), "{label}: one row");
            }
            assert!(first.height() >= Dim::Button.px_in(SizeClass::Regular) - 0.5);
            assert!(near(f.buttons.last().unwrap().max_x(), f.row.max_x()), "{label}: row fills the card");

            let (mut shot, _) = open(make, PHONE);
            let f = found(&mut shot).expect("card");
            assert!(near(f.button_radius, f.card_radius - DIALOG_INSET.px()), "{label}: phone radius");
            for button in &f.buttons {
                assert!(near(button.width(), f.row.width()), "{label}: full width {:?}", f.buttons);
                assert!(button.height() >= TOUCH_MIN - 0.5, "{label}: {} px tall", button.height());
            }
            for pair in f.buttons.windows(2) {
                assert!(near(pair[1].min_y() - pair[0].max_y(), gap), "{label}: stacked gap {:?}", f.buttons);
            }
        }
    }

    /// At compact density the affirmative response is the top button and
    /// Cancel the bottom one.
    #[test]
    fn a_phone_stacks_the_affirmative_response_first() {
        let (mut shot, log) = open(forget, PHONE);
        let f = found(&mut shot).expect("card");
        let top = f.buttons[0];
        shot.click(top.min_x() + top.width() / 2.0, top.min_y() + top.height() / 2.0);
        shot.settle(Duration::from_millis(400));
        assert_eq!(log.take(), ["forget"], "the top button is Forget");
    }

    /// The card is as wide as its text between the minimum and maximum, and a
    /// long title wraps inside it.
    #[test]
    fn the_card_is_content_sized_between_its_limits_and_a_long_title_wraps() {
        let (mut shot, _) = open(info, DESKTOP);
        let narrow = found(&mut shot).expect("card");
        assert!(
            near(narrow.card.width(), Dim::DialogMin.px_in(SizeClass::Regular)),
            "a short dialog is the minimum width, not {}",
            narrow.card.width()
        );
        for (name, size) in [("desktop", DESKTOP), ("phone", PHONE)] {
            let (mut shot, _) = open(long, size);
            let f = found(&mut shot).expect("card");
            let [title, body] = f.text_parts[..] else { panic!("{name}: title and message") };
            assert!(f.card.width() <= Dim::Dialog.px_in(SizeClass::Regular) + 0.5, "{name}: too wide: {}", f.card.width());
            assert!(f.card.width() <= size.0 * 0.94 + 0.5, "{name}: keeps a margin");
            assert!(title.height() > TextSize::Lg.px_in(SizeClass::Regular) * 1.5, "{name}: the title wraps ({} px)", title.height());
            assert!(title.max_y() <= body.min_y() + 0.5, "{name}: the title does not run into the message");
            assert!(f.card.min_x() >= 0.0 && f.card.max_x() <= size.0, "{name}: inside the window");
        }
    }

    #[test]
    fn enter_runs_the_default_and_a_destructive_dialog_defaults_to_cancel() {
        let (mut shot, log) = open(forget, DESKTOP);
        key(&mut shot, enter());
        assert_eq!(log.take(), ["cancel"]);
        assert!(!is_open(&mut shot), "the dialog closed");
    }

    #[test]
    fn enter_runs_a_suggested_default() {
        let (mut shot, log) = open(restart, DESKTOP);
        key(&mut shot, enter());
        assert_eq!(log.take(), ["restart"]);
    }

    #[test]
    fn tab_moves_the_focus_and_enter_presses_the_focused_button() {
        let (mut shot, log) = open(forget, DESKTOP);
        key(&mut shot, tab());
        key(&mut shot, enter());
        assert_eq!(log.take(), ["forget"], "Tab moved to Forget, and Enter pressed it");
        assert!(!is_open(&mut shot));
    }

    /// The role of the control that has the keyboard focus.
    fn focused_role(shot: &mut Snapshot) -> AccessibilityRole {
        let focused = shot
            .runner()
            .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek());
        let who = shot.runner().find_many(|_, element| {
            let data = element.accessibility();
            (data.a11y_id == Some(focused)).then(|| data.builder.role())
        });
        who.first().copied().expect("the focused node is in the tree")
    }

    #[test]
    fn tab_cycles_among_the_buttons_and_every_press_lands_on_one() {
        let (mut shot, _) = open(restart, DESKTOP);
        // Five presses forward, then one back: the focus is always on one of the
        // two buttons, never on a zero-size edge of the scope, which would
        // spend a press on nothing the person can see.
        for press in 1..=5 {
            key(&mut shot, tab());
            assert_eq!(
                focused_role(&mut shot),
                AccessibilityRole::Button,
                "Tab press {press} landed off the buttons"
            );
        }
        shot.key_with(tab(), Modifiers::SHIFT);
        shot.settle(Duration::from_millis(300));
        assert_eq!(focused_role(&mut shot), AccessibilityRole::Button, "Shift+Tab landed off the buttons");
    }

    /// How many focus rings are showing (settled opacity above 0.9).
    fn visible_rings(shot: &mut Snapshot) -> usize {
        shot.runner()
            .find_many(|_, element| {
                let ring = element.accessibility().builder.class_name() == Some("FocusRing");
                let shown = element.effect().and_then(|effect| effect.opacity).unwrap_or(1.0) > 0.9;
                (ring && shown).then_some(())
            })
            .len()
    }

    #[test]
    fn a_dialog_opened_by_a_click_traps_tab_and_escape_returns_to_the_opener() {
        let log = Log::default();
        let seen = log.clone();
        let mut shot = open_with(
            move || Root { child: FromButton { log: seen.clone() }.into() }.into_element(),
            DESKTOP,
        );
        assert!(shot.click_text("Open dialog"), "the opener is on screen");
        let opener = shot
            .runner()
            .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek());
        shot.settle(Duration::from_millis(700));
        assert!(is_open(&mut shot), "the click opened the dialog");
        for press in 1..=5 {
            key(&mut shot, tab());
            assert_eq!(
                focused_role(&mut shot),
                AccessibilityRole::Button,
                "Tab press {press} left the dialog's buttons"
            );
            let now = shot
                .runner()
                .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek());
            assert_ne!(now, opener, "Tab press {press} reached the page behind the dialog");
            assert_eq!(visible_rings(&mut shot), 1, "Tab press {press}: the focused button shows its ring, and nothing else does");
        }
        key(&mut shot, Key::Named(NamedKey::Escape));
        shot.settle(Duration::from_millis(400));
        let back = shot
            .runner()
            .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek());
        assert_eq!(back, opener, "Escape returns the focus to the opener");
    }

    #[test]
    fn enter_on_a_focused_non_default_button_beats_the_default() {
        let (mut shot, log) = open(restart, DESKTOP);
        shot.key_with(tab(), Modifiers::SHIFT);
        shot.settle(Duration::from_millis(300));
        key(&mut shot, enter());
        assert_eq!(log.take(), ["later"], "Shift+Tab to Later, then Enter");
    }

    #[test]
    fn space_presses_the_focused_button() {
        let (mut shot, log) = open(forget, DESKTOP);
        key(&mut shot, tab());
        key(&mut shot, Key::Character(" ".to_string()));
        assert_eq!(log.take(), ["forget"]);
    }

    #[test]
    fn escape_runs_the_cancel_action() {
        let (mut shot, log) = open(forget, DESKTOP);
        key(&mut shot, Key::Named(NamedKey::Escape));
        assert_eq!(log.take(), ["cancel"], "Escape is the Cancel response, not a silent close");
        assert!(!is_open(&mut shot));

        let (mut shot, log) = open(conflict, DESKTOP);
        key(&mut shot, Key::Named(NamedKey::Escape));
        assert_eq!(log.take(), ["skip"], "cancel_action names the response Escape runs");
    }

    #[test]
    fn a_destructive_default_runs_on_enter_without_having_had_the_focus() {
        let (mut shot, log) = open(delete_default, DESKTOP);
        // The scope has the focus, not the destructive button.
        let focused = shot
            .runner()
            .run_in(|| *Platform::get().current_window().focused_accessibility_id.peek());
        let who = shot.runner().find_many(|_, element| {
            let data = element.accessibility();
            (data.a11y_id == Some(focused)).then(|| data.builder.role())
        });
        assert!(!who.is_empty() && !who.contains(&AccessibilityRole::Button), "the scope has the focus, not a button: {who:?}");
        key(&mut shot, enter());
        assert_eq!(log.take(), ["forget"]);
    }

    #[test]
    fn a_dialog_runs_one_response_once() {
        let (mut shot, log) = open(restart, DESKTOP);
        shot.key(enter());
        shot.key(enter());
        shot.settle(Duration::from_millis(400));
        assert_eq!(log.take(), ["restart"], "a second Enter while it fades out does nothing");
    }

    #[test]
    fn enter_in_a_single_line_field_runs_the_default() {
        let log = Log::default();
        let seen = log.clone();
        let mut shot = open_with(
            move || Root { child: WithField { log: seen.clone() }.into() }.into_element(),
            DESKTOP,
        );
        key(&mut shot, enter());
        assert_eq!(log.take(), ["rename"]);
    }

    /// Writes the specimens to `$DIALOG_PNG_DIR`, for looking at.
    #[test]
    fn specimens_render() {
        let Some(dir) = std::env::var_os("DIALOG_PNG_DIR").map(PathBuf::from) else {
            return;
        };
        std::fs::create_dir_all(&dir).expect("create the specimen directory");
        let scenes: [(&str, fn(&Log) -> Dialog, &[Key]); 7] = [
            ("forget", forget, &[]),
            ("forget-tab", forget, &[Key::Named(NamedKey::Tab)]),
            ("restart", restart, &[]),
            ("delete-default", delete_default, &[]),
            ("long", long, &[]),
            ("warning", warning, &[]),
            ("conflict", conflict, &[]),
        ];
        for (name, size) in [("desktop", DESKTOP), ("phone", PHONE)] {
            for (label, make, keys) in &scenes {
                let (mut shot, _) = open(*make, size);
                for key in *keys {
                    shot.key(key.clone());
                    shot.settle(Duration::from_millis(400));
                }
                shot.save(dir.join(format!("{label}-{name}.png")));
            }
        }
    }
}
