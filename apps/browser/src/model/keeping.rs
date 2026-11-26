//! Whether what the person does is kept: one state for the settings and one
//! for the history, which the settings page and a one-time toast show. Every
//! failure of the store is one of these, never a panic and never a silent
//! loss (`docs/browser/profiles-and-store.md`, "Failure modes").

use std::path::PathBuf;

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

impl Keeping {
    pub fn is_saved(&self) -> bool {
        matches!(self, Self::Saved | Self::Loading)
    }

    /// One line for the settings page about `what` ("History", "Settings"),
    /// or `None` when there is nothing to say.
    pub fn text(&self, what: &str) -> Option<String> {
        let Self::NotSaved(why) = self else {
            return None;
        };
        let lower = what.to_lowercase();
        Some(match why {
            NotSaved::Private => "Changes here last until this window closes".to_string(),
            NotSaved::NoProfile => format!("This window has no profile: {lower} is not kept"),
            NotSaved::DiskFull => format!("The disk is full: {lower} is not being saved"),
            NotSaved::ReadOnly => {
                format!("{what} cannot be saved: the profile folder is read-only")
            }
            NotSaved::Busy => format!(
                "Another program is using the {lower} file: trying again with the next change"
            ),
            NotSaved::TooNew { .. } => {
                format!("This profile was used by a newer Browser. Update Browser to keep {lower}")
            }
            NotSaved::Damaged { .. } => {
                format!("{what} could not be read. Start over to keep it again")
            }
            NotSaved::Failed(text) => format!("{what} is not being saved: {text}"),
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
    /// What the person is told after Clear history, and on the settings row.
    pub fn text(self) -> Option<&'static str> {
        (self.history && !self.link_colours)
            .then_some("Link colours on pages are kept until the engine can clear them")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_state_that_keeps_nothing_has_words() {
        assert_eq!(Keeping::Saved.text("History"), None);
        assert_eq!(Keeping::Loading.text("History"), None);
        assert_eq!(
            Keeping::NotSaved(NotSaved::DiskFull)
                .text("History")
                .as_deref(),
            Some("The disk is full: history is not being saved")
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
