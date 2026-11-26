//! Where profiles live (`docs/browser/profiles-and-store.md`, "Where things
//! live"):
//!
//! | What | Where |
//! | ---- | ----- |
//! | the registry and its lock | `$XDG_DATA_HOME/bingux-browser/profiles.json`, `profiles.lock` |
//! | one profile | `$XDG_DATA_HOME/bingux-browser/profiles/<id>/` (lock, settings, store, `engine/`) |
//! | its engine cache | `$XDG_CACHE_HOME/bingux-browser/profiles/<id>` |
//! | its crash dumps | `$XDG_STATE_HOME/bingux-browser/profiles/<id>/crashes` |
//! | its engine sockets | `$XDG_RUNTIME_DIR/bingux-browser/<id>` |
//!
//! `BINGUX_BROWSER_PROFILE` names a profile folder outside the registry, for
//! tests and development: everything of it stays below that folder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::profiles::registry::Id;

/// The browser's own top folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// `$XDG_DATA_HOME/bingux-browser`.
    pub data: PathBuf,
    /// `$XDG_CACHE_HOME/bingux-browser`.
    pub cache: PathBuf,
    /// `$XDG_STATE_HOME/bingux-browser`.
    pub state: PathBuf,
    /// `$XDG_RUNTIME_DIR/bingux-browser`, when there is a runtime folder.
    pub runtime: Option<PathBuf>,
    /// `BINGUX_BROWSER_PROFILE`: a profile folder outside the registry.
    pub named: Option<PathBuf>,
}

/// The folders of one profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folders {
    /// The profile folder: its lock, settings and store, and `engine/`.
    pub root: PathBuf,
    /// The engine's profile.
    pub engine: PathBuf,
    /// The engine's cache.
    pub cache: PathBuf,
    /// Where the engine's crash dumps go.
    pub crash: PathBuf,
    /// Where the engine's sockets go; `None` puts them below the engine's profile.
    pub runtime: Option<PathBuf>,
}

impl Folders {
    pub fn lock(&self) -> PathBuf {
        self.root.join("lock")
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn store(&self) -> PathBuf {
        self.root.join("browser.sqlite")
    }
}

impl Places {
    /// The places of this process's environment.
    pub fn from_environment() -> Self {
        Self::from_lookup(&|name| std::env::var_os(name))
    }

    /// [`Places::from_environment`] over any source of variables. A relative
    /// path is ignored, as the XDG specification says.
    pub fn from_lookup(var_os: &dyn Fn(&str) -> Option<OsString>) -> Self {
        let path = |name: &str| {
            var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let home = path("HOME").unwrap_or_else(|| PathBuf::from("/"));
        let base = |name: &str, fallback: &str| path(name).unwrap_or_else(|| home.join(fallback));
        Self {
            data: base("XDG_DATA_HOME", ".local/share").join("bingux-browser"),
            cache: base("XDG_CACHE_HOME", ".cache").join("bingux-browser"),
            state: base("XDG_STATE_HOME", ".local/state").join("bingux-browser"),
            runtime: path("XDG_RUNTIME_DIR").map(|run| run.join("bingux-browser")),
            named: path("BINGUX_BROWSER_PROFILE"),
        }
    }

    pub fn registry(&self) -> PathBuf {
        self.data.join("profiles.json")
    }

    pub fn registry_lock(&self) -> PathBuf {
        self.data.join("profiles.lock")
    }

    pub fn profiles(&self) -> PathBuf {
        self.data.join("profiles")
    }

    /// The single engine profile of the browser before profiles existed.
    pub fn old_profile(&self) -> PathBuf {
        self.data.join("profile")
    }

    /// Its cache and crash folders.
    pub fn old_cache(&self) -> PathBuf {
        self.cache.join("engine")
    }

    pub fn old_crashes(&self) -> PathBuf {
        self.state.join("crashes")
    }

    /// The folders of registered profile `id`.
    pub fn folders(&self, id: &Id) -> Folders {
        let root = self.profiles().join(id.as_str());
        Folders {
            engine: root.join("engine"),
            root,
            cache: self.cache.join("profiles").join(id.as_str()),
            crash: self
                .state
                .join("profiles")
                .join(id.as_str())
                .join("crashes"),
            runtime: self.runtime.as_ref().map(|run| run.join(id.as_str())),
        }
    }

    /// The folders of the profile `BINGUX_BROWSER_PROFILE` names: all below it.
    pub fn named_folders(root: &Path) -> Folders {
        Folders {
            engine: root.join("engine"),
            cache: root.join("cache"),
            crash: root.join("crashes"),
            runtime: None,
            root: root.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn places(vars: &[(&str, &str)]) -> Places {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        Places::from_lookup(&move |name| vars.get(name).map(OsString::from))
    }

    #[test]
    fn a_profiles_folders_are_its_own_in_each_xdg_place() {
        let places = places(&[
            ("HOME", "/h"),
            ("XDG_DATA_HOME", "/data"),
            ("XDG_CACHE_HOME", "/cache"),
            ("XDG_STATE_HOME", "/state"),
            ("XDG_RUNTIME_DIR", "/run/u"),
        ]);
        let id = Id::parse("k3p7x2qa").unwrap();
        let folders = places.folders(&id);
        assert_eq!(
            folders.root,
            Path::new("/data/bingux-browser/profiles/k3p7x2qa")
        );
        assert_eq!(
            folders.engine,
            Path::new("/data/bingux-browser/profiles/k3p7x2qa/engine")
        );
        assert_eq!(
            folders.cache,
            Path::new("/cache/bingux-browser/profiles/k3p7x2qa")
        );
        assert_eq!(
            folders.crash,
            Path::new("/state/bingux-browser/profiles/k3p7x2qa/crashes")
        );
        assert_eq!(
            folders.runtime.as_deref(),
            Some(Path::new("/run/u/bingux-browser/k3p7x2qa"))
        );
        assert_eq!(
            folders.store(),
            Path::new("/data/bingux-browser/profiles/k3p7x2qa/browser.sqlite")
        );
        assert_eq!(
            places.registry(),
            Path::new("/data/bingux-browser/profiles.json")
        );
        assert_eq!(
            places.old_profile(),
            Path::new("/data/bingux-browser/profile")
        );
    }

    #[test]
    fn without_xdg_variables_everything_is_below_home_and_a_relative_path_is_ignored() {
        let places = places(&[("HOME", "/h"), ("XDG_DATA_HOME", "relative")]);
        assert_eq!(places.data, Path::new("/h/.local/share/bingux-browser"));
        assert_eq!(places.cache, Path::new("/h/.cache/bingux-browser"));
        assert_eq!(places.runtime, None);
        assert_eq!(places.named, None);
    }

    #[test]
    fn a_named_profile_keeps_everything_below_it() {
        let places = places(&[("HOME", "/h"), ("BINGUX_BROWSER_PROFILE", "/tmp/p")]);
        let folders = Places::named_folders(places.named.as_deref().unwrap());
        assert_eq!(folders.engine, Path::new("/tmp/p/engine"));
        assert_eq!(folders.cache, Path::new("/tmp/p/cache"));
        assert_eq!(folders.crash, Path::new("/tmp/p/crashes"));
        assert_eq!(folders.runtime, None);
    }
}
