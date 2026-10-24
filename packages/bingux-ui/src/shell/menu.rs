//! A small popover menu of actions, opened at a point on the window: from
//! a button, or from a right click. Picking an item closes the menu and then
//! runs its action. Escape and a press outside the menu close it.

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink};

use super::overlays::{Overlays, input_shield, use_overlays, use_presence};

/// One action in a menu. Build with [`menu_item`].
#[derive(Clone, PartialEq)]
pub struct MenuItem {
    pub(crate) label: String,
    pub(crate) icon: Option<Icon>,
    pub(crate) checked: Option<bool>,
    pub(crate) enabled: bool,
    pub(crate) destructive: bool,
    pub(crate) separator: bool,
    pub(crate) on_press: EventHandler<()>,
}

impl MenuItem {
    /// Shows an icon before the label.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Shows a tick before the label when `checked` is true. The space stays
    /// when it is false, so labels line up with checked items.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Greys the item out. A disabled item cannot be picked.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Colours the item as a destructive action.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

/// An action for a menu. `on_press` runs after the menu closes.
pub fn menu_item(label: impl Into<String>, on_press: impl FnMut(()) + 'static) -> MenuItem {
    MenuItem {
        label: label.into(),
        icon: None,
        checked: None,
        enabled: true,
        destructive: false,
        separator: false,
        on_press: EventHandler::new(on_press),
    }
}

/// A thin line between groups of items.
pub fn menu_separator() -> MenuItem {
    MenuItem {
        separator: true,
        ..menu_item(String::new(), |_| {})
    }
}

/// The open menu: where it sits on the window, and its items.
#[derive(Clone, PartialEq)]
pub(crate) struct Menu {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) items: Vec<MenuItem>,
}

/// Space kept between a menu and the window edges.
const MENU_EDGE: f32 = S2.px();

/// The height of a menu of `items`: its rows, separators and padding.
fn menu_height(items: &[MenuItem]) -> f32 {
    let rows: f32 = items
        .iter()
        .map(|item| {
            if item.separator {
                Dim::MenuSeparator.px()
            } else {
                Dim::MenuItem.px()
            }
        })
        .sum();
    rows + S2.px() + S2.px()
}

/// Where a menu of `size` opened at `at` sits on a window of `window`. It
/// opens down and to the right of the pointer, flips up or left when there is
/// no room, and is then held inside the window on all four edges. With no
/// window size yet, it stays where it was opened.
fn place_menu(at: (f32, f32), size: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let (x, y) = at;
    let (width, height) = size;
    let (window_width, window_height) = window;
    if window_width <= 0.0 || window_height <= 0.0 {
        return at;
    }
    let x = if x + width + MENU_EDGE > window_width {
        x - width
    } else {
        x
    };
    let y = if y + height + MENU_EDGE > window_height {
        y - height
    } else {
        y
    };
    let x = x.min(window_width - width - MENU_EDGE).max(MENU_EDGE);
    let y = y.min(window_height - height - MENU_EDGE).max(MENU_EDGE);
    (x, y)
}

#[derive(Clone, PartialEq)]
pub(crate) struct MenuLayer {
    pub(crate) menu: Menu,
    pub(crate) closing: bool,
    pub(crate) overlays: Overlays,
    pub(crate) layer: Layer,
}

impl Component for MenuLayer {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let a11y_id = use_a11y();
        let progress = use_presence(0, self.closing, tokens.motion(90), move |_| {
            overlays.finish_menu()
        });
        use_side_effect(move || a11y_id.request_focus());

        let rows = self.menu.items.iter().map(|item| {
            Element::from(MenuRow {
                item: item.clone(),
                close: overlays,
            })
        });

