//! Paths a person types, and the pieces of a path a breadcrumb shows.
//!
//! Everything here is lexical except [`complete`], which reads one folder.
//! Nothing follows symbolic links, so `..` is resolved on the text, as a
//! shell's `cd -L` does.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::listing::Cancel;
use crate::sort::name_key;

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

/// The name of this computer, from the kernel, for the root of the file system.
/// "Computer" when the kernel gives none.
pub fn host_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_string())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Computer".to_string())
}

/// The crumbs of `path`, outermost first. A path inside `home` starts at the
/// home folder, labelled "Home", as GNOME Files does; any other path starts
/// at the root, labelled with the computer's host name.
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
                label: host_name(),
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
    /// The best matches first: names that start with the prefix as typed,
    /// then ones that start with it ignoring case, then ones that contain it.
    /// Within each group, natural order. At most [`CompleteOptions::limit`].
    pub candidates: Vec<Candidate>,
    /// The longest text every candidate that starts with the prefix starts
    /// with, at least the prefix.
    pub common: String,
    /// How many of `candidates` start with the prefix (as typed or ignoring
    /// case); the rest only contain it.
    pub prefix_matches: usize,
    /// There were more matches than `candidates` holds.
    pub truncated: bool,
    /// The folder's names were not available in time (a slow share): the
    /// list is empty or old. Asking again once it has answered gives the
    /// names; the loader keeps reading in the background.
    pub incomplete: bool,
}

impl Completion {
    /// `input` with as much of the answer filled in as is certain: the common
    /// prefix, and a trailing `/` when the only match is a folder.
    pub fn apply(&self, input: &str) -> String {
        let head = &input[..input.rfind('/').map_or(0, |at| at + 1)];
        let mut text = format!("{head}{}", self.common);
        if self.prefix_matches == 1
            && let Some(only) = self.candidates.first()
            && only.is_dir
        {
            text.push('/');
        }
        text
    }
}

/// How [`Completer::complete`] behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompleteOptions {
    /// Only folders.
    pub dirs_only: bool,
    /// The most candidates returned.
    pub limit: usize,
    /// Offer hidden names even without a typed dot.
    pub show_hidden: bool,
}

impl Default for CompleteOptions {
    fn default() -> Self {
        CompleteOptions {
            dirs_only: false,
            limit: 50,
            show_hidden: false,
        }
    }
}

/// One name of a folder, kept for matching.
struct Name {
    text: String,
    lower: String,
    is_dir: bool,
    /// A symbolic link: whether it is a folder is not known yet.
    link: bool,
    hidden: bool,
}

struct Names {
    items: Vec<Name>,
}

/// How long the names of a folder are reused without looking again. Typing
/// the next letter must not read the folder again.
const REUSE: Duration = Duration::from_secs(10);
/// How long a call waits for a folder's names before saying "incomplete".
const DEADLINE: Duration = Duration::from_millis(400);

type Memo = Vec<(PathBuf, Instant, Arc<Names>)>;
static MEMO: Mutex<Memo> = Mutex::new(Vec::new());

fn memo_get(dir: &Path) -> Option<Arc<Names>> {
    let memo = MEMO.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    memo.iter()
        .find(|(known, at, _)| known == dir && at.elapsed() < REUSE)
        .map(|(_, _, names)| names.clone())
}

fn memo_put(dir: &Path, names: Arc<Names>) {
    let mut memo = MEMO.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    memo.retain(|(known, _, _)| known != dir);
    memo.push((dir.to_path_buf(), Instant::now(), names));
    if memo.len() > 8 {
        memo.remove(0);
    }
}

/// Read the names of `dir`: from the cache when the folder's stamp is
/// unchanged, else with one directory read that asks for no `statx` per
/// entry. Runs on a thread of its own, see [`names_of`].
fn read_names(dir: &Path) -> Option<Names> {
    let cache = crate::cache::Cache::shared();
    let stamp = crate::stat::dir_stamp(dir).ok()?;
    if let Some(cached) = cache.load(dir)
        && cached.stamp == stamp
    {
        let items = cached
            .entries
            .into_iter()
            .map(|entry| Name {
                lower: entry.name.to_lowercase(),
                is_dir: entry.is_dir(),
                link: entry.symlink && !entry.is_dir(),
                hidden: entry.name.starts_with('.'),
                text: entry.name,
            })
            .collect();
        return Some(Names { items });
    }
    let reader = fs::read_dir(dir).ok()?;
    let items = reader
        .filter_map(Result::ok)
        .map(|item| {
            let text = item.file_name().to_string_lossy().into_owned();
            let file_type = item.file_type().ok();
            Name {
                lower: text.to_lowercase(),
                is_dir: file_type.is_some_and(|kind| kind.is_dir()),
                link: file_type.is_some_and(|kind| kind.is_symlink()),
                hidden: text.starts_with('.'),
                text,
            }
        })
        .collect();
    Some(Names { items })
}

