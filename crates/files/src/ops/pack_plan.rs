//! What making an archive takes, decided without touching a disk: the
//! formats, the options and what they do to each program's command line, the
//! name, the exclude patterns, how the programs' output reads.
//!
//! Everything here is pure, so the rules are tested to the argument: which
//! flag a level becomes, that a password is never an argument, what a split
//! size is called to each program. [`super::pack`] runs the plans.
//!
//! # Programs
//!
//! | Format | Made by | Password | Split |
//! | ------ | ------- | -------- | ----- |
//! | `.zip` | `zip`; `7z` when there is a password | `7z`, AES-256 | `zip -s`, `7z -v` |
//! | `.tar`, `.tar.gz`, `.tar.xz`, `.tar.zst`, `.tar.bz2` | `tar`, piped through `gzip`, `xz`, `zstd` or `bzip2` | no | no |
//! | `.7z` | `7z` | yes, names too | `7z -v` |
//!
//! `zip` cannot take a password except on a command line (`-P`, which every
//! user on the machine can read) or from a terminal, so a zip with a password
//! is made by 7-Zip, which reads it from standard input and encrypts with
//! AES-256. Without 7-Zip, the password is not offered for zip.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use super::OpError;
use super::tool::{self, Input, Spec};

/// An archive kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Zip,
    TarGz,
    TarXz,
    TarZst,
    TarBz2,
    SevenZip,
    Tar,
}

impl Format {
    /// In the order a dropdown lists them: the most compatible first.
    pub const ALL: [Format; 7] = [
        Format::Zip,
        Format::TarGz,
        Format::TarXz,
        Format::TarZst,
        Format::TarBz2,
        Format::SevenZip,
        Format::Tar,
    ];

    /// The file name ending, with its dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Zip => ".zip",
            Format::TarGz => ".tar.gz",
            Format::TarXz => ".tar.xz",
            Format::TarZst => ".tar.zst",
            Format::TarBz2 => ".tar.bz2",
            Format::SevenZip => ".7z",
            Format::Tar => ".tar",
        }
    }

    /// A short word for the choice: ".zip".
    pub fn label(self) -> &'static str {
        self.extension()
    }

    /// Why a person would pick it, in a few words.
    pub fn note(self) -> &'static str {
        match self {
            Format::Zip => "Works everywhere",
            Format::TarGz => "Fast, common on Linux and Mac",
            Format::TarXz => "Smaller, slower, Linux and Mac",
            Format::TarZst => "Fast and small, newer systems",
            Format::TarBz2 => "Smaller than gzip, slower",
            Format::SevenZip => "Smallest, needs 7-Zip to open",
            Format::Tar => "No compression, keeps permissions",
        }
    }

    /// The format of a file name, by its ending.
    pub fn from_name(name: &str) -> Option<Format> {
        let lower = name.to_lowercase();
        Format::ALL
            .into_iter()
            .find(|format| lower.ends_with(format.extension()))
    }

    /// Does the format have a compression level to choose?
    pub fn has_levels(self) -> bool {
        self != Format::Tar
    }

    /// Can the archive be locked with a password?
    pub fn can_lock(self) -> bool {
        matches!(self, Format::Zip | Format::SevenZip)
    }

    /// Can the names inside be hidden too?
    pub fn can_hide_names(self) -> bool {
        self == Format::SevenZip
    }

    /// Can the archive be cut into volumes?
    pub fn can_split(self) -> bool {
        matches!(self, Format::Zip | Format::SevenZip)
    }

    /// Does the archive record who owns each file?
    pub fn is_tar(self) -> bool {
        !matches!(self, Format::Zip | Format::SevenZip)
    }

    /// The program that squeezes a tar, if it is not plain.
    fn compressor(self) -> Option<&'static str> {
        match self {
            Format::TarGz => Some("gzip"),
            Format::TarXz => Some("xz"),
            Format::TarZst => Some("zstd"),
            Format::TarBz2 => Some("bzip2"),
            _ => None,
        }
    }

    /// Why this machine cannot make the format, or `None` when it can.
    pub fn unavailable(self) -> Option<String> {
        let missing = |name: &str| Some(format!("{name} is not installed."));
        match self {
            Format::Zip => {
                if tool::on_path("zip") || tool::seven_zip().is_some() {
                    None
                } else {
                    missing("zip")
                }
            }
            Format::SevenZip => {
                if tool::seven_zip().is_some() {
                    None
                } else {
                    missing("7-Zip")
                }
            }
            tar_kind => {
                if !tool::on_path("tar") {
                    return missing("tar");
                }
                match tar_kind.compressor() {
                    Some(program) if !tool::on_path(program) => missing(program),
                    _ => None,
                }
            }
        }
    }

    /// Can this machine make the format?
    pub fn is_available(self) -> bool {
        self.unavailable().is_none()
    }

    /// Why a password cannot be used with the format here, or `None`.
    pub fn lock_unavailable(self) -> Option<String> {
        if !self.can_lock() {
            return Some("This format cannot be locked with a password.".into());
        }
        if tool::seven_zip().is_none() {
            return Some("A password needs 7-Zip, which is not installed.".into());
        }
        None
    }

    /// Why volumes cannot be made here, or `None`.
    pub fn split_unavailable(self) -> Option<String> {
        match self {
            Format::Zip if tool::on_path("zip") || tool::seven_zip().is_some() => None,
            Format::SevenZip if tool::seven_zip().is_some() => None,
            Format::Zip | Format::SevenZip => {
                Some("The program for this format is not installed.".into())
            }
            _ => Some("Only .zip and .7z can be cut into volumes.".into()),
        }
    }
}

