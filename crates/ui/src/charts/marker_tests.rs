//! The marker of a line chart: a rule, a name and a dot where it crosses a line.
//!
//! The chart is drawn in a headless window; the marker's name is a label over
//! the drawing, so it is found by its text. (The rule and the dots are in the
//! drawing, which the screenshot review covers.)

use std::time::Duration;

use freya::prelude::*;

use super::{LineChart, Series, line_chart};
use crate::snapshot::Snapshot;
use crate::style::*;
use crate::theme::provide_tokens;
use crate::window::WindowHost;

fn chart() -> LineChart {
    line_chart([Series::new(
        "Temperature",
        vec![(-1.0, 8.0), (0.0, 9.0), (1.0, 10.0)],
    )])
}

#[derive(Clone, PartialEq)]
struct Page(LineChart);

impl Component for Page {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        WindowHost::new(
            rect()
                .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg), p(S4)])
                .child(self.0.clone()),
        )
    }
}

/// How many labels on screen read exactly `text`.
fn named(chart: LineChart, text: &str) -> usize {
    let mut shot = Snapshot::new(move || Page(chart.clone()).into_element(), (640.0, 360.0));
    shot.settle(Duration::from_millis(900));
    shot.runner()
        .find_many(|_, element| {
            let shown = Label::try_downcast(element)
                .map(|label| label.text.to_string())
                .or_else(|| {
                    Paragraph::try_downcast(element).map(|paragraph| {
                        paragraph
                            .spans
                            .iter()
                            .map(|span| span.text.to_string())
                            .collect()
                    })
                })?;
            (shown == text).then_some(())
        })
        .len()
}

#[test]
fn a_marker_in_the_data_is_named_once() {
    assert_eq!(named(chart(), "Now"), 0, "no marker, no name");
    assert_eq!(
        named(chart().marker(0.0, "Now"), "Now"),
        1,
        "the marker is named"
    );
}

#[test]
fn a_marker_outside_the_data_is_not_drawn() {
    assert_eq!(named(chart().marker(40.0, "Now"), "Now"), 0);
}