/// The names of `dir`, waiting at most [`DEADLINE`] and watching `cancel`.
/// The second value is true when the answer did not come in time; the read
/// carries on and fills the memo for the next call.
fn names_of(dir: &Path, cancel: &Cancel) -> (Option<Arc<Names>>, bool) {
    if let Some(names) = memo_get(dir) {
        return (Some(names), false);
    }
    let (sender, receiver) = mpsc::channel();
    let path = dir.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("bingux-files-names".into())
        .spawn(move || {
            let names = read_names(&path).map(Arc::new);
            if let Some(names) = &names {
                memo_put(&path, names.clone());
            }
            let _ = sender.send(names);
        });
    if spawned.is_err() {
        return (None, true);
    }
    let end = Instant::now() + DEADLINE;
    loop {
        match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(names) => return (names, false),
            Err(mpsc::RecvTimeoutError::Disconnected) => return (None, false),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.is_cancelled() {
                    return (None, false);
                }
                if Instant::now() >= end {
                    return (None, true);
                }
            }
        }
    }
}

/// Suggests completions for a path being typed. Cheap to keep around: the
/// names of the last folders are remembered for a few seconds, so typing the
/// next letter filters in memory and a path bar can call it on every
/// keystroke from a worker.
#[derive(Clone, Debug, Default)]
pub struct Completer;

impl Completer {
    pub fn new() -> Completer {
        Completer
    }

    /// Suggest completions for `input`. `base` is the folder a relative path
    /// starts from; `home` is what `~` means. Blocks for at most 400 ms,
    /// less when `cancel` is set: call it on a worker.
    ///
    /// Only the directory's own listing is read: no entry is `statx`ed
    /// (links to folders are resolved for the candidates returned, except on
    /// a network mount). Hidden names are offered after a typed dot, or with
    /// [`CompleteOptions::show_hidden`].
    pub fn complete(
        &mut self,
        input: &str,
        base: &Path,
        home: &Path,
        options: CompleteOptions,
        cancel: &Cancel,
    ) -> Completion {
        complete_with(input, base, home, options, cancel)
    }
}

