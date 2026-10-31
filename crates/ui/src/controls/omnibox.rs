//! The omnibox: a text field that grows a card of suggestions out of itself,
//! as the address bar of a browser does.
//!
//! ```text
//! resting          editing           typing
//! [ field ]        [ field ]         +--------------------------+
//!                                    | field                    |   the same left edge, width
//!                  nothing opens      | [This folder] [Home] ... |   and first row as the field
//!                                    |   row  row  row          |
//!                                    +--------------------------+
//! ```
//!
//! The card is the field's own box made taller, laid over what is under it: it
//! begins at the field's top-left corner, is as wide as the field, and keeps
//! the field as its first row. The field's place in the layout is held by a
//! box of the field's height, so nothing around it moves when the card opens
//! or closes, and the card never leaves the window (it is no taller than the
//! room below the field). It comes only once the person has typed, and goes
//! when Escape is pressed, when the field is left or when there is nothing to
//! offer; editing without it is a plain field.
//!
//! What is offered is the caller's: a list of [`OmniItem`]s, rows to choose,
//! headings, notices and strips of [`omni_chip`]s. The omnibox owns the
//! motion, the keys and the highlight:
//!
//! | Key | Does |
//! | --- | ---- |
//! | Down, Up | the next or previous row, wrapping; Down shows a card that Escape closed |
//! | Enter | the highlighted row (its `on_press`), else `on_submit` with the text |
//! | Ctrl+Enter | the highlighted row's chip |
//! | Tab, Shift+Tab | the row's `fill` or the common start the caller offers ([`Omnibox::complete`]); else the next or previous row |
//! | Escape | the card closes; with no card, `on_close` |
//!
//! The card grows over [`MOTION_MS`](crate::shell::panel::MOTION_MS) from the
//! height it is at: a card closed half way opens again from there, and rows
//! that stream in make it grow a little, never jump. With reduced motion it
//! is at its height at once. The rows are `listbox` options and the card's
//! first row is the field, a combobox for assistive technology.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::text_field;
use crate::icons::{self, Icon};
use crate::motion::use_tween;
use crate::shell::panel::{MOTION_MS, PANEL_EDGE};
use crate::style::*;
use crate::surface::{use_tokens, window_size};
use crate::typography::{BODY, CAPTION, EMPHASIS, Ink};

/// The tallest the list under the field grows, however much room the window
/// has: about ten rows.
const LIST_MAX: f32 = 440.0;

/// A piece of a row's text.
#[derive(Clone, PartialEq, Debug)]
pub struct OmniPart {
    pub text: String,
    /// Drawn heavier: the part of the name that matches what was typed.
    pub emphasis: bool,
    /// Drawn in the muted ink: what follows a name.
    pub muted: bool,
}

impl OmniPart {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            emphasis: false,
            muted: false,
        }
    }

    pub fn muted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            emphasis: false,
            muted: true,
        }
    }

    /// `name` with the bytes of `matched` heavier and `after` muted behind it.
    /// A range that does not lie on character boundaries of `name` marks
    /// nothing.
    pub fn matched(name: &str, matched: std::ops::Range<usize>, after: &str) -> Vec<OmniPart> {
        let mut parts = Vec::new();
        let valid = matched.start <= matched.end
            && matched.end <= name.len()
            && name.is_char_boundary(matched.start)
            && name.is_char_boundary(matched.end);
        let matched = if valid { matched } else { 0..0 };
        if matched.start > 0 {
            parts.push(OmniPart::plain(&name[..matched.start]));
        }
        if !matched.is_empty() {
            parts.push(OmniPart {
                text: name[matched.clone()].to_string(),
                emphasis: true,
                muted: false,
            });
        }
        if matched.end < name.len() {
            parts.push(OmniPart::plain(&name[matched.end..]));
        }
        if parts.is_empty() {
            parts.push(OmniPart::plain(name));
        }
        if !after.is_empty() {
            parts.push(OmniPart::muted(after));
        }
        parts
    }

    /// `name` with every one of `ranges` (byte ranges, in order, not
    /// overlapping) heavier: what a search marked. A range that does not lie on
    /// character boundaries is left out.
    pub fn highlighted(name: &str, ranges: &[std::ops::Range<usize>]) -> Vec<OmniPart> {
        let mut parts = Vec::new();
        let mut at = 0;
        for range in ranges {
            let fits = range.start >= at
                && range.start < range.end
                && range.end <= name.len()
                && name.is_char_boundary(range.start)
                && name.is_char_boundary(range.end);
            if !fits {
                continue;
            }
            if range.start > at {
                parts.push(OmniPart::plain(&name[at..range.start]));
            }
            parts.push(OmniPart {
                text: name[range.clone()].to_string(),
                emphasis: true,
                muted: false,
            });
            at = range.end;
        }
        if at < name.len() {
            parts.push(OmniPart::plain(&name[at..]));
        }
        if parts.is_empty() {
            parts.push(OmniPart::plain(name));
        }
        parts
    }
}