/// How hard to squeeze.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    Fastest,
    Balanced,
    Smallest,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Fastest, Level::Balanced, Level::Smallest];

    pub fn label(self) -> &'static str {
        match self {
            Level::Fastest => "Fastest",
            Level::Balanced => "Balanced",
            Level::Smallest => "Smallest",
        }
    }

    /// The number `zip`, `gzip`, `xz` and `bzip2` take: 1, 6 or 9.
    fn digit(self) -> &'static str {
        match self {
            Level::Fastest => "1",
            Level::Balanced => "6",
            Level::Smallest => "9",
        }
    }

    fn zstd(self) -> &'static str {
        match self {
            Level::Fastest => "-1",
            Level::Balanced => "-3",
            Level::Smallest => "-19",
        }
    }

    fn seven(self) -> &'static str {
        match self {
            Level::Fastest => "-mx=1",
            Level::Balanced => "-mx=5",
            Level::Smallest => "-mx=9",
        }
    }
}

/// A password. It prints as nothing, so a log or a panic message that shows
/// the options never shows it.
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    pub fn new(text: impl Into<String>) -> Password {
        Password(text.into())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// What goes to a program's standard input: the password asked for and
    /// asked for again.
    fn prompt_input(&self) -> Input {
        Input::Bytes(format!("{0}\n{0}\n", self.0).into_bytes())
    }

    /// What goes to a program that asks once.
    pub(super) fn input_once(&self) -> Input {
        Input::Bytes(format!("{}\n", self.0).into_bytes())
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(..)")
    }
}

/// The sizes a volume can be, in MiB. `None` is one file.
pub const SPLIT_SIZES_MIB: [u64; 5] = [100, 650, 700, 2048, 4095];

/// The words for a volume size: "100 MB", "2 GB".
pub fn split_label(mib: u64) -> String {
    if mib >= 1024 && mib % 1024 == 0 {
        format!("{} GB", mib / 1024)
    } else {
        format!("{mib} MB")
    }
}

/// Everything that decides what an archive holds and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackOptions {
    pub format: Format,
    pub level: Level,
    /// Files and folders whose name starts with a dot.
    pub include_hidden: bool,
    /// Store what a link points to rather than the link.
    pub follow_links: bool,
    /// A tar records who owns each file; off, it records nobody.
    pub keep_owner: bool,
    pub excludes: Excludes,
    pub password: Option<Password>,
    /// Hide the names inside as well (`.7z`).
    pub hide_names: bool,
    /// Volume size in MiB.
    pub split_mib: Option<u64>,
    /// Put the originals in the Trash once the archive is checked.
    pub delete_originals: bool,
}

impl Default for PackOptions {
    fn default() -> Self {
        PackOptions {
            format: Format::Zip,
            level: Level::Balanced,
            include_hidden: true,
            follow_links: false,
            keep_owner: true,
            excludes: Excludes::default(),
            password: None,
            hide_names: false,
            split_mib: None,
            delete_originals: false,
        }
    }
}

impl PackOptions {
    /// The options a format can really use: a level for a tar with none,
    /// a password for a format that has no lock, volumes where there are none
    /// are dropped.
    pub fn normalised(mut self) -> PackOptions {
        if !self.format.can_lock() || self.password.as_ref().is_some_and(Password::is_empty) {
            self.password = None;
        }
        if !self.format.can_hide_names() || self.password.is_none() {
            self.hide_names = false;
        }
        if !self.format.can_split() {
            self.split_mib = None;
        }
        self
    }

    /// The program that makes the archive.
    pub fn maker(&self) -> Result<PathBuf, OpError> {
        let missing = |name: &str| OpError::Unsupported(format!("{name} is not installed."));
        match self.format {
            Format::Zip if self.password.is_some() => {
                tool::seven_zip().ok_or_else(|| missing("7-Zip"))
            }
            Format::Zip => tool::find("zip")
                .or_else(tool::seven_zip)
                .ok_or_else(|| missing("zip")),
            Format::SevenZip => tool::seven_zip().ok_or_else(|| missing("7-Zip")),
            _ => tool::find("tar").ok_or_else(|| missing("tar")),
        }
    }
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

/// The longest a file name can be on the file systems that matter.
pub const NAME_LIMIT: usize = 255;

/// What is wrong with a typed archive name, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameCheck {
    Ok,
    Empty,
    HasSlash,
    Reserved,
    TooLong,
    /// Something with that name is in the folder.
    Taken,
}

impl NameCheck {
    /// The sentence for the field's inline error, or `None` when fine.
    pub fn message(self) -> Option<&'static str> {
        match self {
            NameCheck::Ok => None,
            NameCheck::Empty => Some("Enter a name for the archive."),
            NameCheck::HasSlash => Some("A name cannot contain a slash."),
            NameCheck::Reserved => Some("That name cannot be used."),
            NameCheck::TooLong => Some("That name is too long."),
            NameCheck::Taken => Some("A file with this name already exists here."),
        }
    }

    /// Can the archive be made with the name as it is? A taken name can,
    /// once the person picks keep both or replace.
    pub fn is_usable(self) -> bool {
        matches!(self, NameCheck::Ok | NameCheck::Taken)
    }
}

/// The name a person typed without an ending the format already adds, and
/// without surrounding spaces: `Photos.zip` in `.zip` is `Photos`.
pub fn base_of(typed: &str, format: Format) -> String {
    let typed = typed.trim();
    let lower = typed.to_lowercase();
    for candidate in [
        format,
        Format::Zip,
        Format::TarGz,
        Format::TarXz,
        Format::TarZst,
        Format::TarBz2,
        Format::SevenZip,
        Format::Tar,
    ] {
        let ending = candidate.extension();
        if lower.ends_with(ending)
            && typed.len() > ending.len()
            && typed.is_char_boundary(typed.len() - ending.len())
        {
            return typed[..typed.len() - ending.len()].trim_end().to_string();
        }
    }
    typed.to_string()
}

/// The file name for `base` in `format`.
pub fn file_name(base: &str, format: Format) -> String {
    format!("{}{}", base_of(base, format), format.extension())
}

