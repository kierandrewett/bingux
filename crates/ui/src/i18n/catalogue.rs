//! The catalogues the process knows and where their translations are.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// One `.ftl` domain: an app's strings, or bingux-ui's own. Made with
/// [`catalogue!`](crate::catalogue), which compiles the source language in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalogue {
    domain: &'static str,
    source: &'static str,
    source_dir: Option<&'static str>,
}

impl Catalogue {
    /// The catalogue `domain` with `source`, the text of its `en-GB` file.
    pub const fn new(domain: &'static str, source: &'static str) -> Catalogue {
        Catalogue {
            domain,
            source,
            source_dir: None,
        }
    }

    /// Reads translations from `dir` (`<dir>/<language>/<domain>.ftl`) in
    /// debug builds, before the installed ones: the crate's own `locale/`.
    pub const fn with_source_dir(mut self, dir: &'static str) -> Catalogue {
        self.source_dir = Some(dir);
        self
    }

    /// The name of the catalogue: its file is `<domain>.ftl`.
    pub fn domain(&self) -> &'static str {
        self.domain
    }

    /// The source language's text, compiled in.
    pub fn source(&self) -> &'static str {
        self.source
    }

    /// The crate's `locale/` directory, when the catalogue was made with one.
    pub fn source_dir(&self) -> Option<&'static Path> {
        self.source_dir.map(Path::new)
    }
}

/// bingux-ui's own catalogue: dialogs, menus, the file chooser, the edit bar.
pub(crate) fn builtin() -> Catalogue {
    crate::catalogue!("bingux-ui")
}

static REGISTERED: RwLock<Vec<Catalogue>> = RwLock::new(Vec::new());

pub(super) fn register(catalogue: Catalogue) {
    let mut list = REGISTERED
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    list.retain(|known| known.domain != catalogue.domain);
    list.push(catalogue);
}

/// bingux-ui's catalogue, then every registered one, in registration order.
pub(super) fn all() -> Vec<Catalogue> {
    let mut list = vec![builtin()];
    list.extend(
        REGISTERED
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|catalogue| catalogue.domain != "bingux-ui")
            .cloned(),
    );
    list
}

/// The directories translations are read from for `catalogue`, strongest
/// first (see the module documentation), as `var` (the environment) says.
pub(super) fn search_dirs(
    catalogue: &Catalogue,
    var: &dyn Fn(&str) -> Option<String>,
) -> Vec<PathBuf> {
    let value = |name: &str| var(name).filter(|text| !text.trim().is_empty());
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(list) = value("BINGUX_LOCALE_DIR") {
        dirs.extend(list.split(':').filter(|dir| !dir.is_empty()).map(PathBuf::from));
    }
    if cfg!(debug_assertions)
        && let Some(dir) = catalogue.source_dir()
    {
        dirs.push(dir.to_path_buf());
    }
    let data_home = value("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| value("HOME").map(|home| Path::new(&home).join(".local/share")));
    if let Some(home) = data_home {
        dirs.push(home.join("bingux/locale"));
    }
    let data_dirs = value("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(
        data_dirs
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| Path::new(dir).join("bingux/locale")),
    );
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|dir| seen.insert(dir.clone()));
    dirs
}

/// The file of `catalogue` in `language` (a directory name such as `de` or
/// `pt-BR`), from the first directory that has it.
pub(super) fn find(dirs: &[PathBuf], language: &str, catalogue: &Catalogue) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(language).join(format!("{}.ftl", catalogue.domain)))
        .find(|path| path.is_file())
}

/// Every language directory under `dirs` that holds a file of one of
/// `catalogues`.
pub(super) fn languages(dirs: &[PathBuf], catalogues: &[Catalogue]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if found.contains(&name) {
                continue;
            }
            let has_one = catalogues
                .iter()
                .any(|catalogue| entry.path().join(format!("{}.ftl", catalogue.domain)).is_file());
            if has_one {
                found.push(name);
            }
        }
    }
    found.sort();
    found
}
