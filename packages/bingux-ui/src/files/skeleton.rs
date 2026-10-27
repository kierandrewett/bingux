//! The placeholder of a folder that is slow to open: grey rows or tiles in
//! the shape of the list or the grid.
//!
//! It draws nothing for the first [`DELAY`]. A folder that opens in the
//! blink of an eye (the cache, a local drive) never shows it, so there is no
//! flash of grey before the real rows. One that takes longer gets a quiet
//! pulse instead of a spinner, and the real list then replaces it in the
//! same place, row for row.

use std::time::Duration;

use freya::prelude::*;

use crate::component::ComponentName;
use crate::motion::use_tween;
use crate::style::*;
use crate::surface::use_tokens;

/// How long a folder may take before the placeholder shows.
const DELAY: Duration = Duration::from_millis(120);
/// How long one half of the pulse lasts.
const PULSE_MS: u64 = 700;
/// How many rows or lines of tiles are drawn. The box clips the rest.
const LINES: usize = 14;

/// A placeholder for a list (`grid` false) or a grid of thumbnails.
pub fn skeleton(grid: bool) -> Element {
    Element::from(Skeleton { grid })
}

#[derive(PartialEq)]
struct Skeleton {
    grid: bool,
}

impl Component for Skeleton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut shown = use_state(|| false);
        let mut high = use_state(|| true);
        use_hook(move || {
            // Tied to this component: unmounting cancels both timers.
            spawn(async move {
                timer(DELAY).await;
                shown.set(true);
                loop {
                    timer(Duration::from_millis(PULSE_MS)).await;
                    let next = !*high.peek();
                    high.set(next);
                }
            })
        });
        let level = use_tween(if high() { 1.0 } else { 0.55 }, PULSE_MS);
        if !shown() {
            return rect().tw(&tokens, &[w_full(), h_full()]).into_element();
        }
        let bar = |percent: f32| {
            rect()
                .width(Size::percent(percent))
                .tw(
                    &tokens,
                    &[h(Dim::SkeletonLine), rounded(Radius::Mark), bg(Paint::Control)],
                )
        };
        let mut column = rect()
            .component("FileSkeleton")
            .opacity(level)
            .interactive(false)
            .tw(&tokens, &[flex_col(), w_full(), h_full(), clip(), p(S3), gap(S3)]);
        if self.grid {
            for _ in 0..LINES {
                let mut line = rect().tw(&tokens, &[flex_row(), w_full(), gap(S3)]);
                for _ in 0..4 {
                    line = line.child(
                        rect()
                            .tw(&tokens, &[flex_col(), w_flex(), gap(S1)])
                            .child(rect().tw(
                                &tokens,
                                &[w_full(), h(Dim::Box24), rounded(Radius::Control), bg(Paint::Control)],
                            ))
                            .child(bar(60.0)),
                    );
                }
                column = column.child(line);
            }
        } else {
            for index in 0..LINES * 2 {
                let name = [34.0, 22.0, 28.0][index % 3];
                column = column.child(
                    rect()
                        .tw(
                            &tokens,
                            &[flex_row(), items(Align::Center), w_full(), h(Dim::NavItem), px(S3), gap(S3)],
                        )
                        .child(rect().tw(
                            &tokens,
                            &[w(Dim::IconRow), h(Dim::IconRow), rounded(Radius::Mark), bg(Paint::Control)],
                        ))
                        .child(bar(name)),
                );
            }
        }
        column.into_element()
    }
}
