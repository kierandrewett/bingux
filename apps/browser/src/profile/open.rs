//! Opening a profile when the browser starts, before the window exists: finish
//! any delete cut short, set the profiles up on the first start, find the
//! profile asked for, take its lock and read its settings. Each way this can
//! fail is a [`ProfileState`] the window shows instead of the browser.
//!
//! A private window opens no profile: it reads the registry and one
//! profile's settings with plain reads, and writes nothing.

use std::path::PathBuf;

use super::files::{self, Problem};
use super::lock::{LockError, ProfileLock};
use super::migrate::{self, Migrated, MigrationError};
use super::places::{Folders, Places};
use super::registry::{Entry, Id, Profiles, RegistryProblem, Startup};
use super::settings::{Settings, SettingsProblem};

/// Why no profile could be opened: what the window shows instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileState {
    /// Another Browser window has the profile open. `app_id` is that window's
    /// application id, which a launch hands its addresses to.
    InUse { name: String, app_id: String },
    /// `profiles.json` cannot be read.
    RegistryDamaged { detail: String },
    /// A newer Browser set the profiles up.
    RegistryTooNew { found: u32, supported: u32 },
    /// The first start found an older Browser still running from the old folder.
    MoveWaiting { old: PathBuf },
    /// The first start could not move the old folder.
    MoveFailed { old: PathBuf, problem: Problem },
    /// The profile's folder cannot be made or locked.
    Unwritable { path: PathBuf, problem: Problem },
    /// `--profile` named no profile.
    UnknownProfile { asked: String },
}

impl ProfileState {
    /// The page's title.
    pub fn title(&self) -> &'static str {
        match self {
            Self::InUse { .. } => "This profile is already open",
            Self::RegistryDamaged { .. } => "The list of profiles is damaged",
            Self::RegistryTooNew { .. } => "A newer Browser set up these profiles",
            Self::MoveWaiting { .. } => "Browser is still open from before the update",
            Self::MoveFailed { .. } => "Your profile could not be moved",
            Self::Unwritable { .. } => "Browser cannot keep anything",
            Self::UnknownProfile { .. } => "There is no such profile",
        }
    }

    /// What happened and what the person can do.
    pub fn text(&self) -> String {
        match self {
            Self::InUse { name, .. } => {
                format!("{name} is open in another Browser window. Use that window, or close it and try again.")
            }
            Self::RegistryDamaged { .. } => "Browser could not read the list of your profiles. \
                Start over makes a new list and keeps the damaged one beside it; your profile folders are not touched."
                .to_string(),
            Self::RegistryTooNew { .. } => {
                "Update Browser to open these profiles. Nothing was changed.".to_string()
            }
            Self::MoveWaiting { .. } => {
                "Close the older Browser window, then try again. Your profile is moved once, when nothing is using it."
                    .to_string()
            }
            Self::MoveFailed { old, problem } => format!(
                "{} could not be moved into Browser's profiles: {}. Nothing was changed.",
                old.display(),
                problem.text()
            ),
            Self::Unwritable { path, problem } => {
                format!("{} cannot be used: {}.", path.display(), problem.text())
            }
            Self::UnknownProfile { asked } => {
                format!("No profile is called \u{201c}{asked}\u{201d}.")
            }
        }
    }

    /// Whether trying again can help without changing anything first.
    pub fn can_try_again(&self) -> bool {
        matches!(
            self,
            Self::InUse { .. } | Self::MoveWaiting { .. } | Self::Unwritable { .. }
        )
    }
}

/// An open profile: its lock is held for as long as this lives.
#[derive(Debug)]
pub struct OpenProfile {
    /// The registry's entry; `None` for the folder `BINGUX_BROWSER_PROFILE` names.
    pub entry: Option<Entry>,
    pub folders: Folders,
    pub settings: Settings,
    /// Why the settings on disk are not the ones in use, if they are not.
    pub settings_problem: Option<SettingsProblem>,
    /// The old engine cache the first start left, for a thread to remove.
    pub stale_cache: Option<PathBuf>,
    pub lock: ProfileLock,
    /// The application id of the window: one per profile, so a second launch
    /// reaches the window of the profile it asked for.
    pub app_id: String,
}

/// What a start opens.
#[derive(Debug)]
pub enum Opened {
    Profile(OpenProfile),
    /// A private window: the settings it starts with, and the profile they came from.
    Private {
        settings: Settings,
        from: Option<Id>,
    },
    /// Ask which profile (two or more, Ask at start on).
    Choose(Vec<Entry>),
    Problem(ProfileState),
}

