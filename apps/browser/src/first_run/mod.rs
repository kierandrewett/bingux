//! The first start with profiles: today's single engine profile
//! (`$XDG_DATA_HOME/bingux-browser/profile`) becomes the profile "Personal",
//! moved into `profiles/<id>/engine` with one `rename` (no copy). Its crash
//! folder moves the same way; its cache is a cache and is removed.
//!
//! Nothing is moved while an older Browser still runs from that folder: Gecko
//! holds an `fcntl` write lock on its `.parentlock` for as long as it runs.

use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::profiles::registry::{Entry, Id, Profiles, RegistryProblem};
use crate::store::files::{self, Problem};

/// The name and colour of the profile made from the old folder, or of the
/// first profile when there was none.
pub const FIRST_NAME: &str = "Personal";
pub const FIRST_COLOUR: &str = bingux_ui::theme::DEFAULT_ACCENT;

/// Why the first start could not set up the profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationError {
    /// An older Browser is still running from the old folder.
    Waiting {
        old: PathBuf,
    },
    /// The old folder could not be moved (or its new place made).
    Failed {
        old: PathBuf,
        problem: Problem,
    },
    Registry(RegistryProblem),
}

impl From<RegistryProblem> for MigrationError {
    fn from(problem: RegistryProblem) -> Self {
        Self::Registry(problem)
    }
}

/// What the first start did.
#[derive(Clone, Debug, PartialEq)]
pub enum Migrated {
    /// Profiles were already set up (by an earlier start, or one racing this one).
    Nothing,
    /// There was no old folder: a new "Personal".
    Created(Entry),
    /// The old folder is "Personal" now. `stale_cache` is the old engine
    /// cache, which the caller removes off the way of the start.
    Moved {
        personal: Entry,
        stale_cache: Option<PathBuf>,
    },
}

/// The first start of the browser: what the old single profile becomes.
pub struct FirstRun;