/// The name to offer for a selection: a folder's own name, a file's name
/// without its ending, and "Archive" for several things.
pub fn suggested_base(names: &[String], folder: bool) -> String {
    match names {
        [only] if folder => only.clone(),
        [only] => {
            let stem = match only.rfind('.') {
                Some(dot) if dot > 0 => &only[..dot],
                _ => only,
            };
            stem.to_string()
        }
        _ => "Archive".to_string(),
    }
}

/// Check a typed name against the folder it will go in. `taken` says whether
/// a file name is in use there.
pub fn check_name(typed: &str, format: Format, taken: impl Fn(&str) -> bool) -> NameCheck {
    let base = base_of(typed, format);
    if base.trim().is_empty() {
        return NameCheck::Empty;
    }
    if base.contains('/') || base.contains('\0') {
        return NameCheck::HasSlash;
    }
    if base == "." || base == ".." {
        return NameCheck::Reserved;
    }
    let name = file_name(&base, format);
    // A volume's ending is as long as ".7z.001", and the program adds it.
    if name.len() + 4 > NAME_LIMIT {
        return NameCheck::TooLong;
    }
    if taken(&name) {
        NameCheck::Taken
    } else {
        NameCheck::Ok
    }
}

/// The first of `base 2`, `base 3`, ... whose file name is free.
pub fn free_base(base: &str, format: Format, taken: impl Fn(&str) -> bool) -> String {
    let base = base_of(base, format);
    (2u32..)
        .map(|number| format!("{base} {number}"))
        .find(|candidate| !taken(&file_name(candidate, format)))
        .unwrap_or(base)
}

// ---------------------------------------------------------------------------
// Exclude patterns
// ---------------------------------------------------------------------------

/// Why a pattern was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternError {
    pub pattern: String,
    pub reason: &'static str,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "“{}”: {}", self.pattern, self.reason)
    }
}

/// The most patterns one archive takes.
pub const PATTERN_LIMIT: usize = 64;

/// A list of globs. A pattern with no slash matches the name of any file or
/// folder at any depth (`node_modules`, `*.tmp`); one with a slash matches the
/// path inside the archive from its start (`build/*.o`). `*` and `?` stop at
/// a slash, `**` does not, `[abc]` and `[!a-c]` are classes, and a backslash
/// quotes the next character. A folder that matches is left out whole.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Excludes {
    patterns: Vec<String>,
}

impl Excludes {
    /// Parse comma-separated patterns. Blank pieces are ignored.
    pub fn parse(text: &str) -> Result<Excludes, PatternError> {
        let mut patterns = Vec::new();
        for piece in text.split(',') {
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            check_pattern(piece)?;
            let piece = piece
                .trim_start_matches("./")
                .trim_start_matches('/')
                .trim_end_matches('/');
            if !piece.is_empty() {
                patterns.push(piece.to_string());
            }
        }
        if patterns.len() > PATTERN_LIMIT {
            return Err(PatternError {
                pattern: text.chars().take(24).collect(),
                reason: "there are too many patterns",
            });
        }
        Ok(Excludes { patterns })
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// The patterns, as typed back in a field.
    pub fn text(&self) -> String {
        self.patterns.join(", ")
    }

    /// Does an entry at `path` (relative, `/`-separated) get left out?
    pub fn matches(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.patterns.iter().any(|pattern| {
            if pattern.contains('/') {
                glob(pattern, path)
            } else {
                glob(pattern, name)
            }
        })
    }
}

fn check_pattern(pattern: &str) -> Result<(), PatternError> {
    let refuse = |reason| {
        Err(PatternError {
            pattern: pattern.to_string(),
            reason,
        })
    };
    if pattern.len() > NAME_LIMIT * 2 {
        return refuse("the pattern is too long");
    }
    if pattern.contains('\0') {
        return refuse("a pattern cannot hold a null character");
    }
    let chars: Vec<char> = pattern.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '\\' => {
                if at + 1 >= chars.len() {
                    return refuse("a backslash must be followed by a character");
                }
                at += 2;
            }
            '[' => {
                // A class ends at the first ']' after its first member.
                let mut end = at + 1;
                if matches!(chars.get(end), Some('!' | '^')) {
                    end += 1;
                }
                if chars.get(end) == Some(&']') {
                    end += 1;
                }
                while end < chars.len() && chars[end] != ']' {
                    end += 1;
                }
                if end >= chars.len() {
                    return refuse("a [ is never closed");
                }
                at = end + 1;
            }
            _ => at += 1,
        }
    }
    Ok(())
}

/// Match `text` against a glob. See [`Excludes`] for the rules.
pub fn glob(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    glob_at(&pattern, &text)
}

fn glob_at(pattern: &[char], text: &[char]) -> bool {
    let Some((&first, rest)) = pattern.split_first() else {
        return text.is_empty();
    };
    match first {
        '*' => {
            let deep = rest.first() == Some(&'*');
            let rest = if deep { &rest[1..] } else { rest };
            // Try every length the star can take, longest last.
            for taken in 0..=text.len() {
                if glob_at(rest, &text[taken..]) {
                    return true;
                }
                match text.get(taken) {
                    Some('/') if !deep => return false,
                    None => return false,
                    _ => {}
                }
            }
            false
        }
        '?' => matches!(text.first(), Some(c) if *c != '/') && glob_at(rest, &text[1..]),
        '[' => {
            let Some(&c) = text.first() else {
                return false;
            };
            let Some((matched, after)) = class(rest, c) else {
                // Not a class after all: a literal bracket.
                return c == '[' && glob_at(rest, &text[1..]);
            };
            matched && c != '/' && glob_at(after, &text[1..])
        }
        '\\' => match rest.split_first() {
            Some((&literal, rest)) => text.first() == Some(&literal) && glob_at(rest, &text[1..]),
            None => false,
        },
        literal => text.first() == Some(&literal) && glob_at(rest, &text[1..]),
    }
}

