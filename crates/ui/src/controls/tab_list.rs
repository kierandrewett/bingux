//! The list of every tab of a window, which the last button of a tab strip
//! opens ("Search and manage tabs", Firefox's "List all tabs").
//!
//! * **The lines** ([`lines`]): *Search all tabs* with its shortcut and *Close
//!   duplicate tabs* (each only when the host offers it), the pinned tabs under
//!   a *Pinned* label, a separator, then the others. A tab's row is its icon,
//!   its title, a speaker while it plays sound or is muted, and a close button
//!   while the pointer or the keys are on it (a pinned tab has none). The
//!   selected tab is ticked.
//! * **Opening** puts the selected tab in the middle of the tabs that scroll
//!   ([`centre_offset`]). The card stops at three quarters of the window's
//!   height ([`card_cap`]), as Firefox's panel stops short of the window's edge. What is
//!   above the tabs, the commands and (while they are few) the pinned tabs, stays
//!   put ([`head_len`]) and the other tabs scroll under it: a scrolled list never
//!   takes *Search all tabs* off the screen.
//! * **Choosing** a tab selects it and closes the list; the speaker and the
//!   close button answer for their tab and leave it open. Down, Up, Home and
//!   End walk the rows ([`step`]), Enter chooses, Delete closes the tab, and
//!   Escape or a press outside closes the list.
//!
//! The model is plain data so each rule can be tested without a window;
//! [`TabListButton`] draws it.

use std::rc::Rc;

use freya::prelude::*;

use super::button::icon_button;
use super::press::use_press;
use super::tabs::{TabItem, TabStatus};
use crate::a11y::SemanticsExt;
use crate::component::ComponentName;
use crate::icons::{self, Icon};
use crate::shell::panel::{PANEL_EDGE, PANEL_GAP, Panel, PanelKind, PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::{use_tokens, window_size};
use crate::theme::Tokens;
use crate::typography::{BODY, CAPTION, Ink};

/// One line of the list, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    /// *Search all tabs*, with its shortcut: the host opens its address field
    /// on the open tabs.
    Search,
    /// *Close duplicate tabs*, listed only while there are duplicates.
    CloseDuplicates,
    /// The label over the pinned tabs.
    Header,
    Separator,
    /// A tab, by its place in the strip's list of tabs.
    Tab(usize),
}

/// The lines of the list for `tabs` in the strip's order: the commands, then
/// the pinned tabs under their label, then the others.
pub(crate) fn lines(tabs: &[TabItem], search: bool, duplicates: bool) -> Vec<Line> {
    let mut out = Vec::with_capacity(tabs.len() + 5);
    out.extend(search.then_some(Line::Search));
    out.extend(duplicates.then_some(Line::CloseDuplicates));
    let place = |pinned: bool| {
        tabs.iter()
            .enumerate()
            .filter(move |(_, tab)| tab.pinned == pinned)
            .map(|(index, _)| Line::Tab(index))
    };
    let (pinned, others): (Vec<Line>, Vec<Line>) = (place(true).collect(), place(false).collect());
    // The commands are cut off from the tabs by a hairline, and the pinned tabs
    // from the others: only where there is something on both sides of it.
    if !out.is_empty() && !tabs.is_empty() {
        out.push(Line::Separator);
    }
    if !pinned.is_empty() {
        out.push(Line::Header);
        out.extend(pinned);
        if !others.is_empty() {
            out.push(Line::Separator);
        }
    }
    out.extend(others);
    out
}

/// The heights of the lines at one density, so the rules below need no window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Metrics {
    row: f32,
    header: f32,
    separator: f32,
}

impl Metrics {
    /// The heights at the density the window is at now.
    pub(crate) fn current() -> Self {
        Metrics {
            row: Dim::MenuItem.px(),
            header: Dim::Box7.px(),
            separator: Dim::MenuSeparator.px(),
        }
    }

    /// The heights at mouse density.
    #[cfg(test)]
    pub(crate) fn regular() -> Self {
        let class = SizeClass::Regular;
        Metrics {
            row: Dim::MenuItem.px_in(class),
            header: Dim::Box7.px_in(class),
            separator: Dim::MenuSeparator.px_in(class),
        }
    }
}

