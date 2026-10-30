//! Buttons: `ActionButton`, the accent primary button, the small row pill
//! and the square `IconButton`.

use super::press::{is_right_click, use_hold, use_press};
use super::tooltip::hover_tip;
use super::default_mark::{Face, default_mark};
use crate::shell::menu::MenuItem;
use crate::shell::overlays::Overlays;
use crate::shell::panel::{PanelRect, PanelTrigger};
use freya::prelude::*;

use crate::component::{ComponentName, Prop, Published};
use crate::controls::progress::{SpinnerSize, spinner};
use crate::focus::{press_focus, use_focus_core};
use crate::icons::{self, Icon};
use crate::motion::{Focusable, MorphGlyph};
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};
use crate::typography::{BODY, EMPHASIS, Ink};

/// The shell's `ActionButton`: 36 px tall on `control` at radius 8, with no
/// idle outline.
#[derive(PartialEq)]
pub struct ActionButton {
    text: String,
    icon: Option<Icon>,
    flat: bool,
    accent: bool,
    destructive: bool,
    enabled: bool,
    loading: bool,
    grow: bool,
    fill: bool,
    compact: bool,
    autofocus: bool,
    default_action: bool,
    radius: Option<Radius>,
    on_press: EventHandler<()>,
}

pub fn action_button(
    text: impl Into<String>,
    on_press: impl Into<EventHandler<()>>,
) -> ActionButton {
    ActionButton {
        text: text.into(),
        icon: None,
        flat: false,
        accent: false,
        destructive: false,
        enabled: true,
        loading: false,
        grow: false,
        fill: false,
        compact: false,
        autofocus: false,
        default_action: false,
        radius: None,
        on_press: on_press.into(),
    }
}

impl ActionButton {
    /// Fill with the accent colour (the same as [`primary_button`]).
    pub fn suggested(mut self) -> Self {
        self.accent = true;
        self
    }

    /// Fill with the danger colour, for actions that lose data.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Show a small spinner instead of the icon and refuse presses.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Share the parent's width equally with sibling buttons (dialogs).
    pub fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// Fill the parent's width, for a button stacked in a column (a dialog at
    /// compact density).
    pub fn fill_width(mut self) -> Self {
        self.fill = true;
        self
    }

    /// Mark this button as the default action of the dialog or form it is in:
    /// the one that Enter presses when no button has the keyboard focus. A
    /// key cap with the Return glyph after the label shows it on a desktop,
    /// on every fill (see [`crate::controls::default_mark`]), and a phone
    /// shows nothing. The dialog sets it, so call this only for a button
    /// that really is Enter's target.
    pub fn default_action(mut self) -> Self {
        self.default_action = true;
        self
    }