/// Read a class after its `[`. Returns whether `c` is in it and the rest of
/// the pattern, or `None` when the class is not closed.
fn class(pattern: &[char], c: char) -> Option<(bool, &[char])> {
    let mut at = 0;
    let negate = matches!(pattern.first(), Some('!' | '^'));
    if negate {
        at += 1;
    }
    let mut found = false;
    let mut first = true;
    while at < pattern.len() {
        let member = pattern[at];
        if member == ']' && !first {
            return Some((found != negate, &pattern[at + 1..]));
        }
        first = false;
        let low = if member == '\\' && at + 1 < pattern.len() {
            at += 1;
            pattern[at]
        } else {
            member
        };
        if pattern.get(at + 1) == Some(&'-') && pattern.get(at + 2).is_some_and(|end| *end != ']') {
            let high = pattern[at + 2];
            if low <= c && c <= high {
                found = true;
            }
            at += 3;
        } else {
            if low == c {
                found = true;
            }
            at += 1;
        }
    }
    None
}

/// Is a path component hidden: does it start with a dot?
pub fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}

// ---------------------------------------------------------------------------
// The programs' command lines
// ---------------------------------------------------------------------------

/// Names longer than this go to 7-Zip as one argument each, not in the list.
/// (7-Zip reads a list as text: it drops trailing spaces and cannot hold a
/// line break or bytes that are not UTF-8.)
fn fits_a_list(name: &[u8]) -> bool {
    std::str::from_utf8(name).is_ok_and(|text| {
        !text.contains(['\n', '\r', '\0', '"']) && text == text.trim() && !text.is_empty()
    })
}

/// How many threads a squeezer may use. A big archive should not take the
/// desktop's every core.
fn threads() -> String {
    std::thread::available_parallelism()
        .map_or(2, |count| count.get().clamp(1, 4))
        .to_string()
}

/// One step of making an archive.
#[derive(Debug)]
pub enum Steps {
    /// A single program that writes the archive itself.
    One(Spec),
    /// A tar piped through a squeezer, written to the archive file.
    Pipe { tar: Spec, squeeze: Spec },
}

/// A plan: what to run and the list file to write for it first.
#[derive(Debug)]
pub struct Built {
    pub steps: Steps,
    /// A file to write in the work folder before running, and its bytes.
    pub list_file: Option<(PathBuf, Vec<u8>)>,
}

/// The command lines for archiving `names` (paths relative to `parent`, a
/// folder before what it holds) into `work/<file_name>`.
///
/// A name that begins with a dash is safe: tar reads its names from a NUL
/// list with `--verbatim-files-from`, zip from standard input, 7-Zip from a
/// list file and after `--`, and none of them reads a name as a switch.
pub fn build(
    options: &PackOptions,
    parent: &Path,
    work: &Path,
    file_name: &str,
    names: &[PathBuf],
) -> Result<Built, OpError> {
    let options = options.clone().normalised();
    let maker = options.maker()?;
    let archive = work.join(file_name);
    let low = true;
    match options.format {
        Format::Zip
            if options.password.is_none()
                && maker.file_name().is_some_and(|name| name == "zip") =>
        {
            let mut spec = Spec::new(&maker)
                .in_dir(parent)
                .nice(low)
                .arg("-nw")
                .arg(format!("-{}", options.level.digit()));
            // Links are stored as links unless the person asked to follow them.
            if !options.follow_links {
                spec = spec.arg("-y");
            }
            if let Some(mib) = options.split_mib {
                spec = spec.arg("-s").arg(format!("{mib}m"));
            }
            // Names that hold a line break cannot ride in the stdin list.
            let (listed, loose): (Vec<&PathBuf>, Vec<&PathBuf>) = names
                .iter()
                .partition(|name| !tool::bytes_of(name).contains(&b'\n'));
            spec = spec.arg(&archive).arg("-@");
            if !loose.is_empty() {
                spec = spec.arg("--");
                spec.args
                    .extend(loose.iter().map(|name| name.as_os_str().to_os_string()));
            }
            let mut list = Vec::new();
            for name in listed {
                list.extend_from_slice(tool::bytes_of(name));
                list.push(b'\n');
            }
            Ok(Built {
                steps: Steps::One(spec.with_input(Input::Bytes(list))),
                list_file: None,
            })
        }
        Format::Zip | Format::SevenZip => {
            let seven_zip = options.format == Format::SevenZip;
            let list_path = work.join("names.list");
            let mut spec = Spec::new(&maker)
                .in_dir(parent)
                .nice(low)
                .arg("a")
                .arg(if seven_zip { "-t7z" } else { "-tzip" })
                .args(["-bb1", "-bsp1", "-spd", "-r-", "-scsUTF-8"])
                .arg(options.level.seven());
            if !options.follow_links {
                spec = spec.arg("-snl");
            }
            if let Some(mib) = options.split_mib {
                spec = spec.arg(format!("-v{mib}m"));
            }
            if let Some(password) = &options.password {
                spec = spec.arg("-p");
                if seven_zip {
                    if options.hide_names {
                        spec = spec.arg("-mhe=on");
                    }
                } else {
                    spec = spec.arg("-mem=AES256");
                }
                spec = spec.with_input(password.prompt_input());
            }
            spec = spec.arg(&archive).arg(format!("@{}", list_path.display()));
            let mut list = Vec::new();
            let mut loose: Vec<OsString> = Vec::new();
            for name in names {
                let bytes = tool::bytes_of(name);
                if fits_a_list(bytes) {
                    list.extend_from_slice(bytes);
                    list.push(b'\n');
                } else {
                    loose.push(name.as_os_str().to_os_string());
                }
            }
            if !loose.is_empty() {
                spec = spec.arg("--").args(loose);
            }
            Ok(Built {
                steps: Steps::One(spec),
                list_file: Some((list_path, list)),
            })
        }
        tar_kind => {
            let mut tar = Spec::new(&maker).in_dir(parent).nice(low).args([
                "-c",
                "-v",
                "--no-recursion",
                "--null",
                "--verbatim-files-from",
                "--quoting-style=literal",
                "-T",
                "-",
            ]);
            if options.follow_links {
                tar = tar.arg("--dereference");
            }
            if !options.keep_owner {
                tar = tar.args(["--owner=0", "--group=0", "--numeric-owner"]);
            }
            let mut list = Vec::new();
            for name in names {
                list.extend_from_slice(tool::bytes_of(name));
                list.push(0);
            }
            tar = tar.with_input(Input::Bytes(list));
            let Some(program) = tar_kind.compressor() else {
                tar = tar.arg("-f").arg(&archive);
                return Ok(Built {
                    steps: Steps::One(tar),
                    list_file: None,
                });
            };
            tar = tar.args(["-f", "-"]);
            let found = tool::find(program)
                .ok_or_else(|| OpError::Unsupported(format!("{program} is not installed.")))?;
            let mut squeeze = Spec::new(found).nice(low);
            squeeze = match tar_kind {
                Format::TarGz | Format::TarBz2 => {
                    squeeze.arg(format!("-{}", options.level.digit()))
                }
                Format::TarXz => squeeze
                    .arg(format!("-{}", options.level.digit()))
                    .arg(format!("-T{}", threads())),
                _ => squeeze
                    .arg(options.level.zstd())
                    .arg(format!("-T{}", threads()))
                    .arg("-q"),
            };
            Ok(Built {
                steps: Steps::Pipe { tar, squeeze },
                list_file: None,
            })
        }
    }
}

