//! What an archive's name says, and the checks that keep an extraction
//! inside its folder. The extraction itself is [`super::unpack`].
//!
//! Nothing here reads archive formats itself. The programs are looked for on
//! `PATH`; when one is missing the answer is [`OpError::Unsupported`] and
//! [`extract_available`] says so up front, so a menu can hide the item.
//! Making archives is [`super::pack`].

use std::fs;
use std::path::{Path, PathBuf};

use super::OpError;

/// Is `name` an executable file in a folder of `PATH`?
fn on_path(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            fs::metadata(dir.join(name))
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
    })
}

/// Can this machine extract anything? (`unzip` for zip, `tar` for the rest.)
pub fn extract_available() -> bool {
    on_path("unzip") || on_path("tar") || super::tool::seven_zip().is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Zip,
    Tar,
    Seven,
}

/// The kind of an archive and the length of its ending, from its name.
pub(super) fn classify(name: &str) -> Option<(Kind, usize)> {
    let lower = name.to_lowercase();
    let endings: [(&str, Kind); 14] = [
        (".zip", Kind::Zip),
        (".7z", Kind::Seven),
        (".tar.gz", Kind::Tar),
        (".tar.xz", Kind::Tar),
        (".tar.bz2", Kind::Tar),
        (".tar.zst", Kind::Tar),
        (".tgz", Kind::Tar),
        (".txz", Kind::Tar),
        (".tbz2", Kind::Tar),
        (".tbz", Kind::Tar),
        (".tzst", Kind::Tar),
        (".tar", Kind::Tar),
        (".tar.lz4", Kind::Tar),
        (".tar.z", Kind::Tar),
    ];
    endings
        .iter()
        .find(|(ending, _)| lower.ends_with(ending))
        .map(|(ending, kind)| (*kind, ending.len()))
}

/// Is the program that opens `kind` installed?
fn can_open(kind: Kind) -> bool {
    match kind {
        Kind::Zip => on_path("unzip") || super::tool::seven_zip().is_some(),
        Kind::Tar => on_path("tar"),
        Kind::Seven => super::tool::seven_zip().is_some(),
    }
}

/// Does the name look like an archive that [`extract`] handles, and is the
/// program for it installed? Looks at the name only.
pub fn can_extract(archive: &Path) -> bool {
    archive
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(classify)
        .is_some_and(|(kind, _)| can_open(kind))
}

/// Is one archive entry name safe to unpack? No absolute names, no `..`.
pub(super) fn entry_is_safe(name: &str) -> bool {
    !name.contains('\0')
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.split(['/', '\\']).any(|part| part == "..")
}

/// Does a link at `name` pointing to `target` stay inside the archive's own
/// folder? Both are archive-relative text.
pub(super) fn link_stays_inside(name: &str, target: &str) -> bool {
    if target.starts_with('/') || target.contains('\0') {
        return false;
    }
    let mut depth = name
        .trim_end_matches('/')
        .split('/')
        .count()
        .saturating_sub(1) as i64;
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => depth += 1,
        }
    }
    true
}

/// Read `tar -tv` output and say whether every link stays inside. Lines
/// look like `lrwxrwxrwx u/g 0 date time name -> target` and
/// `hrw-r--r-- u/g 0 date time name link to target`.
pub(super) fn tar_links_are_safe(listing: &str) -> bool {
    listing.lines().all(|line| {
        let split = match line.chars().next() {
            Some('l') => " -> ",
            Some('h') => " link to ",
            _ => return true,
        };
        let Some((head, target)) = line.split_once(split) else {
            return false;
        };
        // Skip mode, owner/group, size, date and time to reach the name.
        let mut name = head;
        for _ in 0..5 {
            name = name.trim_start();
            name = name
                .split_once(char::is_whitespace)
                .map_or("", |(_, rest)| rest);
        }
        !target.contains(split) && link_stays_inside(name.trim_start(), target)
    })
}

/// The archive's name without its ending: `photos.tar.gz` is `photos`.
pub(super) fn folder_name(file_name: &str, ending: usize) -> String {
    let stem = &file_name[..file_name.len() - ending];
    if stem.is_empty() {
        "archive".to_string()
    } else {
        stem.to_string()
    }
}

