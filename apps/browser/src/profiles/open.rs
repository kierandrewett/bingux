//! Opening a profile when the browser starts, before the window exists: finish
//! any delete cut short, set the profiles up on the first start, find the
//! profile asked for, take its lock and read its settings. Each way this can
//! fail is a [`ProfileState`] the window shows instead of the browser.
//!
//! A private window opens no profile: it reads the registry and one
//! profile's settings with plain reads, and writes nothing.

use bingux_ui::prelude::tr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::first_run::{self, FirstRun, Migrated, MigrationError};
use crate::profiles::{Entry, Id, Profiles, RegistryProblem, Startup};
use crate::profiles::{Folders, Places};
use crate::profiles::{LockError, ProfileLock};
use crate::settings::{Settings, SettingsProblem};
use crate::store::{Files, Problem};

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
    pub fn title(&self) -> String {
        match self {
            Self::InUse { .. } => tr!("browser-profile-in-use-title"),
            Self::RegistryDamaged { .. } => tr!("browser-profile-damaged-title"),
            Self::RegistryTooNew { .. } => tr!("browser-profile-too-new-title"),
            Self::MoveWaiting { .. } => tr!("browser-profile-move-waiting-title"),
            Self::MoveFailed { .. } => tr!("browser-profile-move-failed-title"),
            Self::Unwritable { .. } => tr!("browser-profile-unwritable-title"),
            Self::UnknownProfile { .. } => tr!("browser-profile-unknown-title"),
        }
    }

    /// What happened and what the person can do.
    pub fn text(&self) -> String {
        match self {
            Self::InUse { name, .. } => {
                tr!("browser-profile-in-use-description", name = name.as_str())
            }
            Self::RegistryDamaged { .. } => tr!("browser-profile-damaged-description"),
            Self::RegistryTooNew { .. } => {
                tr!("browser-profile-too-new-description")
            }
            Self::MoveWaiting { .. } => {
                tr!("browser-profile-move-waiting-description")
            }
            Self::MoveFailed { old, problem } => tr!(
                "browser-profile-move-failed-description",
                path = old.display().to_string(),
                problem = problem.text()
            ),
            Self::Unwritable { path, problem } => {
                tr!(
                    "browser-profile-unwritable-description",
                    path = path.display().to_string(),
                    problem = problem.text()
                )
            }
            Self::UnknownProfile { asked } => {
                tr!(
                    "browser-profile-unknown-description",
                    asked = asked.as_str()
                )
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
    /// The profile is open in another copy, which took this launch's addresses.
    HandedOff,
}

/// How a start waits for a profile another copy holds. A copy that is closing
/// holds its profile for the seconds its engine takes to stop, so a launch
/// right behind the close waits for it rather than showing "This profile is
/// already open". While it waits it offers its addresses to the copy that has
/// the profile open, so a launch of a copy that is running is answered at once.
pub struct Waiting {
    patience: Duration,
    /// Hands this launch to the running copy with the application id given;
    /// true when it took it.
    hand_off: Box<dyn FnMut(&str) -> bool>,
}

impl Waiting {
    /// How long a launch waits for a profile in use: longer than a copy takes to close.
    pub const PATIENCE: Duration = Duration::from_secs(5);
    /// How often it tries the lock and the hand-off again.
    const STEP: Duration = Duration::from_millis(100);

    pub fn new(patience: Duration, hand_off: impl FnMut(&str) -> bool + 'static) -> Self {
        Self {
            patience,
            hand_off: Box::new(hand_off),
        }
    }

    /// No wait and no hand-off: a profile in use is said so at once.
    pub fn none() -> Self {
        Self::new(Duration::ZERO, |_| false)
    }
}

impl ProfileState {
    /// The state a registry that cannot be used gives.
    fn of_registry(problem: RegistryProblem, places: &Places) -> Self {
        match problem {
            RegistryProblem::Damaged(detail) => Self::RegistryDamaged { detail },
            RegistryProblem::TooNew { found, supported } => {
                Self::RegistryTooNew { found, supported }
            }
            RegistryProblem::Io(problem) => Self::Unwritable {
                path: places.data.clone(),
                problem,
            },
        }
    }
}

impl OpenProfile {
    /// The application id of a window of `entry`: `bingux-browser` for the
    /// default profile (and a folder outside the registry), else one per profile.
    pub fn app_id_of(entry: Option<&Entry>, default: Option<&Id>) -> String {
        match entry {
            Some(entry) if Some(&entry.id) != default => {
                format!("bingux-browser-{}", entry.id.as_str())
            }
            _ => "bingux-browser".to_string(),
        }
    }
}

impl Opened {
    /// Locks the profile in `folders` and reads its settings. A profile in
    /// use is waited for as `waiting` says.
    fn locked(
        entry: Option<Entry>,
        folders: Folders,
        stale_cache: Option<PathBuf>,
        default: Option<&Id>,
        waiting: &mut Waiting,
    ) -> Self {
        let app_id = OpenProfile::app_id_of(entry.as_ref(), default);
        let name = entry.as_ref().map_or_else(
            || folders.root.display().to_string(),
            |entry| entry.name.clone(),
        );
        let deadline = Instant::now() + waiting.patience;
        let lock = loop {
            match ProfileLock::take(&folders) {
                Ok(lock) => break lock,
                Err(LockError::InUse) => {
                    if (waiting.hand_off)(&app_id) {
                        return Self::HandedOff;
                    }
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Self::Problem(ProfileState::InUse { name, app_id });
                    }
                    std::thread::sleep(left.min(Waiting::STEP));
                }
                Err(LockError::Io(problem)) => {
                    return Self::Problem(ProfileState::Unwritable {
                        path: folders.root.clone(),
                        problem,
                    });
                }
            }
        };
        if let Err(error) = Files::private_dir(&folders.engine) {
            return Self::Problem(ProfileState::Unwritable {
                path: folders.engine.clone(),
                problem: Problem::of(&error),
            });
        }
        let (settings, settings_problem) = Settings::read(&folders.settings());
        Self::Profile(OpenProfile {
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
    /// default one. A profile another copy holds is waited for as `waiting` says.
    pub fn start(places: &Places, asked: Option<&str>, waiting: &mut Waiting) -> Self {
        if let Some(root) = &places.named {
            return Self::locked(None, Places::named_folders(root), None, None, waiting);
        }
        let profiles = Profiles::new(places.clone());
        // A failure here is the registry's, and the start below reports it.
        let _ = profiles.finish_deletions();
        let stale_cache = match FirstRun::run(&profiles) {
            Ok(Migrated::Moved { stale_cache, .. }) => stale_cache,
            Ok(Migrated::Created(_) | Migrated::Nothing) => None,
            Err(MigrationError::Waiting { old }) => {
                return Self::Problem(ProfileState::MoveWaiting { old });
            }
            Err(MigrationError::Failed { old, problem }) => {
                return Self::Problem(ProfileState::MoveFailed { old, problem });
            }
            Err(MigrationError::Registry(problem)) => {
                return Self::Problem(ProfileState::of_registry(problem, places));
            }
        };
        let default = profiles
            .read()
            .ok()
            .flatten()
            .and_then(|registry| registry.default_entry().map(|entry| entry.id.clone()));
        match profiles.startup(asked) {
            Err(problem) => Self::Problem(ProfileState::of_registry(problem, places)),
            Ok(Startup::Open(entry)) => {
                let folders = places.folders(&entry.id);
                Self::locked(Some(entry), folders, stale_cache, default.as_ref(), waiting)
            }
            Ok(Startup::Ask(entries)) => Self::Choose(entries),
            Ok(Startup::Unknown(asked)) => Self::Problem(ProfileState::UnknownProfile { asked }),
            // Every profile was deleted by hand: start again with one.
            Ok(Startup::Empty) => {
                match profiles.create(first_run::FIRST_NAME, first_run::FIRST_COLOUR) {
                    Ok(entry) => {
                        let folders = places.folders(&entry.id);
                        let id = entry.id.clone();
                        Self::locked(Some(entry), folders, stale_cache, Some(&id), waiting)
                    }
                    Err(crate::profiles::ChangeError::Registry(problem)) => {
                        Self::Problem(ProfileState::of_registry(problem, places))
                    }
                    Err(other) => Self::Problem(ProfileState::RegistryDamaged {
                        detail: format!("{other:?}"),
                    }),
                }
            }
        }
    }

    /// What a private window starts with: the settings of the profile `asked`
    /// names, or of the default one, read without writing anything. A profile
    /// that cannot be read gives the defaults.
    pub fn private(places: &Places, asked: Option<&str>) -> Self {
        if let Some(root) = &places.named {
            let (settings, _) = Settings::read(&Places::named_folders(root).settings());
            return Self::Private {
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
        Self::Private {
            settings,
            from: entry.map(|entry| entry.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::store::Files;

    struct Rig {
        dir: PathBuf,
        places: Places,
    }

    impl Rig {
        fn new() -> Self {
            let dir = Files::temp();
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
            let _ = Files::remove_tree(&self.dir);
        }
    }

    fn opened(result: Opened) -> OpenProfile {
        match result {
            Opened::Profile(profile) => profile,
            other => panic!("not opened: {other:?}"),
        }
    }

    /// Failure guarded against (seen on the real engine): a launch right
    /// behind a window's close found the profile still held by the copy that
    /// was closing, and showed "This profile is already open" instead of the
    /// browser.
    #[test]
    fn a_profile_its_copy_lets_go_of_soon_opens_after_a_short_wait() {
        let rig = Rig::new();
        let closing = opened(Opened::start(&rig.places, None, &mut Waiting::none()));
        let letting_go = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(closing);
        });
        let started = std::time::Instant::now();
        let mut waiting = Waiting::new(Waiting::PATIENCE, |_| false);
        let second = opened(Opened::start(&rig.places, None, &mut waiting));
        assert!(started.elapsed() >= std::time::Duration::from_millis(250));
        assert_eq!(second.entry.as_ref().unwrap().name, "Personal");
        letting_go.join().unwrap();
    }

    #[test]
    fn a_launch_the_running_copy_takes_does_not_wait() {
        let rig = Rig::new();
        let running = opened(Opened::start(&rig.places, None, &mut Waiting::none()));
        let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = std::rc::Rc::clone(&asked);
        let mut waiting = Waiting::new(Waiting::PATIENCE, move |app_id: &str| {
            seen.borrow_mut().push(app_id.to_string());
            true
        });
        let started = std::time::Instant::now();
        assert!(matches!(
            Opened::start(&rig.places, None, &mut waiting),
            Opened::HandedOff
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(*asked.borrow(), ["bingux-browser"]);
        drop(running);
    }

    #[test]
    fn a_profile_held_past_the_patience_is_in_use() {
        let rig = Rig::new();
        let holding = opened(Opened::start(&rig.places, None, &mut Waiting::none()));
        let mut waiting = Waiting::new(std::time::Duration::from_millis(300), |_: &str| false);
        let started = std::time::Instant::now();
        assert!(matches!(
            Opened::start(&rig.places, None, &mut waiting),
            Opened::Problem(ProfileState::InUse { .. })
        ));
        assert!(started.elapsed() >= std::time::Duration::from_millis(300));
        drop(holding);
    }

    #[test]
    fn the_first_start_opens_personal_and_holds_its_lock() {
        let rig = Rig::new();
        let first = opened(Opened::start(&rig.places, None, &mut Waiting::none()));
        assert_eq!(first.entry.as_ref().unwrap().name, "Personal");
        assert!(first.folders.engine.is_dir());
        let Opened::Problem(state) = Opened::start(&rig.places, None, &mut Waiting::none()) else {
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
        opened(Opened::start(&rig.places, None, &mut Waiting::none()));
    }

    #[test]
    fn another_profile_opens_beside_an_open_one() {
        let rig = Rig::new();
        let personal = opened(Opened::start(&rig.places, None, &mut Waiting::none()));
        Profiles::new(rig.places.clone())
            .create("Work", "orange")
            .unwrap();
        let work = opened(Opened::start(
            &rig.places,
            Some("work"),
            &mut Waiting::none(),
        ));
        assert_eq!(work.entry.unwrap().name, "Work");
        drop(personal);
    }

    #[test]
    fn a_profile_nobody_has_is_said_so() {
        let rig = Rig::new();
        let Opened::Problem(state) =
            Opened::start(&rig.places, Some("Study"), &mut Waiting::none())
        else {
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
        let Opened::Problem(ProfileState::RegistryDamaged { .. }) =
            Opened::start(&rig.places, None, &mut Waiting::none())
        else {
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
        let result = Opened::start(&rig.places, None, &mut Waiting::none());
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        let Opened::Problem(ProfileState::Unwritable { problem, .. }) = result else {
            panic!("a read-only folder: {result:?}");
        };
        assert_eq!(problem, Problem::ReadOnly);
    }

    #[test]
    fn ask_at_start_with_two_profiles_asks() {
        let rig = Rig::new();
        drop(opened(Opened::start(
            &rig.places,
            None,
            &mut Waiting::none(),
        )));
        let profiles = Profiles::new(rig.places.clone());
        profiles.create("Work", "orange").unwrap();
        profiles.set_ask_at_start(true).unwrap();
        let Opened::Choose(entries) = Opened::start(&rig.places, None, &mut Waiting::none()) else {
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
        let profile = opened(Opened::start(&places, None, &mut Waiting::none()));
        assert_eq!(profile.entry, None);
        assert_eq!(profile.folders.engine, rig.dir.join("named/engine"));
        assert!(!places.registry().exists(), "nothing registered");
    }

    #[test]
    fn a_private_window_reads_its_profiles_settings() {
        let rig = Rig::new();
        let personal = opened(Opened::start(&rig.places, None, &mut Waiting::none()));
        let mut chosen = Settings::default();
        chosen.search_engine = "startpage".to_string();
        Files::replace(&personal.folders.settings(), &chosen.encode()).unwrap();
        let Opened::Private { settings, from } = Opened::private(&rig.places, None) else {
            panic!("private");
        };
        assert_eq!(settings.search_engine, "startpage");
        assert_eq!(from, personal.entry.map(|entry| entry.id));
    }
}