/// What a line of a maker's output says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Heard {
    /// This entry was just handled (as the bytes of its path).
    Entry(Vec<u8>),
    /// This much of the whole is done, 0 to 100.
    Percent(u8),
}

/// Read one line a maker printed. `format` says whose dialect it is; 7-Zip
/// writes for `.7z` and for a zip made with a password, so `seven` says
/// whether the maker was 7-Zip.
pub fn hear(seven: bool, format: Format, line: &[u8]) -> Option<Heard> {
    if seven {
        return hear_seven(line);
    }
    match format {
        Format::Zip => {
            let rest = line.strip_prefix(b"  adding: ")?;
            let end = rest.windows(2).rposition(|pair| pair == b" (")?;
            Some(Heard::Entry(rest[..end].to_vec()))
        }
        _ => (!line.is_empty()).then(|| Heard::Entry(line.to_vec())),
    }
}

fn hear_seven(line: &[u8]) -> Option<Heard> {
    let text = line.trim_ascii_start();
    // "  12% 3 + name" and "  12%": the meter, optionally with the item.
    if let Some(percent) = text.iter().position(|byte| *byte == b'%')
        && percent > 0
        && percent <= 3
        && text[..percent].iter().all(u8::is_ascii_digit)
    {
        let number: u8 = std::str::from_utf8(&text[..percent]).ok()?.parse().ok()?;
        let after = text[percent + 1..].trim_ascii_start();
        if let Some(name) = entry_after_marker(after) {
            return Some(Heard::Entry(name.to_vec()));
        }
        return Some(Heard::Percent(number.min(100)));
    }
    entry_after_marker(text).map(|name| Heard::Entry(name.to_vec()))
}

/// The name after `+ ` (an item added), with a count before it allowed.
fn entry_after_marker(text: &[u8]) -> Option<&[u8]> {
    let skipped = text
        .iter()
        .position(|byte| !byte.is_ascii_digit() && *byte != b' ')
        .unwrap_or(text.len());
    let text = &text[skipped..];
    text.strip_prefix(b"+ ").filter(|name| !name.is_empty())
}

/// The commands that check a finished archive: read it all, and list it.
#[derive(Debug)]
pub struct Check {
    /// Reads every byte of every entry; success means the archive is whole.
    pub test: Spec,
    /// Prints one line per entry.
    pub list: Spec,
}

/// How to check the archive `archive` made with `options`.
pub fn check_specs(options: &PackOptions, archive: &Path, work: &Path) -> Result<Check, OpError> {
    let options = options.clone().normalised();
    let split = options.split_mib.is_some();
    let seven = |action: &[&str]| -> Result<Spec, OpError> {
        let program = tool::seven_zip()
            .ok_or_else(|| OpError::Unsupported("7-Zip is not installed.".into()))?;
        let mut spec = Spec::new(program).in_dir(work).nice(true).args(action);
        spec = spec.args(["-scsUTF-8", "-bd"]);
        // `-p` with no value means an empty password to 7-Zip when it opens an
        // archive; leaving it out makes it ask on standard input, but only
        // if the archive is locked. An unlocked one never asks, and a locked
        // one with nothing to say fails rather than wait.
        spec = match &options.password {
            Some(password) => spec.with_input(password.input_once()),
            None => spec.with_input(Input::Null),
        };
        Ok(spec.arg("--").arg(archive))
    };
    match options.format {
        Format::Zip if options.password.is_none() && !split => {
            let unzip = tool::find("unzip");
            match unzip {
                Some(unzip) => Ok(Check {
                    test: Spec::new(&unzip)
                        .nice(true)
                        .args(["-tq", "--"])
                        .arg(archive),
                    list: Spec::new(&unzip)
                        .nice(true)
                        .args(["-Z1", "--"])
                        .arg(archive),
                }),
                None => Ok(Check {
                    test: seven(&["t"])?,
                    list: seven(&["l", "-ba"])?,
                }),
            }
        }
        Format::Zip | Format::SevenZip => Ok(Check {
            test: seven(&["t"])?,
            list: seven(&["l", "-ba"])?,
        }),
        _ => {
            let tar = tool::find("tar")
                .ok_or_else(|| OpError::Unsupported("tar is not installed.".into()))?;
            let list = Spec::new(tar)
                .nice(true)
                .args(["-t", "--quoting-style=literal", "-f"])
                .arg(archive);
            Ok(Check {
                test: list.clone(),
                list,
            })
        }
    }
}

