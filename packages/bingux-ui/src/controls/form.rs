//! Forms: a labelled field with a hint and an inline error, the live
//! values beside a form, and the apply bar of a form that edits a profile.
//!
//! [`form_field`] is a [`Group`](crate::group::Group) row: the title (and
//! its hint) at the left and the input at the right, or above it on a
//! phone. The error shows under the input in the danger colour, announced
//! to assistive technology as an alert. Put fields in a group, as with
//! [`form_row`](crate::group::form_row):
//!
//! ```ignore
//! Group::titled("Server")
//!     .row(form_field("Address", address).hint("Host name or IP").validate(validate::hostname_or_ip))
//!     .row(form_field("Port", port).validate(validate::port).suffix("TCP"))
//! ```
//!
//! A form with a draft shows [`pending_bar`] while the draft differs from
//! what is saved, and [`live_state`] lists what is true right now, so a
//! person sees the effect of what they set.
//!
//! Keyboard: Tab follows the visual order (title, then the field; a
//! password field's reveal button after it), Enter in a single-line field
//! calls [`FormField::on_submit`], and a multiline field keeps Enter for
//! new lines.

pub mod validate;

use std::rc::Rc;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::button::{action_button, primary_button};
use crate::controls::text_field::text_field;
use crate::group::{Group, caption, use_row_match};
use crate::search_highlight;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink};

/// How a field is checked: a plain function, or a closure kept by pointer.
#[derive(Clone)]
enum Check {
    Plain(fn(&str) -> Result<(), String>),
    Shared(Rc<dyn Fn(&str) -> Result<(), String>>),
}

impl Check {
    fn run(&self, text: &str) -> Result<(), String> {
        match self {
            Check::Plain(check) => check(text),
            Check::Shared(check) => check(text),
        }
    }
}

impl PartialEq for Check {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Check::Plain(a), Check::Plain(b)) => *a as usize == *b as usize,
            (Check::Shared(a), Check::Shared(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// A labelled text field for a form. See the [module](self).
#[derive(PartialEq)]
pub struct FormField {
    title: String,
    value: State<String>,
    hint: Option<String>,
    placeholder: Option<String>,
    password: bool,
    monospace: bool,
    suffix: Option<String>,
    optional: bool,
    error: Option<String>,
    check: Option<Check>,
    lines: Option<usize>,
    on_submit: Option<EventHandler<()>>,
    autofocus: bool,
    trailing: Option<Element>,
    wide: bool,
}

/// A form row: `title` on the left (above on a phone) and a text field
/// bound to `value` on the right. It implements `Into<Element>`.
pub fn form_field(title: impl Into<String>, value: State<String>) -> FormField {
    FormField {
        title: title.into(),
        value,
        hint: None,
        placeholder: None,
        password: false,
        monospace: false,
        suffix: None,
        optional: false,
        error: None,
        check: None,
        lines: None,
        on_submit: None,
        autofocus: false,
        trailing: None,
        wide: false,
    }
}

impl FormField {
    /// One muted line under the title, for what to enter or why.
    pub fn hint(mut self, text: impl Into<String>) -> Self {
        self.hint = Some(text.into());
        self
    }

    /// Greyed text in the empty field, such as an example value.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// Mask the text; the field has a button to reveal it.
    pub fn password(mut self) -> Self {
        self.password = true;
        self
    }

    /// Show the text in the monospace face, for keys and addresses.
    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }

    /// A muted unit inside the right end of the field, such as `bytes` or `s`.
    pub fn suffix(mut self, text: impl Into<String>) -> Self {
        self.suffix = Some(text.into());
        self
    }

    /// Add the muted word "Optional" after the title. The field is not
    /// checked while it is empty either way.
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Show `message` under the field, now. `None` clears it. An explicit
    /// message wins over [`FormField::validate`], and shows whether or not the field
    /// was touched, so a failed Apply can point at the field.
    pub fn error(mut self, message: Option<String>) -> Self {
        self.error = message;
        self
    }

    /// Check the text with `check` (see [`validate`]) and show its message
    /// under the field. The message waits for the person: it shows only
    /// when the field is not empty and has been left (focus moved on) or
    /// submitted with Enter, never on an untouched or empty field, and
    /// then follows every keystroke, so it goes away as soon as the text is
    /// right. The form must run the same check before applying.
    pub fn validate(mut self, check: fn(&str) -> Result<(), String>) -> Self {
        self.check = Some(Check::Plain(check));
        self
    }

    /// [`FormField::validate`] with a closure, such as
    /// `validate::integer_in(1..=9000)`. The closure is compared by
    /// identity, so the field redraws whenever its parent does; keep
    /// that in mind for large forms and prefer a plain function there.
    pub fn validate_with(mut self, check: impl Fn(&str) -> Result<(), String> + 'static) -> Self {
        self.check = Some(Check::Shared(Rc::new(check)));
        self
    }

    /// A multiline field `lines` lines tall, for pasted keys and lists. Enter
    /// adds a line; there is no submit.
    pub fn multiline(mut self, lines: usize) -> Self {
        self.lines = Some(lines);
        self
    }

    /// Called when Enter is pressed in a single-line field.
    pub fn on_submit(mut self, f: impl Into<EventHandler<()>>) -> Self {
        self.on_submit = Some(f.into());
        self
    }

    /// Take keyboard focus when the field first appears. Only one field of a
    /// page should ask for it.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }

    /// An element after the input, on the same line (on a phone too), such
    /// as the remove button of a list entry.
    pub fn trailing(mut self, element: impl Into<Element>) -> Self {
        self.trailing = Some(element.into());
        self
    }

    /// A wider field on a desktop (`Dim::FieldWide`, 440 px), for a key or a
    /// URL that does not fit 240 px. Give every field of a group the same
    /// width so their left edges line up. On a phone every field fills the row.
    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }
}

