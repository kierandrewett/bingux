//! A paged carousel ([`carousel`]) and its page dots ([`page_dots`]).
//!
//! The carousel is content: one page fills the width, a horizontal drag or a
//! fling moves to the next or previous page, Left and Right do the same with
//! the keyboard, and a page change slides (a cut with reduced motion). The
//! dots choose which page shows, so they are chrome: they have no
//! `Into<Element>` and go in [`Screen::pager`](crate::shell::Screen::pager),
//! over the content's bottom edge (`design/boundaries.md`).

use freya::prelude::*;

use crate::component::ComponentName;
use crate::controls::press::is_touch;
use crate::motion::use_tween;
use crate::style::*;
use crate::surface::use_tokens;

/// How far, as a share of the width, a drag must travel to turn the page.
const TURN_SHARE: f32 = 0.25;
/// A flick faster than this, in px/ms, turns the page however short.
const FLING: f32 = 0.5;
/// How long a page turn slides, in milliseconds.
const TURN_MS: u64 = 320;

/// A row of pages, one on show. Build with [`carousel`].
#[derive(PartialEq)]
pub struct Carousel {
    pages: Vec<Element>,
    index: usize,
    on_change: EventHandler<usize>,
    label: String,
}

/// A carousel of `pages` showing page `index`. `on_change` gets the index a
/// swipe, a fling, an arrow key or the dots ask for; the caller stores it and
/// passes it back.
pub fn carousel(
    pages: Vec<Element>,
    index: usize,
    on_change: impl Into<EventHandler<usize>>,
) -> Carousel {
    Carousel {
        pages,
        index,
        on_change: on_change.into(),
        label: String::new(),
    }
}

impl Carousel {
    /// What the pages are, for a screen reader: "Places".
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

/// The page a drag released at `offset` px (positive: towards the previous
/// page) after `elapsed` ms on a carousel `width` wide asks for, from `index`
/// of `count`.
pub(crate) fn settle(index: usize, count: usize, offset: f32, elapsed: f32, width: f32) -> usize {
    if count == 0 || width <= 0.0 {
        return index;
    }
    let speed = offset.abs() / elapsed.max(1.0);
    let turn = offset.abs() >= width * TURN_SHARE || speed >= FLING;
    match (turn, offset > 0.0) {
        (true, true) => index.saturating_sub(1),
        (true, false) => (index + 1).min(count - 1),
        (false, _) => index,
    }
}

impl Component for Carousel {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let count = self.pages.len();
        let index = self.index.min(count.saturating_sub(1));
        let mut width = use_state(|| 0.0_f32);
        // The drag: where it started, and how far it has moved, in px.
        let mut start = use_state(|| None::<(f32, std::time::Instant)>);
        let mut drag = use_state(|| 0.0_f32);
        let shown = use_tween(index as f32, tokens.motion(TURN_MS));
        let id = use_a11y();
        let on_change = self.on_change.clone();
        let key_change = self.on_change.clone();
        let w = width();
        let offset = drag();

        let mut row = rect()
            .component("Carousel")
            .a11y_id(id)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::Group)
            .a11y_alt(self.label.clone())
            .tw(&tokens, &[w_full(), h_full(), clip()])
            .on_sized(move |event: Event<SizedEventData>| {
                width.set_if_modified(event.area.width());
            })
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let next = match event.key {
                    Key::Named(NamedKey::ArrowLeft) => index.checked_sub(1),
                    Key::Named(NamedKey::ArrowRight) => (index + 1 < count).then_some(index + 1),
                    Key::Named(NamedKey::Home) => Some(0),
                    Key::Named(NamedKey::End) => count.checked_sub(1),
                    _ => None,
                };
                if let Some(next) = next {
                    event.stop_propagation();
                    if next != index {
                        key_change.call(next);
                    }
                }
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                // A mouse turns pages with the dots and the keys; a finger swipes.
                if is_touch(&event) {
                    start.set(Some((
                        event.global_location().x as f32,
                        std::time::Instant::now(),
                    )));
                }
            })
            .on_global_pointer_move(move |event: Event<PointerEventData>| {
                if let Some((from, _)) = *start.peek() {
                    drag.set(event.global_location().x as f32 - from);
                }
            })
            .on_global_pointer_up(move |_| {
                if let Some((_, at)) = start.take() {
                    let moved = *drag.peek();
                    drag.set(0.0);
                    let elapsed = at.elapsed().as_millis() as f32;
                    let next = settle(index, count, moved, elapsed, *width.peek());
                    if next != index {
                        on_change.call(next);
                    }
                }
            });
        // Only the pages in sight are drawn: the one on show and, while it
        // moves, its neighbours.
        for (at, page) in self.pages.iter().enumerate() {
            let left = (at as f32 - shown) * w + offset;
            if w > 0.0 && left.abs() >= w {
                continue;
            }
            row = row.child(
                rect()
                    .position(Position::new_absolute().top(0.0).left(left))
                    .width(Size::px(w.max(1.0)))
                    .height(Size::fill())
                    .a11y_role(AccessibilityRole::Group)
                    .a11y_alt(crate::tr!(
                        "ui-carousel-page-a11y",
                        page = at + 1,
                        count = count
                    ))
                    .child(page.clone()),
            );
        }
        row
    }
}

