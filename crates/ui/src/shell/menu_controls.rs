//! A row of controls in a menu: a label, then small buttons, such as the zoom
//! row of a browser's menu (`Zoom  −  100%  +  ⛶`). The buttons are the row's
//! own: a press runs one and the menu stays open, so a person steps the zoom
//! up three times without opening the menu again, unless the control closes
//! the menu ([`MenuControl::closes_menu`]).
//!
//! The pointer presses a button as it would any button, with a tooltip. The
//! keyboard highlights the row like any other row; Left and Right walk the
//! controls, Enter and Space run the one in focus (`super::menu_nav`).

use std::rc::Rc;

use freya::prelude::*;

use super::menu::{MenuItem, menu_item};
use super::overlays::Overlays;
use crate::component::ComponentName;
use crate::controls::button::{action_button, icon_button};
use crate::controls::tooltip::hover_tip;
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;

/// The text of a value control, read each time the row draws, so a level that
/// the control's own presses change shows at once while the menu stays open.
#[derive(Clone)]
struct Text(Rc<dyn Fn() -> String>);

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, PartialEq)]
enum Face {
    Icon(Icon),
    Value(Text),
}

/// One button of a row of controls. Build with [`menu_control`] or
/// [`menu_value_control`].
#[derive(Clone, PartialEq)]
pub struct MenuControl {
    face: Face,
    /// The control's name, for a screen reader and for its tooltip.
    label: String,
    enabled: bool,
    closes: bool,
    divided: bool,
    on_press: EventHandler<()>,
}

/// A button of a row of controls that shows `icon` and is named `label`.
pub fn menu_control(
    icon: Icon,
    label: impl Into<String>,
    on_press: impl FnMut(()) + 'static,
) -> MenuControl {
    MenuControl {
        face: Face::Icon(icon),
        label: label.into(),
        enabled: true,
        closes: false,
        divided: false,
        on_press: EventHandler::new(on_press),
    }
}

/// A button of a row of controls that shows the text `text` returns, a value
/// the other controls change (the zoom level), and is named `label`. `text`
/// is read each time the row draws; read the state it shows inside it.
pub fn menu_value_control(
    text: impl Fn() -> String + 'static,
    label: impl Into<String>,
    on_press: impl FnMut(()) + 'static,
) -> MenuControl {
    MenuControl {
        face: Face::Value(Text(Rc::new(text))),
        label: label.into(),
        enabled: true,
        closes: false,
        divided: false,
        on_press: EventHandler::new(on_press),
    }
}

impl MenuControl {
    /// Dims the control and makes it do nothing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The menu closes before the control's action runs, as it does for a row.
    /// Without it the menu stays open.
    pub fn closes_menu(mut self) -> Self {
        self.closes = true;
        self
    }

    /// A hairline before the control, to set it apart from the ones before.
    pub fn divided(mut self) -> Self {
        self.divided = true;
        self
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn closes(&self) -> bool {
        self.closes
    }

    pub(crate) fn run(&self) {
        if self.enabled {
            self.on_press.call(());
        }
    }
}

/// A row that holds `controls` after `label`. The row itself does nothing when
/// pressed; each control does.
pub fn menu_controls(label: impl Into<String>, controls: Vec<MenuControl>) -> MenuItem {
    MenuItem {
        controls,
        ..menu_item(label, |_| {})
    }
}

/// The row's controls, drawn after its label.
#[derive(Clone, PartialEq)]
pub(crate) struct ControlButtons {
    pub(crate) controls: Vec<MenuControl>,
    pub(crate) overlays: Overlays,
    /// The control the keyboard is on.
    pub(crate) focused: Option<usize>,
}

impl Component for ControlButtons {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let children: Vec<Element> = self
            .controls
            .iter()
            .enumerate()
            .flat_map(|(index, control)| {
                let mut parts: Vec<Element> = Vec::with_capacity(2);
                if control.divided {
                    parts.push(
                        rect()
                            .tw(&tokens, &[px(S1), items(Align::Center)])
                            .child(rect().tw(
                                &tokens,
                                &[w(Dim::Hairline), h(Dim::Box5), bg(Paint::Separator)],
                            ))
                            .into_element(),
                    );
                }
                let focused = self.focused == Some(index);
                parts.push(match &control.face {
                    Face::Icon(icon) => {
                        let on_press = control.on_press.clone();
                        let closes = control.closes;
                        icon_button(*icon, move |_: ()| {
                            if closes {
                                overlays.close_menu();
                            }
                            on_press.call(());
                        })
                        .tooltip(&control.label)
                        .size(Dim::Box7)
                        .active(focused)
                        .enabled(control.enabled)
                        .into_element()
                    }
                    Face::Value(text) => ValueButton {
                        text: text.clone(),
                        label: control.label.clone(),
                        enabled: control.enabled,
                        closes: control.closes,
                        focused,
                        on_press: control.on_press.clone(),
                        overlays,
                    }
                    .into_element(),
                });
                parts
            })
            .collect();
        rect()
            .component("MenuControls")
            .horizontal()
            .tw(&tokens, &[items(Align::Center), gap(S0)])
            .children(children)
    }
}

/// A control that shows a value as text: a flat button whose words are the
/// value, with the control's name as its tooltip.
#[derive(Clone, PartialEq)]
struct ValueButton {
    text: Text,
    label: String,
    enabled: bool,
    closes: bool,
    focused: bool,
    on_press: EventHandler<()>,
    overlays: Overlays,
}

impl Component for ValueButton {
    fn render(&self) -> impl IntoElement {
        let overlays = self.overlays;
        let closes = self.closes;
        let on_press = self.on_press.clone();
        // Read here, so a press elsewhere in the row that changes the value redraws it.
        let text = (self.text.0)();
        let button = action_button(text, move |_: ()| {
            if closes {
                overlays.close_menu();
            }
            on_press.call(());
        })
        .flat()
        .compact()
        .enabled(self.enabled);
        let button = if self.focused {
            button.default_action()
        } else {
            button
        };
        hover_tip(button, self.label.clone())
    }
}