    /// The corner radius of the face, for a button inset in a rounded card:
    /// the card's radius less the inset ([`Radius::nested`]).
    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = Some(radius);
        self
    }

    /// 32 px high instead of 36, for a button inside a row.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    /// Take the keyboard focus when the button first appears, so Enter
    /// presses it. A dialog gives it to its safe answer.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Component for ActionButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fitted = crate::rows::use_row_fitted();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let core = use_focus_core();
        let focus = use_focus(a11y_id);
        let autofocus = self.autofocus;
        use_side_effect(move || {
            if autofocus {
                a11y_id.request_focus();
            }
        });
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        let press = use_press();
        let usable = use_enabled(self.enabled);
        let enabled = usable && !self.loading;
        let filled = self.accent || self.destructive;
        // The fill is a paint; the state layer on top comes from `surface`.
        // No idle outline: the shell's buttons are filled or flat. Only the
        // keyboard focus ring draws a border.
        let base = if self.destructive {
            Paint::Danger
        } else if filled {
            Paint::Accent
        } else if self.flat {
            Paint::Transparent
        } else {
            Paint::Control
        };
        let ink = if !usable {
            Ink::Muted
        } else if self.destructive {
            Ink::OnDanger
        } else if filled {
            Ink::OnAccent
        } else {
            Ink::Text
        };
        let color = ink.color(&tokens);
        let ink_paint = if !usable {
            Paint::Muted
        } else if self.destructive {
            Paint::OnDanger
        } else if filled {
            Paint::OnAccent
        } else {
            Paint::Text
        };
        let face = if self.destructive {
            Face::Danger
        } else if self.accent {
            Face::Accent
        } else {
            Face::Plain
        };
        let corner = self
            .radius
            .unwrap_or(if fitted { Radius::Inner } else { Radius::Control });
        // The keycap is for a window with a keyboard: a phone shows the focus
        // ring alone.
        let mark = (self.default_action && enabled && !is_compact())
            .then(|| default_mark(&tokens, face));
        rect()
            .component("ActionButton")
            .published(|| {
                Published::call("action_button")
                    .arg(Prop::text("text", self.text.clone()))
                    .maybe_method(self.accent.then(|| Prop::flag("suggested", true)))
                    .maybe_method(self.destructive.then(|| Prop::flag("destructive", true)))
                    .maybe_method((!self.enabled).then(|| Prop::flag("enabled", false)))
            })
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Button)
            .tw(&tokens, if self.compact { BUTTON_COMPACT } else { BUTTON })
            .maybe(fitted, |element| {
                element.tw(&tokens, &[h(Dim::RowButton), rounded(Radius::Inner)])
            })
            .maybe(self.radius.is_some(), |element| {
                element.tw(&tokens, &[rounded(corner)])
            })
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    justify(Align::Center),
                    px(S3),
                    gap(S2),
                    surface(base, hovering(), pressing(), false),
                ],
            )
            // A flat button has no fill, so a minimum width would only centre
            // a short label inside empty space and push it off the row's edge.
            .maybe(!self.flat, |element| {
                element.tw(&tokens, &[min_w(Dim::Box9)])
            })
            .maybe(self.grow || self.fill, |element| element.tw(&tokens, &[w_flex()]))
            .maybe(!usable, |element| element.opacity(0.5))
            .maybe(enabled, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
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
                        press_focus(&core, a11y_id);
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            on_key.call(());
                        }
                    })
            })
            .maybe_child(
                self.loading
                    .then(|| rect().child(spinner(SpinnerSize::Small).ink(ink_paint))),
            )
            .maybe_child(
                (!self.loading)
                    .then(|| {
                        self.icon
                            .map(|icon| icons::icon(icon, IconSize::Control, color))
                    })
                    .flatten(),
            )
            .child(
                BODY.in_ink(ink)
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            )
            .maybe_child(mark).focusable(focus())
            // The focus wrapper is what the row lays out, so it grows too.
            .maybe(self.grow, |wrapper| {
                wrapper.tw(&tokens, &[w_flex(), grow_children()])
            })
            .maybe(self.fill, |wrapper| {
                wrapper.tw(&tokens, &[w_full(), grow_children()])
            })
    }
}

/// The page's primary action, filled with the accent colour.
pub fn primary_button(
    text: impl Into<String>,
    on_press: impl Into<EventHandler<()>>,
) -> ActionButton {
    let mut button = action_button(text, on_press);
    button.accent = true;
    button
}

/// A row's secondary action: the same filled button as every other control.
#[derive(PartialEq)]
pub struct RowButton {
    text: String,
    prominent: bool,
    on_press: EventHandler<()>,
}

pub fn row_button(text: impl Into<String>, on_press: impl Into<EventHandler<()>>) -> RowButton {
    RowButton {
        text: text.into(),
        prominent: false,
        on_press: on_press.into(),
    }
}

impl RowButton {
    /// Text colour instead of muted, for banners and toasts.
    pub fn prominent(mut self) -> Self {
        self.prominent = true;
        self
    }
}

