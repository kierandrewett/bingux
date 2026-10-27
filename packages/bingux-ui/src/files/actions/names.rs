//! File names a person types: what is wrong with one, and which part of it
//! to select when renaming.

/// The longest file name Linux allows, in bytes.
const NAME_MAX: usize = 255;

/// Why a name cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::files) enum NameProblem {
    Empty,
    /// `.` or `..`.
    Reserved,
    Slash,
    TooLong,
    /// A NUL byte.
    Invalid,
    /// Another entry in the folder has this name.
    Exists,
}

impl NameProblem {
    /// One plain sentence, shown under the field.
    pub(in crate::files) fn message(self) -> &'static str {
        match self {
            NameProblem::Empty => "Enter a name.",
            NameProblem::Reserved => "A name cannot be . or ..",
            NameProblem::Slash => "A name cannot contain a slash.",
            NameProblem::TooLong => "The name is longer than 255 bytes.",
            NameProblem::Invalid => "The name contains a character that is not allowed.",
            NameProblem::Exists => "A file or folder with this name already exists.",
        }
    }
}

/// Check `name` for a rename or a new item. `current` is the entry's own name
/// while renaming: keeping it is fine. `taken` says whether the folder holds
/// another entry called that.
pub(in crate::files) fn check_name(
    name: &str,
    current: Option<&str>,
    taken: impl Fn(&str) -> bool,
) -> Result<(), NameProblem> {
    if name.trim().is_empty() {
        Err(NameProblem::Empty)
    } else if name == "." || name == ".." {
        Err(NameProblem::Reserved)
    } else if name.contains('/') {
        Err(NameProblem::Slash)
    } else if name.len() > NAME_MAX {
        Err(NameProblem::TooLong)
    } else if name.contains('\0') {
        Err(NameProblem::Invalid)
    } else if current != Some(name) && taken(name) {
        Err(NameProblem::Exists)
    } else {
        Ok(())
    }
}

/// How many characters of `name` come before its extension, for selecting
/// just the stem when renaming a file. A folder selects all of its name, a
/// leading dot is not an extension (`.bashrc`), and `.tar.gz` counts as one
/// extension.
pub(in crate::files) fn stem_chars(name: &str, is_dir: bool) -> usize {
    let total = name.chars().count();
    if is_dir {
        return total;
    }
    let lower = name.to_lowercase();
    for compressed in [".gz", ".xz", ".bz2", ".zst", ".lz", ".lzma"] {
        if let Some(without) = lower.strip_suffix(compressed)
            && let Some(stem) = without.strip_suffix(".tar")
            && !stem.is_empty()
        {
            return stem.chars().count();
        }
    }
    match name.rfind('.') {
        Some(at) if at > 0 && at + 1 < name.len() => name[..at].chars().count(),
        _ => total,
    }
}

/// The first of `base`, `base 2`, `base 3`, ... that `taken` does not name.
pub(super) fn unused_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|number| format!("{base} {number}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> bool {
        false
    }

    #[test]
    fn rejects_bad_names() {
        assert_eq!(check_name("", None, none), Err(NameProblem::Empty));
        assert_eq!(check_name("   ", None, none), Err(NameProblem::Empty));
        assert_eq!(check_name(".", None, none), Err(NameProblem::Reserved));
        assert_eq!(check_name("..", None, none), Err(NameProblem::Reserved));
        assert_eq!(check_name("a/b", None, none), Err(NameProblem::Slash));
        assert_eq!(check_name("a\0b", None, none), Err(NameProblem::Invalid));
        assert_eq!(
            check_name(&"x".repeat(256), None, none),
            Err(NameProblem::TooLong)
        );
        assert_eq!(check_name(&"x".repeat(255), None, none), Ok(()));
    }

    #[test]
    fn long_name_counts_bytes_not_characters() {
        let name = "é".repeat(128);
        assert_eq!(check_name(&name, None, none), Err(NameProblem::TooLong));
    }

    #[test]
    fn existing_names_conflict_unless_unchanged() {
        let taken = |name: &str| name == "a.txt" || name == "b.txt";
        assert_eq!(
            check_name("b.txt", Some("a.txt"), taken),
            Err(NameProblem::Exists)
        );
        assert_eq!(check_name("a.txt", Some("a.txt"), taken), Ok(()));
        assert_eq!(check_name("c.txt", Some("a.txt"), taken), Ok(()));
        assert_eq!(check_name("a.txt", None, taken), Err(NameProblem::Exists));
    }

    #[test]
    fn stem_skips_the_extension() {
        assert_eq!(stem_chars("report.pdf", false), 6);
        assert_eq!(stem_chars("archive.tar.gz", false), 7);
        assert_eq!(stem_chars("a.b.c", false), 3);
        assert_eq!(stem_chars(".bashrc", false), 7);
        assert_eq!(stem_chars("README", false), 6);
        assert_eq!(stem_chars("trailing.", false), 9);
        assert_eq!(stem_chars("photos.2024", true), 11);
        assert_eq!(stem_chars("café.txt", false), 4);
    }

    #[test]
    fn unused_name_counts_up() {
        let taken = |name: &str| name == "Untitled" || name == "Untitled 2";
        assert_eq!(unused_name("Untitled", taken), "Untitled 3");
        assert_eq!(unused_name("Other", taken), "Other");
    }
}
