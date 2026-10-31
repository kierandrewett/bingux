//! Renaming and creating, and the "(copy)" names for a second copy.

use std::fs;
use std::path::{Path, PathBuf};

use crate::location::{NameError, check_file_name};

use super::durable::sync_dirs;
use super::{OpError, rename_noreplace};

/// A name for one file: the usual rules, and no slash.
pub(super) fn check_name(name: &str) -> Result<(), OpError> {
    check_file_name(name)?;
    if name.contains('/') {
        return Err(OpError::Invalid(NameError::Invalid));
    }
    Ok(())
}

/// Give `path` a new name in the same folder. Never replaces anything: if
/// the name is taken the answer is [`OpError::Exists`]. Renaming to the
/// current name succeeds and changes nothing.
///
/// One `renameat2(RENAME_NOREPLACE)`: atomic, and the file is never copied.
/// On removable media, a share, FAT, exFAT or NTFS the folder is `fsync`ed
/// before this returns, so the new name survives a pulled cable.
pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf, OpError> {
    rename_then(path, new_name, sync_dirs)
}

/// [`rename`], with the flush of the folder given, so a test can watch it.
pub(super) fn rename_then(
    path: &Path,
    new_name: &str,
    sync: impl FnOnce(&[&Path]) -> Result<(), OpError>,
) -> Result<PathBuf, OpError> {
    check_name(new_name)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| OpError::Other("The root folder cannot be renamed.".into()))?;
    if path.file_name().is_some_and(|name| name == new_name) {
        return Ok(path.to_path_buf());
    }
    let target = parent.join(new_name);
    rename_noreplace(path, &target)?;
    sync(&[parent])?;
    Ok(target)
}

/// Make an empty file. Never touches an existing one.
pub fn create_file(parent: &Path, name: &str) -> Result<PathBuf, OpError> {
    check_name(name)?;
    let path = parent.join(name);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    sync_dirs(&[parent])?;
    Ok(path)
}

/// Make a folder. Fails with [`OpError::Exists`] if the name is taken.
pub fn create_dir(parent: &Path, name: &str) -> Result<PathBuf, OpError> {
    check_name(name)?;
    let path = parent.join(name);
    fs::create_dir(&path)?;
    sync_dirs(&[parent, &path])?;
    Ok(path)
}

/// The extension of a file name, dot included, as a person sees it:
/// `.tar.gz` counts as one, a leading dot is not an extension, and a long
/// tail with spaces is part of the name.
pub(super) fn split_extension(name: &str) -> (&str, &str) {
    let Some(dot) = name.rfind('.') else {
        return (name, "");
    };
    let tail = &name[dot + 1..];
    if dot == 0 || tail.is_empty() || tail.len() > 8 || tail.contains(char::is_whitespace) {
        return (name, "");
    }
    let stem = &name[..dot];
    if let Some(inner) = stem.strip_suffix(".tar")
        && !inner.is_empty()
    {
        return (inner, &name[inner.len()..]);
    }
    (stem, &name[dot..])
}

/// "copy", "another copy", "3rd copy", "4th copy", "11th copy", "21st copy".
fn copy_label(count: u32) -> String {
    match count {
        1 => "copy".to_string(),
        2 => "another copy".to_string(),
        n => {
            let suffix = match (n % 100, n % 10) {
                (11..=13, _) => "th",
                (_, 1) => "st",
                (_, 2) => "nd",
                (_, 3) => "rd",
                _ => "th",
            };
            format!("{n}{suffix} copy")
        }
    }
}

/// The copy number a stem already carries, and the stem without it:
/// `"a (another copy)"` is `("a", 2)`.
fn parse_copy_suffix(stem: &str) -> (&str, u32) {
    if let Some(inside) = stem.strip_suffix(')')
        && let Some(open) = inside.rfind(" (")
    {
        let label = &inside[open + 2..];
        let count = match label {
            "copy" => Some(1),
            "another copy" => Some(2),
            other => other.strip_suffix(" copy").and_then(parse_ordinal),
        };
        if let Some(count) = count {
            return (&stem[..open], count);
        }
    }
    (stem, 0)
}