        let window = *Platform::get().current_window().root_size.read();
        let (x, y) = place_menu(
            (self.menu.x, self.menu.y),
            (Dim::Menu.px(), menu_height(&self.menu.items)),
            (window.width, window.height),
        );
        let panel = rect()
            .layer(Layer::Overlay)
            .position(Position::new_absolute().top(y).left(x))
            .opacity(progress)
            .scale(0.96 + 0.04 * progress)
            .transform_origin(TransformOrigin::top_left())
            .tw(&tokens, MENU)
            .tw(&tokens, &[w(Dim::Menu)])
            .on_press(|event: Event<PressEventData>| event.stop_propagation())
            .on_pointer_down(|event: Event<PointerEventData>| event.stop_propagation())
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if event.key == Key::Named(NamedKey::Escape) {
                    event.stop_propagation();
                    overlays.close_menu();
                }
            })
            .children(rows);

        // The shield closes the menu on a press outside it and keeps the
        // window behind from reacting while the menu is open.
        input_shield(&tokens, move || overlays.close_menu())
            .layer(self.layer)
            .child(panel)
    }
}

#[derive(Clone, PartialEq)]
struct MenuRow {
    item: MenuItem,
    close: Overlays,
}

impl Component for MenuRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        if self.item.separator {
            return rect()
                .tw(&tokens, &[w_full(), h(Dim::MenuSeparator), center()])
                .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
                .into_element();
        }
        let enabled = self.item.enabled;
        let close = self.close;
        let on_press = self.item.on_press.clone();
        // A disabled item keeps its colour and fades like any disabled
        // control, so it reads as unavailable at a glance.
        let ink = if enabled && self.item.destructive {
            Ink::Danger
        } else {
            Ink::Text
        };
        let text_color = ink.color(&tokens);
        // A tick for checked items; the icon otherwise. The space is kept
        // whenever either is set, so labels line up.
        let glyph = match (self.item.checked, self.item.icon) {
            (Some(true), _) => Some(Icon::Check),
            (_, icon) => icon,
        };
        let leading = (self.item.checked.is_some() || self.item.icon.is_some()).then(|| {
            rect()
                .tw(&tokens, &[w(Dim::IconControl), h(Dim::IconControl)])
                .maybe_child(glyph.map(|icon| icons::icon(icon, IconSize::Control, text_color)))
        });
        rect()
            .a11y_role(AccessibilityRole::MenuItem)
            .a11y_enabled(enabled)
            .tw(&tokens, MENU_ITEM)
            .maybe(!enabled, |element| element.opacity(DISABLED_OPACITY))
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    enabled && hovering(),
                    enabled && pressing(),
                    false,
                )],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                if enabled {
                    pressing.set(true);
                }
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                pressing.set(false);
                if enabled {
                    // Close first, so the action can open its own popup.
                    close.close_menu();
                    on_press.call(());
                }
            })
            .maybe_child(leading)
            .child(
                BODY.in_ink(ink)
                    .label(&tokens, self.item.label.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .width(Size::flex(1.0)),
            )
            .into_element()
    }
}

/// A button that opens a menu of its items where the pointer is pressed.
/// Build with [`menu_button`].
#[derive(Clone, PartialEq)]
pub struct MenuButton {
    label: String,
    items: Vec<MenuItem>,
}

/// A button that opens a menu of `items` where the pointer is pressed.
pub fn menu_button(label: impl Into<String>, items: Vec<MenuItem>) -> MenuButton {
    MenuButton {
        label: label.into(),
        items,
    }
}

impl Component for MenuButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let items = self.items.clone();
        rect()
            .a11y_role(AccessibilityRole::Button)
            .tw(
                &tokens,
                &[
                    h(Dim::Control),
                    min_w(Dim::Box9),
                    px(S3),
                    gap(S2),
                    flex_row(),
                    center(),
                    rounded(Radius::Control),
                    surface(Paint::Control, hovering(), pressing(), false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                pressing.set(true);
                let at = event.global_location();
                overlays.show_menu((at.x as f32, at.y as f32), items.clone());
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .child(
                BODY.in_ink(Ink::Text)
                    .label(&tokens, self.label.clone())
                    .max_lines(1),
            )
            .child(icons::icon(
                Icon::PanDown,
                IconSize::Mark,
                Paint::Muted.color(&tokens),
            ))
            .into_element()
    }
}