impl Component for FormField {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let compact = is_compact();
        let own_id = use_a11y();
        let focus = use_focus(own_id);
        // The message waits until the person has left the field or pressed
        // Enter in it, so a half-typed address is not called wrong.
        let mut touched = use_state(|| false);
        let mut was_focused = use_state(|| false);
        let focused = focus().is_focused();
        use_side_effect_with_deps(&focused, move |focused| {
            if *focused {
                was_focused.set(true);
            } else if *was_focused.peek() {
                touched.set(true);
            }
        });
        let row_hit = search_highlight::current().as_deref().is_some_and(|query| {
            search_highlight::hits(query, &self.title)
                || self
                    .hint
                    .as_deref()
                    .is_some_and(|hint| search_highlight::hits(query, hint))
        });
        use_row_match(row_hit);

        let message = self.error.clone().or_else(|| {
            let check = self.check.as_ref()?;
            if !touched() {
                return None;
            }
            let text = self.value.read();
            if text.is_empty() {
                return None;
            }
            check.run(&text).err()
        });
        // A field that can fail keeps its message line, so the row does not
        // jump when the message comes and goes.
        let reserve = self.check.is_some() || self.error.is_some();
        let wide = self.wide;

        let submit = self.on_submit.clone();
        let mut field = text_field(self.value, move |_: String| {
            touched.set(true);
            if let Some(submit) = &submit {
                submit.call(());
            }
        })
        .a11y_id(own_id)
        .maybe_placeholder(self.placeholder.clone())
        .danger_outline(message.is_some());
        if compact {
            field = field.fill_width();
        } else {
            field = field.width(if wide { Dim::FieldWide } else { Dim::Field });
        }
        if let Some(suffix) = &self.suffix {
            field = field.suffix(suffix.clone());
        }
        if self.password {
            field = field.password();
        }
        if self.monospace {
            field = field.monospace();
        }
        if let Some(lines) = self.lines {
            field = field.multiline().lines(lines);
        }
        if self.autofocus {
            field = field.autofocus();
        }

        let title_line = rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
            .child(search_highlight::label(BODY, &tokens, &self.title))
            .maybe_child(
                self.optional
                    .then(|| CAPTION.in_ink(Ink::Muted).label(&tokens, "Optional")),
            );
        let hint = self.hint.clone().map(caption);
        let input_line = rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
            .maybe(compact, |line| {
                line.tw(&tokens, &[grow_children(), w_full()])
            })
            // On a phone the field takes what the trailing element leaves.
            .child(if compact {
                Element::from(rect().tw(&tokens, &[w_flex()]).child(field))
            } else {
                Element::from(field)
            })
            .maybe_child(self.trailing.clone());
        // The message sits under the whole row, not under the input: a
        // column that is only as wide as the input would wrap it to three
        // lines and move the rows below it.
        let error_line = reserve.then(|| error_line(&tokens, message));
        let outer = rect().component("FormField").tw(
            &tokens,
            &[flex_col(), w_full(), px(GUTTER), py(S3), gap(S1)],
        );

