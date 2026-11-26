//! The list of downloads, and how a size and a speed read.

use bingux_ui::format::{Bytes, Measure, TimeUnit};
use bingux_ui::prelude::tr;
use bingux_ui::web_view::requests::{DownloadInfo, DownloadState};

/// One download of the engine, read for the list: how far it is, and what its line says.
pub struct Transfer<'a>(pub &'a DownloadInfo);

impl Transfer<'_> {
    /// `bytes` as people read it, in the language shown: `1.2 MB`.
    fn size(bytes: u64) -> String {
        Bytes::new(bytes).to_string()
    }

    /// The time left for `remaining` bytes at `speed`, in the language shown.
    fn time_left(remaining: u64, speed: u64) -> Option<String> {
        if speed == 0 {
            return None;
        }
        let seconds = remaining.div_ceil(speed);
        let (value, unit) = match seconds {
            0..=59 => (seconds, TimeUnit::Second),
            60..=3599 => (seconds.div_ceil(60), TimeUnit::Minute),
            _ => (seconds.div_ceil(3600), TimeUnit::Hour),
        };
        Some(Measure::new(value, unit).format())
    }

    /// How far a download is, 0 to 1, or `None` when the total is not known.
    pub fn fraction(&self) -> Option<f64> {
        let download = self.0;
        let total = download.total.filter(|total| *total > 0)?;
        Some((download.received as f64 / total as f64).clamp(0.0, 1.0))
    }

    /// The line under a download's name.
    pub fn status_line(&self) -> String {
        let download = self.0;
        // How far it is: `1.2 MB of 4.0 MB`, or only what came when the total is not known.
        let done = || match download.total {
            Some(total) => tr!(
                "browser-downloads-progress",
                done = Self::size(download.received),
                total = Self::size(total)
            ),
            None => Self::size(download.received),
        };
        match download.state {
            DownloadState::Running => {
                let rate = (download.speed > 0)
                    .then(|| tr!("browser-downloads-speed", size = Self::size(download.speed)));
                let left = download
                    .total
                    .and_then(|total| {
                        Self::time_left(total.saturating_sub(download.received), download.speed)
                    })
                    .map(|left| tr!("browser-downloads-time-left", time = left));
                // Each part is one unbreakable word run: a line may wrap between
                // them, never inside "1.1 MB/s".
                let parts: Vec<String> = [Some(done()), rate, left]
                    .into_iter()
                    .flatten()
                    .map(|part| part.replace(' ', "\u{a0}"))
                    .collect();
                tr!(
                    "browser-downloads-line",
                    count = parts.len(),
                    first = parts[0].as_str(),
                    second = parts.get(1).map_or("", String::as_str),
                    third = parts.get(2).map_or("", String::as_str)
                )
            }
            DownloadState::Paused => tr!("browser-downloads-paused", done = done()),
            DownloadState::Finished => Self::size(download.received),
            DownloadState::Failed => tr!("browser-downloads-failed"),
            DownloadState::Cancelled => tr!("browser-downloads-cancelled"),
        }
    }
}

/// The downloads of this window, newest first.
#[derive(Clone, Debug, Default)]
pub struct Downloads {
    list: Vec<DownloadInfo>,
    /// Downloads that started or finished since the list was last looked at.
    unseen: u32,
}

impl Downloads {
    pub fn new() -> Self {
        Self::default()
    }

    /// A download started or changed.
    pub fn update(&mut self, info: DownloadInfo) {
        match self.list.iter_mut().find(|existing| existing.id == info.id) {
            Some(existing) => {
                let finished = existing.state != info.state
                    && matches!(info.state, DownloadState::Finished | DownloadState::Failed);
                *existing = info;
                if finished {
                    self.unseen += 1;
                }
            }
            None => {
                self.list.insert(0, info);
                self.unseen += 1;
            }
        }
    }

    pub fn list(&self) -> &[DownloadInfo] {
        &self.list
    }

    /// How many are running or paused.
    pub fn active(&self) -> usize {
        self.list
            .iter()
            .filter(|download| {
                matches!(
                    download.state,
                    DownloadState::Running | DownloadState::Paused
                )
            })
            .count()
    }

    /// The share done of all that are running, for the toolbar button.
    pub fn overall(&self) -> Option<f64> {
        let mut received = 0.0;
        let mut total = 0.0;
        for download in self
            .list
            .iter()
            .filter(|download| download.state == DownloadState::Running)
        {
            total += download.total? as f64;
            received += download.received as f64;
        }
        (total > 0.0).then(|| received / total)
    }

    pub fn unseen(&self) -> u32 {
        self.unseen
    }

    /// The list was opened.
    pub fn mark_seen(&mut self) {
        self.unseen = 0;
    }

    /// Take finished, failed and cancelled downloads off the list.
    pub fn clear_finished(&mut self) {
        self.list.retain(|download| {
            matches!(
                download.state,
                DownloadState::Running | DownloadState::Paused
            )
        });
    }
}

mod button;
mod popover;
mod state;

pub use button::DownloadsButton;
pub use popover::DownloadsPopover;
pub use state::CardState;

#[cfg(test)]
mod tests;