impl Line {
    /// The height of the line.
    pub(crate) fn height(self, metrics: Metrics) -> f32 {
        match self {
            Line::Search | Line::CloseDuplicates | Line::Tab(_) => metrics.row,
            Line::Header => metrics.header,
            Line::Separator => metrics.separator,
        }
    }

    /// Whether the keys can stop on the line.
    pub(crate) fn actionable(self) -> bool {
        matches!(self, Line::Search | Line::CloseDuplicates | Line::Tab(_))
    }
}

/// The height of all the lines.
pub(crate) fn content_height(lines: &[Line], metrics: Metrics) -> f32 {
    lines.iter().map(|line| line.height(metrics)).sum()
}

/// How far the lines are scrolled to put line `line` in the middle of a
/// viewport `viewport` tall: kept inside the lines.
pub(crate) fn centre_offset(lines: &[Line], line: usize, viewport: f32, metrics: Metrics) -> f32 {
    let top: f32 = lines[..line].iter().map(|line| line.height(metrics)).sum();
    let want = top - (viewport - lines[line].height(metrics)) / 2.0;
    want.clamp(0.0, (content_height(lines, metrics) - viewport).max(0.0))
}

/// The line the keys reach from `from` going down or up, skipping what they
/// cannot stop on; `None` at the end, and with no `from` the first or last.
pub(crate) fn step(lines: &[Line], from: Option<usize>, down: bool) -> Option<usize> {
    let stops = |index: &usize| lines[*index].actionable();
    match (from, down) {
        (None, true) => (0..lines.len()).find(stops),
        (None, false) => (0..lines.len()).rev().find(stops),
        (Some(from), true) => (from + 1..lines.len()).find(stops),
        (Some(from), false) => (0..from).rev().find(stops),
    }
}

/// The share of the window's height the open card takes at most: a list of
/// forty tabs does not run to the window's bottom edge (the design's frame is
/// 600 px of 800).
pub(crate) const CARD_SHARE: f32 = 0.75;

/// The tallest the whole card is: that share of the window, and never past what
/// the window has below the button less the panel's gap and its edge.
pub(crate) fn card_cap(window_height: f32, button_bottom: f32) -> f32 {
    (window_height - button_bottom - PANEL_GAP - PANEL_EDGE).min(window_height * CARD_SHARE)
}

/// The height the scrolling lines get inside a card of at most `card`, with
/// `head` of it taken by the lines that stay put and the card's padding off
/// both ends, but at least [`MIN_HEIGHT`].
pub(crate) fn viewport_cap(card: f32, head: f32) -> f32 {
    (card - head - 2.0 * S2.px()).max(MIN_HEIGHT)
}

/// The least height the scrolling lines are given however small the window.
pub(crate) const MIN_HEIGHT: f32 = 160.0;

/// How many of the first lines stay in place while the rest scroll: the
/// commands with their separator, and with them the pinned tabs under their
/// label when those (and the commands) take no more than half of a card of
/// `card`. A long block of pinned tabs would leave the others no room, so it
/// scrolls with them. A list with nothing but pinned tabs scrolls them all.
pub(crate) fn head_len(lines: &[Line], metrics: Metrics, card: f32) -> usize {
    let commands = lines
        .iter()
        .take_while(|line| matches!(line, Line::Search | Line::CloseDuplicates))
        .count();
    let commands =
        commands + usize::from(commands > 0 && lines.get(commands) == Some(&Line::Separator));
    if lines.get(commands) != Some(&Line::Header) {
        return commands;
    }
    // The pinned block ends at its separator: the others follow it.
    let Some(end) = lines[commands..]
        .iter()
        .position(|line| *line == Line::Separator)
        .map(|at| commands + at + 1)
    else {
        return commands;
    };
    if content_height(&lines[..end], metrics) <= card / 2.0 {
        end
    } else {
        commands
    }
}

// ---- The button and its panel ------------------------------------------------

/// The last button of a strip, and the list of tabs it opens.
#[derive(Clone, PartialEq)]
pub(crate) struct TabListButton {
    pub(crate) tabs: Vec<TabItem>,
    pub(crate) selected: u64,
    /// The side of the button.
    pub(crate) size: Dim,
    /// *Search all tabs*: its shortcut hint, and what it does.
    pub(crate) search: Option<(String, EventHandler<()>)>,
    pub(crate) close_duplicates: Option<EventHandler<()>>,
    pub(crate) on_select: EventHandler<u64>,
    pub(crate) on_close: EventHandler<u64>,
    pub(crate) on_status: Option<EventHandler<u64>>,
}

