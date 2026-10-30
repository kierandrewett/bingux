use std::time::{
    Duration,
    Instant,
};

use freya_core::prelude::*;

/// Bingux patch: how long the content must have stood still before a window
/// stops counting as scrolling.
pub const SCROLL_STILL: Duration = Duration::from_millis(200);

/// Bingux patch: a window-level signal that some scroll view is moving.
///
/// Every scroll view tells the window it provides (see [`provide_scroll_activity`])
/// when its content moved: the wheel, a drag of the content or of the
/// scrollbar, the keys, a glide that is still running. The window is
/// *scrolling* from then until the content has been still for
/// [`SCROLL_STILL`]. Hover tooltips and detail cards read it: while it is set,
/// the rows move under a resting pointer, and a tip that opened for one of them
/// would be out of date a frame later.
///
/// Scroll views with no provider above them do nothing.
#[derive(Clone, Copy, PartialEq)]
pub struct ScrollActivity {
    scrolling: State<bool>,
    last: State<Instant>,
    /// A task is sleeping towards the end of the still period.
    waiting: State<bool>,
    /// The scope that owns the sleeping task: the window's, so it lives as
    /// long as the signal, wherever `pulse` is called from.
    scope: ScopeId,
}

impl ScrollActivity {
    pub fn new() -> Self {
        Self {
            scrolling: State::create(false),
            last: State::create(Instant::now()),
            waiting: State::create(false),
            scope: current_scope_id(),
        }
    }

    /// Whether the window is scrolling. Reading it subscribes the caller to the
    /// change.
    pub fn scrolling(&self) -> bool {
        *self.scrolling.read()
    }

    /// Whether the window is scrolling, without subscribing.
    pub fn is_scrolling(&self) -> bool {
        self.scrolling.try_peek().is_some_and(|scrolling| *scrolling)
    }

    /// Some content moved just now. Cheap: it writes the clock, and the flag
    /// only when it was clear.
    pub fn pulse(mut self) {
        if let Some(mut last) = self.last.try_write() {
            *last = Instant::now();
        }
        if !self.is_scrolling()
            && let Some(mut scrolling) = self.scrolling.try_write()
        {
            *scrolling = true;
        }
        if !self.waiting.try_peek().is_some_and(|waiting| *waiting) {
            if let Some(mut waiting) = self.waiting.try_write() {
                *waiting = true;
            }
            self.wait();
        }
    }

    fn wait(&self) {
        let (mut scrolling, last, mut waiting) = (self.scrolling, self.last, self.waiting);
        spawn_in_scope(
            async move {
                loop {
                    let left = SCROLL_STILL.saturating_sub(last.peek().elapsed());
                    if left.is_zero() {
                        scrolling.set_if_modified(false);
                        waiting.set(false);
                        return;
                    }
                    timer(left).await;
                }
            },
            self.scope,
        );
    }
}

impl Default for ScrollActivity {
    fn default() -> Self {
        Self::new()
    }
}

/// Bingux patch: gives the window the signal its scroll views report to. The
/// window host calls it once.
pub fn provide_scroll_activity() {
    use_provide_context(ScrollActivity::new);
}

/// Bingux patch: the window's [`ScrollActivity`], if its host provides one.
pub fn scroll_activity() -> Option<ScrollActivity> {
    try_consume_context::<ScrollActivity>()
}