/// One choosable row: a glyph, a name with its match marked, what follows it
/// and, optionally, a chip for the other thing to do with it.
#[derive(Clone, PartialEq)]
pub struct OmniRow {
    /// What stands before the name: an icon, drawn at the control size.
    pub lead: Element,
    pub parts: Vec<OmniPart>,
    /// Muted text after the name, such as the folder a result is in.
    pub detail: Option<String>,
    /// The other thing to do (`Show in folder`): a label and what it does.
    /// Ctrl+Enter and a press on the chip.
    pub chip: Option<(String, EventHandler<()>)>,
    /// What Enter, and a press on the row, does.
    pub on_press: EventHandler<()>,
    /// The text Tab puts in the field while this row is highlighted.
    pub fill: Option<String>,
    /// The row's name for assistive technology.
    pub name: String,
}

/// What a notice says, and in which ink.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum OmniNotice {
    /// Something is going on: a spinner before the text.
    Busy,
    /// Nothing to choose, said quietly: "No results".
    Quiet,
    /// Why what was typed cannot be used.
    Danger,
}

/// The field's accessibility id, provided to what the card holds, so a press on a chip gives the
/// keyboard back to the field (a press takes it).
#[derive(Clone, Copy)]
struct FieldFocus(AccessibilityId);

/// One thing of the card.
#[derive(Clone, PartialEq)]
pub enum OmniItem {
    Row(OmniRow),
    /// A caption over the rows that follow, with something muted at its end
    /// (a count, "Searching").
    Heading {
        text: String,
        detail: Option<String>,
        busy: bool,
    },
    Notice {
        text: String,
        kind: OmniNotice,
    },
    /// A strip of controls (chips): `height` is the strip's.
    Strip {
        element: Element,
        height: f32,
    },
}

impl OmniItem {
    fn height(&self) -> f32 {
        match self {
            OmniItem::Row(_) | OmniItem::Notice { .. } => Dim::MenuItem.px(),
            OmniItem::Heading { .. } => Dim::Box7.px(),
            OmniItem::Strip { height, .. } => *height,
        }
    }

    fn is_row(&self) -> bool {
        matches!(self, OmniItem::Row(_))
    }
}

/// The text of an omnibox, and the count that makes the field a new one when
/// the text is set from outside (a field that is new puts its caret at the end
/// of the text it starts with). Copy; the owner keeps it where the text
/// should outlive the field (a tab's state).
#[derive(Clone, Copy, PartialEq)]
pub struct OmniDraft {
    value: State<String>,
    fresh: State<u32>,
}

impl OmniDraft {
    /// A draft with no text, made in a state scope that lives as long as the
    /// owner.
    pub fn create() -> Self {
        Self {
            value: State::create(String::new()),
            fresh: State::create(0),
        }
    }

    pub fn use_new() -> Self {
        Self {
            value: use_state(String::new),
            fresh: use_state(|| 0),
        }
    }

    /// The text now, without subscribing.
    pub fn text(&self) -> String {
        self.value.peek().clone()
    }

    /// The text now; the caller draws again when it changes.
    pub fn read(&self) -> String {
        self.value.read().clone()
    }

    /// Put `text` in the field, with the caret at its end.
    pub fn set(&mut self, text: impl Into<String>) {
        self.value.set(text.into());
        *self.fresh.write() += 1;
    }

    /// Put `text` in without moving the caret of the field that has it (the
    /// field's own typing already did).
    pub fn set_in_place(&mut self, text: impl Into<String>) {
        self.value.set(text.into());
    }
}

