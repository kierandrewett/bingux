//! The registry of profiles: `profiles.json` next to the profile folders.
//!
//! Every change reads the file, changes it and replaces it whole, under
//! `flock` on `profiles.lock`, so two processes (Personal's window renaming
//! Work) never lose each other's change. A reader that changes nothing (a
//! private window) reads without the lock: it sees the old file or the new one.
//!
//! A file this build cannot read (damaged, or written by a newer browser) is
//! never replaced: every change refuses, and the window says why.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::profiles::Places;
use crate::profiles::{Held, LockError, ProfileLock};
use crate::store::{Files, Problem};

/// The registry format this build reads and writes.
pub const VERSION: u32 = 1;

/// The longest name, in characters.
pub const NAME_MAX: usize = 64;

/// A profile's id: eight letters of base32 (`a`-`z`, `2`-`7`). Never a path:
/// the folder is always `profiles/<id>`, so a hand-edited registry cannot
/// point the browser at another folder.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id(String);

impl Id {
    pub fn parse(text: &str) -> Option<Self> {
        (text.len() == 8
            && text
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'2'..=b'7')))
        .then(|| Self(text.to_string()))
    }

    /// A new id, from 40 random bits.
    pub fn random() -> Self {
        const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
        let bits = Files::random_u64();
        Self(
            (0..8)
                .map(|at| ALPHABET[((bits >> (at * 5)) & 31) as usize] as char)
                .collect(),
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Id {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| format!("{text:?} is not a profile id"))
    }
}

impl From<Id> for String {
    fn from(id: Id) -> Self {
        id.0
    }
}

/// One profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: Id,
    pub name: String,
    /// A GNOME accent name (`bingux_ui::theme::accent_names`), never a colour value.
    pub colour: String,
    /// Unix seconds.
    pub created: u64,
    /// Set before a delete removes the folders; such a profile is never
    /// listed or opened, and the next start finishes the delete.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleting: bool,
    /// Fields a newer browser added, kept as they were.
    #[serde(flatten)]
    rest: Map<String, Value>,
}

/// `profiles.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub default: Option<Id>,
    pub ask_at_start: bool,
    pub profiles: Vec<Entry>,
    #[serde(flatten)]
    rest: Map<String, Value>,
}

impl Entry {
    /// Unix seconds now.
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs())
    }

    /// A new profile called `name`, made now.
    pub fn new(id: Id, name: &str, colour: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            colour: colour.to_string(),
            created: Self::now(),
            deleting: false,
            rest: Map::new(),
        }
    }
}

impl Registry {
    /// Parses registry bytes.
    pub fn parse(bytes: &[u8]) -> Result<Registry, RegistryProblem> {
        let value: Value = serde_json::from_slice(bytes)
            .map_err(|error| RegistryProblem::Damaged(error.to_string()))?;
        // The version first: a newer file may not parse as this version's shape.
        let found = value.get("version").and_then(Value::as_u64).unwrap_or(0);
        if found > u64::from(VERSION) {
            return Err(RegistryProblem::TooNew {
                found: u32::try_from(found).unwrap_or(u32::MAX),
                supported: VERSION,
            });
        }
        serde_json::from_value(value).map_err(|error| RegistryProblem::Damaged(error.to_string()))
    }

    fn empty() -> Self {
        Self {
            version: VERSION,
            default: None,
            ask_at_start: false,
            profiles: Vec::new(),
            rest: Map::new(),
        }
    }

    /// The profiles that are not being deleted, oldest first.
    pub fn live(&self) -> impl Iterator<Item = &Entry> {
        self.profiles.iter().filter(|entry| !entry.deleting)
    }

    fn find(&self, id: &Id) -> Option<&Entry> {
        self.live().find(|entry| entry.id == *id)
    }

    fn find_mut(&mut self, id: &Id) -> Option<&mut Entry> {
        self.profiles
            .iter_mut()
            .find(|entry| !entry.deleting && entry.id == *id)
    }