impl Component for RowButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fitted = crate::rows::use_row_fitted();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let core = use_focus_core();
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(true);
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        let press = use_press();
        let base = if self.prominent {
            Paint::Transparent
        } else {
            Paint::Control
        };
        let style = if self.prominent { EMPHASIS } else { BODY };
        rect()
            .component("RowButton")
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Button)
            .maybe(!enabled, |element| element.opacity(0.5))
            .tw(&tokens, BUTTON_COMPACT)
            .maybe(fitted, |element| {
                element.tw(&tokens, &[h(Dim::RowButton), rounded(Radius::Inner)])
            })
            .tw(
                &tokens,
                &[
                    center(),
                    px(S3),
                    surface(base, hovering(), pressing(), false),
                ],
            )
            .maybe(enabled, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
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
                        press_focus(&core, a11y_id);
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            on_key.call(());
                        }
                    })
            })
            .child(
                style
                    .in_ink(Ink::Text)
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            ).focusable(focus())
    }
}

/// The shell's `IconButton`: a square labelled icon action with a tooltip.
#[derive(PartialEq)]
pub struct IconButton {
    icon: Icon,
    /// `None` takes the header's size inside a header bar, else a button's.
    size: Option<Dim>,
    tooltip: Option<String>,
    on_press: EventHandler<()>,
    danger: bool,
    filled: bool,
    active: bool,
    enabled: bool,
    busy: bool,
    radius: Radius,
    menu: Vec<MenuItem>,
    refocus: Option<AccessibilityId>,
    pointer_only: bool,
    /// The glyph's colour instead of the text colour, for a button on a
    /// tinted notice.
    ink: Option<Ink>,
    id: Option<AccessibilityId>,
    /// The hover and pressed layers, in place of the page's own: on a surface
    /// they would not show on (a dark toast in a light window).
    layers: Option<(Color, Color)>,
    /// The glyph the icon turns into while the flag is set, see
    /// [`IconButton::morph`].
    morph: Option<(Icon, bool)>,
}

pub fn icon_button(icon: Icon, on_press: impl Into<EventHandler<()>>) -> IconButton {
    IconButton {
        icon,
        size: None,
        tooltip: None,
        on_press: on_press.into(),
        danger: false,
        filled: false,
        active: false,
        enabled: true,
        busy: false,
        radius: Radius::Control,
        menu: Vec::new(),
        refocus: None,
        pointer_only: false,
        ink: None,
        id: None,
        layers: None,
        morph: None,
    }
}

impl IconButton {
    pub fn tooltip(mut self, text: &str) -> Self {
        self.tooltip = Some(text.to_string());
        self
    }

    pub fn size(mut self, size: Dim) -> Self {
        self.size = Some(size);
        self
    }

    /// Hover fills with the danger colour, for close buttons.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// A control fill at rest, for a button that stands on its own rather
    /// than in a header bar.
    pub fn filled(mut self) -> Self {
        self.filled = true;
        self
    }

    /// Tint the button in the accent colour and draw its icon in the accent,
    /// for the one button of a set that is on or playing. No ring.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// The corner radius of the button and its focus ring: `Control` by
    /// default, like every other button; `Mark` for the square toolbar buttons
    /// of the inspector.
    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    /// Draw the glyph in `ink` instead of the text colour, for a button on a
    /// tinted surface such as a banner.
    pub fn ink(mut self, ink: Ink) -> Self {
        self.ink = Some(ink);
        self
    }

    /// The layers a hover and a press lay over the button, in place of the
    /// window's own, for a button on a surface those would not show on: a dark
    /// notice in a light window takes a light layer, so pass its ink's.
    pub fn layers(mut self, hover: Color, pressed: Color) -> Self {
        self.layers = Some((hover, pressed));
        self
    }

    /// Use `id` as the button's focus id, so the caller can tell when it holds
    /// focus.
    pub fn a11y_id(mut self, id: AccessibilityId) -> Self {
        self.id = Some(id);
        self
    }