/// The omnibox. See the module documentation.
#[derive(PartialEq)]
pub struct Omnibox {
    draft: OmniDraft,
    /// The text the field opened with: it is not "typed" until it differs.
    initial: String,
    items: Vec<OmniItem>,
    placeholder: String,
    leading: Icon,
    /// The field's name for assistive technology.
    label: String,
    danger: bool,
    select_all: bool,
    /// Highlight the first row whenever the rows change; else none until an
    /// arrow key.
    auto_highlight: bool,
    complete: Option<String>,
    close_action: Option<(Icon, String)>,
    on_change: Option<EventHandler<String>>,
    on_submit: EventHandler<String>,
    on_close: EventHandler<()>,
    alt_digit: Option<(usize, EventHandler<usize>)>,
}

impl Omnibox {
    /// `draft` is the text; `on_submit` gets it on Enter when no row is
    /// highlighted; `on_close` is called when the field is left (Escape with no
    /// card, focus gone elsewhere).
    pub fn new(
        draft: OmniDraft,
        on_submit: impl Into<EventHandler<String>>,
        on_close: impl Into<EventHandler<()>>,
    ) -> Self {
        Self {
            draft,
            initial: draft.text(),
            items: Vec::new(),
            placeholder: String::new(),
            leading: Icon::Search,
            label: "Location and search".into(),
            danger: false,
            select_all: false,
            auto_highlight: false,
            complete: None,
            close_action: None,
            on_change: None,
            alt_digit: None,
            on_submit: on_submit.into(),
            on_close: on_close.into(),
        }
    }

    /// The text the field rests with: the field counts as typed in once its
    /// text is anything else, from the first frame.
    pub fn initial(mut self, text: impl Into<String>) -> Self {
        self.initial = text.into();
        self
    }

    /// What the card offers now. Empty: no card.
    pub fn items(mut self, items: Vec<OmniItem>) -> Self {
        self.items = items;
        self
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn leading(mut self, icon: Icon) -> Self {
        self.leading = icon;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Outline the field in the danger colour.
    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    /// Select the text when the field appears, so typing replaces it.
    pub fn select_all(mut self, select_all: bool) -> Self {
        self.select_all = select_all;
        self
    }

    /// The first row is highlighted whenever the rows change (Enter takes it).
    pub fn auto_highlight(mut self, auto: bool) -> Self {
        self.auto_highlight = auto;
        self
    }

    /// The text Tab fills in while no row's own `fill` applies (the common
    /// start of the candidates).
    pub fn complete(mut self, text: Option<String>) -> Self {
        self.complete = text;
        self
    }

    /// A button at the field's end that leaves the field, for a window with no
    /// Escape key (a phone).
    pub fn close_action(mut self, icon: Icon, label: impl Into<String>) -> Self {
        self.close_action = Some((icon, label.into()));
        self
    }

    /// Called with every text the person types.
    pub fn on_change(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_change = Some(handler.into());
        self
    }

    /// Alt+1 to Alt+`most` choose among the chips of the card: `handler` gets
    /// the digit (1 is the first). The field takes the key, so it does not
    /// also switch tabs while the field has the keyboard.
    pub fn alt_digit(mut self, most: usize, handler: impl Into<EventHandler<usize>>) -> Self {
        self.alt_digit = Some((most, handler.into()));
        self
    }
}

/// Where each item of `items` begins, and how tall they are together.
fn layout(items: &[OmniItem]) -> (Vec<f32>, f32) {
    let mut tops = Vec::with_capacity(items.len());
    let mut top = 0.0;
    for item in items {
        tops.push(top);
        top += item.height();
    }
    (tops, top)
}

/// The next highlighted row after Down (`forward`) or Up, wrapping round the
/// ends. With none highlighted Down takes the first and Up the last.
pub(crate) fn step(current: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (current, forward) {
        (None, true) => 0,
        (None, false) => count - 1,
        (Some(at), true) => (at + 1) % count,
        (Some(at), false) => (at + count - 1) % count,
    })
}

/// The scroll offset (negative, as Freya has it) that brings `top..top + height`
/// into a viewport of `viewport` that is scrolled to `current`, moving as little
/// as it must.
pub(crate) fn scroll_to_show(
    top: f32,
    height: f32,
    viewport: f32,
    content: f32,
    current: i32,
) -> i32 {
    let shown = -(current as f32);
    let want = if top < shown {
        top
    } else if top + height > shown + viewport {
        top + height - viewport
    } else {
        shown
    };
    -(want.clamp(0.0, (content - viewport).max(0.0)) as i32)
}

impl Component for Omnibox {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let id = use_a11y();
        use_provide_context(|| FieldFocus(id));
        let focus = use_focus(id);
        let mut draft = self.draft;
        // A field that opens with text other than the resting text (a `/` that started the typing)
        // has been typed in already.
        let mut typed = use_state(|| self.draft.text() != self.initial);
        let mut dismissed = use_state(|| false);
        let mut highlight = use_state(|| None::<usize>);
        let mut hovered = use_state(|| None::<usize>);
        let mut field_area = use_state(Area::default);
        let mut had_focus = use_state(|| false);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let inside = use_hook(|| Rc::new(Cell::new(false)));
        // What the card showed last, drawn while it closes.
        let held = use_hook(|| Rc::new(RefCell::new(Vec::<OmniItem>::new())));
        let window = window_size();

