//! The words for a [`Failure`]: one message of the catalogue per kind
//! (`ui-files-failure-*`), with the names and counts the kind holds.

use bingux_files::ops::{CheckProblem, Failure, NameCheck, PatternError, PatternProblem, Step};

use super::op_error;

/// A name between the quotes of the language shown.
fn quoted(name: &str) -> String {
    crate::tr!("ui-run-whole-name", name = name)
}

pub(super) fn step(step: Step) -> String {
    match step {
        Step::MakeArchive => crate::tr!("ui-files-step-make-archive"),
        Step::CheckArchive => crate::tr!("ui-files-step-check-archive"),
        Step::ListNewArchive => crate::tr!("ui-files-step-list-new-archive"),
        Step::EstimateSize => crate::tr!("ui-files-step-estimate-size"),
        Step::Reduce => crate::tr!("ui-files-step-reduce"),
        Step::ReadArchive => crate::tr!("ui-files-step-read-archive"),
        Step::TryPassword => crate::tr!("ui-files-step-try-password"),
        Step::Extract => crate::tr!("ui-files-step-extract"),
        Step::Other => crate::tr!("ui-files-step-other"),
    }
}

pub(super) fn text(failure: &Failure) -> String {
    match failure {
        Failure::WorkFolder => crate::tr!("ui-files-failure-work-folder"),
        Failure::NothingToCompress => crate::tr!("ui-files-failure-nothing-to-compress"),
        Failure::EverythingLeftOut => crate::tr!("ui-files-failure-everything-left-out"),
        Failure::NothingToReduce => crate::tr!("ui-files-failure-nothing-to-reduce"),
        Failure::PathsNotAbsolute => crate::tr!("ui-files-failure-paths-not-absolute"),
        Failure::NoCommonParent => crate::tr!("ui-files-failure-no-common-parent"),
        Failure::RootNotCompressible => crate::tr!("ui-files-failure-root-not-compressible"),
        Failure::DestinationNotFolder => crate::tr!("ui-files-failure-destination-not-folder"),
        Failure::OriginalsHoldArchive => crate::tr!("ui-files-failure-originals-hold-archive"),
        Failure::ProgramWroteNothing => crate::tr!("ui-files-failure-program-wrote-nothing"),
        Failure::ArchiveMissingItems { listed, expected } => crate::tr!(
            "ui-files-failure-archive-missing-items",
            listed = *listed,
            expected = *expected
        ),
        Failure::FolderUnreadable { name, kind } => crate::tr!(
            "ui-files-failure-folder-unreadable",
            name = quoted(name),
            why = op_error::io(*kind)
        ),
        Failure::FileNoPermission { name } => {
            crate::tr!("ui-files-failure-file-no-permission", name = quoted(name))
        }
        Failure::NameNotStorable { name, extension } => crate::tr!(
            "ui-files-failure-name-not-storable",
            name = quoted(name),
            extension = extension.as_str()
        ),
        Failure::NameNotStorableLocked { name } => {
            crate::tr!(
                "ui-files-failure-name-not-storable-locked",
                name = quoted(name)
            )
        }
        Failure::DriveWrite { label, kind } => match label {
            Some(label) => crate::tr!(
                "ui-files-failure-drive-write-named",
                label = label.as_str(),
                why = op_error::io(*kind)
            ),
            None => crate::tr!("ui-files-failure-drive-write", why = op_error::io(*kind)),
        },
        Failure::DriveFinish { label, kind } => crate::tr!(
            "ui-files-failure-drive-finish",
            label = label.as_str(),
            why = op_error::io(*kind)
        ),
        Failure::RootCannotBeRenamed => crate::tr!("ui-files-failure-root-rename"),
        Failure::RootCannotBeDeleted => crate::tr!("ui-files-failure-root-delete"),
        Failure::InsideItself => crate::tr!("ui-files-failure-inside-itself"),
        Failure::CannotCopyOrMove => crate::tr!("ui-files-failure-cannot-copy-or-move"),
        Failure::CannotMoveToTrash => crate::tr!("ui-files-failure-cannot-trash"),
        Failure::DidNotArrive { name } => {
            crate::tr!("ui-files-failure-did-not-arrive", name = quoted(name))
        }
        Failure::ChangedWhileMoving { name } => {
            crate::tr!("ui-files-failure-changed-while-moving", name = quoted(name))
        }
        Failure::FolderInTheWay { name } => {
            crate::tr!("ui-files-failure-folder-in-the-way", name = quoted(name))
        }
        Failure::NotRegularFile { name } => {
            crate::tr!("ui-files-failure-not-regular-file", name = quoted(name))
        }
        Failure::NoHome => crate::tr!("ui-files-failure-no-home"),
        Failure::TrashFolderNotMade { path, kind } => crate::tr!(
            "ui-files-failure-trash-folder-not-made",
            path = path.as_str(),
            why = op_error::io(*kind)
        ),
        Failure::NotCopiedIntoTrash => crate::tr!("ui-files-failure-not-copied-into-trash"),
        Failure::CopiedNotTrashed { path, kind } => crate::tr!(
            "ui-files-failure-copied-not-trashed",
            path = path.as_str(),
            why = op_error::io(*kind)
        ),
        Failure::NoRecord { name } => crate::tr!("ui-files-failure-no-record", name = quoted(name)),
        Failure::NotMovedToMakeRoom { name, why } => crate::tr!(
            "ui-files-failure-not-moved-to-make-room",
            name = quoted(name),
            why = op_error::text(why)
        ),
        Failure::PutBackNotFolder { parent, name } => crate::tr!(
            "ui-files-failure-put-back-not-folder",
            parent = parent.as_str(),
            name = quoted(name)
        ),
        Failure::PutBackFolderGone { parent, name } => crate::tr!(
            "ui-files-failure-put-back-folder-gone",
            parent = parent.as_str(),
            name = quoted(name)
        ),
        Failure::PutBackDenied { parent, name } => crate::tr!(
            "ui-files-failure-put-back-denied",
            parent = parent.as_str(),
            name = quoted(name)
        ),
        Failure::PutBackReadOnly { parent, name } => crate::tr!(
            "ui-files-failure-put-back-read-only",
            parent = parent.as_str(),
            name = quoted(name)
        ),
        Failure::VideoUnreadable => crate::tr!("ui-files-failure-video-unreadable"),
        Failure::NoPicture => crate::tr!("ui-files-failure-no-picture"),
        Failure::AudioUnreadable => crate::tr!("ui-files-failure-audio-unreadable"),
        Failure::NoSound => crate::tr!("ui-files-failure-no-sound"),
        Failure::NullInPath => crate::tr!("ui-files-failure-null-in-path"),
        Failure::NoFolder => crate::tr!("ui-files-failure-no-folder"),
        Failure::ToolFailed { step: at, detail } => crate::tr!(
            "ui-files-failure-tool-failed",
            step = step(*at),
            detail = detail.as_str()
        ),
        Failure::ToolSilent { step: at } => {
            crate::tr!("ui-files-failure-tool-silent", step = step(*at))
        }
        Failure::NoOutputStream => crate::tr!("ui-files-failure-no-output-stream"),
        Failure::NoErrorStream => crate::tr!("ui-files-failure-no-error-stream"),
        Failure::NotAnArchive => crate::tr!("ui-files-failure-not-an-archive"),
        Failure::ArchiveEscapes => crate::tr!("ui-files-failure-archive-escapes"),
        Failure::NeedsPassword => crate::tr!("ui-files-failure-needs-password"),
        Failure::WrongPassword => crate::tr!("ui-files-failure-wrong-password"),
        Failure::NothingExtracted => crate::tr!("ui-files-failure-nothing-extracted"),
        Failure::CannotReduceKind => crate::tr!("ui-files-failure-cannot-reduce-kind"),
        Failure::CannotOpenKind => crate::tr!("ui-files-failure-cannot-open-kind"),
        Failure::NoPictureEncoder { format } => {
            crate::tr!(
                "ui-files-failure-no-picture-encoder",
                format = format.as_str()
            )
        }
        Failure::AvifNeedsFfmpeg => crate::tr!("ui-files-failure-avif-needs-ffmpeg"),
        Failure::NoCodecEncoder { codec } => {
            crate::tr!("ui-files-failure-no-codec-encoder", codec = codec.as_str())
        }
        Failure::NoPictureTools => crate::tr!("ui-files-failure-no-picture-tools"),
        Failure::CannotLock => crate::tr!("ui-files-failure-cannot-lock"),
        Failure::LockNeedsSevenZip => crate::tr!("ui-files-failure-lock-needs-seven-zip"),
        Failure::SplitNeedsProgram => crate::tr!("ui-files-failure-split-needs-program"),
        Failure::SplitOnlyZipAnd7z => crate::tr!("ui-files-failure-split-only-zip-and-7z"),
        Failure::NoSoundEncoder => crate::tr!("ui-files-failure-no-sound-encoder"),
        Failure::NotMediaFile => crate::tr!("ui-files-failure-not-media-file"),
        Failure::NotAFile => crate::tr!("ui-files-failure-not-a-file"),
        Failure::NewFileFailedCheck(problem) => crate::tr!(
            "ui-files-failure-new-file-failed-check",
            problem = check_problem(*problem)
        ),
    }
}

