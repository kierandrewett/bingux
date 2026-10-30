//! The colour button: a swatch that opens the picker.
//!
//! The button is a control-high square holding a ring, and in the ring the
//! colour over a checkerboard. A press, Enter or Space opens the same
//! [`ColourPicker`] as the inline form, in a popover anchored to the button on
//! a desktop and in a bottom sheet on a phone. The popover closes on Escape, on
//! a press outside it, on Back and on the Cancel and Done buttons.
//!
//! What the popover does with the colour depends on [`live`](ColourButton::live):
//!
//! - **Commit on close** (the default). The picker edits a draft and `on_change`
//!   hears nothing until **Done**, which reports the draft. Cancel, Escape, a
//!   press outside and Back drop it: the old colour was never changed, so there
//!   is nothing to put back.
//! - **Live**. `on_change` hears every colour as it is dragged, so the page
//!   behind shows it. **Done** closes. Escape, a press outside and Back close
//!   too and keep what was shown; there is no Cancel, as there is nothing held
//!   back to cancel.
//!
//! The button works in dialogs and sheets as every popover does: it opens
//! through the same panel, over whatever holds it.

use freya::prelude::*;

use super::chip::colour_chip;
use super::core::use_recents;
use super::geometry::Geometry;
use super::picker::{ColourPicker, colour_picker};
use crate::component::ComponentName;
use crate::controls::button::{action_button, primary_button};
use crate::controls::press::use_press;
use crate::controls::tooltip::hover_tip;
use crate::focus::{press_focus, use_focus_core};
use crate::motion::Focusable;
use crate::shell::panel::{Panel, PanelKind, PanelRect, PanelTrigger};
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};
use crate::typography::{EMPHASIS, Ink};

/// A colour as a button that opens the picker. See the [module](self).
#[derive(PartialEq)]
pub struct ColourButton {
    value: Color,
    on_change: EventHandler<Color>,
    live: bool,
    alpha: bool,
    accents: bool,
    presets: Option<Vec<Color>>,
    recent: Option<(Vec<Color>, EventHandler<Vec<Color>>)>,
    title: String,
    enabled: bool,
    dense: bool,
}

/// A colour button for `value`; `on_change` hears the colour the picker ends on.
pub fn colour_button(value: Color, on_change: impl Into<EventHandler<Color>>) -> ColourButton {
    ColourButton {
        value,
        on_change: on_change.into(),
        live: false,
        alpha: false,
        accents: true,
        presets: None,
        recent: None,
        title: "Colour".to_string(),
        enabled: true,
        dense: false,
    }
}

impl ColourButton {
    /// Reports the colour as it is dragged, instead of when Done is pressed.
    pub fn live(mut self, live: bool) -> Self {
        self.live = live;
        self
    }

    /// Shows the opacity strip and box in the picker.
    pub fn alpha(mut self, alpha: bool) -> Self {
        self.alpha = alpha;
        self
    }

    /// Shows the system accent colours in the picker's palette: on by default.
    pub fn accents(mut self, accents: bool) -> Self {
        self.accents = accents;
        self
    }

    /// The picker's swatches after the accents; see [`ColourPicker::presets`].
    pub fn presets(mut self, presets: Vec<Color>) -> Self {
        self.presets = Some(presets);
        self
    }

    /// The recent colours the app keeps; see [`ColourPicker::recent`].
    pub fn recent(
        mut self,
        colours: Vec<Color>,
        on_change: impl Into<EventHandler<Vec<Color>>>,
    ) -> Self {
        self.recent = Some((colours, on_change.into()));
        self
    }

    /// What the popover and the button call the colour, such as "Accent colour".
    /// It is the sheet's title on a phone and the button's spoken name's first word.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The button at a tool's dense row height ([`Dim::DenseControl`], 24 px),
    /// for an inspector's property row. A finger gets the full size.
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }

    /// The picker the popover holds, as far as its size goes.
    fn picker(&self, value: Color, on_change: EventHandler<Color>) -> ColourPicker {
        let mut picker = colour_picker(value, on_change)
            .alpha(self.alpha)
            .accents(self.accents);
        if let Some(presets) = &self.presets {
            picker = picker.presets(presets.clone());
        }
        picker
    }
}

