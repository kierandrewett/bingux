//! Paths a person types, and the pieces of a path a breadcrumb shows.
//!
//! Everything here is lexical except [`complete`], which reads one folder.
//! Nothing follows symbolic links, so `..` is resolved on the text, as a
//! shell's `cd -L` does.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::sort::natural_cmp;

/// Resolve `.` and `..` and repeated slashes without touching the disk.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !path.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(if path.has_root() { "/" } else { "." })
    } else {
        out
    }
}

/// What a typed location means: `~` and `~/x` are in `home`, `file:///x` is
/// `/x`, a relative path is in `base`, an empty string is `base`.
pub fn resolve(input: &str, base: &Path, home: &Path) -> PathBuf {
    let text = input.trim();
    if text.is_empty() {
        return base.to_path_buf();
    }
    if let Some(path) = from_file_uri(text) {
        return normalize(&path);
    }
    let path = if text == "~" {
        home.to_path_buf()
    } else if let Some(rest) = text.strip_prefix("~/") {
        home.join(rest)
    } else {
        base.join(text)
    };
    normalize(&path)
}

/// The `file://` URI of an absolute path, percent-encoded, as the portal
/// returns it.
pub fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                uri.push(byte as char);
            }
            other => uri.push_str(&format!("%{other:02X}")),
        }
    }
    uri
}

/// The path of a `file://` URI (with an empty or `localhost` host), or
/// `None` for any other text.
pub fn from_file_uri(text: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let rest = text.strip_prefix("file://")?;
    let path = rest
        .strip_prefix("localhost")
        .unwrap_or(rest)
        .strip_prefix('/')
        .map(|tail| format!("/{tail}"))?;
    Some(PathBuf::from(std::ffi::OsString::from_vec(percent_decode(
        &path,
    ))))
}

/// `%XX` escapes decoded; bad escapes stay as written.
pub fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Some(value) = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    out
}

/// One crumb of a path: what it says and where it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub path: PathBuf,
}

/// The crumbs of `path`, outermost first. A path inside `home` starts at the
/// home folder, labelled "Home", as GNOME Files does; any other path starts
/// at the root, labelled "Computer".
pub fn crumbs(path: &Path, home: &Path) -> Vec<Crumb> {
    let (mut walked, rest, first) = match path.strip_prefix(home) {
        Ok(rest) if home.has_root() && home != Path::new("/") => (
            home.to_path_buf(),
            rest.to_path_buf(),
            Crumb {
                label: "Home".into(),
                path: home.to_path_buf(),
            },
        ),
        _ => (
            PathBuf::from("/"),
            path.strip_prefix("/").unwrap_or(path).to_path_buf(),
            Crumb {
                label: "Computer".into(),
                path: PathBuf::from("/"),
            },
        ),
    };
    let mut crumbs = vec![first];
    for part in rest.components() {
        walked.push(part.as_os_str());
        crumbs.push(Crumb {
            label: part.as_os_str().to_string_lossy().into_owned(),
            path: walked.clone(),
        });
    }
    crumbs
}

/// Why a file name cannot be saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    Empty,
    /// `.` or `..`.
    Reserved,
    TooLong,
    /// A NUL byte.
    Invalid,
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NameError::Empty => "Enter a name for the file.",
            NameError::Reserved => "A file cannot be called . or ..",
            NameError::TooLong => "The name is longer than 255 bytes.",
            NameError::Invalid => "The name contains a character that is not allowed.",
        })
    }
}

/// Check one file name (no slash). Linux allows everything else.
pub fn check_file_name(name: &str) -> Result<(), NameError> {
    if name.trim().is_empty() {
        Err(NameError::Empty)
    } else if name == "." || name == ".." {
        Err(NameError::Reserved)
    } else if name.len() > 255 {
        Err(NameError::TooLong)
    } else if name.contains('\0') {
        Err(NameError::Invalid)
    } else {
        Ok(())
    }
}

/// One suggestion for a half-typed path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub is_dir: bool,
}

/// The suggestions for a half-typed path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// The folder that was searched.
    pub dir: PathBuf,
    /// The part of the last component that was typed.
    pub prefix: String,
    pub candidates: Vec<Candidate>,
    /// The longest text every candidate starts with, at least the prefix.
    pub common: String,
}

impl Completion {
    /// `input` with as much of the answer filled in as is certain: the common
    /// prefix, and a trailing `/` when the only candidate is a folder.
    pub fn apply(&self, input: &str) -> String {
        let head = &input[..input.rfind('/').map_or(0, |at| at + 1)];
        let mut text = format!("{head}{}", self.common);
        if let [only] = self.candidates.as_slice()
            && only.is_dir
        {
            text.push('/');
        }
        text
    }
}