    /// The default, or the oldest live profile when the default is gone.
    pub fn default_entry(&self) -> Option<&Entry> {
        self.default
            .as_ref()
            .and_then(|id| self.find(id))
            .or_else(|| self.live().next())
    }

    /// `name` trimmed, if it is a name no other profile (but `except`) has.
    fn check_name(&self, name: &str, except: Option<&Id>) -> Result<String, NameProblem> {
        let name = name.trim();
        if name.is_empty() {
            return Err(NameProblem::Empty);
        }
        if name.chars().count() > NAME_MAX {
            return Err(NameProblem::TooLong);
        }
        let lowered = name.to_lowercase();
        if self
            .live()
            .any(|entry| Some(&entry.id) != except && entry.name.to_lowercase() == lowered)
        {
            return Err(NameProblem::Taken);
        }
        Ok(name.to_string())
    }
}

/// Why the registry could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryProblem {
    /// The file is not a registry: what is wrong with it.
    Damaged(String),
    /// A newer browser wrote it.
    TooNew { found: u32, supported: u32 },
    /// The file or its folder could not be read or written.
    Io(Problem),
}

impl RegistryProblem {
    /// A failed read or write of the registry.
    pub fn of_io(error: &std::io::Error) -> Self {
        Self::Io(Problem::of(error))
    }
}

/// What is wrong with a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    TooLong,
    /// Another profile has it (case does not count).
    Taken,
}

/// Why a change was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeError {
    Name(NameProblem),
    /// Not one of bingux-ui's accent names.
    UnknownColour,
    NoSuchProfile,
    /// The last profile cannot be deleted.
    LastProfile,
    /// The profile is open in a browser window.
    InUse,
    Registry(RegistryProblem),
}

impl From<RegistryProblem> for ChangeError {
    fn from(problem: RegistryProblem) -> Self {
        Self::Registry(problem)
    }
}

/// What a start opens.
#[derive(Clone, Debug, PartialEq)]
pub enum Startup {
    /// This profile.
    Open(Entry),
    /// Ask the person which one (two or more profiles, Ask at start on).
    Ask(Vec<Entry>),
    /// `--profile` named no profile.
    Unknown(String),
    /// There are no profiles yet.
    Empty,
}

/// The registry of one data folder.
#[derive(Clone, Debug)]
pub struct Profiles {
    places: Places,
}

impl Profiles {
    pub fn new(places: Places) -> Self {
        Self { places }
    }

    pub fn places(&self) -> &Places {
        &self.places
    }

    /// The registry, or `None` when there is none yet.
    pub fn read(&self) -> Result<Option<Registry>, RegistryProblem> {
        match Files::read_if_present(&self.places.registry()) {
            Ok(None) => Ok(None),
            Ok(Some(bytes)) => Registry::parse(&bytes).map(Some),
            Err(error) => Err(RegistryProblem::of_io(&error)),
        }
    }

    /// The profiles, oldest first.
    pub fn list(&self) -> Result<Vec<Entry>, RegistryProblem> {
        Ok(self
            .read()?
            .map(|registry| registry.live().cloned().collect())
            .unwrap_or_default())
    }

    /// Reads the registry (an empty one when there is none), lets `change`
    /// change it and writes it back, all under the registry's lock. Nothing
    /// is written when `change` refuses.
    pub(crate) fn change<T, E: From<RegistryProblem>>(
        &self,
        change: impl FnOnce(&mut Registry) -> Result<T, E>,
    ) -> Result<T, E> {
        Files::private_dir(&self.places.data).map_err(|error| RegistryProblem::of_io(&error))?;
        let _held = Held::wait(&self.places.registry_lock())
            .map_err(|error| RegistryProblem::of_io(&error))?;
        let mut registry = self.read()?.unwrap_or_else(Registry::empty);
        let result = change(&mut registry)?;
        let mut bytes = serde_json::to_vec_pretty(&registry)
            .map_err(|error| RegistryProblem::Damaged(error.to_string()))?;
        bytes.push(b'\n');
        Files::replace(&self.places.registry(), &bytes)
            .map_err(|error| RegistryProblem::of_io(&error))?;
        Ok(result)
    }