        let rows: Vec<&OmniRow> = self
            .items
            .iter()
            .filter_map(|item| match item {
                OmniItem::Row(row) => Some(row),
                _ => None,
            })
            .collect();
        let wants = *typed.read() && !*dismissed.read() && !self.items.is_empty();
        if wants {
            *held.borrow_mut() = self.items.clone();
        }
        let shown: Vec<OmniItem> = if wants {
            self.items.clone()
        } else {
            held.borrow().clone()
        };
        let (tops, content) = layout(&shown);
        let pad = S2.px();
        let field_h = Dim::Control.px();
        let room = (window.height - field_area.read().max_y() - PANEL_EDGE - field_h).max(0.0);
        let cap = LIST_MAX.min(room.max(Dim::MenuItem.px()));
        let target = if wants {
            (content + 2.0 * pad).min(cap)
        } else {
            0.0
        };
        let ms = if tokens.reduced_motion { 0 } else { MOTION_MS };
        let list_h = use_tween(target, ms);

        // The highlight follows the rows: when what they are changes, the first
        // is highlighted (or none).
        let key: Vec<String> = rows.iter().map(|row| row.name.clone()).collect();
        let auto = self.auto_highlight;
        use_side_effect_with_deps(&(key, auto), move |(key, auto): &(Vec<String>, bool)| {
            let first = (*auto && !key.is_empty()).then_some(0);
            highlight.set_if_modified(first);
            hovered.set_if_modified(None);
        });

        let on_close = self.on_close.clone();
        let finish = EventHandler::new(move |_: ()| on_close.call(()));

        // Focus leaving the field is a way out, unless the pointer went into
        // the card (a press on a row takes the field's focus and gives it
        // back) or the field's own menu is open.
        use_side_effect_with_deps(&focus().is_focused(), {
            let finish = finish.clone();
            let inside = inside.clone();
            move |focused| {
                if *focused {
                    had_focus.set(true);
                    return;
                }
                if !*had_focus.peek() {
                    return;
                }
                had_focus.set(false);
                let menu_open = try_consume_context::<crate::shell::Overlays>()
                    .is_some_and(|overlays| overlays.menu_owned_by(id));
                if !inside.get() && !menu_open {
                    finish.call(());
                }
            }
        });

        let on_submit = {
            let on_text = self.on_submit.clone();
            let presses: Vec<EventHandler<()>> =
                rows.iter().map(|row| row.on_press.clone()).collect();
            let showing = wants;
            EventHandler::new(move |text: String| {
                let at = *highlight.peek();
                match at {
                    Some(index) if showing && index < presses.len() => presses[index].call(()),
                    _ => on_text.call(text),
                }
            })
        };

        let on_change = {
            let on_change = self.on_change.clone();
            EventHandler::new(move |text: String| {
                // Select all and other commands that leave the text alone report a change too.
                if text == *draft.value.peek() {
                    return;
                }
                typed.set_if_modified(true);
                dismissed.set_if_modified(false);
                hovered.set_if_modified(None);
                if let Some(on_change) = &on_change {
                    on_change.call(text);
                }
            })
        };

