//! The docked pane: a column that slides out of the right edge of a view.
//!
//! The owner puts a [`PreviewDock`] last in the row that holds its list, and
//! gives it the row's measured area. The dock is always mounted, so its slide
//! has a state to start from: its width tweens between 0 and the remembered
//! width with the standard motion curve (instant under reduced motion), the
//! list beside it takes the rest, and the preview inside is laid out at its
//! final width the whole time, so only the box moves.
//!
//! Dragging the pane's left edge (or the arrow keys on it) resizes it, within
//! the limits of [`pane_state`]; the width is saved when the drag ends and
//! restored the next time. A window too narrow for a pane shows none.

use freya::prelude::*;

use super::chrome::PreviewNav;
use super::host::PreviewHost;
use super::mode::Mode;
use super::pane_state::{self, DEFAULT_WIDTH, STATE_KEY};
use crate::component::ComponentName;
use crate::files::{remember, remembered};
use crate::motion::{MOVE_MS, use_tween};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;
use bingux_preview_core::PreviewSource;

/// The width of the edge's grip, in logical pixels.
const GRIP: f32 = 6.0;

/// The docked pane. `row` is the area of the row the pane sits in; `source` is
/// what it shows (`None` when nothing is selected).
#[derive(Clone, PartialEq)]
pub struct PreviewDock {
    open: bool,
    row: Area,
    source: Option<PreviewSource>,
    nav: PreviewNav,
}

impl PreviewDock {
    pub fn new(open: bool, row: Area, source: Option<PreviewSource>, nav: PreviewNav) -> Self {
        PreviewDock { open, row, source, nav }
    }
}

impl Component for PreviewDock {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut remembered_width = use_state(|| remembered::<f32>(STATE_KEY).unwrap_or(DEFAULT_WIDTH));
        let mut dragging = use_state(|| false);
        // The width the content is laid out at: the last one the pane was open
        // at, so a closing pane does not reflow its content as it shrinks.
        let mut inner = use_state(|| DEFAULT_WIDTH);
        let available = self.row.width();
        let wanted = pane_state::target_width(self.open, *remembered_width.read(), available);
        if wanted > 0.0 && (*inner.peek() - wanted).abs() > 0.5 {
            inner.set(wanted);
        }
        let shown = use_tween(wanted, tokens.motion(MOVE_MS));
        let content_width = *inner.read();

        let right = self.row.max_x();
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_move = move |event: Event<PointerEventData>| {
            if !*dragging.peek() {
                return;
            }
            event.prevent_default();
            remembered_width.set_if_modified(pane_state::drag_width(event.global_location().x as f32, right, available));
        };
        let on_up = move |_: Event<PointerEventData>| {
            if *dragging.peek() {
                dragging.set(false);
                remember(STATE_KEY, &*remembered_width.peek());
            }
        };
        let on_key = move |event: Event<KeyboardEventData>| {
            let wider = match &event.key {
                Key::Named(NamedKey::ArrowLeft) => true,
                Key::Named(NamedKey::ArrowRight) => false,
                _ => return,
            };
            event.stop_propagation();
            let next = pane_state::key_width(*remembered_width.peek(), wider, available);
            remembered_width.set_if_modified(next);
            remember(STATE_KEY, &next);
        };

        let nav = self.nav.clone();
        let close_nav = self.nav.clone();
        let content = (shown > 0.5 || self.open).then(|| {
            Element::from(
                rect()
                    .width(Size::px((content_width - GRIP).max(0.0)))
                    .height(Size::fill())
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        // The keyboard is in the pane (the person clicked in): Escape
                        // hides it and gives the list back its keyboard.
                        if matches!(event.key, Key::Named(NamedKey::Escape)) {
                            event.stop_propagation();
                            close_nav.on_close.call(());
                        }
                    })
                    .child(PreviewHost::new(self.source.clone(), Mode::Pane, nav)),
            )
        });

        let grip = rect()
            .width(Size::px(GRIP))
            .height(Size::fill())
            .tw(&tokens, &[items(Align::Center), justify(Align::Center)])
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Splitter)
            .a11y_alt("Preview pane edge")
            .on_key_down(on_key)
            .on_pointer_down(move |event: Event<PointerEventData>| {
                if event.data().is_primary() {
                    event.stop_propagation();
                    dragging.set(true);
                }
            })
            .focusable(focus())
            .child(rect().tw(&tokens, &[h_full(), w(Dim::Hairline), bg(Paint::Separator)]));

        rect()
            .component("PreviewDock")
            .width(Size::px(shown))
            .height(Size::fill())
            .tw(&tokens, &[flex_row(), clip(), bg(Paint::Bg)])
            .on_capture_global_pointer_move(on_move)
            .on_global_pointer_up(on_up)
            .maybe_child((shown > 0.5).then_some(grip))
            .maybe_child(content)
    }
}
