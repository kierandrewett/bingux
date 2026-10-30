//! Form dialogs: a card with a header, a body that scrolls and a footer with
//! Cancel and the action (`AdwDialog` with a form, macOS sheet, GNOME Files'
//! Compress and Extract).
//!
//! An alert ([`dialog`](super::dialog::dialog)) is a question with up to
//! three answers and no more than a field in it. A form dialog is for a
//! handful of settings and one action: Compress, Extract, Add a network.
//!
//! # Layout
//!
//! The card is one size, whatever the body holds: a header (the title and a
//! close button, the standard [`HeaderBar`](super::window::HeaderBar)), the
//! body, which scrolls inside the card, and a footer of two buttons. Options
//! that appear and disappear do so with [`reveal`](crate::motion::reveal), so
//! the card never grows or shrinks and nothing under an option jumps.
//!
//! The buttons are the alert's: Cancel first and the action last, equal
//! widths, the action filled with the accent colour and carrying the key cap
//! with the Return glyph. At compact density the card fills the window as a
//! sheet and the buttons stack, the action on top.
//!
//! # Keyboard
//!
//! Enter runs the action wherever the focus is, unless the thing that has it
//! takes Enter itself (a button, a multi-line field, an open dropdown). A
//! single-line field's own Enter reaches the action too. Escape, Back and a
//! press on the scrim close the dialog, as for every dialog. Tab cycles
//! inside the card.
//!
//! # The action while the form is not valid
//!
//! The action is disabled, and the footer says why, in words, next to it: the
//! reason is what a person needs, a dimmed button is not.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::{action_button, icon_button, primary_button};
use crate::icons::Icon;
use crate::shell::dialog::framed_dialog;
use crate::shell::overlays::{Overlays, use_overlays};
use crate::shell::window::HeaderBar;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// The size of a form dialog's card on a desktop, in logical pixels. It never
/// grows past the window: a small window shrinks it, a phone fills it.
pub const FORM_WIDTH: f32 = 520.0;
/// See [`FORM_WIDTH`].
pub const FORM_HEIGHT: f32 = 680.0;

/// A form dialog's contents: render it inside the component that owns the
/// form's state, so the action, the reason it is blocked and the body always
/// follow what is typed.
#[derive(Clone, PartialEq)]
pub struct FormDialog {
    title: String,
    body: Element,
    action: String,
    /// Why the action cannot run now; `None` when it can.
    blocked: Option<String>,
    /// Shown in the footer while the action is busy (a check that takes a
    /// moment); the action is disabled meanwhile.
    busy: Option<String>,
    on_action: EventHandler<()>,
    /// Whether pressing the action closes the dialog. A form that checks
    /// something first closes it itself, with [`Overlays::dismiss_dialog`].
    closes: bool,
}

/// A form dialog titled `title` with `body`, whose action is labelled
/// `action` ("Create", "Extract").
pub fn form_dialog(
    title: impl Into<String>,
    body: impl Into<Element>,
    action: impl Into<String>,
    on_action: impl Into<EventHandler<()>>,
) -> FormDialog {
    FormDialog {
        title: title.into(),
        body: body.into(),
        action: action.into(),
        blocked: None,
        busy: None,
        on_action: on_action.into(),
        closes: true,
    }
}

impl FormDialog {
    /// The action is disabled, and the footer says why.
    pub fn blocked(mut self, reason: Option<String>) -> Self {
        self.blocked = reason;
        self
    }

    /// The action is disabled while `message` shows in the footer.
    pub fn busy(mut self, message: Option<String>) -> Self {
        self.busy = message;
        self
    }

    /// The action does not close the dialog; its handler does, when it is
    /// satisfied.
    pub fn stays_open(mut self) -> Self {
        self.closes = false;
        self
    }
}

/// Put a form dialog on screen. `content` is the component that renders a
/// [`FormDialog`]; `closed` is called once the dialog has faded out and is
/// gone, whichever way it closed. A phone gets a sheet that fills the window.
pub fn open_form(
    overlays: Overlays,
    title: impl Into<String>,
    content: impl Into<Element>,
    closed: Option<EventHandler<()>>,
) {
    let dialog = framed_dialog(title, content, FORM_WIDTH, FORM_HEIGHT);
    let dialog = if is_compact() { dialog.fill_window() } else { dialog };
    let dialog = match closed {
        Some(closed) => dialog.on_closed(closed),
        None => dialog,
    };
    overlays.show_dialog(dialog);
}

impl Component for FormDialog {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let form = self;
        let compact = is_compact();
        // Concentric with the card's corners, as the alert's buttons are.
        let button_radius = Radius::Overlay.nested(DIALOG_INSET.px());
        let runnable = form.blocked.is_none() && form.busy.is_none();
        let act = {
            let on_action = form.on_action.clone();
            let closes = form.closes;
            move || {
                on_action.call(());
                if closes {
                    overlays.dismiss_dialog();
                }
            }
        };
        let press = {
            let act = act.clone();
            move |_: ()| act()
        };
        let cancel = move |_: ()| overlays.dismiss_dialog();

        let header = HeaderBar::new(form.title.clone()).draggable(false).trailing(
            icon_button(Icon::WindowClose, move |_| overlays.dismiss_dialog()).tooltip("Close"),
        );
        let action = primary_button(form.action.clone(), press)
            .default_action()
            .enabled(runnable)
            .radius(button_radius);
        let dismiss = action_button("Cancel", cancel).radius(button_radius);
        let (action, dismiss) = if compact {
            (action.fill_width(), dismiss.fill_width())
        } else {
            (action.grow(), dismiss.grow())
        };
        // Why the action cannot run, or what is being waited for: one line,
        // always the same height, so the footer never jumps.
        let note = form.busy.clone().or_else(|| form.blocked.clone());
        let note_ink = if form.busy.is_some() { Ink::Muted } else { Ink::Warning };
        let note_line = rect()
            .tw(&tokens, &[w_full(), min_h(Dim::Control), flex_row(), items(Align::Center)])
            .child(
                CAPTION
                    .in_ink(if note.is_some() { note_ink } else { Ink::Muted })
                    .paragraph(&tokens, note.unwrap_or_default())
                    .width(Size::fill())
                    .max_lines(2)
                    .text_overflow(TextOverflow::Ellipsis),
            );
        let buttons = if compact {
            rect()
                .tw(&tokens, &[w_full(), flex_col(), gap(S2)])
                .child(action)
                .child(dismiss)
        } else {
            rect()
                .tw(&tokens, &[w_full(), flex_row(), grow_children(), gap(S2)])
                .child(dismiss)
                .child(action)
        };
        let footer = rect()
            .component("FormDialogFooter")
            .tw(&tokens, &[w_full(), flex_col(), p(DIALOG_INSET), gap(S2)])
            .child(note_line)
            .child(buttons);

        // Enter from anything inside that does not take it runs the action.
        let keys = {
            let act = act.clone();
            move |event: Event<KeyboardEventData>| {
                if event.key == Key::Named(NamedKey::Enter)
                    && !event.modifiers.intersects(Modifiers::ALT | Modifiers::CONTROL | Modifiers::META)
                    && runnable
                {
                    event.stop_propagation();
                    act();
                }
            }
        };
        rect()
            .component("FormDialog")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .on_key_down(keys)
            .child(header)
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .component("FormDialogBody")
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(
                        ScrollView::new().child(
                            rect()
                                .tw(&tokens, &[flex_col(), w_full(), gap(S4), p(S4)])
                                .child(form.body.clone()),
                        ),
                    ),
            )
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(footer)
    }
}