/// Suggest completions for `input`, a path being typed. Only folders when
/// `dirs_only`. Hidden names are offered only after a typed dot. Matching is
/// exact first and, when nothing matches, ignores case.
pub fn complete(input: &str, base: &Path, home: &Path, dirs_only: bool) -> Completion {
    let split = input.rfind('/').map_or(0, |at| at + 1);
    let (head, prefix) = input.split_at(split);
    let dir = if head.is_empty() {
        resolve("", base, home)
    } else {
        resolve(head, base, home)
    };
    let mut all: Vec<Candidate> = fs::read_dir(&dir)
        .map(|reader| {
            reader
                .filter_map(Result::ok)
                .filter_map(|item| {
                    let name = item.file_name().to_string_lossy().into_owned();
                    let is_dir = fs::metadata(item.path())
                        .map(|m| m.is_dir())
                        .unwrap_or(false);
                    (!dirs_only || is_dir).then_some(Candidate { name, is_dir })
                })
                .filter(|candidate| prefix.starts_with('.') || !candidate.name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    let mut candidates: Vec<Candidate> = all
        .iter()
        .filter(|candidate| candidate.name.starts_with(prefix))
        .cloned()
        .collect();
    if candidates.is_empty() {
        let lower = prefix.to_lowercase();
        candidates = all
            .drain(..)
            .filter(|candidate| candidate.name.to_lowercase().starts_with(&lower))
            .collect();
    }
    candidates.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    let common = common_prefix(candidates.iter().map(|c| c.name.as_str()))
        .filter(|common| common.len() >= prefix.len())
        .unwrap_or_else(|| prefix.to_string());
    Completion {
        dir,
        prefix: prefix.to_string(),
        candidates,
        common,
    }
}

fn common_prefix<'a>(mut names: impl Iterator<Item = &'a str>) -> Option<String> {
    let first = names.next()?;
    let mut length = first.len();
    for name in names {
        length = first
            .char_indices()
            .zip(name.chars())
            .take_while(|((_, a), b)| a == b)
            .map(|((at, a), _)| at + a.len_utf8())
            .last()
            .unwrap_or(0)
            .min(length);
    }
    Some(first[..length].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_typed_paths() {
        let base = Path::new("/home/a/Pictures");
        let home = Path::new("/home/a");
        assert_eq!(resolve("", base, home), base);
        assert_eq!(resolve("~", base, home), home);
        assert_eq!(
            resolve("~/Music/../Videos", base, home),
            Path::new("/home/a/Videos")
        );
        assert_eq!(resolve("../x/./y", base, home), Path::new("/home/a/x/y"));
        assert_eq!(resolve("/etc//ssh/", base, home), Path::new("/etc/ssh"));
        assert_eq!(
            resolve("file:///tmp/a%20b", base, home),
            Path::new("/tmp/a b")
        );
        assert_eq!(resolve("/../..", base, home), Path::new("/"));
    }

    #[test]
    fn uris_round_trip() {
        let path = Path::new("/tmp/a b/é.txt");
        assert_eq!(from_file_uri(&file_uri(path)).as_deref(), Some(path));
        assert_eq!(from_file_uri("http://x"), None);
    }

    #[test]
    fn crumbs_start_at_home_or_root() {
        let home = Path::new("/home/a");
        let inside: Vec<_> = crumbs(Path::new("/home/a/Pictures/x"), home)
            .into_iter()
            .map(|c| c.label)
            .collect();
        assert_eq!(inside, ["Home", "Pictures", "x"]);
        let outside = crumbs(Path::new("/etc/ssh"), home);
        assert_eq!(outside[0].label, "Computer");
        assert_eq!(outside[2].path, Path::new("/etc/ssh"));
    }

    #[test]
    fn names_are_checked() {
        assert_eq!(check_file_name(" "), Err(NameError::Empty));
        assert_eq!(check_file_name(".."), Err(NameError::Reserved));
        assert!(check_file_name("report.pdf").is_ok());
    }

    #[test]
    fn completes_a_prefix() {
        let dir =
            std::env::temp_dir().join(format!("bingux-files-complete-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Pictures")).unwrap();
        fs::create_dir_all(dir.join("Photos")).unwrap();
        fs::write(dir.join("Pdf.txt"), b"").unwrap();
        let typed = format!("{}/P", dir.display());
        let result = complete(&typed, &dir, &dir, false);
        assert_eq!(result.candidates.len(), 3);
        assert_eq!(result.common, "P");
        let narrow = complete(&format!("{}/Pic", dir.display()), &dir, &dir, true);
        assert_eq!(
            narrow.apply(&format!("{}/Pic", dir.display())),
            format!("{}/Pictures/", dir.display())
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