/// Is this line of a `check_specs` listing one entry? 7-Zip's listing is a
/// table with a date first on each row, and may print a password prompt or a
/// warning among the rows; the others print a name a line.
pub fn is_listing_row(seven: bool, line: &[u8]) -> bool {
    if seven {
        line.len() > 20
            && line[..4].iter().all(u8::is_ascii_digit)
            && line[4] == b'-'
            && line[7] == b'-'
    } else {
        !line.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn one(built: &Built) -> &Spec {
        match &built.steps {
            Steps::One(spec) => spec,
            Steps::Pipe { .. } => panic!("expected one program"),
        }
    }

    #[test]
    fn extensions_and_names_round_trip() {
        for format in Format::ALL {
            assert_eq!(
                Format::from_name(&format!("a{}", format.extension())),
                Some(format)
            );
        }
        assert_eq!(Format::from_name("A.TAR.GZ"), Some(Format::TarGz));
        assert_eq!(Format::from_name("a.txt"), None);
    }

    #[test]
    fn only_the_formats_that_can_lock_or_split_say_so() {
        assert!(Format::Zip.can_lock() && Format::SevenZip.can_lock());
        assert!(!Format::TarGz.can_lock());
        assert!(Format::SevenZip.can_hide_names() && !Format::Zip.can_hide_names());
        assert!(Format::Zip.can_split() && !Format::Tar.can_split());
        assert!(!Format::Tar.has_levels() && Format::TarXz.has_levels());
    }

    #[test]
    fn a_single_file_loses_its_ending_and_a_folder_keeps_its_name() {
        let names = |name: &str| vec![name.to_string()];
        assert_eq!(suggested_base(&names("report.pdf"), false), "report");
        assert_eq!(suggested_base(&names("Holiday 2.0"), true), "Holiday 2.0");
        assert_eq!(suggested_base(&names(".bashrc"), false), ".bashrc");
        assert_eq!(suggested_base(&["a".into(), "b".into()], false), "Archive");
    }

    #[test]
    fn the_ending_is_not_added_twice() {
        assert_eq!(base_of("Photos.zip", Format::Zip), "Photos");
        assert_eq!(base_of("Photos.TAR.GZ", Format::Zip), "Photos");
        assert_eq!(base_of("  Photos  ", Format::Zip), "Photos");
        assert_eq!(base_of(".zip", Format::Zip), ".zip");
        assert_eq!(file_name("Photos.zip", Format::Zip), "Photos.zip");
        assert_eq!(file_name("v1.2", Format::TarXz), "v1.2.tar.xz");
    }

    #[test]
    fn names_are_checked_as_they_are_typed() {
        let none = |_: &str| false;
        assert_eq!(check_name("", Format::Zip, none), NameCheck::Empty);
        assert_eq!(check_name("   ", Format::Zip, none), NameCheck::Empty);
        assert_eq!(check_name("a/b", Format::Zip, none), NameCheck::HasSlash);
        assert_eq!(check_name("..", Format::Zip, none), NameCheck::Reserved);
        assert_eq!(
            check_name(&"x".repeat(300), Format::Zip, none),
            NameCheck::TooLong
        );
        assert_eq!(check_name("fine", Format::Zip, none), NameCheck::Ok);
        let taken = |name: &str| name == "fine.zip";
        assert_eq!(check_name("fine", Format::Zip, taken), NameCheck::Taken);
        assert_eq!(check_name("fine", Format::TarGz, taken), NameCheck::Ok);
        assert!(NameCheck::Taken.is_usable() && !NameCheck::Empty.is_usable());
        assert!(NameCheck::Ok.message().is_none() && NameCheck::Taken.message().is_some());
    }

    #[test]
    fn a_taken_name_is_numbered_not_copied() {
        let taken = |name: &str| ["Archive.zip", "Archive 2.zip", "Archive 3.zip"].contains(&name);
        assert_eq!(free_base("Archive", Format::Zip, taken), "Archive 4");
        assert_eq!(free_base("Archive.zip", Format::Zip, taken), "Archive 4");
        assert_eq!(free_base("Other", Format::Zip, taken), "Other 2");
        assert_eq!(
            file_name(&free_base("Archive", Format::TarGz, taken), Format::TarGz),
            "Archive 2.tar.gz"
        );
    }

    #[test]
    fn patterns_split_at_commas_and_ignore_blanks() {
        let excludes = Excludes::parse(" *.tmp , node_modules,, build/*.o ,").unwrap();
        assert_eq!(excludes.text(), "*.tmp, node_modules, build/*.o");
        assert!(Excludes::parse("").unwrap().is_empty());
        assert!(Excludes::parse(" , ").unwrap().is_empty());
    }

    #[test]
    fn a_pattern_without_a_slash_matches_a_name_at_any_depth() {
        let excludes = Excludes::parse("*.tmp, node_modules, .git").unwrap();
        assert!(excludes.matches("a.tmp"));
        assert!(excludes.matches("deep/er/a.tmp"));
        assert!(excludes.matches("app/node_modules"));
        assert!(excludes.matches(".git"));
        assert!(!excludes.matches("a.txt"));
        assert!(!excludes.matches("tmp"));
    }

    #[test]
    fn a_pattern_with_a_slash_matches_from_the_start() {
        let excludes = Excludes::parse("build/*.o, docs/**").unwrap();
        assert!(excludes.matches("build/a.o"));
        assert!(!excludes.matches("src/build/a.o"));
        assert!(
            !excludes.matches("build/sub/a.o"),
            "a star stops at a slash"
        );
        assert!(excludes.matches("docs/a/b/c.md"), "two stars do not");
    }

    #[test]
    fn globs_know_classes_and_quotes() {
        assert!(glob("file?.txt", "file1.txt"));
        assert!(!glob("file?.txt", "file12.txt"));
        assert!(glob("[abc]x", "bx"));
        assert!(!glob("[!abc]x", "bx"));
        assert!(glob("[a-c]x", "cx"));
        assert!(glob("a\\*b", "a*b"));
        assert!(!glob("a\\*b", "axb"));
        assert!(glob("*", ""));
        assert!(glob("a*", "a"));
        assert!(glob("*é", "café"));
    }

    #[test]
    fn broken_patterns_are_refused_with_a_reason() {
        assert_eq!(
            Excludes::parse("a[bc").unwrap_err().reason,
            "a [ is never closed"
        );
        assert_eq!(
            Excludes::parse("ok, trailing\\").unwrap_err().reason,
            "a backslash must be followed by a character"
        );
        assert!(Excludes::parse("[]]").is_ok());
        let many = (0..=PATTERN_LIMIT)
            .map(|n| format!("p{n}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(Excludes::parse(&many).is_err());
    }

    #[test]
    fn a_password_never_shows_in_a_debug_line() {
        let options = PackOptions {
            password: Some(Password::new("hunter2")),
            ..PackOptions::default()
        };
        assert!(!format!("{options:?}").contains("hunter2"));
    }

    #[test]
    fn options_drop_what_a_format_cannot_use() {
        let options = PackOptions {
            format: Format::TarGz,
            password: Some(Password::new("x")),
            hide_names: true,
            split_mib: Some(100),
            ..PackOptions::default()
        }
        .normalised();
        assert!(options.password.is_none() && !options.hide_names && options.split_mib.is_none());
        let zip = PackOptions {
            format: Format::Zip,
            password: Some(Password::new("x")),
            hide_names: true,
            ..PackOptions::default()
        }
        .normalised();
        assert!(
            zip.password.is_some() && !zip.hide_names,
            "a zip cannot hide its names"
        );
        let empty = PackOptions {
            password: Some(Password::new("")),
            ..PackOptions::default()
        }
        .normalised();
        assert!(empty.password.is_none());
    }

    #[test]
    fn split_sizes_have_words() {
        assert_eq!(split_label(100), "100 MB");
        assert_eq!(split_label(2048), "2 GB");
        assert_eq!(split_label(4095), "4095 MB");
    }

    fn names(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn levels_become_each_programs_flag() {
        if !Format::TarGz.is_available() || !Format::TarZst.is_available() {
            return;
        }
        let work = Path::new("/w");
        for (level, digit, zstd, seven) in [
            (Level::Fastest, "-1", "-1", "-mx=1"),
            (Level::Balanced, "-6", "-3", "-mx=5"),
            (Level::Smallest, "-9", "-19", "-mx=9"),
        ] {
            let gz = build(
                &PackOptions {
                    format: Format::TarGz,
                    level,
                    ..PackOptions::default()
                },
                Path::new("/p"),
                work,
                "a.tar.gz",
                &names(&["x"]),
            )
            .unwrap();
            let Steps::Pipe { squeeze, .. } = &gz.steps else {
                panic!("gzip pipes")
            };
            assert_eq!(text(&squeeze.args)[0], digit);
            let zst = build(
                &PackOptions {
                    format: Format::TarZst,
                    level,
                    ..PackOptions::default()
                },
                Path::new("/p"),
                work,
                "a.tar.zst",
                &names(&["x"]),
            )
            .unwrap();
            let Steps::Pipe { squeeze, .. } = &zst.steps else {
                panic!("zstd pipes")
            };
            assert_eq!(text(&squeeze.args)[0], zstd);
            if Format::SevenZip.is_available() {
                let built = build(
                    &PackOptions {
                        format: Format::SevenZip,
                        level,
                        ..PackOptions::default()
                    },
                    Path::new("/p"),
                    work,
                    "a.7z",
                    &names(&["x"]),
                )
                .unwrap();
                assert!(text(&one(&built).args).contains(&seven.to_string()));
            }
        }
    }

    #[test]
    fn tar_reads_its_names_from_a_nul_list_so_no_name_is_a_switch() {
        if !Format::Tar.is_available() {
            return;
        }
        let built = build(
            &PackOptions {
                format: Format::Tar,
                ..PackOptions::default()
            },
            Path::new("/p"),
            Path::new("/w"),
            "a.tar",
            &names(&["-rf", "sp ace", "dir/x"]),
        )
        .unwrap();
        let spec = one(&built);
        let args = text(&spec.args);
        assert!(args.contains(&"--verbatim-files-from".to_string()));
        assert!(
            !args.contains(&"-rf".to_string()),
            "names are not arguments"
        );
        let Input::Bytes(list) = &spec.input else {
            panic!("the names ride on stdin")
        };
        assert_eq!(list, b"-rf\0sp ace\0dir/x\0");
    }

    #[test]
    fn links_and_owners_follow_the_options() {
        if !Format::Tar.is_available() {
            return;
        }
        let args = |follow, owner| {
            let built = build(
                &PackOptions {
                    format: Format::Tar,
                    follow_links: follow,
                    keep_owner: owner,
                    ..PackOptions::default()
                },
                Path::new("/p"),
                Path::new("/w"),
                "a.tar",
                &names(&["x"]),
            )
            .unwrap();
            text(&one(&built).args)
        };
        assert!(args(true, true).contains(&"--dereference".to_string()));
        assert!(!args(false, true).contains(&"--dereference".to_string()));
        assert!(args(false, false).contains(&"--owner=0".to_string()));
        assert!(!args(false, true).contains(&"--owner=0".to_string()));
    }

    #[test]
    fn zip_takes_names_on_stdin_and_never_a_password() {
        if tool::find("zip").is_none() {
            return;
        }
        let built = build(
            &PackOptions {
                format: Format::Zip,
                split_mib: Some(100),
                ..PackOptions::default()
            },
            Path::new("/p"),
            Path::new("/w"),
            "a.zip",
            &names(&["-x", "a b", "line\nbreak"]),
        )
        .unwrap();
        let spec = one(&built);
        let args = text(&spec.args);
        assert!(args.contains(&"-y".to_string()), "links are kept");
        assert!(args.windows(2).any(|pair| pair == ["-s", "100m"]));
        assert!(args.contains(&"-nw".to_string()), "no wildcards in names");
        let archive = args.iter().position(|arg| arg == "/w/a.zip").unwrap();
        assert_eq!(
            &args[archive + 1..],
            ["-@", "--", "line\nbreak"],
            "a name with a line break is an argument"
        );
        assert!(!args.contains(&"-x".to_string()));
        let Input::Bytes(list) = &spec.input else {
            panic!("names on stdin")
        };
        assert_eq!(list, b"-x\na b\n");
        let following = build(
            &PackOptions {
                format: Format::Zip,
                follow_links: true,
                ..PackOptions::default()
            },
            Path::new("/p"),
            Path::new("/w"),
            "a.zip",
            &names(&["x"]),
        )
        .unwrap();
        assert!(!text(&one(&following).args).contains(&"-y".to_string()));
    }

    #[test]
    fn a_password_goes_to_stdin_never_to_the_arguments() {
        if tool::seven_zip().is_none() {
            return;
        }
        for (format, hide) in [(Format::Zip, false), (Format::SevenZip, true)] {
            let built = build(
                &PackOptions {
                    format,
                    password: Some(Password::new("s3cret-pass")),
                    hide_names: hide,
                    ..PackOptions::default()
                },
                Path::new("/p"),
                Path::new("/w"),
                &file_name("a", format),
                &names(&["x", " lead", "trail "]),
            )
            .unwrap();
            let spec = one(&built);
            let all = text(&spec.args).join(" ");
            assert!(!all.contains("s3cret"), "{format:?}: {all}");
            assert!(!format!("{spec:?}").contains("s3cret"));
            let Input::Bytes(input) = &spec.input else {
                panic!("the password rides on stdin")
            };
            assert_eq!(input, b"s3cret-pass\ns3cret-pass\n");
            let args = text(&spec.args);
            assert!(args.contains(&"-p".to_string()), "-p alone asks on stdin");
            assert_eq!(args.contains(&"-mhe=on".to_string()), hide);
            assert_eq!(
                args.contains(&"-mem=AES256".to_string()),
                format == Format::Zip
            );
            // The name with a trailing space cannot ride in a list.
            let (path, list) = built.list_file.as_ref().unwrap();
            assert_eq!(path, Path::new("/w/names.list"));
            assert_eq!(
                list, b"x\n",
                "a name 7-Zip would trim cannot ride in the list"
            );
            let at = args.iter().position(|arg| arg == "--").unwrap();
            assert_eq!(&args[at + 1..], [" lead", "trail "]);
        }
    }

    #[test]
    fn seven_zip_keeps_links_unless_told_to_follow_them() {
        if tool::seven_zip().is_none() {
            return;
        }
        let args = |follow| {
            let built = build(
                &PackOptions {
                    format: Format::SevenZip,
                    follow_links: follow,
                    ..PackOptions::default()
                },
                Path::new("/p"),
                Path::new("/w"),
                "a.7z",
                &names(&["x"]),
            )
            .unwrap();
            text(&one(&built).args)
        };
        assert!(args(false).contains(&"-snl".to_string()));
        assert!(!args(true).contains(&"-snl".to_string()));
        assert!(
            args(false).contains(&"-spd".to_string()),
            "no wildcards in names"
        );
        assert!(
            args(false).contains(&"-r-".to_string()),
            "folders are listed, not walked"
        );
    }

    #[test]
    fn volumes_are_sized_for_each_program() {
        if tool::seven_zip().is_none() {
            return;
        }
        let built = build(
            &PackOptions {
                format: Format::SevenZip,
                split_mib: Some(650),
                ..PackOptions::default()
            },
            Path::new("/p"),
            Path::new("/w"),
            "a.7z",
            &names(&["x"]),
        )
        .unwrap();
        assert!(text(&one(&built).args).contains(&"-v650m".to_string()));
    }

    #[test]
    fn what_a_maker_prints_is_read_back_as_progress() {
        assert_eq!(
            hear(
                false,
                Format::Zip,
                b"  adding: dir/a (b).txt (deflated 40%)"
            ),
            Some(Heard::Entry(b"dir/a (b).txt".to_vec()))
        );
        assert_eq!(hear(false, Format::Zip, b"zip warning: x"), None);
        assert_eq!(
            hear(false, Format::TarGz, b"dir/a.txt"),
            Some(Heard::Entry(b"dir/a.txt".to_vec()))
        );
        assert_eq!(
            hear(true, Format::SevenZip, b"+ dir/a.txt"),
            Some(Heard::Entry(b"dir/a.txt".to_vec()))
        );
        assert_eq!(
            hear(true, Format::SevenZip, b" 42%"),
            Some(Heard::Percent(42))
        );
        assert_eq!(
            hear(true, Format::SevenZip, b" 42% 3 + dir/a.txt"),
            Some(Heard::Entry(b"dir/a.txt".to_vec()))
        );
        assert_eq!(hear(true, Format::SevenZip, b"Everything is Ok"), None);
        assert_eq!(hear(true, Format::SevenZip, b"Scanning the drive:"), None);
    }

    #[test]
    fn checks_test_and_list_the_archive() {
        let Ok(check) = check_specs(
            &PackOptions::default(),
            Path::new("/w/a.zip"),
            Path::new("/w"),
        ) else {
            return;
        };
        assert!(!check.test.args.is_empty() && !check.list.args.is_empty());
        let locked = PackOptions {
            password: Some(Password::new("pw-1")),
            ..PackOptions::default()
        };
        if let Ok(check) = check_specs(&locked, Path::new("/w/a.zip"), Path::new("/w")) {
            assert!(!format!("{check:?}").contains("pw-1"));
            assert!(!text(&check.test.args).join(" ").contains("pw-1"));
        }
    }
}
