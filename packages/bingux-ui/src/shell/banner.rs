//! `Banner` (`AdwBanner`): the one notice component. A banner is a pill with
//! an optional leading icon, an optional title, a message, any number of
//! action buttons and an optional dismiss cross. Its [`BannerKind`] decides
//! every colour (see [`crate::theme::NoticePalette`]).
//!
//! A toast is this same banner floating: [`crate::shell::Toast`] builds a
//! `Banner` and adds only the floating shell (layer, shadow, position,
//! motion, timer). Nothing here is drawn twice.
//!
//! Geometry, shared by both:
//!
//! - The trailing buttons are [`NOTICE_BUTTON`] tall (the control height, 44 px
//!   on a phone) and sit [`NOTICE_PAD`] from the edge above, below and at the
//!   trailing end. The text keeps clear of the curve with a wider leading
//!   inset.
//! - A button's radius is its container's radius less the padding between
//!   them. The container is a pill and a pill less 4 px is a pill, so the
//!   buttons are pills. When a long message makes the banner tall the
//!   container takes [`Radius::Overlay`] and the buttons [`Radius::Card`]
//!   (20 px less the padding), so the corners stay concentric and the text clear of them.
//! - Text wraps; the buttons stay centred on the banner's height. A toast
//!   keeps two lines of text and ends the rest with an ellipsis.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::icon_button;
use crate::controls::press::use_press;
use crate::icons::{self, Icon};
use crate::motion::{Focusable, reveal, reveal_block};
use crate::style::*;
use crate::surface::{is_activation_key, use_tokens};
pub use crate::theme::BannerKind;
use crate::typography::{BODY, EMPHASIS, Ink};

/// The height of the buttons a notice carries (dismiss, and an action such as
/// Undo): the control height, so it is 44 px on a phone, the touch minimum,
/// and the notice grows with it rather than overlapping it.
pub(crate) const NOTICE_BUTTON: Dim = Dim::Control;

/// The padding between a notice's edge and its trailing button: equal above,
/// below and at the trailing edge.
pub(crate) const NOTICE_PAD: Space = S1;

/// The widest a floating notice's text gets, as a share of the window width,
/// so the actions and the close button stay on a phone.
pub(crate) const FLOATING_TEXT_MAX_WIDTH_PERCENT: f32 = 60.0;

/// How much taller than the one-line notice a banner must be before it counts
/// as tall and trades its pill corners for [`Radius::Overlay`].
const TALL_EXTRA: Space = S3;

/// The container radius and the radius of the buttons inside it, for a
/// banner that is `tall` or one line. The buttons' radius is the container's
/// less [`NOTICE_PAD`].
pub(crate) fn notice_radii(tall: bool) -> (Radius, Radius) {
    if tall {
        // 20 px less the 4 px padding is 16 px: `Radius::Card`.
        (Radius::Overlay, Radius::Card)
    } else {
        (Radius::Pill, Radius::Pill)
    }
}

impl BannerKind {
    /// The glyph a banner of this kind leads with. A neutral banner has none.
    pub fn icon(self) -> Option<Icon> {
        match self {
            BannerKind::Neutral => None,
            BannerKind::Info => Some(Icon::DialogInformation),
            BannerKind::Success => Some(Icon::Check),
            BannerKind::Warning => Some(Icon::DialogWarning),
            BannerKind::Error => Some(Icon::DialogError),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum IconChoice {
    /// The kind's own glyph.
    Kind,
    None,
    Custom(Icon),
}

/// A tinted pill with a message, optional icon, title, buttons and dismiss
/// cross. Inline it fills the width of its column and opens and closes by
/// height when `visible` changes (no motion under reduced motion).
#[derive(Clone, PartialEq)]
pub struct Banner {
    text: String,
    title: Option<String>,
    kind: BannerKind,
    icon: IconChoice,
    visible: bool,
    gap: Option<Space>,
    actions: Vec<(String, EventHandler<()>)>,
    on_dismiss: Option<EventHandler<()>>,
    floating: bool,
    button_ids: Vec<AccessibilityId>,
}

pub fn banner(text: impl Into<String>) -> Banner {
    Banner::new(text)
}

impl Banner {
    /// An info banner with `text`.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            title: None,
            kind: BannerKind::Info,
            icon: IconChoice::Kind,
            visible: true,
            gap: None,
            actions: Vec::new(),
            on_dismiss: None,
            floating: false,
            button_ids: Vec::new(),
        }
    }

    pub fn kind(mut self, kind: BannerKind) -> Self {
        self.kind = kind;
        self
    }

    /// A bold line above the message.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// A leading glyph other than the kind's own.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = IconChoice::Custom(icon);
        self
    }