impl Component for TabListButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut open = use_state(|| false);
        // The row the pointer or the keys are on.
        let mut highlighted = use_state(|| None::<usize>);
        let mut anchor = use_state(Area::default);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let button_id = use_a11y();
        let panel_id = use_a11y();
        let window = window_size();

        let all = Rc::new(lines(
            &self.tabs,
            self.search.is_some(),
            self.close_duplicates.is_some(),
        ));
        let metrics = Metrics::current();
        let card = card_cap(window.height, anchor.read().max_y());
        // The lines before `head` stay put; the others scroll under them.
        let head = head_len(&all, metrics, card);
        let head_height = content_height(&all[..head], metrics);
        let viewport = content_height(&all[head..], metrics).min(viewport_cap(card, head_height));
        let selected_line = all.iter().position(
            |line| matches!(line, Line::Tab(index) if self.tabs[*index].id == self.selected),
        );

        // Opening scrolls first, so the list is drawn with the selected tab in the
        // middle, not scrolled there a frame later.
        let show = {
            let all = all.clone();
            EventHandler::new(move |_: ()| {
                let at = selected_line
                    .and_then(|line| line.checked_sub(head))
                    .map_or(0.0, |line| {
                        centre_offset(&all[head..], line, viewport, metrics)
                    });
                scroll.scroll_to_y(-(at as i32));
                highlighted.set(None);
                open.set(true);
            })
        };
        let close = EventHandler::new(move |_: ()| {
            open.set(false);
            highlighted.set(None);
            button_id.request_focus();
        });

        // What choosing a line does: close the list, then the host's answer.
        let activate: EventHandler<usize> = {
            let all = all.clone();
            let tabs = self.tabs.clone();
            let close = close.clone();
            let (search, duplicates, on_select) = (
                self.search.clone(),
                self.close_duplicates.clone(),
                self.on_select.clone(),
            );
            EventHandler::new(move |index: usize| match all[index] {
                Line::Search => {
                    close.call(());
                    if let Some((_, search)) = &search {
                        search.call(());
                    }
                }
                Line::CloseDuplicates => {
                    close.call(());
                    if let Some(duplicates) = &duplicates {
                        duplicates.call(());
                    }
                }
                Line::Tab(tab) => {
                    close.call(());
                    on_select.call(tabs[tab].id);
                }
                Line::Header | Line::Separator => {}
            })
        };

        let on_key = {
            let (all, activate) = (all.clone(), activate.clone());
            let (tabs, on_close) = (self.tabs.clone(), self.on_close.clone());
            move |event: Event<KeyboardEventData>| {
                // With nothing highlighted the walk starts from the selected tab.
                let from = (*highlighted.peek()).or(selected_line);
                let next = match &event.key {
                    Key::Named(NamedKey::ArrowDown) => step(&all, from, true),
                    Key::Named(NamedKey::ArrowUp) => step(&all, from, false),
                    Key::Named(NamedKey::Home) => step(&all, None, true),
                    Key::Named(NamedKey::End) => step(&all, None, false),
                    Key::Named(NamedKey::Enter) => {
                        event.stop_propagation();
                        if let Some(index) = *highlighted.peek() {
                            activate.call(index);
                        }
                        return;
                    }
                    Key::Named(NamedKey::Delete) => {
                        event.stop_propagation();
                        // A pinned tab is closed from its own menu.
                        if let Some(Line::Tab(tab)) = (*highlighted.peek()).map(|index| all[index])
                            && !tabs[tab].pinned
                        {
                            on_close.call(tabs[tab].id);
                        }
                        return;
                    }
                    _ => return,
                };
                event.stop_propagation();
                if let Some(next) = next {
                    highlighted.set(Some(next));
                    scroll.scroll_to_y(
                        -(keep_in_view(
                            &all[head..],
                            next.checked_sub(head),
                            viewport,
                            -<(i32, i32)>::from(scroll).1 as f32,
                            metrics,
                        ) as i32),
                    );
                }
            }
        };

        // Every line drawn; what is left after the split is the head.
        let mut drawn: Vec<Element> = all
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let on_hover = EventHandler::new(move |inside: bool| {
                    if inside {
                        highlighted.set(Some(index));
                    } else if *highlighted.peek() == Some(index) {
                        highlighted.set(None);
                    }
                });
                let press = {
                    let activate = activate.clone();
                    EventHandler::new(move |_: ()| activate.call(index))
                };
                match *line {
                    Line::Separator => separator(&tokens),
                    Line::Header => header(&tokens, crate::tr!("ui-tab-list-pinned")),
                    Line::Search => Element::from(ListRow {
                        label: crate::tr!("ui-tab-list-search"),
                        icon: Some(Icon::Search),
                        shortcut: self.search.as_ref().map(|(hint, _)| hint.clone()),
                        highlighted: highlighted() == Some(index),
                        on_press: press,
                        on_hover,
                        ..ListRow::plain()
                    }),
                    Line::CloseDuplicates => Element::from(ListRow {
                        label: crate::tr!("ui-tab-list-close-duplicates"),
                        highlighted: highlighted() == Some(index),
                        on_press: press,
                        on_hover,
                        ..ListRow::plain()
                    }),
                    Line::Tab(tab) => {
                        let item = &self.tabs[tab];
                        let id = item.id;
                        let on_close = self.on_close.clone();
                        let on_status = self.on_status.clone();
                        Element::from(ListRow {
                            label: item.title.clone(),
                            icon: item.icon,
                            tab: Some(Tab {
                                selected: id == self.selected,
                                sound: match item.status {
                                    Some(TabStatus::Audio) => Some(false),
                                    Some(TabStatus::Muted) => Some(true),
                                    _ => None,
                                },
                                closable: !item.pinned,
                                on_status: on_status.map(|handler| {
                                    EventHandler::new(move |_: ()| handler.call(id))
                                }),
                                on_close: EventHandler::new(move |_: ()| on_close.call(id)),
                            }),
                            highlighted: highlighted() == Some(index),
                            on_press: press,
                            on_hover,
                            ..ListRow::plain()
                        })
                    }
                }
            })
            .collect();
        let rows: Vec<Element> = std::iter::once(Element::from(FocusOnMount { id: panel_id }))
            .chain(drawn.split_off(head))
            .collect();

        let panel = Panel::new(
            PanelKind::Menu,
            open(),
            PanelTrigger::Control(PanelRect::from_area(&anchor.read())),
            rows,
        )
        .size(Dim::TabList.px(), content_height(&all[head..], metrics))
        .list_cap(viewport_cap(card, head_height))
        .head(head_height, drawn)
        .scroll(scroll)
        .navigation(panel_id, on_key, |_: (f32, f32)| {})
        .on_dismiss(close.clone());

        rect()
            .component("TabListButton")
            .on_sized(move |event: Event<SizedEventData>| anchor.set_if_modified(event.area))
            .child(
                icon_button(Icon::PanDown, move |_: ()| {
                    if open() {
                        close.call(());
                    } else {
                        show.call(());
                    }
                })
                .a11y_id(button_id)
                .size(self.size)
                .tooltip(crate::tr!("ui-tab-list.tooltip")),
            )
            .child(Element::from(panel))
    }
}

