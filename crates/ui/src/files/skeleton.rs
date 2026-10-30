//! The placeholder of a folder that is slow to open: grey rows or tiles in
//! the shape of the list or the grid.
//!
//! It stands where the rows will be, in a file view that already has its
//! column header, the room of its scrollbar and its status line, so it is
//! drawn in the rows' own metrics: a row is as high as a real row at the icon
//! size, a line of tiles is as high as a real line. The real rows replace it
//! in the same place, row for row, and nothing around them moves.
//!
//! It draws nothing for the first [`DELAY`]. A folder that opens in the
//! blink of an eye (the cache, a local drive) never shows it, so there is no
//! flash of grey before the real rows. One that takes longer gets a quiet
//! pulse instead of a spinner.

use std::time::Duration;

use freya::prelude::*;

use super::file_grid::Metrics;
use super::file_list::row_height;
use super::zoom::{ViewKind, kind_of};
use crate::component::ComponentName;
use crate::motion::use_tween;
use crate::style::*;
use crate::surface::use_tokens;

/// How long a folder may take before the placeholder shows.
const DELAY: Duration = Duration::from_millis(120);
/// How long one half of the pulse lasts.
const PULSE_MS: u64 = 700;
/// How many rows or lines of tiles are drawn when the size of the view is not
/// known. The box clips the rest.
const LINES: usize = 14;

/// A placeholder for the rows of a view with icons of `icon_px` (a list or a
/// grid of thumbnails, as the size decides) in a viewport of `viewport`, the
/// size the view had the last time it was on show.
pub fn skeleton(icon_px: u16, viewport: Area) -> Element {
    Element::from(Skeleton { icon_px, viewport })
}

#[derive(PartialEq)]
struct Skeleton {
    icon_px: u16,
    viewport: Area,
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
        let height = self.viewport.height();
        let lines = |pitch: f32| {
            if height > 0.0 {
                (height / pitch).ceil() as usize + 1
            } else {
                LINES
            }
        };
        let mut column = rect()
            .component("FileSkeleton")
            .opacity(level)
            .interactive(false)
            .tw(&tokens, &[flex_col(), w_full(), h_full(), clip()]);
        if kind_of(self.icon_px) == ViewKind::Grid {
            let metrics = Metrics::new(self.viewport.width(), self.icon_px, Dim::Box7.px(), S1.px());
            for _ in 0..lines(metrics.line) {
                let mut line = rect()
                    .tw(&tokens, &[flex_row(), w_full()])
                    .child(rect().width(Size::px(metrics.gap)));
                for _ in 0..metrics.columns {
                    line = line
                        .child(
                            rect()
                                .tw(&tokens, &[flex_col(), items(Align::Center), gap(S1)])
                                .width(Size::px(metrics.tile))
                                .height(Size::px(metrics.tile_height))
                                .child(
                                    rect()
                                        .tw(&tokens, &[rounded(Radius::Control), bg(Paint::Control)])
                                        .width(Size::px(metrics.icon))
                                        .height(Size::px(metrics.icon)),
                                )
                                .child(bar(60.0)),
                        )
                        .child(rect().width(Size::px(metrics.gap)));
                }
                column = column.child(
                    rect()
                        .tw(&tokens, &[flex_col(), w_full()])
                        .height(Size::px(metrics.line))
                        .child(rect().height(Size::px(metrics.gap)))
                        .child(line),
                );
            }
        } else {
            let row = row_height(self.icon_px);
            for index in 0..lines(row) {
                let name = [34.0, 22.0, 28.0][index % 3];
                column = column.child(
                    rect()
                        .tw(&tokens, &[flex_row(), items(Align::Center), w_full(), px(S3), gap(S3)])
                        .height(Size::px(row))
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