    /// Dim the button and ignore presses, for an action that is not
    /// available yet. It stays on screen so a header keeps its shape.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Show a spinner in place of the icon and ignore presses, while the
    /// action it starts is running. The button keeps its size.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    /// Right click, or a hold of the primary button or a finger (450 ms),
    /// opens `items` as a menu anchored under the button. The press that ends
    /// a hold does not also run the button's action. The back button's history
    /// is one; any icon button with a secondary list (a split button's, a
    /// zoom control's) can use it.
    pub fn context_menu(mut self, items: Vec<MenuItem>) -> Self {
        self.menu = items;
        self
    }

    /// After a press, give the keyboard focus to `id` and not to the button.
    /// A button inside a text field (the clear button, the password reveal)
    /// hands the focus back to the field, so typing carries on.
    pub fn refocus(mut self, id: AccessibilityId) -> Self {
        self.refocus = Some(id);
        self
    }

    /// Keep the button out of the Tab order and out of arrow-key walks: for
    /// a button at the end of a row or a tab whose own keyboard stop already
    /// offers the same action. The pointer and touch use it as any icon
    /// button, with the same hover and pressed fill and tooltip.
    pub fn pointer_only(mut self) -> Self {
        self.pointer_only = true;
        self
    }

    /// Turn the icon into `to` while `on` is true, and back when it is not:
    /// the icon quarter-turns, shrinks and fades out as `to` turns to rest,
    /// grows and fades in (`motion::icon_morph`), over the motion token's
    /// duration, instantly under reduced motion. Both glyphs share the
    /// button's icon slot, so the button keeps its size, hover, pressed and
    /// focus state and keyboard focus through the change; a flip in the middle
    /// of the motion goes back from where the icon is. The caller sets the
    /// tooltip (which is also the button's accessible name) and the press
    /// handler to match the state it is in: Reload and Stop is
    /// `icon_button(Icon::Refresh, f).morph(Icon::MediaPlaybackStop, loading)`.
    pub fn morph(mut self, to: Icon, on: bool) -> Self {
        self.morph = Some((to, on));
        self
    }
}

/// Marks the subtree of a header bar: icon buttons in it are
/// [`HEADER_BUTTON`] tall, whoever builds them.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct InHeader;

/// Provided by a header bar with the flag set when it holds a strip of tabs:
/// the bar is then a tab bar tall, so its icon buttons are the tab button,
/// 32 px (the 44 px touch target at compact density), centred in it: the same
/// size and the same distance from the bar's top and bottom as the strip's
/// own New tab button.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct InTabBar(pub bool);

/// The size an icon button takes when its caller sets none: [`HEADER_BUTTON`]
/// inside a header bar, [`Dim::TabButton`] inside one that holds tabs,
/// `outside` elsewhere.
pub(crate) fn header_size(outside: Dim) -> Dim {
    if try_consume_context::<InTabBar>().is_some_and(|bar| bar.0) {
        Dim::TabButton
    } else if try_consume_context::<InHeader>().is_some() {
        HEADER_BUTTON
    } else {
        outside
    }
}

