//! What assistive technology hears: names, hints, values, states, relations
//! and spoken announcements.
//!
//! Every bingux-ui control already says its role, name and state; an app
//! reaches for this module only to say more, or to say something a control
//! cannot know:
//!
//! * [`Semantics`] is one value that describes an element: its label, hint,
//!   value, role, states (checked, expanded, selected, busy, required,
//!   invalid), heading level, keyboard shortcut, live politeness and its
//!   relations (labelled by, controls). Put it on any Freya element with
//!   [`SemanticsExt::semantics`], or on a bingux-ui control with the
//!   control's own `.semantics(..)`.
//! * [`FieldLabel`] is the name a row or form line gives the control in it.
//!   A switch, dropdown, slider, stepper or text field with no name of its own
//!   is named by the row's title and described by its subtitle, so
//!   `switch_row("Wi-Fi", ..)` reads "Wi-Fi, switch, on" with no extra code.
//! * [`use_announcer`] says something that has no element of its own ("Copied",
//!   "3 results"): a polite or an assertive live message.
//!
//! The tree reaches the desktop's screen readers (Orca) through AccessKit and
//! AT-SPI; `scripts/a11y-run.sh` dumps what they see.

use std::time::Duration;

use freya::prelude::*;

use crate::component::ComponentName;
pub use crate::controls::CheckState;

/// How urgently a live message interrupts what a screen reader is saying.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Politeness {
    /// After what is being read: a result, a saved state, a finished task.
    Polite,
    /// At once, cutting in: an error the person must act on.
    Assertive,
}

impl Politeness {
    fn live(self) -> accesskit::Live {
        match self {
            Politeness::Polite => accesskit::Live::Polite,
            Politeness::Assertive => accesskit::Live::Assertive,
        }
    }
}

/// Everything an element says to assistive technology beyond what its control
/// already says. Each field left `None` (or `false`) keeps the control's own.
///
/// ```ignore
/// icon_button(Icon::Reload, reload)
///     .tooltip("Reload")
///     .semantics(Semantics::new().hint("Fetches the list again").shortcut("Ctrl+R"));
///
/// rect().semantics(Semantics::new().group("Battery").value("82 %, charging"));
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Semantics {
    pub label: Option<String>,
    pub hint: Option<String>,
    pub value: Option<String>,
    pub role: Option<AccessibilityRole>,
    pub hidden: bool,
    pub heading: Option<usize>,
    pub checked: Option<CheckState>,
    pub expanded: Option<bool>,
    pub selected: Option<bool>,
    pub busy: bool,
    pub required: bool,
    pub invalid: bool,
    pub shortcut: Option<String>,
    pub live: Option<Politeness>,
    pub read_only: bool,
    pub labelled_by: Vec<AccessibilityId>,
    pub described_by: Vec<AccessibilityId>,
    pub error_message: Option<AccessibilityId>,
    pub controls: Vec<AccessibilityId>,
    pub active_descendant: Option<AccessibilityId>,
    pub position: Option<(usize, usize)>,
}

impl Semantics {
    pub fn new() -> Self {
        Self::default()
    }

    /// The name read for the element, in place of the text inside it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// A longer explanation read after the name and role ("Moves it to the
    /// bin"). Say what happens, not how to press it.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The current value as words ("82 %, charging"), for an element that
    /// shows one in a way text does not (a gauge, a picture).
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// A role other than the element's own.
    pub fn role(mut self, role: AccessibilityRole) -> Self {
        self.role = Some(role);
        self
    }

    /// Leaves the element and everything in it out: decoration, or a copy of
    /// something already said.
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// A named group whose members are read as one: "Battery, group".
    pub fn group(self, label: impl Into<String>) -> Self {
        self.role(AccessibilityRole::Group).label(label)
    }

    /// A heading of `level` (1 is the page title, 2 a section).
    pub fn heading(mut self, level: usize) -> Self {
        self.role = Some(AccessibilityRole::Heading);
        self.heading = Some(level.max(1));
        self
    }

