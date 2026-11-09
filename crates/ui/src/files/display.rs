//! Sizes and times as Files and the previews write them, in the person's
//! language and with their conventions ([`crate::format`]). `bingux_files`
//! holds no words: it cannot depend on bingux-ui, so its listing gives the
//! numbers and this module writes them.

use std::time::SystemTime;

use crate::format::{Bytes, Date, TimePrecision};

/// `1.2 MB`, `340 kB`, `12 bytes`: decimal units, one decimal under 10, a
/// no-break space before the unit.
pub(crate) fn size(bytes: u64) -> String {
    Bytes::new(bytes).words().format()
}

/// A time in full, for Properties and the preview's details:
/// `9 Mar 2024, 12:34`.
pub(crate) fn full(time: SystemTime) -> String {
    Date::new(time).with_time(TimePrecision::Minutes).format()
}
