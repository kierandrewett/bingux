//! The file operations the actions call: the model crate's `ops`. Every
//! function blocks, so they run on the worker pool.

pub(super) use bingux_files::ops::{
    Conflict, ConflictPolicy, OpError, Progress, Resolution, Trashed, can_extract, copy,
    create_dir, create_file, empty_trash, in_trash, move_to, rename, trash, untrash,
};

// The Trash: refusals with their reasons, restoring, deleting from the bins,
// and counting what is in them.
pub(super) use bingux_files::ops::{
    RestoreFailure, RestorePlan, Restored, TrashRefusal, TrashSummary, check_trash, delete_trashed,
    plan_restore, restore, trash_copying, trash_items, trash_place, trash_summary,
};
