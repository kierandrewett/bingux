//! The words for an [`OpError`]: the one place that turns why an operation did
//! not happen into a sentence. `bingux_files` says what happened as a value;
//! the sentence is a message of this crate's catalogue (`docs/files-errors.md`).

use bingux_files::location::NameError;
use bingux_files::ops::{IoKind, OpError};

/// The sentence for `error`, in the language shown.
pub(in crate::files) fn text(error: &OpError) -> String {
    match error {
        OpError::Exists => crate::tr!("ui-files-error-exists"),
        OpError::Permission => crate::tr!("ui-files-error-permission"),
        OpError::NotFound => crate::tr!("ui-files-error-not-found"),
        OpError::Cancelled => crate::tr!("ui-files-error-cancelled"),
        OpError::Invalid(NameError::Empty) => crate::tr!("ui-files-error-name-empty"),
        OpError::Invalid(NameError::Reserved) => crate::tr!("ui-files-error-name-reserved"),
        OpError::Invalid(NameError::TooLong) => crate::tr!("ui-files-error-name-too-long"),
        OpError::Invalid(NameError::Invalid) => crate::tr!("ui-files-error-name-invalid"),
        OpError::Missing(program) => {
            crate::tr!("ui-files-error-missing", program = program.as_str())
        }
        OpError::Io(kind) => io(*kind),
        OpError::CannotTrash(refusal) => super::bin::refusal_text(refusal, false),
        OpError::CopyNeeded => crate::tr!("ui-files-error-copy-needed"),
        OpError::Failed(failure) => super::failure_text::text(failure),
    }
}

pub(super) fn io(kind: IoKind) -> String {
    match kind {
        IoKind::Full => crate::tr!("ui-files-error-io-full"),
        IoKind::ReadOnly => crate::tr!("ui-files-error-io-read-only"),
        IoKind::QuotaFull => crate::tr!("ui-files-error-io-quota-full"),
        IoKind::NotEmpty => crate::tr!("ui-files-error-io-not-empty"),
        IoKind::InUse => crate::tr!("ui-files-error-io-in-use"),
        IoKind::BetweenDrives => crate::tr!("ui-files-error-io-between-drives"),
        IoKind::TimedOut => crate::tr!("ui-files-error-io-timed-out"),
        IoKind::NotAFolder => crate::tr!("ui-files-error-io-not-a-folder"),
        IoKind::IsAFolder => crate::tr!("ui-files-error-io-is-a-folder"),
        IoKind::BadName => crate::tr!("ui-files-error-io-bad-name"),
        IoKind::Unsupported => crate::tr!("ui-files-error-io-unsupported"),
        IoKind::Interrupted => crate::tr!("ui-files-error-io-interrupted"),
        IoKind::Unreadable => crate::tr!("ui-files-error-io-unreadable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_sentence_and_the_program_is_named() {
        for kind in [
            IoKind::Full,
            IoKind::ReadOnly,
            IoKind::QuotaFull,
            IoKind::NotEmpty,
            IoKind::InUse,
            IoKind::BetweenDrives,
            IoKind::TimedOut,
            IoKind::NotAFolder,
            IoKind::IsAFolder,
            IoKind::BadName,
            IoKind::Unsupported,
            IoKind::Interrupted,
            IoKind::Unreadable,
        ] {
            let words = text(&OpError::Io(kind));
            assert!(
                !words.is_empty() && !words.starts_with("ui-"),
                "{kind:?}: {words}"
            );
        }
        assert_eq!(
            text(&OpError::Missing("zip".into())),
            "zip is not installed."
        );
        assert_eq!(
            text(&OpError::Io(IoKind::ReadOnly)),
            "The drive is read-only. Choose another place."
        );
        assert_eq!(
            text(&OpError::Invalid(NameError::TooLong)),
            "The name is longer than 255 bytes."
        );
    }
}
