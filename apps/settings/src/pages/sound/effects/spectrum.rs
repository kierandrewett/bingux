//! The spectrum strip across the top of the page: the engine's analyser
//! reading, 64 bands on a log frequency axis from 20 Hz to 20 kHz, drawn with
//! the Bingux chart. It shows nothing while the analyser has published
//! nothing, and costs nothing unless the page is on screen in a focused
//! window (see `meter::use_spectrum`).

use bingux_audio_effects::Direction;
use bingux_ui::charts::{Curve, Series, line_chart};
use bingux_ui::style::*;
use freya::prelude::*;

use super::super::meter::use_spectrum;
use crate::ui::use_tokens;

/// The quietest level drawn, in dBFS.
const FLOOR: f64 = -90.0;
const LOW: f64 = 20.0;
const HIGH: f64 = 20_000.0;

fn frequency_label(log_frequency: f64) -> String {
    let frequency = 10f64.powf(log_frequency);
    if frequency >= 1000.0 {
        format!("{} kHz", (frequency / 100.0).round() / 10.0)
    } else {
        format!("{} Hz", frequency.round())
    }
}

#[derive(PartialEq)]
pub struct SpectrumStrip {
    pub direction: Direction,
}

impl Component for SpectrumStrip {
    /// Another direction is another analyser, so it starts afresh.
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.direction as u64)
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let spectrum = use_spectrum(self.direction);
        let bands = spectrum.read().clone();
        // Nothing is drawn, and no height reserved, until there is a reading:
        // a fixed-height card with a short caption keeps the page still.
        let body: Element = match bands {
            Some(bands) => {
                let count = bands.len();
                let points = bands.iter().enumerate().map(|(index, level)| {
                    let frequency = LOW * (HIGH / LOW).powf((index as f64 + 0.5) / count as f64);
                    (frequency.log10(), f64::from(*level).max(FLOOR) - FLOOR)
                });
                line_chart([Series::new("Level", points)])
                    .curve(Curve::Monotone)
                    .area()
                    .x_format(frequency_label)
                    .y_format(|value| format!("{:.0} dB", value + FLOOR))
                    .height(Dim::Chart)
                    .label("Spectrum")
                    .into()
            }
            None => rect()
                .tw(&tokens, &[w_full(), h(Dim::Chart), center()])
                .child(crate::ui::caption("No spectrum while the effects are off or nothing plays."))
                .into(),
        };
        crate::ui::Group::new(None::<String>)
            .row(rect().tw(&tokens, &[w_full(), px(GUTTER), py(S2)]).child(body))
    }
}