    pub fn checked(mut self, state: impl Into<CheckState>) -> Self {
        self.checked = Some(state.into());
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Working: what it shows is about to change.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// The keys that do the same from anywhere in the window ("Ctrl+R"),
    /// written as the menus show them.
    pub fn shortcut(mut self, keys: impl Into<String>) -> Self {
        self.shortcut = Some(keys.into());
        self
    }

    /// Reads every change of the element's name out as it happens.
    pub fn live(mut self, politeness: Politeness) -> Self {
        self.live = Some(politeness);
        self
    }

    /// Named by these elements' text, as a control is by a label beside it.
    pub fn labelled_by(mut self, id: AccessibilityId) -> Self {
        self.labelled_by.push(id);
        self
    }

    /// Described by these elements' text: the help line under a field.
    pub fn described_by(mut self, id: AccessibilityId) -> Self {
        self.described_by.push(id);
        self
    }

    /// Wrong, and `id` is the element that says why ("Enter a port from 1 to
    /// 65535"). Sets [`invalid`](Self::invalid) too.
    pub fn error_message(mut self, id: AccessibilityId) -> Self {
        self.invalid = true;
        self.error_message = Some(id);
        self
    }

    /// Shown and focusable but not changeable.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Changes these elements: a search field and its results, a tab and its page.
    pub fn controls(mut self, id: AccessibilityId) -> Self {
        self.controls.push(id);
        self
    }

    /// The item inside a composite control (a list, a menu, a grid) that the
    /// arrows have reached while the focus stays on this element: a search
    /// field over its results, a combo box over its options (WAI-ARIA).
    pub fn active_descendant(mut self, id: Option<AccessibilityId>) -> Self {
        self.active_descendant = id;
        self
    }

    /// The element is item `index` (from 0) of `count` in its list.
    pub fn position(mut self, index: usize, count: usize) -> Self {
        self.position = Some((index, count));
        self
    }

    /// Writes what is set into an AccessKit node, over what is there.
    pub fn apply(&self, node: &mut accesskit::Node) {
        if let Some(role) = self.role {
            node.set_role(role);
        }
        if let Some(label) = &self.label {
            node.set_label(label.as_str());
        }
        if let Some(hint) = &self.hint {
            node.set_description(hint.as_str());
        }
        if let Some(value) = &self.value {
            node.set_value(value.as_str());
        }
        if self.hidden {
            node.set_hidden();
        }
        if let Some(level) = self.heading {
            node.set_level(level);
        }
        if let Some(state) = self.checked {
            node.set_toggled(match state {
                CheckState::Off => accesskit::Toggled::False,
                CheckState::On => accesskit::Toggled::True,
                CheckState::Mixed => accesskit::Toggled::Mixed,
            });
        }
        if let Some(expanded) = self.expanded {
            node.set_expanded(expanded);
        }
        if let Some(selected) = self.selected {
            node.set_selected(selected);
        }
        if self.busy {
            node.set_busy();
        }
        if self.required {
            node.set_required();
        }
        if self.invalid {
            node.set_invalid(accesskit::Invalid::True);
        }
        if let Some(keys) = &self.shortcut {
            node.set_keyboard_shortcut(keys.as_str());
        }
        if let Some(politeness) = self.live {
            node.set_live(politeness.live());
        }
        for id in &self.labelled_by {
            node.push_labelled_by(*id);
        }
        if self.read_only {
            node.set_read_only();
        }
        for id in &self.described_by {
            node.push_described_by(*id);
        }
        if let Some(id) = self.error_message {
            node.set_error_message(id);
        }
        for id in &self.controls {
            node.push_controlled(*id);
        }
        if let Some(id) = self.active_descendant {
            node.set_active_descendant(id);
        }
        if let Some((index, count)) = self.position {
            node.set_position_in_set(index + 1);
            node.set_size_of_set(count);
        }
    }
}

/// [`Semantics`] on any Freya element (`rect()`, `label()`, ...), and the
/// short forms of its most used parts.
pub trait SemanticsExt: AccessibilityExt + Sized {
    fn semantics(self, semantics: Semantics) -> Self {
        self.a11y_builder(move |node| semantics.apply(node))
    }

