//! What the inspector window's tabs share: the active tab and the selected
//! node. Created once by the window root and read with [`use_nav`]. Also the
//! tab strip that switches between the tabs.

use freya::prelude::*;

use crate::controls::press::use_press;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};
use crate::typography::{BODY, Ink, TextStyle};

/// The three tabs, in header order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Elements,
    Performance,
    Accessibility,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Elements, Tab::Performance, Tab::Accessibility];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Elements => "Elements",
            Tab::Performance => "Performance",
            Tab::Accessibility => "Accessibility",
        }
    }
}

/// An item of a tab strip. Each enum names its own labels.
pub trait StripTab: Copy + PartialEq + 'static {
    /// The items, in strip order.
    const ITEMS: &'static [Self];

    fn title(self) -> &'static str;
}

impl StripTab for Tab {
    const ITEMS: &'static [Tab] = &Tab::ALL;

    fn title(self) -> &'static str {
        Tab::title(self)
    }
}

/// The shared state. `Copy`, so closures can capture it.
#[derive(Clone, Copy)]
pub struct Nav {
    pub tab: State<Tab>,
    pub selected: State<Option<u64>>,
}

impl Nav {
    /// Selects `node` and switches to the Elements tab, where it is shown.
    /// The accessibility list calls this when a row is pressed.
    pub fn reveal(mut self, node: u64) {
        self.selected.set(Some(node));
        self.tab.set(Tab::Elements);
    }
}

/// Creates the shared state for the window root.
pub fn provide_nav() -> Nav {
    use_provide_context(|| Nav {
        tab: State::create(Tab::Elements),
        selected: State::create(None),
    })
}

/// The shared state, from any component below the window root.
pub fn use_nav() -> Nav {
    use_consume::<Nav>()
}

/// A tab's label: body text one step smaller, as Chrome's panel tabs.
const TAB: TextStyle = TextStyle {
    size: TextSize::Sm,
    ..BODY
};

/// The panel's tab strip: flat labels with the selected one underlined in the
/// accent, as Chrome's panel tabs. Only the selected tab is in the tab order.
/// Left and Right arrows move the selection and focus to the neighbouring
/// tab, and Home and End go to the first and last.
#[derive(PartialEq)]
pub struct TabStrip<T: StripTab> {
    selected: T,
    on_select: EventHandler<T>,
}

/// The tab strip. `on_select` is called with the tab the user picked.
pub fn tab_strip<T: StripTab>(selected: T, on_select: impl Into<EventHandler<T>>) -> TabStrip<T> {
    TabStrip {
        selected,
        on_select: on_select.into(),
    }
}

impl<T: StripTab> Component for TabStrip<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        // One accessibility id per tab, kept for the life of the strip, so the
        // arrow keys can move focus to a neighbouring tab.
        let ids = use_hook(|| T::ITEMS.iter().map(|_| AccessibilityId::new_unique()).collect::<Vec<_>>());
        let selected = self.selected;
        let on_select = self.on_select.clone();
        let key_on_select = on_select.clone();
        let key_ids = ids.clone();
        rect()
            .tw(&tokens, &[h(Dim::Box7), flex_row()])
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let Some(index) = T::ITEMS.iter().position(|tab| *tab == selected) else {
                    return;
                };
                let last = T::ITEMS.len() - 1;
                let next = match &event.key {
                    Key::Named(NamedKey::ArrowLeft) => index.saturating_sub(1),
                    Key::Named(NamedKey::ArrowRight) => (index + 1).min(last),
                    Key::Named(NamedKey::Home) => 0,
                    Key::Named(NamedKey::End) => last,
                    _ => return,
                };
                event.stop_propagation();
                if next != index {
                    key_on_select.call(T::ITEMS[next]);
                }
                key_ids[next].request_focus();
            })
            .children(T::ITEMS.iter().copied().enumerate().map(|(index, tab)| {
                Element::from(TabLabel {
                    tab,
                    selected: tab == selected,
                    a11y_id: ids[index],
                    on_select: on_select.clone(),
                })
            }))
    }
}

#[derive(PartialEq)]
struct TabLabel<T: StripTab> {
    tab: T,
    selected: bool,
    a11y_id: AccessibilityId,
    on_select: EventHandler<T>,
}

impl<T: StripTab> Component for TabLabel<T> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(true);
        let press = use_press();
        let tab = self.tab;
        let selected = self.selected;
        let on_select = self.on_select.clone();
        // Only the selected tab is brighter; hover brightens the label without
        // drawing a second underline.
        let ink = if selected || hovering() {
            Ink::Text
        } else {
            Ink::Muted
        };
        let underline = if selected {
            Paint::Accent
        } else {
            Paint::Transparent
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled && selected)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(move |builder| builder.set_selected(selected))
            .tw(&tokens, &[h_full(), px(S3), center()])
            .maybe(enabled, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| hovering.set(false))
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        if !press.tap(&event) {
                            return;
                        }
                        on_select.call(tab);
                        a11y_id.request_focus();
                    })
            })
            .child(TAB.in_ink(ink).label(&tokens, tab.title()).max_lines(1))
            .child(
                rect()
                    .position(Position::new_absolute().bottom(0.0).left(0.0))
                    .tw(&tokens, &[w_full(), h(Dim::Indicator), bg(underline)]),
            )
            .focusable(focus(), Radius::Inner)
    }
}