    /// No leading glyph.
    pub fn without_icon(mut self) -> Self {
        self.icon = IconChoice::None;
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// The space between this banner and its neighbours in the column that
    /// holds it. A hidden banner then takes none, so the content does not
    /// jump when it appears.
    pub fn gap(mut self, gap: Space) -> Self {
        self.gap = Some(gap);
        self
    }

    /// A quiet button: a label in the notice's ink with no fill until it is
    /// hovered or pressed. Call it again for more.
    pub fn action(
        mut self,
        label: impl Into<String>,
        on_press: impl Into<EventHandler<()>>,
    ) -> Self {
        self.actions.push((label.into(), on_press.into()));
        self
    }

    /// Show a dismiss cross that calls `on_dismiss`; the caller hides the
    /// banner by rendering it with `visible(false)`.
    pub fn dismissible(mut self, on_dismiss: impl Into<EventHandler<()>>) -> Self {
        self.on_dismiss = Some(on_dismiss.into());
        self
    }

    /// Draw this banner as a floating notice: the shadow, a content-sized
    /// width and a two-line message. A toast does this; it adds the rest of
    /// what floats.
    pub(crate) fn floating(mut self) -> Self {
        self.floating = true;
        self
    }

    /// Run `then` after any action's own handler, so a toast closes once its
    /// action is taken.
    pub(crate) fn after_action(mut self, then: EventHandler<()>) -> Self {
        for (_, on_press) in &mut self.actions {
            let inner = on_press.clone();
            let then = then.clone();
            *on_press = EventHandler::new(move |_: ()| {
                inner.call(());
                then.call(());
            });
        }
        self
    }

    /// How many action buttons it carries.
    pub(crate) fn action_count(&self) -> usize {
        self.actions.len()
    }

    /// The focus ids of the action buttons in order, then the dismiss
    /// cross, so a floating shell can tell that one of them holds focus.
    pub(crate) fn button_ids(mut self, ids: Vec<AccessibilityId>) -> Self {
        self.button_ids = ids;
        self
    }

    fn glyph(&self) -> Option<Icon> {
        match self.icon {
            IconChoice::Kind => self.kind.icon(),
            IconChoice::None => None,
            IconChoice::Custom(icon) => Some(icon),
        }
    }
}

impl Component for Banner {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut tall = use_state(|| false);
        let kind = self.kind;
        let ink = Ink::Notice(kind);
        let floating = self.floating;
        // Several buttons would crush the message on a phone: they go on a row
        // of their own under it, and the banner is as tall as that makes it.
        let stacked = !floating && is_compact() && self.actions.len() > 1;
        let trailing = !self.actions.is_empty() || self.on_dismiss.is_some();
        let (radius, button_radius) = notice_radii(tall() || stacked);
        let one_line = NOTICE_BUTTON.px() + 2.0 * NOTICE_PAD.px();
        let tall_above = one_line + TALL_EXTRA.px();

        let line = |style: crate::typography::TextStyle, text: String| {
            let label = style.in_ink(ink).label(&tokens, text);
            if floating {
                label
                    .max_lines(if style == EMPHASIS { 1 } else { 2 })
                    .text_overflow(TextOverflow::Ellipsis)
                    .max_width(Size::window_percent(FLOATING_TEXT_MAX_WIDTH_PERCENT))
            } else {
                label
            }
        };
        let column = rect()
            .tw(&tokens, &[flex_col()])
            .maybe(self.title.is_some(), |column| column.tw(&tokens, &[py(S1)]))
            .maybe(!floating, |column| column.width(Size::flex(1.0)))
            .maybe_child(
                self.title
                    .clone()
                    .map(|title| line(EMPHASIS, title)),
            )
            .child(line(BODY, self.text.clone()));
        // The message sits on the middle line of a row at least a button tall
        // (the strut), so a banner with no buttons is as tall as one with them.
        let text = rect()
            .tw(&tokens, &[flex_row(), items(Align::Center)])
            .maybe(!floating, |row| row.tw(&tokens, &[grow_children()]).width(Size::flex(1.0)))
            .child(rect().tw_layout(&[h(NOTICE_BUTTON)]))
            .child(column);
        let glyph = self.glyph().map(|glyph| {
            rect()
                .tw(&tokens, &[mr(S1)])
                .child(icons::icon(glyph, IconSize::Row, ink.color(&tokens)))
        });
        let actions = self.actions.iter().enumerate().map(|(index, (label, on_press))| {
            NoticeAction {
                label: label.clone(),
                kind,
                radius: button_radius,
                on_press: on_press.clone(),
                id: self.button_ids.get(index).copied(),
            }
            .into_element()
        });
        let dismiss = self.on_dismiss.clone().map(|on_dismiss| {
            let button = icon_button(Icon::WindowClose, move |_| on_dismiss.call(()))
                .size(NOTICE_BUTTON)
                .radius(button_radius)
                .ink(ink)
                .tooltip("Dismiss");
            match self.button_ids.get(self.actions.len()) {
                Some(id) => button.a11y_id(*id),
                None => button,
            }
        });

