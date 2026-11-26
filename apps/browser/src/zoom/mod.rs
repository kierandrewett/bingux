//! Page zoom. The steps are Firefox's, which are the ones the engine's own
//! Ctrl+wheel uses (`docs/input.md`), so the keyboard and the wheel agree.

use bingux_ui::web_view::TabId;

mod chip;

pub use chip::ZoomChip;

use crate::browser::Outbox;
use crate::engine::Engine;

/// The zoom of one tab: 1.0 is 100 %.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zoom(f64);

impl Default for Zoom {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Zoom {
    /// The zoom levels, in percent.
    pub const LEVELS: [u32; 16] = [
        30, 50, 67, 80, 90, 100, 110, 120, 133, 150, 170, 200, 240, 300, 400, 500,
    ];

    /// 100 %.
    pub const DEFAULT: Self = Self(1.0);

    /// What the engine says the page's zoom is (its own Ctrl+wheel moves it).
    pub fn reported(level: f64) -> Self {
        Self(level)
    }

    /// `level`, kept to what the engine allows.
    pub fn clamped(level: f64) -> Self {
        Self(level.clamp(0.3, 5.0))
    }

    /// The zoom as a factor: 1.0 is 100 %.
    pub fn level(self) -> f64 {
        self.0
    }

    /// The zoom as a whole percentage.
    pub fn percent(self) -> u32 {
        (self.0 * 100.0).round().max(0.0) as u32
    }

    /// The next level above this one, or this one at the top.
    pub fn stepped_in(self) -> Self {
        let now = self.percent();
        Self::LEVELS
            .iter()
            .find(|level| **level > now)
            .map_or(self, |level| Self(f64::from(*level) / 100.0))
    }

    /// The next level below this one, or this one at the bottom.
    pub fn stepped_out(self) -> Self {
        let now = self.percent();
        Self::LEVELS
            .iter()
            .rev()
            .find(|level| **level < now)
            .map_or(self, |level| Self(f64::from(*level) / 100.0))
    }

    /// Whether this is the default, to a percent.
    pub fn is_default(self) -> bool {
        self.percent() == 100
    }

    /// Make the tab with engine session `session` this zoom, and ask the
    /// engine to show it.
    pub fn set(&mut self, session: Option<TabId>, to: Self, engine: &Engine, out: &mut Outbox) {
        let Some(session) = session else {
            return;
        };
        let to = Self::clamped(to.level());
        engine.call(out, |backend| backend.set_zoom(session, to.0));
        *self = to;
    }
}

#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;
