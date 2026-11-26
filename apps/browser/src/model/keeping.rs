//! Whether what the person does is kept: one state for the settings and one
//! for the history, which the settings page and a one-time toast show. Every
//! failure of the store is one of these, never a panic and never a silent
//! loss (`docs/browser/profiles-and-store.md`, "Failure modes").

use std::path::PathBuf;

use bingux_ui::tr;

/// Whether a part of the profile is being kept on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keeping {
    /// The last write reached the disk.
    Saved,
    /// Not read yet; the page keeps its layout meanwhile.
    Loading,
    /// Kept for this window only, and why.
    NotSaved(NotSaved),
}

/// Why something is not being kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotSaved {
    /// A private window keeps nothing.
    Private,
    /// The window was opened without a profile (a test, a snapshot).
    NoProfile,
    /// The disk is full. The next change tries again.
    DiskFull,
    /// The folder or the file may not be written.
    ReadOnly,
    /// Another program holds the file. The next change tries again.
    Busy,
    /// A newer browser wrote it: nothing is read or written until an update.
    TooNew { found: u32, supported: u32 },
    /// The file could not be read. Start over keeps it as `kept_as`.
    Damaged { detail: String },
    /// Anything else, as the system worded it.
    Failed(String),
}

/// The parts of a profile that are kept, each with its own [`Keeping`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Settings,
    History,
}

impl Part {
    /// The `$part` the catalogue's messages select on.
    fn id(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::History => "history",
        }
    }
}

impl Keeping {
    /// One line for the settings page about `part`, or `None` when there is
    /// nothing to say.
    pub fn text(&self, part: Part) -> Option<String> {
        let Self::NotSaved(why) = self else {
            return None;
        };
        let part = part.id();
        Some(match why {
            NotSaved::Private => tr!("browser-keeping-private"),
            NotSaved::NoProfile => tr!("browser-keeping-no-profile", part = part),
            NotSaved::DiskFull => tr!("browser-keeping-disk-full", part = part),
            NotSaved::ReadOnly => tr!("browser-keeping-read-only", part = part),
            NotSaved::Busy => tr!("browser-keeping-busy", part = part),
            NotSaved::TooNew { .. } => tr!("browser-keeping-too-new", part = part),
            NotSaved::Damaged { .. } => tr!("browser-keeping-damaged", part = part),
            NotSaved::Failed(error) => tr!("browser-keeping-failed", part = part, error = error),
        })
    }

    /// Whether Start over is offered: a damaged file, moved aside first.
    pub fn can_start_over(&self) -> bool {
        matches!(self, Self::NotSaved(NotSaved::Damaged { .. }))
    }
}

/// A damaged file's new name: `<name>.damaged-<unix seconds>`, beside it.
pub fn damaged_name(path: &std::path::Path) -> PathBuf {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".damaged-{seconds}"));
    path.with_file_name(name)
}

/// What Clear history reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClearScope {
    /// The browser's own history: the store and the search index.
    pub history: bool,
    /// The engine's visited-link colours (Places). Only when the engine can
    /// clear them (Vivarium proposal V1).
    pub link_colours: bool,
}

impl ClearScope {
    /// What the settings page says Clear history does not reach.
    pub fn text(self) -> Option<String> {
        (self.history && !self.link_colours).then(|| tr!("browser-history-link-colours-kept"))
    }

    /// The toast after Clear history.
    pub fn cleared_text(self) -> String {
        if self.text().is_some() {
            tr!("browser-history-cleared-link-colours-kept")
        } else {
            tr!("browser-history-cleared")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_state_that_keeps_nothing_has_words() {
        crate::locale::register();
        assert_eq!(Keeping::Saved.text(Part::History), None);
        assert_eq!(Keeping::Loading.text(Part::History), None);
        let full = Keeping::NotSaved(NotSaved::DiskFull);
        assert_eq!(
            full.text(Part::History).as_deref(),
            Some("The disk is full: history is not being saved")
        );
        assert_eq!(
            full.text(Part::Settings).as_deref(),
            Some("The disk is full: settings are not being saved")
        );
        assert!(
            Keeping::NotSaved(NotSaved::Damaged {
                detail: String::new()
            })
            .can_start_over()
        );
        assert!(!Keeping::NotSaved(NotSaved::DiskFull).can_start_over());
    }

    #[test]
    fn clear_history_says_what_it_does_not_reach() {
        crate::locale::register();
        let today = ClearScope {
            history: true,
            link_colours: false,
        };
        assert!(today.text().unwrap().contains("Link colours"));
        let with_v1 = ClearScope {
            history: true,
            link_colours: true,
        };
        assert_eq!(with_v1.text(), None);
    }
}
