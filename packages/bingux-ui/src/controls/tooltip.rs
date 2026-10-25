//! A glyph that shows a short tooltip on hover, for state icons in rows.
//!
//! The tooltip appears after 500 ms and uses the popup surface style. It is
//! placed on the side of the glyph with room in the window, so it never
//! runs off an edge. The same text is the glyph's accessible label.

use std::time::Duration;

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::motion::{PRESENCE_SCALE, use_presence};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// How long the pointer rests on the glyph before the tooltip shows.
const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
/// How long the tooltip takes to fade and scale in or out.
const TOOLTIP_MOTION_MS: u64 = 120;

/// The approximate width of one character of the tooltip label, as a
/// fraction of its font size. Used only to decide the tooltip's side.
const CHAR_WIDTH: f32 = 0.6;

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
        let mut visible = use_state(|| false);
        let mut anchor = use_state(|| (0.0_f32, 0.0_f32));
        let mut pending = use_state::<Option<TaskHandle>>(|| None);

        let glyph = rect()
            .a11y_role(AccessibilityRole::Image)
            .a11y_alt(self.text.clone())
            .tw(&tokens, &[w(Dim::IconBox), h(Dim::IconBox), center()])
            .on_pointer_enter(move |event: Event<PointerEventData>| {
                let at = event.global_location();
                anchor.set((at.x as f32, at.y as f32));
                if let Some(handle) = pending.write().take() {
                    handle.cancel();
                }
                let task = spawn(async move {
                    timer(TOOLTIP_DELAY).await;
                    visible.set_if_modified(true);
                });
                pending.set(Some(task));
            })
            .on_pointer_leave(move |_| {
                if let Some(handle) = pending.write().take() {
                    handle.cancel();
                }
                visible.set_if_modified(false);
            })
            .child(icons::icon(self.icon, self.size, self.paint.color(&tokens)));

        let progress = use_presence(visible(), tokens.motion(TOOLTIP_MOTION_MS), || {});
        if !visible() && progress <= 0.0 {
            return glyph.into_element();
        }

        // The bubble's size, estimated from the TOOLTIP recipe's padding and
        // hairline, to pick a side that fits.
        let label_size = CAPTION.size;
        let edges = 2.0 * Dim::Hairline.px();
        let bubble_width =
            self.text.chars().count() as f32 * label_size * CHAR_WIDTH + 2.0 * S2.px() + edges;
        let bubble_height = label_size + 2.0 * S1.px() + edges;
        let window = *Platform::get().current_window().root_size.read();
        let position = choose_position(
            anchor(),
            self.size.px() / 2.0,
            (bubble_width, bubble_height),
            (window.width, window.height),
        );
        let bubble = rect()
            .layer(Layer::Overlay)
            .opacity(progress)
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
        Attached::new(glyph)
            .position(position)
            .child(bubble)
            .into_element()
    }
}

/// Picks the first side of the glyph where the tooltip fits in the window,
/// trying below, above, right and left. `anchor` is the pointer, `half` is
/// half the glyph size, and `bubble` is the tooltip's size. Below is the
/// fallback when nothing fits.
fn choose_position(
    anchor: (f32, f32),
    half: f32,
    bubble: (f32, f32),
    window: (f32, f32),
) -> AttachedPosition {
    let (x, y) = anchor;
    let (width, height) = bubble;
    let (window_width, window_height) = window;
    let gap = half + S1.px();
    let margin = S2.px();
    let fits = |left: f32, top: f32| {
        left >= margin
            && top >= margin
            && left + width <= window_width - margin
            && top + height <= window_height - margin
    };
    let centred_x = x - width / 2.0;
    let centred_y = y - height / 2.0;
    if fits(centred_x, y + gap) {
        AttachedPosition::Bottom
    } else if fits(centred_x, y - gap - height) {
        AttachedPosition::Top
    } else if fits(x + gap, centred_y) {
        AttachedPosition::Right
    } else if fits(x - gap - width, centred_y) {
        AttachedPosition::Left
    } else {
        AttachedPosition::Bottom
    }
}
