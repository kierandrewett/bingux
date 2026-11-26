//! What went wrong in an operation, as a value.
//!
//! [`Failure`] names the kind and holds the facts a sentence needs (a name, a
//! count, an [`IoKind`]); it never holds a sentence. The window words each kind
//! with a message of its catalogue (`docs/files-errors.md`), so the same
//! failure reads in the language the person chose. `Display` is a short
//! English line for logs and test failures.

use std::fmt;

use super::{IoKind, OpError};

/// The step of a program's work that failed, for the lead of its complaint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Making an archive.
    MakeArchive,
    /// Testing a new archive before it gets its name.
    CheckArchive,
    /// Listing the entries of a new archive.
    ListNewArchive,
    /// Estimating the size of a reduced file.
    EstimateSize,
    /// Reducing a picture, film or song.
    Reduce,
    /// Reading the entries of an archive to open.
    ReadArchive,
    /// Trying a password on an archive.
    TryPassword,
    /// Extracting an archive.
    Extract,
    /// A program the caller did not name a step for.
    Other,
}

/// Why an operation failed, when none of the plain [`OpError`] kinds says it.
/// Names are file names as the person sees them, without quotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    // ---- Making archives -------------------------------------------------
    /// A work folder for the archive could not be made.
    WorkFolder,
    /// Nothing was given to compress.
    NothingToCompress,
    /// Every item was left out by the exclude patterns.
    EverythingLeftOut,
    /// Nothing was given to reduce.
    NothingToReduce,
    /// The paths given must be absolute.
    PathsNotAbsolute,
    /// The paths have no folder in common.
    NoCommonParent,
    /// The root folder cannot be compressed.
    RootNotCompressible,
    /// The destination is not a folder.
    DestinationNotFolder,
    /// The originals cannot be removed when the archive is saved inside them.
    OriginalsHoldArchive,
    /// The archiving program wrote nothing.
    ProgramWroteNothing,
    /// The new archive holds fewer entries than were put in.
    ArchiveMissingItems { listed: u64, expected: u64 },
    /// A folder could not be read.
    FolderUnreadable { name: String, kind: IoKind },
    /// A file cannot be read for lack of permission.
    FileNoPermission { name: String },
    /// A name cannot be stored in this format (`extension` without the dot).
    NameNotStorable { name: String, extension: String },
    /// A name cannot be stored in a locked .zip.
    NameNotStorableLocked { name: String },

    // ---- The drive -------------------------------------------------------
    /// Writing to a drive failed (`label` is the drive's name when known).
    DriveWrite { label: Option<String>, kind: IoKind },
    /// Finishing the write to a drive failed.
    DriveFinish { label: String, kind: IoKind },

    // ---- Names, copying and moving ---------------------------------------
    /// The root folder cannot be renamed.
    RootCannotBeRenamed,
    /// The root folder cannot be deleted.
    RootCannotBeDeleted,
    /// A folder cannot be put inside itself.
    InsideItself,
    /// The path has no name to copy or move.
    CannotCopyOrMove,
    /// The path cannot be moved to the Trash.
    CannotMoveToTrash,
    /// A copy did not arrive complete, so the original was left.
    DidNotArrive { name: String },
    /// The original changed while it was moved, so it was left.
    ChangedWhileMoving { name: String },
    /// A folder is in the way of a file.
    FolderInTheWay { name: String },
    /// Only regular files can be copied.
    NotRegularFile { name: String },
    /// The home folder could not be found.
    NoHome,

    // ---- The Trash -------------------------------------------------------
    /// The Trash folder could not be made.
    TrashFolderNotMade { path: String, kind: IoKind },
    /// The copy for the Trash could not be made.
    NotCopiedIntoTrash,
    /// It was copied to `path` but could not be put in the Trash.
    CopiedNotTrashed { path: String, kind: IoKind },

    // ---- Restoring -------------------------------------------------------
    /// The Trash has no record of where an item came from.
    NoRecord { name: String },
    /// What is in the way could not be moved to the Trash.
    NotMovedToMakeRoom { name: String, why: Box<OpError> },
    /// The folder to put an item back in is not a folder.
    PutBackNotFolder { parent: String, name: String },
    /// The folder to put an item back in no longer exists.
    PutBackFolderGone { parent: String, name: String },
    /// There is no permission to put an item back.
    PutBackDenied { parent: String, name: String },
    /// The drive is read-only.
    PutBackReadOnly { parent: String, name: String },

    // ---- Reducing --------------------------------------------------------
    /// The film could not be read.
    VideoUnreadable,
    /// The file has no picture to reduce.
    NoPicture,
    /// The song could not be read.
    AudioUnreadable,
    /// The file has no sound to reduce.
    NoSound,
    /// The path holds a null character.
    NullInPath,
    /// The file has no folder.
    NoFolder,

    // ---- Programs --------------------------------------------------------
    /// A program failed; `detail` is the line it complained with.
    ToolFailed { step: Step, detail: String },
    /// The program gave no reason for failing.
    ToolSilent { step: Step },
    /// The program's output could not be read.
    NoOutputStream,
    /// The program's error output could not be read.
    NoErrorStream,

    // ---- Opening archives ------------------------------------------------
    /// The file is not an archive.
    NotAnArchive,
    /// The archive would put files outside its folder.
    ArchiveEscapes,
    /// The archive needs a password.
    NeedsPassword,
    /// The password is not right.
    WrongPassword,
    /// Nothing was extracted.
    NothingExtracted,

    // ---- What this machine cannot do --------------------------------------
    /// This kind of file cannot be reduced here.
    CannotReduceKind,
    /// This kind of archive cannot be opened here.
    CannotOpenKind,
    /// The format cannot be locked with a password.
    CannotLock,
    /// A password needs 7-Zip, which is not installed.
    LockNeedsSevenZip,
    /// The program that cuts this format into volumes is not installed.
    SplitNeedsProgram,
    /// Only .zip and .7z can be cut into volumes.
    SplitOnlyZipAnd7z,
    /// A picture format has no encoder here (`format` is `JPEG`, `PNG`, ...).
    NoPictureEncoder { format: String },
    /// ffmpeg has no encoder for the sound of a film.
    NoSoundEncoder,
    /// The file is not a picture, a film or a song.
    NotMediaFile,
    /// It is not a file.
    NotAFile,
    /// A new file did not pass the check made on it before it was kept.
    NewFileFailedCheck(CheckProblem),
    /// AVIF needs ffmpeg, and it is missing.
    AvifNeedsFfmpeg,
    /// ffmpeg has no encoder for a codec (`codec` is `AV1`, `H.264`, ...).
    NoCodecEncoder { codec: String },
    /// Neither ImageMagick nor ffmpeg is installed.
    NoPictureTools,
}

impl Failure {
    /// The same failure as an [`OpError`].
    pub fn error(self) -> OpError {
        OpError::Failed(self)
    }
}

impl fmt::Display for Failure {
    /// A short English line for the log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::ToolFailed { step, detail } => write!(f, "{step:?} failed: {detail}"),
            Failure::ToolSilent { step } => write!(f, "{step:?} failed with no reason"),
            Failure::DriveWrite { label, kind } => {
                write!(
                    f,
                    "write to {} failed: {kind:?}",
                    label.as_deref().unwrap_or("the drive")
                )
            }
            Failure::DriveFinish { label, kind } => write!(f, "finishing {label} failed: {kind:?}"),
            other => write!(f, "{other:?}"),
        }
    }
}

/// What was wrong with a file that was just made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckProblem {
    /// It cannot be opened.
    CannotOpen,
    /// It has no picture.
    NoPicture,
    /// It has no sound.
    NoSound,
    /// It is shorter than the original (lengths in tenths of a second).
    TooShort {
        after_tenths: u32,
        before_tenths: u32,
    },
}
