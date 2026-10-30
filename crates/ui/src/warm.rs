//! Warms the text engine's font lookups once the window is up, a style at a
//! time, so no page pays for them.
//!
//! The first paragraph shaped in a style asks the system's font library
//! (fontconfig) for each family of the style's fallback list, and the engine
//! keeps the answer by family list and weight. A search for a family the
//! system has no file for takes several milliseconds, and a style has a few of
//! them: the first Date & Time page, whose figures are the first tabular text,
//! or the first page with a heading in a weight no page before it used, took
//! 40 ms on the frame that mounted it. The frame gate fails on that frame, and
//! so does a person watching a page slide in.
//!
//! [`FontWarm`] sits in the window and, one frame at a time, lays out a word
//! in each style the apps use, with a real [`Label`](freya::prelude::Label) so
//! the lookup is the one the labels make. It draws nothing (a hairline box at
//! zero opacity, taken out of every layout) and is gone after the last style.
//! It runs in a window of an application only (`Application::run` turns it
//! on), not in the headless runner of the tests and snapshots, which make a
//! new text engine for each run and would pay for it every time.

use std::sync::atomic::{AtomicBool, Ordering};

use freya::prelude::*;

use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CODE, HERO, NUMERIC, PAGE_TITLE, SECTION, TextStyle};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Turns the warm-up on for the windows of this process.
pub(crate) fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// One style for each lookup key: a weight (light, regular, medium, semi-bold),
/// tabular figures and the monospace family.
const STYLES: [TextStyle; 6] = [BODY, SECTION, PAGE_TITLE, HERO, NUMERIC, CODE];

/// The warm-up: one more style each frame, until all are done.
#[derive(PartialEq)]
pub(crate) struct FontWarm;

impl Component for FontWarm {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut warmed = use_state(|| 0usize);
        let enabled = ENABLED.load(Ordering::Relaxed);
        use_hook(move || {
            enabled.then(|| {
                let ticker = RenderingTicker::get();
                let window = Platform::get().current_window();
                spawn(async move {
                    while *warmed.peek() < STYLES.len() {
                        window.request_redraw();
                        ticker.tick().await;
                        let next = *warmed.peek() + 1;
                        warmed.set(next);
                    }
                })
            })
        });
        let shown = if enabled { warmed() } else { STYLES.len() };
        rect()
            .component("FontWarm")
            .position(Position::new_absolute().top(0.0).left(0.0))
            .tw(&tokens, &[w(Dim::Hairline), h(Dim::Hairline)])
            .opacity(0.0)
            .interactive(false)
            .maybe(shown < STYLES.len(), |warm| {
                // The style of this frame is the one at `shown`; the ones
                // before it are done and stay out.
                warm.child(STYLES[shown].label(&tokens, "Agj 0").max_lines(1))
            })
    }
}