    /// A longer explanation, read after the name and role.
    fn a11y_hint(self, hint: impl Into<String>) -> Self {
        let hint = hint.into();
        self.a11y_builder(move |node| node.set_description(hint))
    }

    /// Leaves the element and everything in it out of the tree.
    fn a11y_hidden(self) -> Self {
        self.a11y_builder(|node| node.set_hidden())
    }

    /// A heading of `level`, named by its text.
    fn a11y_heading(self, level: usize) -> Self {
        self.a11y_role(AccessibilityRole::Heading)
            .a11y_builder(move |node| node.set_level(level.max(1)))
    }
}

impl<T: AccessibilityExt> SemanticsExt for T {}

/// The name a row or a form line gives the control in it: its title, and its
/// subtitle as the hint. Provided by [`crate::rows::Row`], `LevelRow`,
/// `EntryRow` and the form lines; read by every control that can sit in one
/// through [`use_field_label`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldLabel {
    pub title: String,
    pub hint: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
struct FieldLabelContext(State<FieldLabel>);

/// Names the controls rendered under this component. Call once in a row's
/// `render`; a new title or hint reaches the controls on the next render.
pub fn provide_field_label(title: &str, hint: Option<&str>) {
    let next = FieldLabel {
        title: title.to_string(),
        hint: hint.map(str::to_string),
    };
    let initial = next.clone();
    let mut context = use_provide_context(move || FieldLabelContext(State::create(initial)));
    context.0.set_if_modified(next);
}

/// The name of the row or form line this control sits in, if any.
pub fn use_field_label() -> Option<FieldLabel> {
    use_try_consume::<FieldLabelContext>().map(|context| context.0.read().clone())
}

/// Names `node` from `field` where the control has no name of its own.
pub fn label_from_field(node: &mut accesskit::Node, field: Option<&FieldLabel>) {
    let Some(field) = field else {
        return;
    };
    if node.label().is_none() && !field.title.is_empty() {
        node.set_label(field.title.as_str());
    }
    if node.description().is_none()
        && let Some(hint) = &field.hint
    {
        node.set_description(hint.as_str());
    }
}

/// How long an announcement stays in the tree after it was made. A screen
/// reader that starts later must not read an old one as new.
const ANNOUNCEMENT_LIFE: Duration = Duration::from_secs(6);

#[derive(Clone, Debug, PartialEq)]
struct Said {
    id: u64,
    text: String,
    politeness: Politeness,
}

/// Says a message to assistive technology with no element of its own: a
/// result count, "Copied", a finished download. Get one with
/// [`use_announcer`]; it is `Copy`, so move it into a handler.
///
/// Toasts and banners announce themselves; use this for what is not shown,
/// or shown somewhere focus is not.
#[derive(Clone, Copy, PartialEq)]
pub struct Announcer {
    said: Option<State<Option<Said>>>,
}

impl Announcer {
    /// Read after whatever is being read now.
    pub fn announce(self, text: impl Into<String>) {
        self.say(text, Politeness::Polite);
    }

    /// Read at once, interrupting. Only for something the person must act on.
    pub fn announce_now(self, text: impl Into<String>) {
        self.say(text, Politeness::Assertive);
    }

    pub fn say(self, text: impl Into<String>, politeness: Politeness) {
        let Some(mut said) = self.said else {
            return;
        };
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        let id = said.peek().as_ref().map_or(0, |last| last.id + 1);
        said.set(Some(Said {
            id,
            text,
            politeness,
        }));
        spawn(async move {
            timer(ANNOUNCEMENT_LIFE).await;
            if said.peek().as_ref().is_some_and(|last| last.id == id) {
                said.set(None);
            }
        });
    }

    /// Whether anything hears this announcer: false outside a window host
    /// (a snapshot, a test without one), where `announce` does nothing.
    pub fn is_connected(self) -> bool {
        self.said.is_some()
    }