/// The scroll that keeps line `line` of the scrolling `lines` in view, moving
/// no more than it must: `current` is how far the lines are scrolled now, and a
/// line of the head (`None`) is always in view.
fn keep_in_view(
    lines: &[Line],
    line: Option<usize>,
    viewport: f32,
    current: f32,
    metrics: Metrics,
) -> f32 {
    let Some(line) = line else {
        return current;
    };
    let top: f32 = lines[..line].iter().map(|line| line.height(metrics)).sum();
    let bottom = top + lines[line].height(metrics);
    let furthest = (content_height(lines, metrics) - viewport).max(0.0);
    if top < current {
        top
    } else if bottom > current + viewport {
        bottom - viewport
    } else {
        current
    }
    .clamp(0.0, furthest)
}

/// Takes the keyboard when the open list's surface mounts, so its keys reach
/// the list: the button that opened it gets the focus back when it closes.
#[derive(PartialEq)]
struct FocusOnMount {
    id: AccessibilityId,
}

impl Component for FocusOnMount {
    fn render(&self) -> impl IntoElement {
        let id = self.id;
        use_side_effect(move || id.request_focus());
        rect().width(Size::px(0.0)).height(Size::px(0.0))
    }
}

fn separator(tokens: &Tokens) -> Element {
    rect()
        .tw(tokens, &[w_full(), h(Dim::MenuSeparator), center()])
        .child(rect().tw(tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
        .into_element()
}

/// The label over the pinned tabs.
fn header(tokens: &Tokens, text: String) -> Element {
    rect()
        .component("TabListHeader")
        .a11y_heading(2)
        .a11y_alt(text.clone())
        .tw(
            tokens,
            &[
                w_full(),
                h(Dim::Box7),
                flex_row(),
                items(Align::Center),
                px(S3),
            ],
        )
        .child(CAPTION.in_ink(Ink::Muted).label(tokens, text).max_lines(1))
        .into_element()
}

/// What only a tab's row has.
#[derive(Clone, PartialEq)]
struct Tab {
    selected: bool,
    /// A speaker, crossed out when `true`: the tab plays sound or is muted.
    sound: Option<bool>,
    closable: bool,
    on_status: Option<EventHandler<()>>,
    on_close: EventHandler<()>,
}

/// One row: the tick column, the icon column, the label, and at the end the
/// shortcut hint or a tab's speaker and close button.
#[derive(Clone, PartialEq)]
struct ListRow {
    label: String,
    icon: Option<Icon>,
    shortcut: Option<String>,
    tab: Option<Tab>,
    highlighted: bool,
    on_press: EventHandler<()>,
    on_hover: EventHandler<bool>,
}

impl ListRow {
    fn plain() -> Self {
        ListRow {
            label: String::new(),
            icon: None,
            shortcut: None,
            tab: None,
            highlighted: false,
            on_press: EventHandler::new(|_: ()| {}),
            on_hover: EventHandler::new(|_: bool| {}),
        }
    }
}

impl Component for ListRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut pressing = use_state(|| false);
        let press = use_press();
        let (on_press, on_enter, on_leave) = (
            self.on_press.clone(),
            self.on_hover.clone(),
            self.on_hover.clone(),
        );
        let slot = IconSize::Control.px();
        let ticked = self.tab.as_ref().is_some_and(|tab| tab.selected);
        let tick = rect()
            .width(Size::px(slot))
            .height(Size::px(slot))
            .maybe_child(ticked.then(|| {
                icons::icon(Icon::Check, IconSize::Control, Paint::Accent.color(&tokens))
            }));
        let icon = rect()
            .width(Size::px(slot))
            .height(Size::px(slot))
            .maybe_child(
                self.icon
                    .map(|icon| icons::icon(icon, IconSize::Control, Paint::Text.color(&tokens))),
            );
        let label = BODY
            .in_ink(Ink::Text)
            .label(&tokens, self.label.clone())
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis)
            .width(Size::flex(1.0));
        let mut parts: Vec<Element> = vec![tick.into(), icon.into(), label.into()];
        if let Some(hint) = &self.shortcut {
            parts.push(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, hint.clone())
                    .max_lines(1)
                    .into(),
            );
        }
        if let Some(tab) = &self.tab {
            if let Some(muted) = tab.sound {
                parts.push(sound(&tokens, muted, &self.label, tab.on_status.clone()));
            }
            // The close button shows where the pointer or the keys are; its room is
            // always there, so what is before it does not move out from under the pointer.
            if tab.closable {
                parts.push(close(&self.label, tab.on_close.clone(), self.highlighted));
            }
        }
        let selected = ticked;
        rect()
            .component("TabListRow")
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_alt(self.label.clone())
            .a11y_builder(move |node| {
                if selected {
                    node.set_selected(true);
                }
            })
            .tw(&tokens, MENU_ITEM)
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    self.highlighted,
                    pressing(),
                    false,
                )],
            )
            .on_pointer_enter(move |_| on_enter.call(true))
            .on_pointer_leave(move |_| {
                pressing.set_if_modified(false);
                on_leave.call(false);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                // A touch passes up to the list, which scrolls under the finger.
                if !super::press::is_touch(&event) {
                    event.stop_propagation();
                }
                pressing.set(true);
                press.begin(&event, None);
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                pressing.set(false);
                if press.tap(&event) {
                    on_press.call(());
                }
            })
            .children(parts)
    }
}