fn parse_ordinal(text: &str) -> Option<u32> {
    let digits = text.find(|c: char| !c.is_ascii_digit())?;
    let number: u32 = text[..digits].parse().ok()?;
    (number >= 3 && matches!(&text[digits..], "st" | "nd" | "rd" | "th")).then_some(number)
}

/// The first of `name (copy).ext`, `name (another copy).ext`,
/// `name (3rd copy).ext`, ... that `taken` does not claim, as GNOME Files
/// names a second copy. A name that is already a copy continues the count.
pub fn keep_both_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let (stem, extension) = split_extension(name);
    keep_both_with(stem, extension, taken)
}

/// [`keep_both_name`] with the extension already decided (folders have none).
pub(super) fn keep_both_with(stem: &str, extension: &str, taken: impl Fn(&str) -> bool) -> String {
    let (base, already) = parse_copy_suffix(stem);
    let mut count = already + 1;
    loop {
        let candidate = format!("{base} ({}){extension}", copy_label(count));
        if !taken(&candidate) {
            return candidate;
        }
        count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::*;

    #[test]
    fn rename_refuses_to_overwrite() {
        let temp = Temp::new("rename");
        let a = temp.write("a.txt", "a");
        let b = temp.write("b.txt", "b");
        assert_eq!(rename(&a, "b.txt"), Err(OpError::Exists));
        assert_eq!(fs::read_to_string(&a).unwrap(), "a");
        assert_eq!(fs::read_to_string(&b).unwrap(), "b");
        let c = rename(&a, "c.txt").unwrap();
        assert_eq!(c, temp.path().join("c.txt"));
        assert!(!a.exists());
        assert_eq!(fs::read_to_string(&c).unwrap(), "a");
    }

    #[test]
    fn rename_to_the_same_name_is_fine() {
        let temp = Temp::new("rename-same");
        let a = temp.write("a.txt", "a");
        assert_eq!(rename(&a, "a.txt"), Ok(a.clone()));
        assert!(a.exists());
    }

    #[test]
    fn rename_checks_the_name() {
        let temp = Temp::new("rename-name");
        let a = temp.write("a.txt", "a");
        assert_eq!(rename(&a, ""), Err(OpError::Invalid(NameError::Empty)));
        assert_eq!(rename(&a, ".."), Err(OpError::Invalid(NameError::Reserved)));
        assert_eq!(rename(&a, "x/y"), Err(OpError::Invalid(NameError::Invalid)));
        assert_eq!(
            rename(&a, &"n".repeat(256)),
            Err(OpError::Invalid(NameError::TooLong))
        );
        assert!(a.exists());
    }

    #[test]
    fn rename_moves_a_folder_and_a_dangling_link() {
        let temp = Temp::new("rename-kinds");
        let dir = temp.dir("old");
        assert_eq!(rename(&dir, "new").unwrap(), temp.path().join("new"));
        let link = temp.path().join("link");
        std::os::unix::fs::symlink("/nonexistent/target", &link).unwrap();
        let blocker = temp.write("taken", "x");
        assert_eq!(rename(&link, "taken"), Err(OpError::Exists));
        assert!(blocker.exists());
        assert!(rename(&link, "moved").is_ok());
    }

    #[test]
    fn rename_of_a_missing_file_is_not_found() {
        let temp = Temp::new("rename-missing");
        assert_eq!(
            rename(&temp.path().join("nope"), "x"),
            Err(OpError::NotFound)
        );
    }

    #[test]
    fn create_never_overwrites() {
        let temp = Temp::new("create");
        let file = create_file(temp.path(), "new.txt").unwrap();
        fs::write(&file, "kept").unwrap();
        assert_eq!(create_file(temp.path(), "new.txt"), Err(OpError::Exists));
        assert_eq!(fs::read_to_string(&file).unwrap(), "kept");
        let dir = create_dir(temp.path(), "folder").unwrap();
        assert!(dir.is_dir());
        assert_eq!(create_dir(temp.path(), "folder"), Err(OpError::Exists));
        assert_eq!(create_dir(temp.path(), "new.txt"), Err(OpError::Exists));
        assert_eq!(
            create_dir(temp.path(), "a/b"),
            Err(OpError::Invalid(NameError::Invalid))
        );
        assert_eq!(
            create_file(&temp.path().join("missing"), "x"),
            Err(OpError::NotFound)
        );
    }

    #[test]
    fn extensions() {
        assert_eq!(split_extension("photo.jpg"), ("photo", ".jpg"));
        assert_eq!(split_extension("a.tar.gz"), ("a", ".tar.gz"));
        assert_eq!(split_extension(".bashrc"), (".bashrc", ""));
        assert_eq!(split_extension("README"), ("README", ""));
        assert_eq!(split_extension("trailing."), ("trailing.", ""));
        assert_eq!(split_extension("Mr. Smith"), ("Mr. Smith", ""));
        assert_eq!(split_extension("v1.2.3"), ("v1.2", ".3"));
    }

    #[test]
    fn keep_both_names_follow_gnome_files() {
        let none = |_: &str| false;
        assert_eq!(keep_both_name("a.txt", none), "a (copy).txt");
        assert_eq!(keep_both_name("a (copy).txt", none), "a (another copy).txt");
        assert_eq!(
            keep_both_name("a (another copy).txt", none),
            "a (3rd copy).txt"
        );
        assert_eq!(keep_both_name("a (3rd copy).txt", none), "a (4th copy).txt");
        assert_eq!(keep_both_name("a (10th copy)", none), "a (11th copy)");
        assert_eq!(keep_both_name("a (11th copy)", none), "a (12th copy)");
        assert_eq!(keep_both_name("a (20th copy)", none), "a (21st copy)");
        assert_eq!(keep_both_name("a (21st copy)", none), "a (22nd copy)");
        assert_eq!(keep_both_name("a (22nd copy)", none), "a (23rd copy)");
        assert_eq!(keep_both_name("a.tar.gz", none), "a (copy).tar.gz");
        assert_eq!(keep_both_name("Makefile", none), "Makefile (copy)");
        assert_eq!(keep_both_name(".bashrc", none), ".bashrc (copy)");
    }

    #[test]
    fn keep_both_skips_taken_names() {
        let taken = |name: &str| ["a (copy).txt", "a (another copy).txt"].contains(&name);
        assert_eq!(keep_both_name("a.txt", taken), "a (3rd copy).txt");
        assert_eq!(keep_both_name("a (copy).txt", taken), "a (3rd copy).txt");
    }

    #[test]
    fn rename_writes_the_folder_out_once_the_name_has_changed() {
        use super::super::durable::recorder::{Call, Recorder, stick};
        use super::super::durable::sync_dirs_with;
        let temp = Temp::new("rename-durable");
        let drive = temp.dir("stick");
        let file = temp.write("stick/a.txt", "a");
        let recorder = Recorder::default();
        let (old, new) = (file.clone(), drive.join("b.txt"));
        let at_sync = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = at_sync.clone();
        recorder.on_call(move |call| {
            if matches!(call, Call::Dir(_)) {
                sink.lock().unwrap().push((old.exists(), new.exists()));
            }
            Ok(())
        });
        let level = stick(&drive);
        let policy = move |_: &Path| level.clone();
        let renamed = rename_then(&file, "b.txt", |dirs| {
            sync_dirs_with(&recorder, &policy, dirs)
        })
        .unwrap();
        assert_eq!(renamed, drive.join("b.txt"));
        assert_eq!(*at_sync.lock().unwrap(), [(false, true)]);
        assert_eq!(
            recorder.calls(),
            [Call::Dir(drive.clone()), Call::Filesystem(drive.clone())]
        );
    }
}