fn complete_with(
    input: &str,
    base: &Path,
    home: &Path,
    options: CompleteOptions,
    cancel: &Cancel,
) -> Completion {
    let split = input.rfind('/').map_or(0, |at| at + 1);
    let (head, prefix) = input.split_at(split);
    let dir = if head.is_empty() {
        resolve("", base, home)
    } else {
        resolve(head, base, home)
    };
    let (names, incomplete) = names_of(&dir, cancel);
    let mut completion = Completion {
        dir: dir.clone(),
        prefix: prefix.to_string(),
        candidates: Vec::new(),
        common: prefix.to_string(),
        prefix_matches: 0,
        truncated: false,
        incomplete,
    };
    let Some(names) = names else {
        return completion;
    };
    let lower_prefix = prefix.to_lowercase();
    let allow_hidden = options.show_hidden || prefix.starts_with('.');
    let network = crate::mounts::kind_of(&dir) == crate::mounts::MountKind::Network;
    // (tier, index): tier 0 starts with the prefix as typed, 1 ignoring
    // case, 2 contains it.
    let mut matches: Vec<(u8, usize)> = Vec::new();
    for (index, name) in names.items.iter().enumerate() {
        if name.hidden && !allow_hidden {
            continue;
        }
        let tier = if name.text.starts_with(prefix) {
            0
        } else if name.lower.starts_with(&lower_prefix) {
            1
        } else if name.lower.contains(&lower_prefix) {
            2
        } else {
            continue;
        };
        if options.dirs_only && !name.is_dir && !(name.link && !network) {
            continue;
        }
        matches.push((tier, index));
    }
    // Where the prefix ends the match, longest agreement among the best tier.
    let best_tier = matches.iter().map(|(tier, _)| *tier).min();
    if let Some(tier @ (0 | 1)) = best_tier {
        let common = common_prefix(
            matches
                .iter()
                .filter(|(found, _)| *found == tier)
                .map(|(_, index)| names.items[*index].text.as_str()),
        );
        if let Some(common) = common
            && common.len() >= prefix.len()
        {
            completion.common = common;
        }
    }
    // Rank: tier, then natural order, computed once per name.
    let mut arena: Vec<u8> = Vec::with_capacity(matches.len() * 12);
    let mut keyed: Vec<(u8, u32, u32, usize)> = Vec::with_capacity(matches.len());
    for (tier, index) in &matches {
        let start = arena.len() as u32;
        name_key(&names.items[*index].text, &mut arena);
        keyed.push((*tier, start, arena.len() as u32, *index));
    }
    let order = |a: &(u8, u32, u32, usize), b: &(u8, u32, u32, usize)| {
        a.0.cmp(&b.0)
            .then_with(|| arena[a.1 as usize..a.2 as usize].cmp(&arena[b.1 as usize..b.2 as usize]))
            .then_with(|| names.items[a.3].text.cmp(&names.items[b.3].text))
    };
    let limit = options.limit.max(1);
    // Resolving links can drop candidates, so keep a little extra.
    let wanted = limit.saturating_add(16);
    if keyed.len() > wanted.saturating_mul(4) {
        keyed.select_nth_unstable_by(wanted, order);
        keyed.truncate(wanted);
    }
    keyed.sort_unstable_by(order);
    let mut candidates = Vec::with_capacity(limit.min(keyed.len()));
    let mut prefix_matches = 0;
    let mut dropped = false;
    for (tier, _, _, index) in &keyed {
        let name = &names.items[*index];
        let is_dir = if name.link && !network {
            fs::metadata(dir.join(&name.text)).is_ok_and(|meta| meta.is_dir())
        } else {
            name.is_dir
        };
        if options.dirs_only && !is_dir {
            dropped = true;
            continue;
        }
        if candidates.len() == limit {
            completion.truncated = true;
            break;
        }
        if *tier < 2 {
            prefix_matches += 1;
        }
        candidates.push(Candidate {
            name: name.text.clone(),
            is_dir,
        });
    }
    completion.truncated |= matches.len() > keyed.len() && !dropped;
    completion.candidates = candidates;
    completion.prefix_matches = prefix_matches;
    completion
}

/// Suggest completions for `input`, a path being typed. Only folders when
/// `dirs_only`. Hidden names are offered only after a typed dot. At most 200
/// are returned; use [`Completer`] for a different limit.
pub fn complete(input: &str, base: &Path, home: &Path, dirs_only: bool) -> Completion {
    complete_with(
        input,
        base,
        home,
        CompleteOptions {
            dirs_only,
            limit: 200,
            show_hidden: false,
        },
        &Cancel::new(),
    )
}

/// The folders inside `dir`, hidden ones left out, in natural order, at most
/// `limit`: for the dropdown of a breadcrumb. Answered from the names the
/// last completion or listing left, so opening the dropdown of a folder that
/// was just shown costs nothing, and never waits long on a slow share.
pub fn subfolders(dir: &Path, limit: usize, cancel: &Cancel) -> Vec<Candidate> {
    let (names, _) = names_of(dir, cancel);
    let Some(names) = names else {
        return Vec::new();
    };
    let network = crate::mounts::kind_of(dir) == crate::mounts::MountKind::Network;
    let mut arena: Vec<u8> = Vec::new();
    let mut keyed: Vec<(u32, u32, usize)> = Vec::new();
    for (index, name) in names.items.iter().enumerate() {
        if name.hidden || !(name.is_dir || (name.link && !network)) {
            continue;
        }
        let start = arena.len() as u32;
        name_key(&name.text, &mut arena);
        keyed.push((start, arena.len() as u32, index));
    }
    keyed.sort_unstable_by(|a, b| {
        arena[a.0 as usize..a.1 as usize]
            .cmp(&arena[b.0 as usize..b.1 as usize])
            .then_with(|| names.items[a.2].text.cmp(&names.items[b.2].text))
    });
    let mut out = Vec::new();
    for (_, _, index) in keyed {
        let name = &names.items[index];
        let is_dir = if name.link {
            fs::metadata(dir.join(&name.text)).is_ok_and(|meta| meta.is_dir())
        } else {
            true
        };
        if is_dir {
            out.push(Candidate { name: name.text.clone(), is_dir: true });
            if out.len() == limit {
                break;
            }
        }
    }
    out
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

/// Why a folder could not be created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateError {
    /// Something with that name is already there.
    Exists,
    Permission,
    /// The name cannot be used.
    Invalid(NameError),
    Other(String),
}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateError::Exists => f.write_str("Something with that name already exists."),
            CreateError::Permission => f.write_str("You do not have permission to create a folder here."),
            CreateError::Invalid(error) => error.fmt(f),
            CreateError::Other(detail) => write!(f, "The folder could not be created: {detail}"),
        }
    }
}

