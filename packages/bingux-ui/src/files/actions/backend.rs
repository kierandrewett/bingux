//! The file operations the actions call: the model crate's `ops`. Every
//! function blocks, so they run on the worker pool.

pub(super) use bingux_files::ops::{
    Conflict, ConflictPolicy, Format, OpError, Progress, Resolution, Trashed, can_extract,
    can_trash, compress, compress_available, copy, create_dir, create_file, delete, empty_trash,
    extract, in_trash, move_to, rename, trash, untrash,
};