/// A folder made fresh in `dir` under a hidden name. It becomes the result
/// folder when the archive has no single top folder, so it gets the usual
/// mode for a new folder.
pub(super) fn make_scratch(dir: &Path) -> Result<PathBuf, OpError> {
    for attempt in 0..1000u32 {
        let path = dir.join(format!(".extracting-{}-{attempt}", std::process::id()));
        match fs::DirBuilder::new().create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(OpError::Other("A work folder could not be made.".into()))
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::super::unpack::extract;
    use super::*;

    #[test]
    fn unsafe_entry_names_are_refused() {
        assert!(entry_is_safe("a.txt"));
        assert!(entry_is_safe("dir/sub/a.txt"));
        assert!(entry_is_safe("dir/"));
        assert!(entry_is_safe("..hidden"));
        assert!(entry_is_safe("a..b/c"));
        assert!(entry_is_safe("./a"));
        assert!(!entry_is_safe("../evil"));
        assert!(!entry_is_safe("a/../../evil"));
        assert!(!entry_is_safe("a/.."));
        assert!(!entry_is_safe("/etc/passwd"));
        assert!(!entry_is_safe("..\\evil"));
        assert!(!entry_is_safe("\\evil"));
        assert!(!entry_is_safe("a\0b"));
    }

    #[test]
    fn links_must_stay_inside() {
        assert!(link_stays_inside("bin/tool", "../lib/tool"));
        assert!(link_stays_inside("a", "b"));
        assert!(link_stays_inside("a/b/c", "../../a/x"));
        assert!(!link_stays_inside("a", "../x"));
        assert!(!link_stays_inside("a/b", "../../x"));
        assert!(!link_stays_inside("a", "/etc/passwd"));
        assert!(link_stays_inside("a/b", "c/../d"));
        assert!(!link_stays_inside("a", "x/../../y"));
    }

    #[test]
    fn tar_listings_with_escaping_links_are_refused() {
        let ok = "drwxr-xr-x me/me 0 2026-01-01 10:00 pkg/\n\
                  lrwxrwxrwx me/me 0 2026-01-01 10:00 pkg/bin/tool -> ../lib/tool\n\
                  -rw-r--r-- me/me 5 2026-01-01 10:00 pkg/lib/tool\n";
        assert!(tar_links_are_safe(ok));
        let absolute = "lrwxrwxrwx me/me 0 2026-01-01 10:00 pkg/x -> /etc/passwd\n";
        assert!(!tar_links_are_safe(absolute));
        let climbing = "lrwxrwxrwx me/me 0 2026-01-01 10:00 pkg/x -> ../../outside\n";
        assert!(!tar_links_are_safe(climbing));
        let hard = "hrw-r--r-- me/me 0 2026-01-01 10:00 pkg/y link to ../../outside\n";
        assert!(!tar_links_are_safe(hard));
        let tricky = "lrwxrwxrwx me/me 0 2026-01-01 10:00 pkg/z -> a -> /etc/passwd\n";
        assert!(!tar_links_are_safe(tricky));
    }

    #[test]
    fn archive_kinds_from_names() {
        assert_eq!(classify("a.zip"), Some((Kind::Zip, 4)));
        assert_eq!(classify("A.ZIP"), Some((Kind::Zip, 4)));
        assert_eq!(classify("a.tar.gz"), Some((Kind::Tar, 7)));
        assert_eq!(classify("a.tgz"), Some((Kind::Tar, 4)));
        assert_eq!(classify("a.tar"), Some((Kind::Tar, 4)));
        assert_eq!(classify("a.txt"), None);
        assert_eq!(folder_name("photos.tar.gz", 7), "photos");
        assert_eq!(folder_name(".zip", 4), "archive");
    }

    #[test]
    fn can_extract_reads_the_name() {
        assert!(!can_extract(Path::new("/x/notes.txt")));
        assert_eq!(
            can_extract(Path::new("/x/a.zip")),
            on_path("unzip") || super::super::tool::seven_zip().is_some(),
        );
        assert_eq!(can_extract(Path::new("/x/a.tar.xz")), on_path("tar"));
        assert_eq!(
            can_extract(Path::new("/x/a.7z")),
            super::super::tool::seven_zip().is_some()
        );
        assert_eq!(classify("a.7z"), Some((Kind::Seven, 3)));
    }

    #[test]
    fn extract_refuses_what_it_cannot_read() {
        let temp = Temp::new("extract-bad");
        let text = temp.write("notes.txt", "x");
        let dest = temp.dir("dest");
        assert!(matches!(
            extract(&text, &dest),
            Err(OpError::Unsupported(_))
        ));
        if on_path("tar") {
            let junk = temp.write("junk.tar", "this is not a tar file");
            let error = extract(&junk, &dest).unwrap_err();
            assert!(matches!(error, OpError::Other(_)), "{error:?}");
            assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
        }
        assert_eq!(
            extract(&temp.path().join("missing.zip"), &dest)
                .map_err(|e| matches!(e, OpError::NotFound | OpError::Unsupported(_))),
            Err(true)
        );
    }
}