    /// A new profile with its folders. The first profile becomes the default.
    pub fn create(&self, name: &str, colour: &str) -> Result<Entry, ChangeError> {
        if !bingux_ui::theme::accent_names().any(|known| known == colour) {
            return Err(ChangeError::UnknownColour);
        }
        self.change(|registry| {
            let name = registry.check_name(name, None).map_err(ChangeError::Name)?;
            let id = loop {
                let id = Id::random();
                if registry.profiles.iter().all(|entry| entry.id != id) {
                    break id;
                }
            };
            let folders = self.places.folders(&id);
            Files::private_dir(&self.places.profiles())
                .map_err(|error| RegistryProblem::of_io(&error))?;
            Files::private_dir(&folders.engine).map_err(|error| RegistryProblem::of_io(&error))?;
            let entry = Entry::new(id.clone(), &name, colour);
            registry.profiles.push(entry.clone());
            if registry
                .default
                .as_ref()
                .and_then(|default| registry.find(default))
                .is_none()
            {
                registry.default = Some(id);
            }
            Ok(entry)
        })
    }

    pub fn rename(&self, id: &Id, name: &str) -> Result<(), ChangeError> {
        self.change(|registry| {
            let name = registry
                .check_name(name, Some(id))
                .map_err(ChangeError::Name)?;
            registry
                .find_mut(id)
                .ok_or(ChangeError::NoSuchProfile)?
                .name = name;
            Ok(())
        })
    }

    pub fn set_colour(&self, id: &Id, colour: &str) -> Result<(), ChangeError> {
        if !bingux_ui::theme::accent_names().any(|known| known == colour) {
            return Err(ChangeError::UnknownColour);
        }
        self.change(|registry| {
            registry
                .find_mut(id)
                .ok_or(ChangeError::NoSuchProfile)?
                .colour = colour.to_string();
            Ok(())
        })
    }

    pub fn set_default(&self, id: &Id) -> Result<(), ChangeError> {
        self.change(|registry| {
            registry.find(id).ok_or(ChangeError::NoSuchProfile)?;
            registry.default = Some(id.clone());
            Ok(())
        })
    }

    /// Ask which profile at start. It only asks with two or more profiles.
    pub fn set_ask_at_start(&self, ask: bool) -> Result<(), ChangeError> {
        self.change(|registry| {
            registry.ask_at_start = ask;
            Ok(())
        })
    }

    /// Deletes a profile and every folder of it. Refused for the last profile
    /// and for one that is open. The default moves to the oldest profile left.
    pub fn delete(&self, id: &Id) -> Result<(), ChangeError> {
        // Checked before the lock is taken: taking it makes a missing folder.
        let registry = self.read()?.ok_or(ChangeError::NoSuchProfile)?;
        if registry.find(id).is_none() {
            return Err(ChangeError::NoSuchProfile);
        }
        if registry.live().count() < 2 {
            return Err(ChangeError::LastProfile);
        }
        let folders = self.places.folders(id);
        let held = match ProfileLock::take(&folders) {
            Ok(held) => held,
            Err(LockError::InUse) => return Err(ChangeError::InUse),
            Err(LockError::Io(problem)) => {
                return Err(ChangeError::Registry(RegistryProblem::Io(problem)));
            }
        };
        self.change(|registry| {
            if registry.find(id).is_none() {
                return Err(ChangeError::NoSuchProfile);
            }
            if registry.live().count() < 2 {
                return Err(ChangeError::LastProfile);
            }
            if let Some(entry) = registry.find_mut(id) {
                entry.deleting = true;
            }
            if registry.default.as_ref() == Some(id) {
                let oldest = registry.live().next().map(|entry| entry.id.clone());
                registry.default = oldest;
            }
            Ok(())
        })?;
        let removed = self.remove_folders(id);
        drop(held);
        removed?;
        self.change(|registry| {
            registry.profiles.retain(|entry| entry.id != *id);
            Ok(())
        })
    }