/// What was wrong with a file just made, as a clause.
fn check_problem(problem: CheckProblem) -> String {
    match problem {
        CheckProblem::CannotOpen => crate::tr!("ui-files-check-cannot-open"),
        CheckProblem::NoPicture => crate::tr!("ui-files-check-no-picture"),
        CheckProblem::NoSound => crate::tr!("ui-files-check-no-sound"),
        CheckProblem::TooShort {
            after_tenths,
            before_tenths,
        } => crate::tr!(
            "ui-files-check-too-short",
            after = format!("{:.1}", f64::from(after_tenths) / 10.0),
            before = format!("{:.1}", f64::from(before_tenths) / 10.0)
        ),
    }
}

/// Why the name typed for an archive cannot be used, when it cannot.
pub(in crate::files) fn name_check(check: NameCheck) -> Option<String> {
    Some(match check {
        NameCheck::Ok => return None,
        NameCheck::Empty => crate::tr!("ui-compress-name-empty"),
        NameCheck::HasSlash => crate::tr!("ui-compress-name-has-slash"),
        NameCheck::Reserved => crate::tr!("ui-compress-name-reserved"),
        NameCheck::TooLong => crate::tr!("ui-compress-name-too-long"),
        NameCheck::Taken => crate::tr!("ui-compress-name-taken"),
    })
}

/// Why an exclude pattern was refused.
pub(in crate::files) fn pattern(error: &PatternError) -> String {
    let typed = quoted(&error.pattern);
    match error.problem {
        PatternProblem::TooMany => crate::tr!("ui-files-pattern-too-many"),
        PatternProblem::TooLong => crate::tr!("ui-files-pattern-too-long", pattern = typed),
        PatternProblem::NullCharacter => crate::tr!("ui-files-pattern-null", pattern = typed),
        PatternProblem::TrailingBackslash => {
            crate::tr!("ui-files-pattern-backslash", pattern = typed)
        }
        PatternProblem::OpenClass => crate::tr!("ui-files-pattern-open-class", pattern = typed),
    }
}