        let row_tops: Vec<(f32, f32)> = shown
            .iter()
            .zip(&tops)
            .filter(|(item, _)| item.is_row())
            .map(|(item, top)| (*top, item.height()))
            .collect();
        let row_count = rows.len();
        let move_highlight = EventHandler::new({
            let row_tops = row_tops.clone();
            move |forward: bool| {
                if row_count == 0 {
                    return;
                }
                if *dismissed.peek() {
                    dismissed.set(false);
                }
                let next = step(*highlight.peek(), row_count, forward);
                highlight.set(next);
                if let Some((top, height)) = next.and_then(|at| row_tops.get(at).copied()) {
                    let current = <(i32, i32)>::from(scroll).1;
                    scroll.scroll_to_y(scroll_to_show(
                        top,
                        height,
                        list_h - 2.0 * pad,
                        content,
                        current,
                    ));
                }
            }
        });
        let on_arrow = {
            let move_highlight = move_highlight.clone();
            EventHandler::new(move |amount: f64| move_highlight.call(amount >= 0.0))
        };

        let on_escape = {
            let finish = finish.clone();
            EventHandler::new(move |_: ()| {
                if *typed.peek() && !*dismissed.peek() && !held.borrow().is_empty() && target > 0.0
                {
                    // The card goes first; the field keeps the keyboard.
                    dismissed.set(true);
                    hovered.set(None);
                } else {
                    finish.call(());
                }
            })
        };

        let on_key_down = {
            let complete = self.complete.clone();
            let fills: Vec<Option<String>> = rows.iter().map(|row| row.fill.clone()).collect();
            let rows_for_chip: Vec<Option<EventHandler<()>>> = rows
                .iter()
                .map(|row| row.chip.as_ref().map(|(_, handler)| handler.clone()))
                .collect();
            let move_highlight = move_highlight.clone();
            let on_change = self.on_change.clone();
            move |event: Event<KeyboardEventData>| {
                if event.key == Key::Named(NamedKey::Enter) {
                    // Enter is the field's: the dialog around it must not also take it as its own accept.
                    event.stop_propagation();
                    if event.modifiers.contains(Modifiers::CONTROL)
                        && let Some(Some(handler)) = highlight
                            .peek()
                            .and_then(|at| rows_for_chip.get(at))
                            .map(|chip| chip.clone())
                    {
                        handler.call(());
                    }
                    return;
                }
                if event.key != Key::Named(NamedKey::Tab)
                    || event
                        .modifiers
                        .intersects(Modifiers::CONTROL | Modifiers::ALT)
                {
                    return;
                }
                if !*typed.peek() || *dismissed.peek() || row_count == 0 {
                    return;
                }
                event.stop_propagation();
                event.prevent_default();
                let backward = event.modifiers.contains(Modifiers::SHIFT);
                let now = draft.text();
                let fill = highlight
                    .peek()
                    .and_then(|at| fills.get(at).cloned().flatten())
                    .or_else(|| complete.clone())
                    .filter(|fill| *fill != now);
                match fill {
                    Some(fill) if !backward => {
                        draft.set(fill.clone());
                        if let Some(on_change) = &on_change {
                            on_change.call(fill);
                        }
                    }
                    _ => move_highlight.call(!backward),
                }
            }
        };

        let mut field = text_field(draft.value, on_submit)
            .fill_width()
            .placeholder(self.placeholder.clone())
            .leading(self.leading)
            .a11y_id(id)
            .autofocus()
            .danger_outline(self.danger)
            .on_change(on_change)
            .on_arrow(on_arrow)
            .on_escape(on_escape);
        if self.select_all {
            field = field.select_all_on_open();
        }
        if let Some((most, handler)) = &self.alt_digit {
            field = field.on_alt_digit(*most, handler.clone());
        }
        if let Some((icon, label)) = &self.close_action {
            let finish = finish.clone();
            field = field.trailing_action(*icon, label.clone(), move |_: ()| finish.call(()));
        }

