//! Zip and tar archives, through the `zip`, `unzip` and `tar` programs.
//!
//! Nothing here reads archive formats itself. The programs are looked for on
//! `PATH`; when one is missing the answer is [`OpError::Unsupported`] and
//! [`compress_available`] / [`extract_available`] say so up front, so a menu
//! can hide the item.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};

use super::durable::sync_written;
use super::names::{keep_both_name, keep_both_with};
use super::{OpError, rename_noreplace};

/// An archive kind [`compress`] can make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Zip,
    TarGz,
}

impl Format {
    /// The file name ending, with its dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Zip => ".zip",
            Format::TarGz => ".tar.gz",
        }
    }

    fn tool(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::TarGz => "tar",
        }
    }
}

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

/// Can this machine make `format` archives?
pub fn compress_available(format: Format) -> bool {
    on_path(format.tool())
}

/// Can this machine extract anything? (`unzip` for zip, `tar` for the rest.)
pub fn extract_available() -> bool {
    on_path("unzip") || on_path("tar")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Zip,
    Tar,
}

/// The kind of an archive and the length of its ending, from its name.
fn classify(name: &str) -> Option<(Kind, usize)> {
    let lower = name.to_lowercase();
    let endings: [(&str, Kind); 13] = [
        (".zip", Kind::Zip),
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

fn extractor_for(kind: Kind) -> &'static str {
    match kind {
        Kind::Zip => "unzip",
        Kind::Tar => "tar",
    }
}

/// Does the name look like an archive that [`extract`] handles, and is the
/// program for it installed? Looks at the name only.
pub fn can_extract(archive: &Path) -> bool {
    archive
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(classify)
        .is_some_and(|(kind, _)| on_path(extractor_for(kind)))
}

fn missing_tool(tool: &str) -> OpError {
    OpError::Unsupported(format!("{tool} is not installed."))
}

fn run(command: &mut Command, tool: &str) -> Result<Output, OpError> {
    command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => missing_tool(tool),
            _ => error.into(),
        })
}

/// The first line a program complained with, as a sentence.
fn complaint(what: &str, output: &Output) -> OpError {
    let text = String::from_utf8_lossy(&output.stderr);
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("The program gave no reason");
    let line = line.trim_end_matches('.');
    OpError::Other(format!("{what}: {line}."))
}

/// The deepest folder that holds every path.
fn common_parent(paths: &[PathBuf]) -> PathBuf {
    let mut parents = paths.iter().filter_map(|path| path.parent());
    let Some(first) = parents.next() else {
        return PathBuf::new();
    };
    let mut common: Vec<Component<'_>> = first.components().collect();
    for parent in parents {
        let keep = common
            .iter()
            .zip(parent.components())
            .take_while(|(a, b)| *a == b)
            .count();
        common.truncate(keep);
    }
    common.iter().collect()
}

/// Archive `paths` into the file `dest`. Names inside the archive are
/// relative to the folder the paths share, never absolute. The archive is
/// not overwritten: if `dest` is taken, the next "(copy)" name is used. A
/// missing extension is added.
///
/// Returns the path of the archive that was made.
pub fn compress(paths: &[PathBuf], dest: &Path, format: Format) -> Result<PathBuf, OpError> {
    if !compress_available(format) {
        return Err(missing_tool(format.tool()));
    }
    if paths.is_empty() {
        return Err(OpError::Other("There is nothing to compress.".into()));
    }
    if paths.iter().any(|path| !path.is_absolute()) {
        return Err(OpError::Other("The paths must be absolute.".into()));
    }
    let parent = common_parent(paths);
    let mut names = Vec::with_capacity(paths.len());
    for path in paths {
        path.symlink_metadata()?;
        let relative = path
            .strip_prefix(&parent)
            .map_err(|_| OpError::Other("The paths have nothing in common.".into()))?;
        if relative.as_os_str().is_empty() {
            return Err(OpError::Other(
                "The root folder cannot be compressed.".into(),
            ));
        }
        names.push(relative.to_path_buf());
    }
    let dest_dir = dest
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .ok_or_else(|| OpError::Other("The archive needs a folder to go in.".into()))?;
    let mut file_name = dest
        .file_name()
        .ok_or_else(|| OpError::Other("The archive needs a name.".into()))?
        .to_string_lossy()
        .into_owned();
    if !file_name.to_lowercase().ends_with(format.extension()) {
        file_name.push_str(format.extension());
    }
    if dest_dir.join(&file_name).symlink_metadata().is_ok() {
        file_name = keep_both_name(&file_name, |name| {
            dest_dir.join(name).symlink_metadata().is_ok()
        });
    }
    let archive = dest_dir.join(&file_name);
    let canonical_dest = fs::canonicalize(dest_dir)?;
    for path in paths {
        if path.is_dir()
            && !path.is_symlink()
            && canonical_dest.starts_with(fs::canonicalize(path)?)
        {
            return Err(OpError::Other(
                "The archive cannot be saved inside a folder it contains.".into(),
            ));
        }
    }

    // The tool writes a part file, and the archive gets its name only when the
    // tool has finished. The part keeps the extension, or `zip` adds one.
    let part = dest_dir.join(format!(
        ".bingux-{}-compress.part{}",
        std::process::id(),
        format.extension()
    ));
    let mut command = Command::new(format.tool());
    command.current_dir(&parent);
    match format {
        Format::Zip => {
            command.args(["-r", "-y", "-q"]).arg(&part).arg("--");
        }
        Format::TarGz => {
            command.arg("-czf").arg(&part).arg("--");
        }
    }
    command.args(&names);
    let output = run(&mut command, format.tool());
    let made = match &output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(complaint("The archive could not be made", output)),
        Err(error) => Err(error.clone()),
    };
    if let Err(error) = made {
        let _ = fs::remove_file(&part);
        return Err(error);
    }
    // The part file is complete: make it durable, then give it its name.
    if let Err(error) = sync_written(&part).and_then(|()| {
        rename_noreplace(&part, &archive).map_err(OpError::from)
    }) {
        let _ = fs::remove_file(&part);
        return Err(error);
    }
    sync_written(&archive)?;
    Ok(archive)
}

