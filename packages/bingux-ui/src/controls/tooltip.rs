//! Hover tooltips for controls: a short label on the popup surface, shown
//! after a rest of 500 ms and kept inside the window with an 8 px margin.
//!
//! `hover_tip` wraps any element. The bubble sits below, above, right or left
//! of the element, whichever first fits. If none fits, it is shifted along the
//! window edge until it does, so it is never cut off by the window.

use std::time::Duration;

use freya::prelude::*;

use super::press::{is_touch, use_press};
use crate::icons::{self, Icon};
use crate::motion::{PRESENCE_SCALE, use_presence};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};
use crate::shell::panel::{EdgeAlign, PANEL_GAP, PanelRect, Side, place_aligned};

/// How long the pointer rests on the element before the tooltip shows.
const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
/// How long the tooltip takes to fade and scale in or out.
const TOOLTIP_MOTION_MS: u64 = 120;

/// A state glyph with a hover tooltip. `text` is both the tooltip and the
/// glyph's accessible label, so the state is announced without a tooltip.
pub fn tooltip_icon(icon: Icon, size: IconSize, paint: Paint, text: impl Into<String>) -> Element {
    Element::from(TooltipIcon {
        icon,
        size,
        paint,
        text: text.into(),
    })
}

#[derive(Clone, PartialEq)]
struct TooltipIcon {
    icon: Icon,
    size: IconSize,
    paint: Paint,
    text: String,
}

impl Component for TooltipIcon {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let press = use_press();
        // Tooltips are hover help. A touch never shows one, so the press only
        // tracks the pointer for the release.
        let glyph = rect()
            .a11y_role(AccessibilityRole::Image)
            .a11y_alt(self.text.clone())
            .tw(&tokens, &[w(Dim::IconBox), h(Dim::IconBox), center()])
            .on_pointer_down(move |event: Event<PointerEventData>| press.begin(&event, None))
            .on_global_pointer_move(move |event: Event<PointerEventData>| press.moved(&event))
            .on_global_pointer_up(move |_| press.release())
            .child(icons::icon(self.icon, self.size, self.paint.color(&tokens)));
        hover_tip(glyph, self.text.clone())
    }
}

/// Wraps `child` so that resting the pointer on it shows `text` in a bubble.
pub fn hover_tip(child: impl IntoElement, text: impl Into<String>) -> Element {
    Element::from(HoverTip {
        child: child.into_element(),
        text: text.into(),
    })
}

#[derive(Clone, PartialEq)]
struct HoverTip {
    child: Element,
    text: String,
}

impl Component for HoverTip {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut visible = use_state(|| false);
        let mut anchor = use_state::<Option<Area>>(|| None);
        let mut bubble = use_state::<Option<(f32, f32)>>(|| None);
        let mut pending = use_state::<Option<TaskHandle>>(|| None);

        let wrapper = rect()
            .on_sized(move |e: Event<SizedEventData>| {
                anchor.set_if_modified(Some(e.area));
            })
            .on_pointer_over(move |event: Event<PointerEventData>| {
                // A touch has no hover, so it never shows the tooltip.
                if is_touch(&event) {
                    return;
                }
                if let Some(handle) = pending.write().take() {
                    handle.cancel();
                }
                let task = spawn(async move {
                    timer(TOOLTIP_DELAY).await;
                    visible.set_if_modified(true);
                });
                pending.set(Some(task));
            })
            .on_pointer_out(move |event: Event<PointerEventData>| {
                if is_touch(&event) {
                    return;
                }
                if let Some(handle) = pending.write().take() {
                    handle.cancel();
                }
                visible.set_if_modified(false);
            })
            .child(self.child.clone());

        let progress = use_presence(visible(), tokens.motion(TOOLTIP_MOTION_MS), || {});
        if !visible() && progress <= 0.0 {
            return rect().child(wrapper).into_element();
        }

        // The bubble stays invisible until both it and its anchor are
        // measured, so it never flashes in the wrong place.
        let (offset_x, offset_y, measured) = match (anchor(), bubble()) {
            (Some(area), Some(size)) => {
                let window = *Platform::get().current_window().root_size.read();
                let (left, top) = place_aligned(
                    PanelRect::from_area(&area),
                    size,
                    (window.width, window.height),
                    Side::Below,
                    PANEL_GAP,
                    EdgeAlign::Centre,
                );
                (left - area.min_x(), top - area.min_y(), true)
            }
            _ => (0.0, 0.0, false),
        };

        let label = rect()
            .on_sized(move |e: Event<SizedEventData>| {
                bubble.set_if_modified(Some((e.area.width(), e.area.height())));
            })
            .layer(Layer::Overlay)
            .position(Position::new_absolute().left(offset_x).top(offset_y))
            .opacity(if measured { progress } else { 0.0 })
            .scale(PRESENCE_SCALE + (1.0 - PRESENCE_SCALE) * progress)
            .tw(&tokens, TOOLTIP)
            .tw(
                &tokens,
                &[border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline))],
            )
            .child(
                CAPTION
                    .in_ink(Ink::Text)
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            );

        rect().child(wrapper).child(label).into_element()
    }
}