        let viewport = (list_h - 2.0 * pad).max(0.0);
        let mut column = rect()
            .tw(&tokens, &[flex_col(), w_full(), px(S1)])
            .a11y_role(AccessibilityRole::ListBox);
        let mut ordinal = 0;
        for item in &shown {
            match item {
                OmniItem::Row(row) => {
                    let index = ordinal;
                    ordinal += 1;
                    let on_press = EventHandler::new({
                        let handler = row.on_press.clone();
                        move |_: ()| {
                            handler.call(());
                            // The press took the field's focus; give it back.
                            id.request_focus();
                        }
                    });
                    column = column.child(RowView {
                        row: row.clone(),
                        highlighted: hovered.read().is_none() && *highlight.read() == Some(index),
                        on_press,
                        on_hover: EventHandler::new(move |inside: bool| {
                            hovered.set(inside.then_some(index))
                        }),
                    });
                }
                OmniItem::Heading { text, detail, busy } => {
                    column = column.child(HeadingView {
                        text: text.clone(),
                        detail: detail.clone(),
                        busy: *busy,
                    });
                }
                OmniItem::Notice { text, kind } => {
                    column = column.child(NoticeView {
                        text: text.clone(),
                        kind: *kind,
                    });
                }
                OmniItem::Strip { element, height } => {
                    column = column.child(StripView {
                        element: element.clone(),
                        height: *height,
                    });
                }
            }
        }

        let surface_opacity = (list_h / Dim::MenuItem.px()).clamp(0.0, 1.0);
        let card_h = field_h + list_h;
        let inside_enter = inside.clone();
        let inside_leave = inside.clone();
        let inside_press = inside.clone();
        let card_area = *field_area.read();

        // The box that holds the field's place in the layout: the card is laid over it.
        rect()
            .component("Omnibox")
            .tw(&tokens, &[w_full(), h(Dim::Control)])
            .on_sized(move |event: Event<SizedEventData>| field_area.set_if_modified(event.area))
            .child(
                rect()
                    .component("OmniboxCard")
                    .position(Position::new_absolute().top(0.0).left(0.0))
                    .layer(crate::layers::OMNIBOX)
                    .width(Size::fill())
                    .height(Size::px(card_h))
                    .on_pointer_enter(move |_| inside_enter.set(true))
                    .on_pointer_leave(move |_| inside_leave.set(false))
                    // A press anywhere on the card is on the card, whether or not the pointer was seen
                    // to enter it (a touch has no pointer to move): the field is not left.
                    .on_global_pointer_down(move |event: Event<PointerEventData>| {
                        let at = event.global_location();
                        let (x, y) = (at.x as f32, at.y as f32);
                        inside_press.set(
                            x >= card_area.min_x()
                                && x <= card_area.max_x()
                                && y >= card_area.min_y()
                                && y <= card_area.min_y() + card_h,
                        );
                    })
                    .child(
                        rect()
                            .component("OmniboxSurface")
                            .position(Position::new_absolute().top(0.0).left(0.0))
                            .width(Size::fill())
                            .height(Size::px(card_h))
                            .opacity(surface_opacity)
                            .interactive(false)
                            .tw(
                                &tokens,
                                &[
                                    rounded(Radius::Control),
                                    bg(Paint::Popover),
                                    border(Line::Hairline),
                                    shadow(Elevation::Raised),
                                ],
                            ),
                    )
                    .child(
                        rect()
                            .key(*draft.fresh.read())
                            .width(Size::fill())
                            .on_key_down(on_key_down)
                            .a11y_role(AccessibilityRole::ComboBox)
                            .a11y_alt(self.label.clone())
                            .child(field),
                    )
                    .maybe_child((list_h > 0.5).then(|| {
                        rect()
                            .component("OmniboxList")
                            .width(Size::fill())
                            .height(Size::px(list_h))
                            .tw(&tokens, &[py(S2)])
                            .on_wheel(|event: Event<WheelEventData>| event.stop_propagation())
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .height(Size::px(viewport))
                                    .child(ScrollView::new_controlled(scroll).child(column)),
                            )
                    })),
            )
    }
}

/// A strip of chips: one row that scrolls sideways when the card is narrower than the chips, so
/// none is out of reach on a phone.
#[derive(PartialEq)]
struct StripView {
    element: Element,
    height: f32,
}

impl Component for StripView {
    fn render(&self) -> impl IntoElement {
        let scroll = use_scroll_controller(ScrollConfig::default);
        rect()
            .component("OmniboxStrip")
            .width(Size::fill())
            .height(Size::px(self.height))
            .child(
                scroll_x_controlled(scroll)
                    .show_scrollbar(false)
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(self.element.clone()),
            )
    }
}

/// A choosable row.
#[derive(PartialEq)]
struct RowView {
    row: OmniRow,
    highlighted: bool,
    on_press: EventHandler<()>,
    on_hover: EventHandler<bool>,
}