/// The dots of a paged view: one per page, the current one filled. They choose
/// which page shows, so they are chrome: give them to
/// [`Screen::pager`](crate::shell::Screen::pager). Build with [`page_dots`].
#[derive(Clone, PartialEq)]
pub struct PageDots {
    pub(crate) labels: Vec<String>,
    pub(crate) index: usize,
    pub(crate) on_change: EventHandler<usize>,
}

/// Dots for pages named `labels` (each dot's spoken name, "Sheffield"), with
/// page `index` on show. `on_change` gets the page a dot asks for.
pub fn page_dots(
    labels: Vec<String>,
    index: usize,
    on_change: impl Into<EventHandler<usize>>,
) -> PageDots {
    PageDots {
        labels,
        index,
        on_change: on_change.into(),
    }
}

/// The drawn dots.
#[derive(PartialEq)]
pub(crate) struct PageDotsView {
    pub dots: PageDots,
}

impl Component for PageDotsView {
    fn render(&self) -> impl IntoElement {
        crate::shell::boundary::check_chrome("PageDots");
        let tokens = use_tokens();
        let count = self.dots.labels.len();
        let mut row = rect()
            .component("PageDots")
            .a11y_role(AccessibilityRole::TabList)
            .tw(
                &tokens,
                &[flex_row(), items(Align::Center), gap(S2), px(S3), py(S2)],
            );
        for (at, label) in self.dots.labels.iter().enumerate() {
            let on = at == self.dots.index;
            let on_change = self.dots.on_change.clone();
            row = row.child(
                rect()
                    .a11y_role(AccessibilityRole::Tab)
                    .a11y_focusable(true)
                    .a11y_alt(crate::tr!(
                        "ui-page-dot-a11y",
                        name = label.clone(),
                        page = at + 1,
                        count = count
                    ))
                    .a11y_builder(move |builder| builder.set_selected(on))
                    .tw(
                        &tokens,
                        &[
                            w(Dim::Dot),
                            h(Dim::Dot),
                            rounded(Radius::Pill),
                            bg(if on { Paint::Text } else { Paint::Muted }),
                        ],
                    )
                    .opacity(if on { 1.0 } else { 0.5 })
                    .on_press(move |_| on_change.call(at)),
            );
        }
        row
    }
}

#[cfg(test)]
mod tests {
    use super::settle;

    #[test]
    fn a_short_slow_drag_springs_back() {
        assert_eq!(settle(2, 5, 40.0, 400.0, 400.0), 2);
        assert_eq!(settle(2, 5, -40.0, 400.0, 400.0), 2);
    }

    #[test]
    fn a_long_drag_turns_one_page_towards_the_finger() {
        assert_eq!(settle(2, 5, 150.0, 600.0, 400.0), 1);
        assert_eq!(settle(2, 5, -150.0, 600.0, 400.0), 3);
    }

    #[test]
    fn a_flick_turns_the_page_however_short() {
        assert_eq!(settle(2, 5, -40.0, 50.0, 400.0), 3);
    }

    #[test]
    fn the_ends_hold() {
        assert_eq!(settle(0, 5, 300.0, 100.0, 400.0), 0);
        assert_eq!(settle(4, 5, -300.0, 100.0, 400.0), 4);
        assert_eq!(settle(0, 0, -300.0, 100.0, 400.0), 0);
    }
}