impl FirstRun {
    /// Whether a running Gecko holds the profile lock of `engine_profile`.
    pub fn in_use(engine_profile: &Path) -> io::Result<bool> {
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(engine_profile.join(".parentlock"))
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        // SAFETY: an all-zero flock is a valid value; the fields are set below.
        let mut lock: libc::flock = unsafe { std::mem::zeroed() };
        lock.l_type = libc::F_WRLCK as libc::c_short;
        lock.l_whence = libc::SEEK_SET as libc::c_short;
        // An open-file-description lock conflicts with Gecko's classic fcntl
        // lock, even within one process; closing `file` releases ours.
        // SAFETY: the descriptor belongs to `file` and `lock` outlives the call.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_OFD_SETLK, &lock) } == 0 {
            return Ok(false);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EAGAIN | libc::EACCES) => Ok(true),
            _ => Err(error),
        }
    }

    /// Sets up the profiles on the first start, once. Does nothing when a
    /// registry exists. Runs under the registry's lock, so two first starts
    /// serialise and the second finds the work done.
    pub fn run(profiles: &Profiles) -> Result<Migrated, MigrationError> {
        if profiles.read()?.is_some() {
            return Ok(Migrated::Nothing);
        }
        let places = profiles.places().clone();
        profiles.change(|registry| {
            if !registry.profiles.is_empty() {
                return Ok(Migrated::Nothing);
            }
            let old = places.old_profile();
            let failed = |problem: Problem| MigrationError::Failed {
                old: old.clone(),
                problem,
            };
            let id = Id::random();
            let folders = places.folders(&id);
            let personal = Entry::new(id.clone(), FIRST_NAME, FIRST_COLOUR);
            registry.profiles.push(personal.clone());
            registry.default = Some(id);
            if !old.is_dir() {
                files::private_dir(&folders.engine).map_err(|error| failed(Problem::of(&error)))?;
                return Ok(Migrated::Created(personal));
            }
            if Self::in_use(&old).map_err(|error| failed(Problem::of(&error)))? {
                return Err(MigrationError::Waiting { old: old.clone() });
            }
            let moved = files::private_dir(&folders.root)
                .and_then(|()| std::fs::rename(&old, &folders.engine));
            if let Err(error) = moved {
                let _ = std::fs::remove_dir(&folders.root);
                return Err(failed(Problem::of(&error)));
            }
            // The crash folder follows when it can; a dump left behind costs
            // nothing but space.
            let old_crashes = places.old_crashes();
            if old_crashes.is_dir()
                && let Some(parent) = folders.crash.parent()
                && files::private_dir(parent).is_ok()
            {
                let _ = std::fs::rename(&old_crashes, &folders.crash);
            }
            let old_cache = places.old_cache();
            Ok(Migrated::Moved {
                personal,
                stale_cache: old_cache.is_dir().then_some(old_cache),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::profiles::places::Places;
    use crate::store::files::{remove_tree, tests::temp};

    struct Rig {
        dir: PathBuf,
        profiles: Profiles,
    }

    impl Rig {
        fn new() -> Self {
            let dir = temp();
            let profiles = Profiles::new(Places {
                data: dir.join("data/bingux-browser"),
                cache: dir.join("cache/bingux-browser"),
                state: dir.join("state/bingux-browser"),
                runtime: None,
                named: None,
            });
            Self { dir, profiles }
        }

        fn places(&self) -> &Places {
            self.profiles.places()
        }

        /// Today's layout: an engine profile with a cookie jar, a cache and a dump.
        fn old_layout(&self) {
            let places = self.places();
            fs::create_dir_all(places.old_profile()).unwrap();
            fs::write(places.old_profile().join("cookies.sqlite"), b"cookies").unwrap();
            fs::write(places.old_profile().join(".parentlock"), b"").unwrap();
            fs::create_dir_all(places.old_cache()).unwrap();
            fs::write(places.old_cache().join("entry"), b"cache").unwrap();
            fs::create_dir_all(places.old_crashes()).unwrap();
            fs::write(places.old_crashes().join("a.dmp"), b"dump").unwrap();
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = remove_tree(&self.dir);
        }
    }

    #[test]
    fn without_an_old_folder_the_first_start_makes_personal() {
        let rig = Rig::new();
        let Migrated::Created(personal) = FirstRun::run(&rig.profiles).unwrap() else {
            panic!("a new profile");
        };
        assert_eq!(
            (personal.name.as_str(), personal.colour.as_str()),
            ("Personal", "blue")
        );
        assert!(rig.places().folders(&personal.id).engine.is_dir());
        let registry = rig.profiles.read().unwrap().unwrap();
        assert_eq!(registry.default, Some(personal.id));
        assert_eq!(
            FirstRun::run(&rig.profiles).unwrap(),
            Migrated::Nothing,
            "once only"
        );
    }

    #[test]
    fn the_old_folder_becomes_personal_with_its_files_and_dumps() {
        let rig = Rig::new();
        rig.old_layout();
        let Migrated::Moved {
            personal,
            stale_cache,
        } = FirstRun::run(&rig.profiles).unwrap()
        else {
            panic!("the old folder moved");
        };
        let folders = rig.places().folders(&personal.id);
        assert_eq!(
            fs::read(folders.engine.join("cookies.sqlite")).unwrap(),
            b"cookies"
        );
        assert_eq!(fs::read(folders.crash.join("a.dmp")).unwrap(), b"dump");
        assert!(!rig.places().old_profile().exists(), "moved, not copied");
        assert!(!rig.places().old_crashes().exists());
        assert_eq!(
            stale_cache.as_deref(),
            Some(rig.places().old_cache().as_path())
        );
        assert_eq!(rig.profiles.list().unwrap(), [personal]);
    }

    #[test]
    fn nothing_moves_while_an_older_browser_runs_from_the_old_folder() {
        let rig = Rig::new();
        rig.old_layout();
        // What Gecko holds while it runs: a write lock on .parentlock.
        let gecko = OpenOptions::new()
            .read(true)
            .write(true)
            .open(rig.places().old_profile().join(".parentlock"))
            .unwrap();
        // SAFETY: as in `FirstRun::in_use`.
        let mut lock: libc::flock = unsafe { std::mem::zeroed() };
        lock.l_type = libc::F_WRLCK as libc::c_short;
        assert_eq!(
            unsafe { libc::fcntl(gecko.as_raw_fd(), libc::F_OFD_SETLK, &lock) },
            0
        );
        assert_eq!(
            FirstRun::run(&rig.profiles),
            Err(MigrationError::Waiting {
                old: rig.places().old_profile()
            })
        );
        assert!(rig.places().old_profile().join("cookies.sqlite").exists());
        assert_eq!(rig.profiles.read().unwrap(), None, "no registry written");
        assert!(
            !rig.places().profiles().exists()
                || fs::read_dir(rig.places().profiles())
                    .unwrap()
                    .next()
                    .is_none()
        );
        drop(gecko);
        assert!(
            matches!(
                FirstRun::run(&rig.profiles).unwrap(),
                Migrated::Moved { .. }
            ),
            "and moves once it is closed"
        );
    }

    #[test]
    fn a_move_that_cannot_be_made_leaves_everything_as_it_was() {
        use std::os::unix::fs::PermissionsExt;
        let rig = Rig::new();
        rig.old_layout();
        fs::create_dir_all(rig.places().profiles()).unwrap();
        fs::set_permissions(rig.places().profiles(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = FirstRun::run(&rig.profiles);
        fs::set_permissions(rig.places().profiles(), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            result,
            Err(MigrationError::Failed {
                old: rig.places().old_profile(),
                problem: Problem::ReadOnly
            })
        );
        assert!(rig.places().old_profile().join("cookies.sqlite").exists());
        assert_eq!(rig.profiles.read().unwrap(), None);
    }

    #[test]
    fn two_first_starts_at_once_make_one_personal() {
        let rig = Rig::new();
        rig.old_layout();
        let results: Vec<Migrated> = std::thread::scope(|scope| {
            let a = scope.spawn(|| FirstRun::run(&rig.profiles).unwrap());
            let b = scope.spawn(|| FirstRun::run(&rig.profiles).unwrap());
            vec![a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Migrated::Moved { .. }))
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Migrated::Nothing)
                .count(),
            1
        );
        assert_eq!(rig.profiles.list().unwrap().len(), 1);
    }
}