        if compact {
            outer
                .tw(&tokens, &[gap(S2)])
                .child(title_line)
                .maybe_child(hint)
                .child(input_line)
                .maybe_child(error_line)
        } else {
            // The title sits level with the first line of the input, so a
            // multiline field or a hint grows the row downwards, not around.
            let title_side = rect()
                .tw(&tokens, &[flex_col(), w_flex(), gap(S0_5)])
                .child(
                    rect()
                        .tw(
                            &tokens,
                            &[flex_row(), items(Align::Center), h(Dim::Control)],
                        )
                        .child(title_line),
                )
                .maybe_child(hint);
            outer
                .child(
                    rect()
                        .tw(
                            &tokens,
                            &[
                                flex_row(),
                                grow_children(),
                                items(Align::Start),
                                w_full(),
                                gap(S3),
                            ],
                        )
                        .child(title_side)
                        .child(input_line),
                )
                .maybe_child(error_line)
        }
    }
}

/// The message line under a field: one caption line tall when empty, so the
/// row keeps its height when a one-line message comes, and an alert for
/// assistive technology when it has text.
fn error_line(tokens: &crate::theme::Tokens, message: Option<String>) -> Element {
    match message {
        Some(message) => rect()
            .tw(tokens, &[w_full()])
            .a11y_role(AccessibilityRole::Alert)
            .a11y_alt(message.clone())
            .child(
                CAPTION
                    .in_ink(Ink::Danger)
                    .paragraph(tokens, message)
                    .max_lines(3),
            )
            .into(),
        None => rect().tw(tokens, &[w_full(), h(Dim::CaptionLine)]).into(),
    }
}

/// A read-only "Right now" group of label and value rows, for beside or above
/// a form: the address, gateway, DNS and link speed a connection really
/// has. Values are in the monospace face and can be selected and copied.
/// An empty value shows as a dash.
pub fn live_state(rows: Vec<(String, String)>) -> Element {
    Group::titled("Right now")
        .rows(rows.into_iter().map(|(label, value)| {
            let value = if value.is_empty() {
                "\u{2013}".to_string()
            } else {
                value
            };
            crate::rows::row(label).mono_value(value).into()
        }))
        .into()
}

/// The bar of a form that applies its changes together: "You have unsaved
/// changes" with Revert and Apply. It exists only while `dirty`, so it
/// returns `None` otherwise; put it at the top of the page's form with
/// `.maybe_child(..)`. Apply is disabled while `valid` is false.
///
/// The bar is a keyed box around its group: Freya pairs unkeyed siblings of
/// one type by position, so a bare group appearing above other groups would
/// take over the first one's state and rebuild the fields below it, and the
/// field being typed in would lose focus.
pub fn pending_bar(
    dirty: bool,
    valid: bool,
    on_apply: impl Into<EventHandler<()>>,
    on_revert: impl Into<EventHandler<()>>,
) -> Option<Element> {
    dirty.then(|| {
        let bar = Group::new(None::<String>).row(PendingBar {
            valid,
            on_apply: on_apply.into(),
            on_revert: on_revert.into(),
        });
        rect()
            .key("pending-bar")
            .tw_layout(&[w_full()])
            .child(bar)
            .into()
    })
}

#[derive(PartialEq)]
struct PendingBar {
    valid: bool,
    on_apply: EventHandler<()>,
    on_revert: EventHandler<()>,
}

impl Component for PendingBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let note = BODY.paragraph(&tokens, "You have unsaved changes");
        let phone = is_compact();
        let revert = action_button("Revert", self.on_revert.clone()).compact();
        let apply = primary_button("Apply", self.on_apply.clone())
            .compact()
            .enabled(self.valid);
        let buttons = rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
            .maybe(phone, |line| {
                line.tw(&tokens, &[w_full(), justify(Align::End)])
            })
            .child(revert)
            .child(apply);
        if phone {
            rect()
                .component("PendingBar")
                .tw(
                    &tokens,
                    &[flex_col(), w_full(), px(GUTTER), py(S3), gap(S3)],
                )
                .child(note)
                .child(buttons)
        } else {
            rect()
                .component("PendingBar")
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        w_full(),
                        h(Dim::Row),
                        px(GUTTER),
                        gap(S3),
                    ],
                )
                .child(rect().tw(&tokens, &[w_flex()]).child(note))
                .child(buttons)
        }
    }
}