        let bar = rect()
            .component("Banner")
            .a11y_role(AccessibilityRole::Status)
            .a11y_alt(self.text.clone())
            .tw(
                &tokens,
                &[
                    pl(S4),
                    pr(if trailing { NOTICE_PAD } else { S4 }),
                    py(NOTICE_PAD),
                    gap(S2),
                    rounded(radius),
                    bg(Paint::NoticeFill(kind)),
                    border(Line::Stroke(Dim::Hairline, Paint::NoticeEdge(kind))),
                ],
            )
            .maybe(floating, |bar| bar.tw(&tokens, &[shadow(Elevation::Raised)]))
            .maybe(!floating, |bar| bar.tw(&tokens, &[w_full(), grow_children()]))
            .on_sized(move |event: Event<SizedEventData>| {
                tall.set_if_modified(event.area.height() > tall_above);
            });
        let bar = if stacked {
            bar.tw(&tokens, &[flex_col()])
                .child(
                    rect()
                        .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2), w_full(), grow_children()])
                        .maybe_child(glyph)
                        .child(text)
                        .maybe_child(dismiss),
                )
                .child(
                    rect()
                        .tw(&tokens, &[flex_row(), justify(Align::End), gap(S2), w_full()])
                        .children(actions),
                )
        } else {
            bar.tw(&tokens, &[flex_row(), items(Align::Center)])
                .maybe_child(glyph)
                .child(text)
                .children(actions)
                .maybe_child(dismiss)
        };

        if floating {
            return Element::from(bar);
        }
        match self.gap {
            Some(gap) => Element::from(reveal_block(self.visible, gap, bar).lazy()),
            None => Element::from(reveal(self.visible, bar).lazy()),
        }
    }
}

/// An action button of a notice, drawn quiet: no fill at rest, so the notice
/// reads as one calm surface, its label in the notice's own ink (the colour
/// of its text and its dismiss glyph, AAA on the fill), the hover and pressed
/// layers of every control over the fill, and the focus ring. It is as tall
/// as the dismiss button beside it, with the same radius.
#[derive(PartialEq)]
struct NoticeAction {
    label: String,
    kind: BannerKind,
    radius: Radius,
    on_press: EventHandler<()>,
    id: Option<AccessibilityId>,
}

impl Component for NoticeAction {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let press = use_press();
        let on_press = self.on_press.clone();
        let on_key = on_press.clone();
        let own_id = use_a11y();
        let a11y_id = self.id.unwrap_or(own_id);
        let focus = use_focus(a11y_id);
        rect()
            .component("NoticeAction")
            .a11y_id(a11y_id)
            .a11y_role(AccessibilityRole::Button)
            .a11y_focusable(true)
            .tw(
                &tokens,
                &[
                    h(NOTICE_BUTTON),
                    px(S3),
                    rounded(self.radius),
                    center(),
                    surface(Paint::Transparent, hovering(), pressing(), false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                pressing.set(true);
                press.begin(&event, None);
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .on_global_pointer_up(move |_| {
                press.release();
                pressing.set_if_modified(false);
            })
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                // A touch that scrolled off the action is not a tap.
                if !press.tap(&event) {
                    return;
                }
                on_press.call(());
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if is_activation_key(&event.key) {
                    on_key.call(());
                }
            })
            .child(
                EMPHASIS
                    .in_ink(Ink::Notice(self.kind))
                    .label(&tokens, self.label.clone())
                    .max_lines(1),
            )
            .focusable(focus())
    }
}