impl std::error::Error for CreateError {}

/// Create the folder `name` in `parent` and return its path. `name` is one
/// file name: no slash, and not `.` or `..`. Blocks: call it on a worker.
pub fn create_dir(parent: &Path, name: &str) -> Result<PathBuf, CreateError> {
    let name = name.trim();
    check_file_name(name).map_err(CreateError::Invalid)?;
    if name.contains('/') {
        return Err(CreateError::Invalid(NameError::Invalid));
    }
    let path = parent.join(name);
    match fs::create_dir(&path) {
        Ok(()) => Ok(path),
        Err(error) => Err(match error.kind() {
            std::io::ErrorKind::AlreadyExists => CreateError::Exists,
            std::io::ErrorKind::PermissionDenied => CreateError::Permission,
            _ => CreateError::Other(error.to_string()),
        }),
    }
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
        assert_eq!(outside[0].label, host_name());
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
        assert_eq!(result.prefix_matches, 3);
        let narrow = complete(&format!("{}/Pic", dir.display()), &dir, &dir, true);
        assert_eq!(
            narrow.apply(&format!("{}/Pic", dir.display())),
            format!("{}/Pictures/", dir.display())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    fn names(completion: &Completion) -> Vec<&str> {
        completion.candidates.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn ranks_prefix_then_case_then_substring() {
        let dir = std::env::temp_dir().join(format!("bingux-files-rank-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for name in ["report10.txt", "report2.txt", "Report1.txt", "my-report.txt", "other.txt", ".report-hidden"] {
            fs::write(dir.join(name), b"").unwrap();
        }
        let mut completer = Completer::new();
        let found = completer.complete("report", &dir, &dir, CompleteOptions::default(), &Cancel::new());
        assert_eq!(names(&found), ["report2.txt", "report10.txt", "Report1.txt", "my-report.txt"]);
        assert_eq!(found.prefix_matches, 3);
        assert_eq!(found.common, "report");
        assert!(!found.truncated && !found.incomplete);
        let limited = completer.complete("report", &dir, &dir, CompleteOptions { limit: 2, ..Default::default() }, &Cancel::new());
        assert_eq!(names(&limited), ["report2.txt", "report10.txt"]);
        assert!(limited.truncated);
        let dotted = completer.complete(".rep", &dir, &dir, CompleteOptions::default(), &Cancel::new());
        assert_eq!(names(&dotted), [".report-hidden"]);
        let nothing = completer.complete("zz", &dir, &dir, CompleteOptions::default(), &Cancel::new());
        assert!(nothing.candidates.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_folders_includes_links_to_folders() {
        let dir = std::env::temp_dir().join(format!("bingux-files-dirsonly-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("alpha")).unwrap();
        fs::write(dir.join("alpine.txt"), b"").unwrap();
        std::os::unix::fs::symlink(dir.join("alpha"), dir.join("al-link")).unwrap();
        std::os::unix::fs::symlink(dir.join("alpine.txt"), dir.join("al-file")).unwrap();
        let found = complete(&format!("{}/al", dir.display()), &dir, &dir, true);
        assert_eq!(names(&found), ["al-link", "alpha"]);
        let subs = subfolders(&dir, 10, &Cancel::new());
        assert_eq!(subs.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["al-link", "alpha"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_folders_and_refuses_bad_names() {
        let dir = std::env::temp_dir().join(format!("bingux-files-mkdir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(create_dir(&dir, " New ").unwrap(), dir.join("New"));
        assert_eq!(create_dir(&dir, "New"), Err(CreateError::Exists));
        assert_eq!(create_dir(&dir, ""), Err(CreateError::Invalid(NameError::Empty)));
        assert_eq!(create_dir(&dir, "a/b"), Err(CreateError::Invalid(NameError::Invalid)));
        assert_eq!(create_dir(&dir, ".."), Err(CreateError::Invalid(NameError::Reserved)));
        let _ = fs::remove_dir_all(&dir);
    }
}