impl Component for IconButton {
    fn render(&self) -> impl IntoElement {
        let fitted = crate::rows::use_row_fitted();
        let size = if fitted {
            Dim::RowButton
        } else {
            self.size.unwrap_or_else(|| header_size(Dim::Button))
        };
        let radius = if fitted { Radius::Inner } else { self.radius };
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let own_id = use_a11y();
        let a11y_id = self.id.unwrap_or(own_id);
        let core = use_focus_core();
        let focus = use_focus(a11y_id);
        let usable = use_enabled(self.enabled);
        // A busy button swallows presses, so a slow action is not asked twice.
        let on_press = if self.busy {
            EventHandler::new(|_: ()| {})
        } else {
            self.on_press.clone()
        };
        let on_key = on_press.clone();
        // Where the keyboard focus goes once the press has run: the button
        // itself unless the caller named another control.
        let focus_after = self.refocus.unwrap_or(a11y_id);
        let press = use_press();
        let hold = use_hold();
        let overlays = use_try_consume::<Overlays>();
        let mut area = use_state(Area::default);
        let has_menu = !self.menu.is_empty();
        let open_menu: EventHandler<()> = {
            let items = self.menu.clone();
            EventHandler::new(move |_: ()| {
                if let Some(overlays) = overlays {
                    overlays.show_menu_from(
                        Some(a11y_id),
                        PanelTrigger::Control(PanelRect::from_area(&area.read())),
                        items.clone(),
                        false,
                    );
                }
            })
        };
        // Only a hovered danger button takes the danger fill; the state
        // layer from `surface` then sits over it.
        let base = if self.danger && hovering() {
            Paint::Danger
        } else if self.active {
            Paint::AccentSoft
        } else if self.filled {
            Paint::Control
        } else {
            Paint::Transparent
        };
        let state = match self.layers {
            Some((hover, pressed)) => fill(if pressing() {
                pressed
            } else if hovering() {
                hover
            } else {
                Paint::Transparent.color(&tokens)
            }),
            None => surface(base, hovering(), pressing(), false),
        };
        let button = rect()
            .component("IconButton")
            .a11y_id(a11y_id)
            .a11y_focusable(usable && !self.pointer_only)
            .a11y_enabled(usable)
            .a11y_role(AccessibilityRole::Button)
            .maybe(self.tooltip.is_some(), |element| {
                element.a11y_alt(self.tooltip.clone().unwrap_or_default())
            })
            .tw(&tokens, ICON_BUTTON)
            .tw(
                &tokens,
                &[
                    w(size),
                    h(size),
                    state,
                    rounded(radius),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .maybe(!usable, |element| element.opacity(0.5))
            .maybe(usable, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        if has_menu {
                            // A right click opens the menu at once; a hold of the
                            // primary button (or a finger) opens it after a beat.
                            if is_right_click(&event) {
                                open_menu.call(());
                                return;
                            }
                            hold.begin(&event, open_menu.clone());
                        }
                        pressing.set(true);
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                        hold.moved(&event);
                    })
                    .on_global_pointer_up(move |_| {
                        press.release();
                        hold.release();
                        pressing.set_if_modified(false);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        // The press that ends a hold already opened the menu.
                        if hold.fired() || !press.tap(&event) {
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
            .child(if self.busy {
                Element::from(spinner(SpinnerSize::Small))
            } else {
                let ink = if self.danger && hovering() {
                    Paint::OnDanger.color(&tokens)
                } else if self.active {
                    Ink::Accent.color(&tokens)
                } else {
                    self.ink.unwrap_or(Ink::Text).color(&tokens)
                };
                match self.morph {
                    Some((to, on)) => Element::from(MorphGlyph {
                        from: self.icon,
                        to,
                        size: IconSize::Control,
                        color: ink,
                        on,
                    }),
                    None => icons::icon(self.icon, IconSize::Control, ink).into(),
                }
            }).focusable(focus());
        match &self.tooltip {
            Some(text) => hover_tip(button, text.clone()),
            None => Element::from(button),
        }
    }
}

#[cfg(test)]
mod header_button_tests {
    use super::*;
    use crate::style::SizeClass;

    #[test]
    fn a_header_button_is_the_control_height_at_both_densities() {
        // The segmented control and the split button stand on the same token,
        // so a row of header controls lines up on one height.
        assert_eq!(HEADER_BUTTON, Dim::Control);
        assert_eq!(HEADER_BUTTON.px_in(SizeClass::Regular), 36.0);
        assert_eq!(HEADER_BUTTON.px_in(SizeClass::Compact), 44.0);
    }

    #[test]
    fn a_header_button_fits_its_bar_with_room_to_spare() {
        for class in [SizeClass::Regular, SizeClass::Compact] {
            assert!(HEADER_BUTTON.px_in(class) + 2.0 * S1.px_in(class) <= Dim::Header.px_in(class));
        }
    }
}