impl Component for RowView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let on_press = self.on_press.clone();
        let on_hover = self.on_hover.clone();
        let on_leave = on_hover.clone();
        let mut label = paragraph()
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);
        for part in &self.row.parts {
            let style = if part.emphasis { EMPHASIS } else { BODY };
            let style = style.in_ink(if part.muted { Ink::Muted } else { Ink::Text });
            label = label.span(style.span(&tokens, part.text.clone()));
        }
        let detail = self.row.detail.clone().filter(|detail| !detail.is_empty());
        let chip = self.row.chip.clone();
        rect()
            .component("OmniboxRow")
            .a11y_role(AccessibilityRole::ListBoxOption)
            .a11y_alt(self.row.name.clone())
            .tw(&tokens, MENU_ITEM)
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering() || self.highlighted,
                    false,
                    self.highlighted,
                )],
            )
            .on_pointer_enter(move |_| {
                hovering.set(true);
                on_hover.call(true);
            })
            .on_pointer_leave(move |_| {
                hovering.set(false);
                on_leave.call(false);
            })
            // A press never reaches the field or the page under the row.
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(());
            })
            .child(self.row.lead.clone())
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[flex_row(), items(Align::Center), gap(S2), w_flex()],
                    )
                    .child(label.width(Size::flex(2.0)))
                    .maybe_child(detail.map(|detail| {
                        CAPTION
                            .in_ink(Ink::Muted)
                            .label(&tokens, detail)
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis)
                            .width(Size::flex(1.0))
                    })),
            )
            .maybe_child(chip.map(|(label, handler)| OmniChipView { label, handler }))
    }
}

/// The trailing chip of a row: a small outlined pill.
#[derive(PartialEq)]
struct OmniChipView {
    label: String,
    handler: EventHandler<()>,
}

impl Component for OmniChipView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let handler = self.handler.clone();
        rect()
            .component("OmniboxRowChip")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(self.label.clone())
            .tw(
                &tokens,
                &[
                    flex_row(),
                    center(),
                    h(Dim::Box7),
                    px(S3),
                    rounded(Radius::Control),
                    border(Line::Hairline),
                    surface(Paint::Transparent, hovering(), false, false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                handler.call(());
                if let Some(FieldFocus(id)) = try_consume_context::<FieldFocus>() {
                    id.request_focus();
                }
            })
            .child(
                CAPTION
                    .in_ink(Ink::Text)
                    .label(&tokens, self.label.clone())
                    .max_lines(1),
            )
    }
}

/// A caption over a group of rows.
#[derive(PartialEq)]
struct HeadingView {
    text: String,
    detail: Option<String>,
    busy: bool,
}

impl Component for HeadingView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .component("OmniboxHeading")
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    gap(S2),
                    w_full(),
                    h(Dim::Box7),
                    px(S3),
                ],
            )
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .child(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            )
            .maybe_child(self.detail.clone().map(|detail| {
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, detail)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
            }))
            .maybe_child(self.busy.then(|| {
                Element::from(crate::controls::spinner(
                    crate::controls::SpinnerSize::Small,
                ))
            }))
    }
}

/// One thing said instead of a choice.
#[derive(PartialEq)]
struct NoticeView {
    text: String,
    kind: OmniNotice,
}

impl Component for NoticeView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ink = if self.kind == OmniNotice::Danger {
            Ink::Danger
        } else {
            Ink::Muted
        };
        rect()
            .component("OmniboxNotice")
            .a11y_role(if self.kind == OmniNotice::Danger {
                AccessibilityRole::Alert
            } else {
                AccessibilityRole::Status
            })
            .tw(&tokens, MENU_ITEM)
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .maybe_child((self.kind == OmniNotice::Danger).then(|| {
                icons::icon(
                    Icon::DialogWarning,
                    IconSize::Control,
                    Ink::Danger.color(&tokens),
                )
            }))
            .maybe_child((self.kind == OmniNotice::Busy).then(|| {
                Element::from(crate::controls::spinner(
                    crate::controls::SpinnerSize::Small,
                ))
            }))
            .child(
                BODY.in_ink(ink)
                    .label(&tokens, self.text.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0)),
            )
    }
}

/// A chip for a strip: a toggle that is on or off, such as a scope or a file
/// type.
pub fn omni_chip(
    label: impl Into<String>,
    selected: bool,
    on_press: impl Into<EventHandler<()>>,
) -> OmniChip {
    OmniChip {
        label: label.into(),
        selected,
        on_press: on_press.into(),
        icon: None,
        removable: false,
    }
}