    /// The message in the tree now, for tests.
    pub fn current(self) -> Option<(String, Politeness)> {
        self.said.and_then(|said| {
            said.peek()
                .as_ref()
                .map(|said| (said.text.clone(), said.politeness))
        })
    }
}

/// The window's announcer. With no window host above it returns one that
/// says nothing, so a component works in a snapshot too.
pub fn use_announcer() -> Announcer {
    use_try_consume::<Announcer>().unwrap_or(Announcer { said: None })
}

/// Makes the window's announcer: the overlay host calls it once and draws
/// [`AnnouncerRegion`] beside the page.
pub(crate) fn provide_announcer() -> Announcer {
    let announcer = use_hook(|| Announcer {
        said: Some(State::create(None)),
    });
    use_provide_context(move || announcer);
    announcer
}

/// The live region an [`Announcer`] speaks through: a one-pixel status node
/// with no paint, made anew for each message (a new live node is read even
/// when its words are those of the last one).
#[derive(PartialEq)]
pub(crate) struct AnnouncerRegion {
    pub(crate) announcer: Announcer,
}

impl Component for AnnouncerRegion {
    fn render(&self) -> impl IntoElement {
        let said = self.announcer.said.and_then(|said| said.read().clone());
        rect()
            .component("Announcer")
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::px(1.0))
            .height(Size::px(1.0))
            .interactive(false)
            .maybe_child(said.map(|said| {
                rect()
                    .key(said.id)
                    .width(Size::px(1.0))
                    .height(Size::px(1.0))
                    .a11y_role(AccessibilityRole::Status)
                    .a11y_alt(said.text)
                    .a11y_builder(move |node| node.set_live(said.politeness.live()))
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantics_write_every_part_they_set() {
        let mut node = accesskit::Node::new(AccessibilityRole::Button);
        node.set_label("Old");
        Semantics::new()
            .label("Reload")
            .hint("Fetches the list again")
            .shortcut("Ctrl+R")
            .busy(true)
            .expanded(false)
            .position(2, 5)
            .apply(&mut node);
        assert_eq!(node.label(), Some("Reload"));
        assert_eq!(node.description(), Some("Fetches the list again"));
        assert_eq!(node.keyboard_shortcut(), Some("Ctrl+R"));
        assert!(node.is_busy());
        assert_eq!(node.is_expanded(), Some(false));
        assert_eq!(
            node.position_in_set(),
            Some(3),
            "position is one-based for AT"
        );
        assert_eq!(node.size_of_set(), Some(5));
        assert_eq!(
            node.role(),
            AccessibilityRole::Button,
            "no role given keeps the control's"
        );
    }

    #[test]
    fn unset_parts_leave_the_control_alone() {
        let mut node = accesskit::Node::new(AccessibilityRole::CheckBox);
        node.set_label("Remember");
        node.set_toggled(accesskit::Toggled::True);
        Semantics::new().hint("Stays signed in").apply(&mut node);
        assert_eq!(node.label(), Some("Remember"));
        assert_eq!(node.toggled(), Some(accesskit::Toggled::True));
        assert!(!node.is_hidden());
    }

    #[test]
    fn a_heading_has_its_role_and_level() {
        let mut node = accesskit::Node::new(AccessibilityRole::Label);
        Semantics::new().heading(0).apply(&mut node);
        assert_eq!(node.role(), AccessibilityRole::Heading);
        assert_eq!(node.level(), Some(1), "level 0 is not a level");
    }

    #[test]
    fn a_field_names_only_a_control_with_no_name() {
        let field = FieldLabel {
            title: "Wi-Fi".into(),
            hint: Some("Connected to Home".into()),
        };
        let mut unnamed = accesskit::Node::new(AccessibilityRole::Switch);
        label_from_field(&mut unnamed, Some(&field));
        assert_eq!(unnamed.label(), Some("Wi-Fi"));
        assert_eq!(unnamed.description(), Some("Connected to Home"));

        let mut named = accesskit::Node::new(AccessibilityRole::Switch);
        named.set_label("Airplane mode");
        label_from_field(&mut named, Some(&field));
        assert_eq!(named.label(), Some("Airplane mode"));
    }

    #[test]
    fn checked_maps_mixed() {
        let mut node = accesskit::Node::new(AccessibilityRole::CheckBox);
        Semantics::new().checked(CheckState::Mixed).apply(&mut node);
        assert_eq!(node.toggled(), Some(accesskit::Toggled::Mixed));
    }
}