    fn remove_folders(&self, id: &Id) -> Result<(), ChangeError> {
        let folders = self.places.folders(id);
        let crash_profile = folders.crash.parent().unwrap_or(Path::new("/nonexistent"));
        for dir in [&folders.cache, crash_profile, &folders.root] {
            Files::remove_tree(dir)
                .map_err(|error| ChangeError::Registry(RegistryProblem::of_io(&error)))?;
        }
        Ok(())
    }

    /// Finishes the deletes a process did not live to finish. A profile still
    /// open somewhere is left for a later start.
    pub fn finish_deletions(&self) -> Result<(), RegistryProblem> {
        let Some(registry) = self.read()? else {
            return Ok(());
        };
        for entry in registry.profiles.iter().filter(|entry| entry.deleting) {
            let Ok(held) = ProfileLock::take(&self.places.folders(&entry.id)) else {
                continue;
            };
            if self.remove_folders(&entry.id).is_err() {
                continue;
            }
            drop(held);
            let id = entry.id.clone();
            self.change::<_, RegistryProblem>(|registry| {
                registry.profiles.retain(|entry| entry.id != id);
                Ok(())
            })?;
        }
        Ok(())
    }

    /// After a registry that cannot be read: moves it aside (it is kept, as
    /// `profiles.json.damaged-<time>`) and lists again the profile folders that
    /// are there, as "Profile 1", "Profile 2" and so on, the first the default.
    /// Nothing in a profile folder is touched. Returns where the old list went.
    pub fn start_over(&self) -> Result<std::path::PathBuf, RegistryProblem> {
        let path = self.places.registry();
        let kept = crate::store::NotSaved::damaged_name(&path);
        {
            let _held = Held::wait(&self.places.registry_lock())
                .map_err(|error| RegistryProblem::of_io(&error))?;
            match std::fs::rename(&path, &kept) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(RegistryProblem::of_io(&error)),
            }
        }
        let mut found: Vec<Id> = std::fs::read_dir(self.places.profiles())
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                    .filter_map(|entry| Id::parse(&entry.file_name().to_string_lossy()))
                    .collect()
            })
            .unwrap_or_default();
        found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        self.change::<_, RegistryProblem>(|registry| {
            let accents: Vec<&str> = bingux_ui::theme::accent_names().collect();
            for (at, id) in found.into_iter().enumerate() {
                let colour = accents[at % accents.len()];
                registry
                    .profiles
                    .push(Entry::new(id, &format!("Profile {}", at + 1), colour));
            }
            registry.default = registry.profiles.first().map(|entry| entry.id.clone());
            Ok(())
        })?;
        Ok(kept)
    }

    /// Bytes the profile uses on disk: its folder and its cache.
    pub fn size(&self, id: &Id) -> u64 {
        let folders = self.places.folders(id);
        Files::size_of(&folders.root) + Files::size_of(&folders.cache)
    }

    /// What a start opens. `asked` is `--profile`: a name (any case) or an id.
    pub fn startup(&self, asked: Option<&str>) -> Result<Startup, RegistryProblem> {
        let Some(registry) = self.read()? else {
            return Ok(match asked {
                Some(asked) => Startup::Unknown(asked.to_string()),
                None => Startup::Empty,
            });
        };
        if let Some(asked) = asked {
            let lowered = asked.trim().to_lowercase();
            return Ok(registry
                .live()
                .find(|entry| entry.id.as_str() == asked || entry.name.to_lowercase() == lowered)
                .cloned()
                .map_or_else(|| Startup::Unknown(asked.to_string()), Startup::Open));
        }
        let live: Vec<Entry> = registry.live().cloned().collect();
        if registry.ask_at_start && live.len() >= 2 {
            return Ok(Startup::Ask(live));
        }
        Ok(registry
            .default_entry()
            .cloned()
            .map_or(Startup::Empty, Startup::Open))
    }
}