/// A row's speaker: crossed out when the tab is muted; a button when the host
/// answers it (it mutes or unmutes the tab).
fn sound(
    tokens: &Tokens,
    muted: bool,
    title: &str,
    on_status: Option<EventHandler<()>>,
) -> Element {
    let icon = if muted {
        Icon::AudioVolumeMuted
    } else {
        Icon::AudioVolumeMedium
    };
    let slot = rect().component("TabListSound").tw(
        tokens,
        &[w(Dim::TabClose), h(Dim::TabClose), flex_row(), center()],
    );
    match on_status {
        Some(handler) => slot
            .child(
                icon_button(icon, move |_: ()| handler.call(()))
                    .size(Dim::TabClose)
                    .glyph(IconSize::Mark)
                    .radius(Radius::Pill)
                    .tooltip(if muted {
                        crate::tr!("ui-tab-unmute.tooltip", title = title)
                    } else {
                        crate::tr!("ui-tab-mute.tooltip", title = title)
                    })
                    .pointer_only(),
            )
            .into_element(),
        None => slot
            .child(icons::icon(
                icon,
                IconSize::Mark,
                Paint::Muted.color(tokens),
            ))
            .into_element(),
    }
}

/// A row's close button.
fn close(title: &str, on_close: EventHandler<()>, shown: bool) -> Element {
    let slot = rect().tw_layout(&[w(Dim::TabClose), h(Dim::TabClose)]);
    if !shown {
        return slot.into_element();
    }
    slot.component("TabListClose")
        .child(
            icon_button(Icon::WindowClose, move |_: ()| on_close.call(()))
                .size(Dim::TabClose)
                .glyph(IconSize::Mark)
                .radius(Radius::Pill)
                .tooltip(crate::tr!("ui-tab-close-named.tooltip", title = title))
                .pointer_only(),
        )
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Dim, SizeClass};

    fn items(pinned: &[bool]) -> Vec<TabItem> {
        pinned
            .iter()
            .enumerate()
            .map(|(index, pinned)| {
                TabItem::new(index as u64 + 1, format!("Tab {index}")).pinned(*pinned)
            })
            .collect()
    }

    #[test]
    fn the_commands_come_first_then_pinned_tabs_under_their_label_then_the_others() {
        let tabs = items(&[false, true, false, true]);
        assert_eq!(
            lines(&tabs, true, true),
            vec![
                Line::Search,
                Line::CloseDuplicates,
                Line::Separator,
                Line::Header,
                Line::Tab(1),
                Line::Tab(3),
                Line::Separator,
                Line::Tab(0),
                Line::Tab(2),
            ],
            "pinned tabs first, in the strip's order, whatever order the host listed them in"
        );
    }

    #[test]
    fn a_command_the_host_does_not_offer_is_not_listed_and_nothing_else_leaves_a_stray_separator() {
        let tabs = items(&[false, false]);
        assert_eq!(
            lines(&tabs, false, false),
            vec![Line::Tab(0), Line::Tab(1)],
            "no commands, no pinned tabs: only the tabs"
        );
        assert_eq!(
            lines(&tabs, true, false),
            vec![Line::Search, Line::Separator, Line::Tab(0), Line::Tab(1)],
            "Close duplicate tabs is listed only when the host has duplicates to close"
        );
        let pinned_only = items(&[true, true]);
        assert_eq!(
            lines(&pinned_only, false, false),
            vec![Line::Header, Line::Tab(0), Line::Tab(1)],
            "no others: no separator after the pinned tabs"
        );
        assert!(
            lines(&[], true, true)
                .iter()
                .all(|line| *line != Line::Header)
        );
    }

    #[test]
    fn every_line_has_a_height_and_only_commands_and_tabs_can_be_reached_by_the_keys() {
        let m = Metrics::regular();
        let class = SizeClass::Regular;
        let all = [
            Line::Search,
            Line::CloseDuplicates,
            Line::Header,
            Line::Separator,
            Line::Tab(0),
        ];
        assert_eq!(
            Line::Tab(3).height(m),
            Dim::MenuItem.px_in(class),
            "a row is a menu item tall"
        );
        assert_eq!(Line::Separator.height(m), Dim::MenuSeparator.px_in(class));
        assert!(
            Line::Header.height(m) > 0.0 && Line::Header.height(m) < Dim::MenuItem.px_in(class)
        );
        let expected: f32 = all.iter().map(|line| line.height(m)).sum();
        assert_eq!(content_height(&all, m), expected);
        let reachable: Vec<bool> = all.iter().map(|line| line.actionable()).collect();
        assert_eq!(reachable, vec![true, true, false, false, true]);
    }

    #[test]
    fn the_selected_row_is_scrolled_to_the_middle_unless_that_would_scroll_past_an_end() {
        let m = Metrics::regular();
        let tabs = items(&[false; 40]);
        let all = lines(&tabs, false, false);
        let row = Dim::MenuItem.px_in(SizeClass::Regular);
        let viewport = 10.0 * row;
        let content = content_height(&all, m);
        assert_eq!(
            centre_offset(&all, 0, viewport, m),
            0.0,
            "the first row is at the top"
        );
        assert_eq!(
            centre_offset(&all, 39, viewport, m),
            content - viewport,
            "the last row is at the bottom"
        );
        let middle = centre_offset(&all, 20, viewport, m);
        assert!(
            (middle - (20.0 * row - (viewport - row) / 2.0)).abs() < 0.01,
            "row 20 has as much above it as below it: {middle}"
        );
        assert_eq!(
            centre_offset(&all[..5], 4, viewport, m),
            0.0,
            "lines that all fit do not scroll"
        );
    }

    #[test]
    fn the_keys_step_over_what_they_cannot_stop_on() {
        let tabs = items(&[true, false, false]);
        let all = lines(&tabs, true, false);
        // Search, Separator, Header, Tab(0), Separator, Tab(1), Tab(2).
        assert_eq!(all.len(), 7);
        assert_eq!(
            step(&all, None, true),
            Some(0),
            "down from nothing: the first"
        );
        assert_eq!(
            step(&all, None, false),
            Some(6),
            "up from nothing: the last"
        );
        assert_eq!(
            step(&all, Some(0), true),
            Some(3),
            "over the separator and the label"
        );
        assert_eq!(step(&all, Some(3), true), Some(5), "over the separator");
        assert_eq!(step(&all, Some(5), false), Some(3));
        assert_eq!(step(&all, Some(6), true), None, "no wrapping past the end");
        assert_eq!(step(&all, Some(0), false), None, "nor past the start");
    }

    #[test]
    fn the_card_stops_at_three_quarters_of_the_window_and_the_scrolling_rows_never_have_less_than_a_few()
     {
        let card = card_cap(800.0, 40.0);
        assert_eq!(card, 600.0, "three quarters of 800, as the design's frame");
        assert!(card_cap(900.0, 40.0) > card, "a taller window gives more");
        assert!(
            card_cap(500.0, 400.0) < 100.0,
            "and never past the window's edge below a button low in it"
        );
        let m = Metrics::regular();
        let head = m.row * 2.0 + m.separator;
        assert_eq!(
            viewport_cap(card, head),
            card - head - 2.0 * S2.px(),
            "the card less what stays put and its padding"
        );
        assert_eq!(
            viewport_cap(200.0, head),
            MIN_HEIGHT,
            "a short window still shows some rows"
        );
    }

    #[test]
    fn the_commands_and_a_short_block_of_pinned_tabs_stay_put_and_a_long_one_scrolls() {
        let m = Metrics::regular();
        let card = 600.0;
        // Search, Close duplicates, Separator, Header, Tab(0), Tab(1), Separator, Tab(2..).
        let tabs = items(&[true, true, false, false, false]);
        let all = lines(&tabs, true, true);
        assert_eq!(
            head_len(&all, m, card),
            7,
            "the commands, their separator and the pinned block with its separator"
        );
        assert_eq!(
            head_len(&all, m, content_height(&all[..7], m) * 2.0 - 1.0),
            3,
            "a pinned block over half of the card scrolls with the others; the commands stay"
        );
        let unpinned = items(&[false; 30]);
        let all = lines(&unpinned, true, false);
        assert_eq!(head_len(&all, m, card), 2, "Search and its separator");
        let all = lines(&unpinned, false, false);
        assert_eq!(head_len(&all, m, card), 0, "no commands: all of it scrolls");
        let pinned_only = lines(&items(&[true, true]), true, false);
        assert_eq!(
            head_len(&pinned_only, m, card),
            2,
            "pinned tabs with no others have nothing to stay above: the list scrolls them"
        );
    }
}