/// Is one archive entry name safe to unpack? No absolute names, no `..`.
fn entry_is_safe(name: &str) -> bool {
    !name.contains('\0')
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.split(['/', '\\']).any(|part| part == "..")
}

/// Does a link at `name` pointing to `target` stay inside the archive's own
/// folder? Both are archive-relative text.
fn link_stays_inside(name: &str, target: &str) -> bool {
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
fn tar_links_are_safe(listing: &str) -> bool {
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

fn list_entries(kind: Kind, archive: &Path) -> Result<Vec<String>, OpError> {
    let output = match kind {
        Kind::Zip => run(
            Command::new("unzip").arg("-Z1").arg("--").arg(archive),
            "unzip",
        )?,
        Kind::Tar => run(Command::new("tar").arg("-tf").arg(archive), "tar")?,
    };
    if !output.status.success() {
        return Err(complaint("The archive could not be read", &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

/// The archive's name without its ending: `photos.tar.gz` is `photos`.
fn folder_name(file_name: &str, ending: usize) -> String {
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
fn make_scratch(dir: &Path) -> Result<PathBuf, OpError> {
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

/// Move `from` to `dir/name`, or to the next free "(copy)" name. Never
/// replaces anything.
fn place_unique(from: &Path, dir: &Path, name: &str) -> Result<PathBuf, OpError> {
    let taken = |candidate: &str| dir.join(candidate).symlink_metadata().is_ok();
    for _ in 0..1000 {
        let candidate = if taken(name) {
            keep_both_with(name, "", taken)
        } else {
            name.to_string()
        };
        let to = dir.join(&candidate);
        match rename_noreplace(from, &to) {
            Ok(()) => return Ok(to),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(OpError::Exists)
}

/// Extract a zip or tar archive (`.zip`, `.tar`, `.tar.gz`, `.tgz`,
/// `.tar.xz`, `.tar.bz2`, `.tar.zst` and their short forms) into `dest_dir`.
///
/// The contents land in a new folder named after the archive, or, when the
/// archive has a single folder at its top, that folder itself: nothing is
/// unpacked into `dest_dir` loose and nothing is overwritten. The archive is
/// listed first and refused whole if any entry is absolute or climbs out
/// with `..`, or if a link in a tar points outside.
///
/// Returns the new folder.
pub fn extract(archive: &Path, dest_dir: &Path) -> Result<PathBuf, OpError> {
    let file_name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| OpError::Other("That is not an archive.".into()))?;
    let (kind, ending) = classify(file_name).ok_or_else(|| {
        OpError::Unsupported("This kind of archive cannot be extracted here.".into())
    })?;
    let tool = extractor_for(kind);
    if !on_path(tool) {
        return Err(missing_tool(tool));
    }
    fs::metadata(archive)?;
    if !fs::metadata(dest_dir)?.is_dir() {
        return Err(OpError::Other("The destination is not a folder.".into()));
    }

    let unsafe_archive = || {
        OpError::Other(
            "The archive has files that would be put outside its folder, so it was not extracted."
                .into(),
        )
    };
    if !list_entries(kind, archive)?
        .iter()
        .all(|name| entry_is_safe(name))
    {
        return Err(unsafe_archive());
    }
    if kind == Kind::Tar {
        let output = run(Command::new("tar").arg("-tvf").arg(archive), "tar")?;
        if !output.status.success() {
            return Err(complaint("The archive could not be read", &output));
        }
        if !tar_links_are_safe(&String::from_utf8_lossy(&output.stdout)) {
            return Err(unsafe_archive());
        }
    }

    let scratch = make_scratch(dest_dir)?;
    let output = match kind {
        Kind::Zip => run(
            Command::new("unzip")
                .args(["-q", "-n"])
                .arg(archive)
                .arg("-d")
                .arg(&scratch),
            "unzip",
        ),
        Kind::Tar => run(
            Command::new("tar")
                .arg("-xf")
                .arg(archive)
                .arg("-C")
                .arg(&scratch)
                .arg("--no-same-owner"),
            "tar",
        ),
    };
    let failure = match &output {
        // unzip exits 1 for warnings that left everything extracted.
        Ok(output)
            if output.status.success() || output.status.code() == Some(1) && kind == Kind::Zip =>
        {
            None
        }
        Ok(output) => Some(complaint("The archive could not be extracted", output)),
        Err(error) => Some(error.clone()),
    };
    if let Some(error) = failure {
        let _ = fs::remove_dir_all(&scratch);
        return Err(error);
    }

    let result = settle(&scratch, dest_dir, &folder_name(file_name, ending));
    let _ = fs::remove_dir_all(&scratch);
    let folder = result?;
    sync_written(&folder)?;
    Ok(folder)
}

/// Move the unpacked contents of `scratch` to their final place in
/// `dest_dir`: the single folder inside it, or `scratch` itself renamed.
fn settle(scratch: &Path, dest_dir: &Path, archive_folder: &str) -> Result<PathBuf, OpError> {
    let mut top = fs::read_dir(scratch)?.collect::<Result<Vec<_>, _>>()?;
    if top.len() == 1 {
        let only = top.remove(0);
        if only.file_type()?.is_dir() {
            let name = only.file_name().to_string_lossy().into_owned();
            return place_unique(&only.path(), dest_dir, &name);
        }
    }
    place_unique(scratch, dest_dir, archive_folder)
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
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
            on_path("unzip"),
            "zip needs unzip"
        );
        assert_eq!(can_extract(Path::new("/x/a.tar.xz")), on_path("tar"));
    }

    #[test]
    fn common_parents() {
        let paths = [PathBuf::from("/a/b/c"), PathBuf::from("/a/b/d/e")];
        assert_eq!(common_parent(&paths), PathBuf::from("/a/b"));
        let paths = [PathBuf::from("/a/x"), PathBuf::from("/b/y")];
        assert_eq!(common_parent(&paths), PathBuf::from("/"));
        let paths = [PathBuf::from("/a/b/c")];
        assert_eq!(common_parent(&paths), PathBuf::from("/a/b"));
    }

    #[test]
    fn format_extensions() {
        assert_eq!(Format::Zip.extension(), ".zip");
        assert_eq!(Format::TarGz.extension(), ".tar.gz");
    }

    fn listing(archive: &Path, format: Format) -> Vec<String> {
        let kind = match format {
            Format::Zip => Kind::Zip,
            Format::TarGz => Kind::Tar,
        };
        let mut names = list_entries(kind, archive).unwrap();
        names.sort();
        names
    }

    #[test]
    fn zip_round_trip() {
        if !compress_available(Format::Zip) || !can_extract(Path::new("a.zip")) {
            eprintln!("skipped: zip or unzip is not installed");
            return;
        }
        let temp = Temp::new("zip");
        temp.write("src/proj/a.txt", "alpha");
        temp.write("src/proj/sub/b.txt", "beta");
        let loose = temp.write("src/loose.txt", "loose");
        let archive = compress(
            &[temp.path().join("src/proj"), loose],
            &temp.path().join("out.zip"),
            Format::Zip,
        )
        .unwrap();
        assert_eq!(archive, temp.path().join("out.zip"));
        let names = listing(&archive, Format::Zip);
        assert!(
            names
                .iter()
                .all(|name| !name.starts_with('/') && !name.contains("src/")),
            "{names:?}"
        );
        assert!(names.contains(&"proj/sub/b.txt".to_string()), "{names:?}");
        assert!(names.contains(&"loose.txt".to_string()), "{names:?}");

        // A second archive does not overwrite the first.
        let second = compress(
            &[temp.path().join("src/proj")],
            &temp.path().join("out.zip"),
            Format::Zip,
        )
        .unwrap();
        assert_eq!(second, temp.path().join("out (copy).zip"));

        let dest = temp.dir("unpacked");
        let folder = extract(&archive, &dest).unwrap();
        assert_eq!(folder, dest.join("out"));
        assert_eq!(
            fs::read_to_string(folder.join("proj/a.txt")).unwrap(),
            "alpha"
        );
        assert_eq!(
            fs::read_to_string(folder.join("loose.txt")).unwrap(),
            "loose"
        );

        // Extracting again makes a second folder.
        let again = extract(&archive, &dest).unwrap();
        assert_eq!(again, dest.join("out (copy)"));
        assert_eq!(
            fs::read_dir(&dest).unwrap().count(),
            2,
            "no scratch folder is left behind"
        );
    }

    #[test]
    fn a_single_top_level_folder_is_not_wrapped_again() {
        if !compress_available(Format::TarGz) {
            eprintln!("skipped: tar is not installed");
            return;
        }
        let temp = Temp::new("tar");
        temp.write("src/proj/a.txt", "alpha");
        let archive = compress(
            &[temp.path().join("src/proj")],
            &temp.path().join("bundle"),
            Format::TarGz,
        )
        .unwrap();
        assert_eq!(archive, temp.path().join("bundle.tar.gz"));
        let dest = temp.dir("unpacked");
        let folder = extract(&archive, &dest).unwrap();
        assert_eq!(folder, dest.join("proj"));
        assert_eq!(fs::read_to_string(folder.join("a.txt")).unwrap(), "alpha");
        // A folder already called proj is not touched.
        let again = extract(&archive, &dest).unwrap();
        assert_eq!(again, dest.join("proj (copy)"));
        assert_eq!(fs::read_to_string(folder.join("a.txt")).unwrap(), "alpha");
    }

    #[test]
    fn a_zip_slip_tar_is_refused_and_nothing_is_written() {
        if !on_path("tar") {
            eprintln!("skipped: tar is not installed");
            return;
        }
        let temp = Temp::new("slip");
        temp.write("inner/ok.txt", "ok");
        temp.write("evil.txt", "evil");
        let sub = temp.dir("inner/deep");
        let archive = temp.path().join("slip.tar");
        // -P keeps the `..` that tar would otherwise strip.
        let status = Command::new("tar")
            .arg("-cPf")
            .arg(&archive)
            .arg("-C")
            .arg(&sub)
            .arg("../ok.txt")
            .status()
            .unwrap();
        assert!(status.success());
        let dest = temp.dir("dest");
        let error = extract(&archive, &dest).unwrap_err();
        assert!(
            matches!(&error, OpError::Other(text) if text.contains("outside")),
            "{error:?}"
        );
        assert_eq!(
            fs::read_dir(&dest).unwrap().count(),
            0,
            "nothing was extracted"
        );
    }

    #[test]
    fn a_tar_with_an_escaping_link_is_refused() {
        if !on_path("tar") {
            eprintln!("skipped: tar is not installed");
            return;
        }
        let temp = Temp::new("slip-link");
        let work = temp.dir("work");
        std::os::unix::fs::symlink("/etc", work.join("etc-link")).unwrap();
        let archive = temp.path().join("link.tar");
        let status = Command::new("tar")
            .arg("-cf")
            .arg(&archive)
            .arg("-C")
            .arg(&work)
            .arg("etc-link")
            .status()
            .unwrap();
        assert!(status.success());
        let dest = temp.dir("dest");
        assert!(matches!(extract(&archive, &dest), Err(OpError::Other(_))));
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
    }

    #[test]
    fn compress_refuses_bad_requests() {
        let temp = Temp::new("compress-bad");
        let dir = temp.dir("d");
        if compress_available(Format::TarGz) {
            assert!(matches!(
                compress(&[], &temp.path().join("x.tar.gz"), Format::TarGz),
                Err(OpError::Other(_))
            ));
            assert!(matches!(
                compress(
                    &[PathBuf::from("relative")],
                    &temp.path().join("x.tar.gz"),
                    Format::TarGz
                ),
                Err(OpError::Other(_))
            ));
            assert_eq!(
                compress(
                    &[temp.path().join("missing")],
                    &temp.path().join("x.tar.gz"),
                    Format::TarGz
                ),
                Err(OpError::NotFound)
            );
            let error = compress(&[dir.clone()], &dir.join("x.tar.gz"), Format::TarGz).unwrap_err();
            assert!(
                matches!(&error, OpError::Other(text) if text.contains("inside")),
                "{error:?}"
            );
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        }
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
