//! The list of downloads, and how a size and a speed read.

use bingux_ui::web_view::requests::{DownloadInfo, DownloadState};

/// `bytes` as people read it: `1.2 MB`. Powers of 1000, as a file manager
/// shows them.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// The time left for `remaining` bytes at `speed`: `12 s`, `3 min`, `2 h`.
pub fn time_left(remaining: u64, speed: u64) -> Option<String> {
    if speed == 0 {
        return None;
    }
    let seconds = remaining.div_ceil(speed);
    Some(match seconds {
        0..=59 => format!("{seconds} s"),
        60..=3599 => format!("{} min", seconds.div_ceil(60)),
        _ => format!("{} h", seconds.div_ceil(3600)),
    })
}

/// How far a download is, 0 to 1, or `None` when the total is not known.
pub fn fraction(download: &DownloadInfo) -> Option<f64> {
    let total = download.total.filter(|total| *total > 0)?;
    Some((download.received as f64 / total as f64).clamp(0.0, 1.0))
}

/// The line under a download's name.
pub fn status_line(download: &DownloadInfo) -> String {
    match download.state {
        DownloadState::Running => {
            let done = match download.total {
                Some(total) => format!("{} of {}", size(download.received), size(total)),
                None => size(download.received),
            };
            let rate = (download.speed > 0).then(|| format!("{}/s", size(download.speed)));
            let left = download
                .total
                .and_then(|total| {
                    time_left(total.saturating_sub(download.received), download.speed)
                })
                .map(|left| format!("{left} left"));
            // Each part is one unbreakable word run: a line may wrap between
            // them, never inside "1.1 MB/s".
            [Some(done), rate, left]
                .into_iter()
                .flatten()
                .map(|part| part.replace(' ', "\u{a0}"))
                .collect::<Vec<_>>()
                .join(" \u{b7} ")
        }
        DownloadState::Paused => match download.total {
            Some(total) => format!(
                "Paused \u{b7} {} of {}",
                size(download.received),
                size(total)
            ),
            None => format!("Paused \u{b7} {}", size(download.received)),
        },
        DownloadState::Finished => size(download.received),
        DownloadState::Failed => "Failed".to_string(),
        DownloadState::Cancelled => "Cancelled".to_string(),
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

pub mod button;
pub mod popover;

#[cfg(test)]
mod tests;