fn registry_state(problem: RegistryProblem, places: &Places) -> ProfileState {
    match problem {
        RegistryProblem::Damaged(detail) => ProfileState::RegistryDamaged { detail },
        RegistryProblem::TooNew { found, supported } => {
            ProfileState::RegistryTooNew { found, supported }
        }
        RegistryProblem::Io(problem) => ProfileState::Unwritable {
            path: places.data.clone(),
            problem,
        },
    }
}

/// The application id of a window of `entry`: `bingux-browser` for the
/// default profile (and a folder outside the registry), else one per profile.
pub fn app_id(entry: Option<&Entry>, default: Option<&Id>) -> String {
    match entry {
        Some(entry) if Some(&entry.id) != default => {
            format!("bingux-browser-{}", entry.id.as_str())
        }
        _ => "bingux-browser".to_string(),
    }
}

/// Locks the profile in `folders` and reads its settings.
fn lock_and_read(
    entry: Option<Entry>,
    folders: Folders,
    stale_cache: Option<PathBuf>,
    default: Option<&Id>,
) -> Opened {
    let app_id = app_id(entry.as_ref(), default);
    let name = entry.as_ref().map_or_else(
        || folders.root.display().to_string(),
        |entry| entry.name.clone(),
    );
    let lock = match ProfileLock::take(&folders) {
        Ok(lock) => lock,
        Err(LockError::InUse) => return Opened::Problem(ProfileState::InUse { name, app_id }),
        Err(LockError::Io(problem)) => {
            return Opened::Problem(ProfileState::Unwritable {
                path: folders.root.clone(),
                problem,
            });
        }
    };
    if let Err(error) = files::private_dir(&folders.engine) {
        return Opened::Problem(ProfileState::Unwritable {
            path: folders.engine.clone(),
            problem: Problem::of(&error),
        });
    }
    let (settings, settings_problem) = Settings::read(&folders.settings());
    Opened::Profile(OpenProfile {
        entry,
        folders,
        settings,
        settings_problem,
        stale_cache,
        lock,
        app_id,
    })
}

/// Opens the profile `asked` names (`--profile`, a name or an id), or the
/// default one.
pub fn open(places: &Places, asked: Option<&str>) -> Opened {
    if let Some(root) = &places.named {
        return lock_and_read(None, Places::named_folders(root), None, None);
    }
    let profiles = Profiles::new(places.clone());
    // A failure here is the registry's, and the start below reports it.
    let _ = profiles.finish_deletions();
    let stale_cache = match migrate::first_run(&profiles) {
        Ok(Migrated::Moved { stale_cache, .. }) => stale_cache,
        Ok(Migrated::Created(_) | Migrated::Nothing) => None,
        Err(MigrationError::Waiting { old }) => {
            return Opened::Problem(ProfileState::MoveWaiting { old });
        }
        Err(MigrationError::Failed { old, problem }) => {
            return Opened::Problem(ProfileState::MoveFailed { old, problem });
        }
        Err(MigrationError::Registry(problem)) => {
            return Opened::Problem(registry_state(problem, places));
        }
    };
    let default = profiles
        .read()
        .ok()
        .flatten()
        .and_then(|registry| registry.default_entry().map(|entry| entry.id.clone()));
    match profiles.startup(asked) {
        Err(problem) => Opened::Problem(registry_state(problem, places)),
        Ok(Startup::Open(entry)) => {
            let folders = places.folders(&entry.id);
            lock_and_read(Some(entry), folders, stale_cache, default.as_ref())
        }
        Ok(Startup::Ask(entries)) => Opened::Choose(entries),
        Ok(Startup::Unknown(asked)) => Opened::Problem(ProfileState::UnknownProfile { asked }),
        // Every profile was deleted by hand: start again with one.
        Ok(Startup::Empty) => match profiles.create(migrate::FIRST_NAME, migrate::FIRST_COLOUR) {
            Ok(entry) => {
                let folders = places.folders(&entry.id);
                let id = entry.id.clone();
                lock_and_read(Some(entry), folders, stale_cache, Some(&id))
            }
            Err(super::registry::ChangeError::Registry(problem)) => {
                Opened::Problem(registry_state(problem, places))
            }
            Err(other) => Opened::Problem(ProfileState::RegistryDamaged {
                detail: format!("{other:?}"),
            }),
        },
    }
}

