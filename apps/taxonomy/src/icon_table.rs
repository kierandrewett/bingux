//! The icon table `build.rs` writes: one row per SVG in bingux-ui's `icons/`.

use bingux_ui::icons::Icon;

/// One icon file, with the variant that names it and where it is used.
pub struct IconFile {
    /// The file stem without `-symbolic.svg`: `network-wireless`.
    pub file: &'static str,
    /// The `Icon` variant name, when the set has one for this file.
    pub variant: Option<&'static str>,
    pub icon: Option<Icon>,
    pub svg: &'static [u8],
    /// Where the user sees it (`scripts/icons/labels.tsv`).
    pub place: &'static str,
    /// The metaphor people expect there.
    pub metaphor: &'static str,
    /// The fork glyph and weight, or `custom: ...`.
    pub glyph: &'static str,
    /// The contact-sheet group (`scripts/icons/groups.txt`).
    pub group: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/icon_table.rs"));