impl Component for ColourButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let class = size_class();
        let g = Geometry::new(class);
        let mut open = use_state(|| false);
        let mut draft = use_state(|| self.value);
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut area = use_state(Area::default);
        let scroll = use_scroll_controller(ScrollConfig::default);
        let a11y_id = use_a11y();
        let core = use_focus_core();
        let focus = use_focus(a11y_id);
        let press = use_press();
        let enabled = use_enabled(self.enabled);
        let recents = use_recents(self.recent.clone(), true);
        let live = self.live;
        let value = self.value;
        let on_change = self.on_change.clone();

        let hex = colour_to_hex(value);
        let mut toggle = move || {
            let next = !*open.peek();
            if next {
                draft.set(value);
            }
            open.set(next);
        };
        let mut toggle_key = toggle;

        // The swatch sits in a ring, a ring's width and a hair inside it.
        let dense = self.dense && !g.compact();
        let (side, ring_box) = if dense {
            (Dim::DenseControl, Dim::Box5)
        } else {
            (Dim::Control, Dim::Box7)
        };
        let chip = (ring_box.px() - 2.0 * (Dim::Ring.px() + S0_5.px())).max(0.0);
        let face = rect()
            .component("ColourButton")
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{}, {hex}", self.title))
            .a11y_builder(move |builder| builder.set_expanded(open()))
            .maybe(!enabled, |face| face.opacity(0.5))
            .tw(
                &tokens,
                &[
                    w(side),
                    h(side),
                    center(),
                    rounded(Radius::Control),
                    surface(Paint::Transparent, hovering() || open(), pressing(), false),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                area.set_if_modified(event.area);
            })
            .maybe(enabled, |face| {
                face.on_pointer_enter(move |_| hovering.set(true))
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
                        toggle();
                        press_focus(&core, a11y_id);
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            toggle_key();
                        }
                    })
            })
            .child(
                rect()
                    .component("ColourRing")
                    .tw(
                        &tokens,
                        &[
                            w(ring_box),
                            h(ring_box),
                            center(),
                            rounded(Radius::Pill),
                            border(Line::Ring(Paint::Outline)),
                        ],
                    )
                    .child(colour_chip(tokens, value, chip)),
            )
            .focusable(focus());

        // The popover's picker edits a draft; it is the draft the preview's
        // new half shows and Done reports.
        let drafting = {
            let on_change = on_change.clone();
            EventHandler::new(move |colour: Color| {
                draft.set(colour);
                if live {
                    on_change.call(colour);
                }
            })
        };
        let picker = self
            .picker(draft(), drafting)
            .original(value)
            .sharing(recents.clone(), live);
        let height = picker.height();

        let done = {
            let recents = recents.clone();
            EventHandler::new(move |_: ()| {
                if !live {
                    let chosen = *draft.peek();
                    if chosen != value {
                        on_change.call(chosen);
                    }
                    recents.add(chosen);
                }
                open.set(false);
            })
        };
        // Everything but Done lets go: a draft was never reported, and a live
        // colour was, and stays.
        let dismiss = EventHandler::new(move |_: ()| open.set(false));

        let actions = {
            let mut row = rect()
                .component("ColourActions")
                .tw(&tokens, &[w_full(), flex_row(), gap(S2), justify(Align::End)]);
            if !live {
                row = row.child(action_button("Cancel", dismiss.clone()));
            }
            row.child(primary_button("Done", done.clone()))
        };
        let header = {
            let side = |content: Option<Element>, end: bool| {
                rect()
                    .tw(
                        &tokens,
                        &[
                            w(Dim::Box24),
                            flex_row(),
                            justify(if end { Align::End } else { Align::Start }),
                        ],
                    )
                    .maybe_child(content)
            };
            let cancel = (!live).then(|| Element::from(action_button("Cancel", dismiss.clone()).flat()));
            rect()
                .component("ColourHeader")
                .tw(&tokens, &[w_full(), flex_row(), grow_children(), items(Align::Center)])
                .child(side(cancel, false))
                .child(
                    rect()
                        .tw(&tokens, &[w_flex(), center()])
                        .child(
                            EMPHASIS
                                .in_ink(Ink::Text)
                                .label(&tokens, self.title.clone())
                                .max_lines(1),
                        ),
                )
                .child(side(Some(Element::from(primary_button("Done", done.clone()))), true))
        };

        let compact = g.compact();
        let content = rect()
            .component("ColourPopover")
            .tw(&tokens, &[w_full(), flex_col(), gap(S3)])
            .maybe_child(compact.then_some(header))
            .child(picker)
            .maybe_child((!compact).then_some(actions));
        let content_height = height + g.actions();
        let width = Dim::PickerWidth.px() + 2.0 * S2.px();
        let mut panel = Panel::new(
            PanelKind::Choice,
            open() && enabled,
            PanelTrigger::Control(PanelRect::from_area(&area.read())),
            vec![Element::from(content)],
        )
        .size(width, content_height)
        .tall()
        .on_dismiss(dismiss);
        if compact {
            panel = panel.scroll(scroll);
        }

        rect()
            .component("ColourButtonHost")
            .child(hover_tip(face, hex))
            .child(Element::from(panel))
    }
}