/// See [`omni_chip`].
#[derive(PartialEq)]
pub struct OmniChip {
    label: String,
    selected: bool,
    on_press: EventHandler<()>,
    icon: Option<Icon>,
    removable: bool,
}

impl OmniChip {
    /// A glyph before the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The chip can be taken away: it ends in a close glyph, and its name for
    /// assistive technology says so.
    pub fn removable(mut self) -> Self {
        self.removable = true;
        self
    }
}

impl Component for OmniChip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let on_press = self.on_press.clone();
        let selected = self.selected;
        let ink = if selected { Ink::Accent } else { Ink::Text };
        rect()
            .component("OmniboxChip")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(if self.removable {
                format!("Remove {}", self.label)
            } else {
                self.label.clone()
            })
            .a11y_builder(move |builder| {
                builder.set_toggled(if selected {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                })
            })
            .tw(
                &tokens,
                &[
                    flex_row(),
                    center(),
                    gap(S1),
                    h(Dim::Box7),
                    px(S3),
                    rounded(Radius::Control),
                    surface(
                        if selected {
                            Paint::AccentSoft
                        } else {
                            Paint::Control
                        },
                        hovering(),
                        false,
                        false,
                    ),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(());
                if let Some(FieldFocus(id)) = try_consume_context::<FieldFocus>() {
                    id.request_focus();
                }
            })
            .maybe_child(
                self.icon
                    .map(|icon| icons::icon(icon, IconSize::Text, ink.color(&tokens))),
            )
            .child(
                CAPTION
                    .in_ink(ink)
                    .label(&tokens, self.label.clone())
                    .max_lines(1),
            )
            .maybe_child(
                self.removable
                    .then(|| icons::icon(Icon::WindowClose, IconSize::Text, ink.color(&tokens))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_highlight_wraps() {
        assert_eq!(step(None, 3, true), Some(0));
        assert_eq!(step(None, 3, false), Some(2));
        assert_eq!(step(Some(2), 3, true), Some(0));
        assert_eq!(step(Some(0), 3, false), Some(2));
        assert_eq!(step(Some(1), 3, true), Some(2));
        assert_eq!(step(None, 0, true), None);
    }

    #[test]
    fn a_row_is_scrolled_into_view_by_the_least_it_takes() {
        // Ten rows of 36 in a viewport of 108: the content is 360 tall.
        assert_eq!(
            scroll_to_show(0.0, 36.0, 108.0, 360.0, 0),
            0,
            "already in view"
        );
        assert_eq!(
            scroll_to_show(144.0, 36.0, 108.0, 360.0, 0),
            -72,
            "brought in at the bottom edge"
        );
        assert_eq!(
            scroll_to_show(0.0, 36.0, 108.0, 360.0, -72),
            0,
            "brought back at the top edge"
        );
        assert_eq!(
            scroll_to_show(324.0, 36.0, 108.0, 360.0, 0),
            -252,
            "never past the end"
        );
    }

    #[test]
    fn the_match_is_drawn_heavier_and_what_follows_is_muted() {
        let parts = OmniPart::matched("My Pictures", 3..6, "/");
        let texts: Vec<(&str, bool, bool)> = parts
            .iter()
            .map(|part| (part.text.as_str(), part.emphasis, part.muted))
            .collect();
        assert_eq!(
            texts,
            [
                ("My ", false, false),
                ("Pic", true, false),
                ("tures", false, false),
                ("/", false, true)
            ]
        );
        // A range inside a character, or past the end, marks nothing.
        let inside = OmniPart::matched("Écoute", 1..3, "");
        assert_eq!(inside.len(), 1);
        assert!(!inside[0].emphasis);
        assert_eq!(OmniPart::matched("abc", 2..9, "").len(), 1);
    }

    #[test]
    fn the_card_is_as_tall_as_what_it_holds() {
        let items = [
            OmniItem::Strip {
                element: rect().into(),
                height: 30.0,
            },
            OmniItem::Strip {
                element: rect().into(),
                height: 12.0,
            },
        ];
        let (tops, total) = layout(&items);
        assert_eq!(tops, [0.0, 30.0]);
        assert_eq!(total, 42.0);
    }
}