/// What a private window starts with: the settings of the profile `asked`
/// names, or of the default one, read without writing anything. A profile
/// that cannot be read gives the defaults.
pub fn open_private(places: &Places, asked: Option<&str>) -> Opened {
    if let Some(root) = &places.named {
        let (settings, _) = Settings::read(&Places::named_folders(root).settings());
        return Opened::Private {
            settings,
            from: None,
        };
    }
    let profiles = Profiles::new(places.clone());
    let entry = match profiles.startup(asked) {
        Ok(Startup::Open(entry)) => Some(entry),
        Ok(Startup::Ask(_) | Startup::Unknown(_) | Startup::Empty) | Err(_) => profiles
            .read()
            .ok()
            .flatten()
            .and_then(|registry| registry.default_entry().cloned()),
    };
    let settings = entry
        .as_ref()
        .map(|entry| Settings::read(&places.folders(&entry.id).settings()).0)
        .unwrap_or_default();
    Opened::Private {
        settings,
        from: entry.map(|entry| entry.id),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::files::{remove_tree, tests::temp};
    use super::*;

    struct Rig {
        dir: PathBuf,
        places: Places,
    }

    impl Rig {
        fn new() -> Self {
            let dir = temp();
            let places = Places {
                data: dir.join("data/bingux-browser"),
                cache: dir.join("cache/bingux-browser"),
                state: dir.join("state/bingux-browser"),
                runtime: None,
                named: None,
            };
            Self { dir, places }
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = remove_tree(&self.dir);
        }
    }

    fn opened(result: Opened) -> OpenProfile {
        match result {
            Opened::Profile(profile) => profile,
            other => panic!("not opened: {other:?}"),
        }
    }

    #[test]
    fn the_first_start_opens_personal_and_holds_its_lock() {
        let rig = Rig::new();
        let first = opened(open(&rig.places, None));
        assert_eq!(first.entry.as_ref().unwrap().name, "Personal");
        assert!(first.folders.engine.is_dir());
        let Opened::Problem(state) = open(&rig.places, None) else {
            panic!("a second open of the same profile");
        };
        assert_eq!(
            state,
            ProfileState::InUse {
                name: "Personal".to_string(),
                app_id: "bingux-browser".to_string()
            }
        );
        drop(first);
        opened(open(&rig.places, None));
    }

    #[test]
    fn another_profile_opens_beside_an_open_one() {
        let rig = Rig::new();
        let personal = opened(open(&rig.places, None));
        Profiles::new(rig.places.clone())
            .create("Work", "orange")
            .unwrap();
        let work = opened(open(&rig.places, Some("work")));
        assert_eq!(work.entry.unwrap().name, "Work");
        drop(personal);
    }

    #[test]
    fn a_profile_nobody_has_is_said_so() {
        let rig = Rig::new();
        let Opened::Problem(state) = open(&rig.places, Some("Study")) else {
            panic!("no such profile");
        };
        assert_eq!(
            state,
            ProfileState::UnknownProfile {
                asked: "Study".to_string()
            }
        );
    }

    #[test]
    fn a_damaged_registry_is_a_page_not_a_new_registry() {
        let rig = Rig::new();
        fs::create_dir_all(&rig.places.data).unwrap();
        fs::write(rig.places.registry(), b"[").unwrap();
        let Opened::Problem(ProfileState::RegistryDamaged { .. }) = open(&rig.places, None) else {
            panic!("a damaged registry");
        };
        assert_eq!(fs::read(rig.places.registry()).unwrap(), b"[");
    }

    #[test]
    fn a_read_only_data_folder_is_unwritable() {
        use std::os::unix::fs::PermissionsExt;
        let rig = Rig::new();
        let parent = rig.places.data.parent().unwrap().to_path_buf();
        fs::create_dir_all(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o500)).unwrap();
        let result = open(&rig.places, None);
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        let Opened::Problem(ProfileState::Unwritable { problem, .. }) = result else {
            panic!("a read-only folder: {result:?}");
        };
        assert_eq!(problem, Problem::ReadOnly);
    }

    #[test]
    fn ask_at_start_with_two_profiles_asks() {
        let rig = Rig::new();
        drop(opened(open(&rig.places, None)));
        let profiles = Profiles::new(rig.places.clone());
        profiles.create("Work", "orange").unwrap();
        profiles.set_ask_at_start(true).unwrap();
        let Opened::Choose(entries) = open(&rig.places, None) else {
            panic!("asks");
        };
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn a_named_folder_opens_without_a_registry() {
        let rig = Rig::new();
        let places = Places {
            named: Some(rig.dir.join("named")),
            ..rig.places.clone()
        };
        let profile = opened(open(&places, None));
        assert_eq!(profile.entry, None);
        assert_eq!(profile.folders.engine, rig.dir.join("named/engine"));
        assert!(!places.registry().exists(), "nothing registered");
    }

    #[test]
    fn a_private_window_reads_its_profiles_settings() {
        let rig = Rig::new();
        let personal = opened(open(&rig.places, None));
        let mut chosen = Settings::default();
        chosen.search_engine = "startpage".to_string();
        files::replace(&personal.folders.settings(), &chosen.encode()).unwrap();
        let Opened::Private { settings, from } = open_private(&rig.places, None) else {
            panic!("private");
        };
        assert_eq!(settings.search_engine, "startpage");
        assert_eq!(from, personal.entry.map(|entry| entry.id));
    }
}
