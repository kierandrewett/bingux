//! The greeter appearance cache and its [`Publisher`].
//!
//! Layout, one directory per uid, owned by the account bsm runs as (mode 0755):
//!
//! ```text
//! <cache>/<uid>/avatar          sanitised PNG, at most 512x512
//! <cache>/<uid>/wallpaper       sanitised JPEG, at most 3840x2400
//! <cache>/<uid>/wallpaper-blur  blurred and dimmed JPEG, at most 960x600
//! <cache>/<uid>/profile.json    revisions, content hashes, validated appearance
//! ```
//!
//! Every file is written to a temporary name in the same directory, `fsync`ed,
//! and moved into place with `renameat`, so a reader sees the old file or the new
//! one, never half of either. `profile.json` is replaced the same way and
//! carries the revision numbers; a revision changes exactly when the content
//! of the file it names changes.
//!
//! bsm never opens a path a user gave it. Images come in as descriptors the
//! user's own process opened ([`ImageUpdate::Set`]) or from two trusted system
//! sources that bsm opens itself with `O_NOFOLLOW` below a fixed root
//! ([`Publisher::adopt_system_sources`]): the AccountsService icon directory and the
//! `bingux-session export` snapshot.

use std::fmt;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bsm_proto::control::Appearance;
use serde::{Deserialize, Serialize};

use crate::convert::{ConvertReport, ConvertRequest, Converter, ImageKind, OutputInfo, sha256_hex};
use crate::image_ops::{OutputRole, inspect_output};
use crate::record::{PublishRecord, RecordError};
use crate::sys;

pub const AVATAR_FILE: &str = "avatar";
pub const WALLPAPER_FILE: &str = "wallpaper";
pub const WALLPAPER_BLUR_FILE: &str = "wallpaper-blur";
pub const PROFILE_FILE: &str = "profile.json";
const LOCK_FILE: &str = ".lock";
/// Mode 0600, readable by the publisher only: hashes of the users' private original images.
/// `profile.json` is world readable and must not carry anything derived from a private file
/// that an observer could use to confirm a guess of it.
const PRIVATE_FILE: &str = ".private.json";
const STATE_VERSION: u32 = 1;
/// The most we read back from a file the converter wrote, or from `profile.json`.
const MAX_READBACK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 256 * 1024;
/// Where AccountsService keeps account icons.
pub const ACCOUNTS_SERVICE_ICONS: &str = "/var/lib/AccountsService/icons";

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct PublisherConfig {
    /// `paths::GREETER_CACHE_DIR` in production.
    pub cache_dir: PathBuf,
    pub converter: Converter,
    /// Trusted directories an AccountsService `IconFile` may be under.
    pub icon_roots: Vec<PathBuf>,
    /// Uids that may own an icon file and the directories below the icon root: root, who
    /// runs accounts-daemon. Anything else (a user-writable icon) is refused.
    pub trusted_icon_owners: Vec<u32>,
}

impl PublisherConfig {
    pub fn new(cache_dir: impl Into<PathBuf>, converter: Converter) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            converter,
            icon_roots: vec![PathBuf::from(ACCOUNTS_SERVICE_ICONS)],
            trusted_icon_owners: vec![0],
        }
    }

    /// Production defaults: the standard cache directory and the installed converter.
    pub fn system() -> Self {
        Self::new(bsm_proto::paths::GREETER_CACHE_DIR, Converter::system())
    }
}

/// What to do with one image.
#[derive(Debug, Default)]
pub enum ImageUpdate {
    /// Leave the published copy as it is.
    #[default]
    Keep,
    /// The account has no such image any more: remove the published copy.
    Clear,
    /// Convert this file. Opened by the account's own process; bsm only reads the descriptor.
    Set(File),
}

/// The images that accompany a [`PublishRecord`].
#[derive(Debug, Default)]
pub struct Sources {
    pub avatar: ImageUpdate,
    pub wallpaper: ImageUpdate,
}

/// Trusted, bsm-opened sources for an account that is not signed in.
#[derive(Debug, Clone, Copy)]
pub struct SystemSources<'a> {
    pub user_name: &'a str,
    /// AccountsService `IconFile`, used only if it is below one of `icon_roots`.
    pub icon_file: Option<&'a Path>,
}

#[derive(Debug)]
pub enum PublishError {
    Record(RecordError),
    Io { context: &'static str, error: io::Error },
    /// The account's cache directory exists but is not ours (wrong owner, or writable by others).
    UntrustedDirectory { path: PathBuf, why: String },
    /// `profile.json` is from a newer version than this build understands.
    UnsupportedState(u32),
}

impl PublishError {
    fn io(context: &'static str) -> impl FnOnce(io::Error) -> Self {
        move |error| Self::Io { context, error }
    }
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Record(error) => write!(f, "invalid appearance record: {error}"),
            Self::Io { context, error } => write!(f, "{context}: {error}"),
            Self::UntrustedDirectory { path, why } => write!(f, "refusing to use {}: {why}", path.display()),
            Self::UnsupportedState(version) => write!(f, "profile.json has unsupported version {version}"),
        }
    }
}

impl std::error::Error for PublishError {}

impl From<RecordError> for PublishError {
    fn from(error: RecordError) -> Self {
        Self::Record(error)
    }
}

/// A problem with one image that did not stop the rest of the publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageWarning {
    pub image: ImageKind,
    pub error: String,
    /// True when the failed source was unavailable and the earlier copy is still published.
    pub kept_previous: bool,
}

/// What the greeter may show for one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub uid: u32,
    pub avatar_path: Option<String>,
    pub avatar_rev: u64,
    pub wallpaper_path: Option<String>,
    pub wallpaper_blur_path: Option<String>,
    pub wallpaper_rev: u64,
    pub show_personal_images: bool,
    pub appearance: Appearance,
    /// Not persisted: problems of the publish call that produced this value.
    pub warnings: Vec<ImageWarning>,
}

impl Published {
    /// Copies the published fields into a greeter profile.
    pub fn apply_to(&self, profile: &mut bsm_proto::control::UserProfile) {
        profile.avatar_path = self.avatar_path.clone();
        profile.avatar_rev = self.avatar_rev;
        profile.wallpaper_path = self.wallpaper_path.clone();
        profile.wallpaper_blur_path = self.wallpaper_blur_path.clone();
        profile.wallpaper_rev = self.wallpaper_rev;
        profile.show_personal_images = self.show_personal_images;
        profile.appearance = self.appearance.clone();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Converted from a descriptor the account's own process supplied.
    User,
    /// Converted from the AccountsService icon.
    AccountIcon,
    /// Converted from the `bingux-session export` snapshot.
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ImageState {
    origin: Origin,
    /// SHA-256 of the source bytes, to skip an unchanged source. Never written to the world
    /// readable `profile.json`; it lives in the 0600 `.private.json` (see `PrivateState`).
    #[serde(skip)]
    source_sha256: String,
    /// SHA-256 of the published file.
    sha256: String,
    /// SHA-256 of the published blurred twin (wallpapers only).
    blur_sha256: Option<String>,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    version: u32,
    uid: u32,
    show_personal_images: bool,
    appearance: Appearance,
    /// Never decreases, also across removals.
    avatar_rev: u64,
    wallpaper_rev: u64,
    avatar: Option<ImageState>,
    wallpaper: Option<ImageState>,
}

/// The 0600 sidecar. Each hash is bound to the published file's hash, so a sidecar that does not
/// match `profile.json` (a crash between the two writes) is ignored and costs one reconversion.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PrivateState {
    avatar: Option<PrivateImage>,
    wallpaper: Option<PrivateImage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PrivateImage {
    published_sha256: String,
    source_sha256: String,
}

impl PrivateState {
    fn of(state: &State) -> Self {
        let image = |image: &Option<ImageState>| {
            image.as_ref().filter(|image| !image.source_sha256.is_empty()).map(|image| PrivateImage {
                published_sha256: image.sha256.clone(),
                source_sha256: image.source_sha256.clone(),
            })
        };
        Self { avatar: image(&state.avatar), wallpaper: image(&state.wallpaper) }
    }

    fn apply(self, state: &mut State) {
        for (slot, private) in [(&mut state.avatar, self.avatar), (&mut state.wallpaper, self.wallpaper)] {
            if let (Some(image), Some(private)) = (slot.as_mut(), private)
                && image.sha256 == private.published_sha256
            {
                image.source_sha256 = private.source_sha256;
            }
        }
    }
}

impl State {
    fn new(uid: u32, base_rev: u64) -> Self {
        Self {
            version: STATE_VERSION,
            uid,
            show_personal_images: true,
            appearance: Appearance::default(),
            avatar_rev: base_rev,
            wallpaper_rev: base_rev,
            avatar: None,
            wallpaper: None,
        }
    }
}

/// A temporary file in the user's directory, removed on drop unless it was moved into place.
struct Staged {
    dir: RawFd,
    name: String,
    file: File,
    placed: bool,
}

impl Staged {
    fn new(dir: RawFd, base: &str) -> Result<Self, PublishError> {
        Self::with_mode(dir, base, 0o644)
    }

    fn with_mode(dir: RawFd, base: &str, mode: u32) -> Result<Self, PublishError> {
        let counter = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!(".{base}.tmp.{}.{counter}", std::process::id());
        let file = sys::create_exclusive_mode_at(dir, &name, mode).map_err(PublishError::io("creating a temporary file"))?;
        Ok(Self { dir, name, file, placed: false })
    }

    /// The bytes the converter wrote, structurally checked against what it reported and what the
    /// published file must be (see `inspect_output`).
    fn read_back(&self, info: &OutputInfo, role: OutputRole) -> Result<Vec<u8>, String> {
        if info.bytes == 0 || info.bytes > MAX_READBACK_BYTES {
            return Err(format!("converter reported {} output bytes", info.bytes));
        }
        let mut bytes = vec![0u8; info.bytes as usize];
        self.file.read_exact_at(&mut bytes, 0).map_err(|error| format!("reading the converter output: {error}"))?;
        let mut extra = [0u8; 1];
        if self.file.read_at(&mut extra, info.bytes).map_err(|error| error.to_string())? != 0 {
            return Err("converter wrote more than it reported".into());
        }
        inspect_output(&bytes, role, info.format, info.width, info.height)?;
        Ok(bytes)
    }

    /// Makes the file durable and moves it to `final_name`.
    fn place(mut self, final_name: &str) -> Result<(), PublishError> {
        sys::fsync(self.file.as_raw_fd()).map_err(PublishError::io("syncing an image"))?;
        sys::rename_at(self.dir, &self.name, final_name).map_err(PublishError::io("moving an image into place"))?;
        self.placed = true;
        Ok(())
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.placed {
            let _ = sys::unlink_at(self.dir, &self.name);
        }
    }
}

/// Publishes appearance and images into the greeter cache.
#[derive(Debug, Clone)]
pub struct Publisher {
    config: PublisherConfig,
}

/// The per-account directory, locked for the duration of one publish.
struct Locked {
    dir: OwnedFd,
    _lock: File,
}

fn now_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(1)
}

impl Publisher {
    pub fn new(config: PublisherConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &PublisherConfig {
        &self.config
    }

    /// The directory published files of `uid` live in.
    pub fn user_dir(&self, uid: u32) -> PathBuf {
        self.config.cache_dir.join(uid.to_string())
    }

    fn open_user_dir(&self, uid: u32, create: bool) -> Result<OwnedFd, PublishError> {
        let root = sys::open_dir_follow(&self.config.cache_dir).map_err(PublishError::io("opening the cache directory"))?;
        let name = uid.to_string();
        if create {
            sys::mkdir_at(root.as_raw_fd(), &name, 0o755).map_err(PublishError::io("creating the account directory"))?;
        }
        let me = sys::effective_uid();
        let opened = match sys::open_dir_at(root.as_raw_fd(), &name) {
            Ok(dir) => match sys::fstat(dir.as_raw_fd()) {
                Ok(stat) if stat.uid == me => Ok(dir),
                Ok(stat) => Err(format!("owned by uid {}, not by the publisher (uid {me})", stat.uid)),
                Err(error) => Err(error.to_string()),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(PublishError::Io { context: "opening the account directory", error });
            }
            // A symlink (ELOOP), a file (ENOTDIR) or a directory we may not enter.
            Err(error) => Err(error.to_string()),
        };
        let dir = match opened {
            Ok(dir) => dir,
            Err(why) if create => self.replace_squatter(root.as_raw_fd(), &name, uid, why)?,
            Err(why) => return Err(PublishError::UntrustedDirectory { path: self.user_dir(uid), why }),
        };
        let stat = sys::fstat(dir.as_raw_fd()).map_err(PublishError::io("inspecting the account directory"))?;
        // A directory the publisher owns can only be loosened by the publisher or root; tighten
        // a writable-by-others mode back rather than trusting it.
        if stat.mode & 0o022 != 0 || stat.mode & 0o755 != 0o755 {
            sys::fchmod(dir.as_raw_fd(), 0o755).map_err(PublishError::io("fixing the account directory mode"))?;
        }
        Ok(dir)
    }

    /// `<cache>/<uid>` exists but is not a directory of ours: somebody squatted the name (the
    /// legacy snapshot root is world-writable, so any user can create `<cache>/<other uid>`).
    /// A fresh directory is built under a private name, exchanged with the squatter in one
    /// atomic step (a reader sees the old entry or the new directory, never nothing), and the
    /// squatter is moved to `.squatted.<uid>.<time>.<n>`, never followed, never deleted
    /// recursively. A non-directory is unlinked; an empty directory is removed.
    ///
    /// This needs write access to the cache root, i.e. the root belongs to the publisher. In a
    /// sticky root owned by someone else the rename is refused and the account stays blocked
    /// with `UntrustedDirectory`, which is the documented packaging requirement.
    fn replace_squatter(&self, root: RawFd, name: &str, uid: u32, why: String) -> Result<OwnedFd, PublishError> {
        let blocked = |error: io::Error| PublishError::UntrustedDirectory {
            path: self.user_dir(uid),
            why: format!("{why}; cannot replace it: {error}"),
        };
        let serial = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let staging = format!(".fresh.{uid}.{}.{serial}", std::process::id());
        let quarantine = format!(".squatted.{uid}.{}.{serial}", now_seconds());
        sys::mkdir_exclusive_at(root, &staging, 0o755).map_err(blocked)?;
        let undo = |error: io::Error| {
            let _ = sys::rmdir_at(root, &staging);
            blocked(error)
        };
        let fresh = sys::open_dir_at(root, &staging).map_err(undo)?;
        sys::fchmod(fresh.as_raw_fd(), 0o755).map_err(undo)?;
        match sys::exchange_at(root, &staging, name) {
            Ok(()) => sys::rename_at(root, &staging, &quarantine).map_err(blocked)?,
            // No atomic exchange (old kernel or filesystem): move the squatter away, then the
            // fresh directory in. A reader may see "no directory" for an instant: neutral.
            Err(error) if matches!(error.raw_os_error(), Some(libc::EINVAL) | Some(libc::ENOSYS)) => {
                sys::rename_at(root, name, &quarantine).map_err(undo)?;
                sys::rename_at(root, &staging, name).map_err(blocked)?;
            }
            Err(error) => return Err(undo(error)),
        }
        // Best effort tidy-up of what is cheap to remove; a populated directory stays for an
        // administrator (it is hidden: the name starts with a dot).
        if !sys::unlink_at(root, &quarantine).unwrap_or(false) {
            let _ = sys::rmdir_at(root, &quarantine);
        }
        Ok(fresh)
    }

    fn lock(&self, uid: u32) -> Result<Locked, PublishError> {
        let dir = self.open_user_dir(uid, true)?;
        let lock = sys::open_lock_at(dir.as_raw_fd(), LOCK_FILE).map_err(PublishError::io("opening the lock file"))?;
        sys::flock_exclusive(lock.as_raw_fd()).map_err(PublishError::io("locking the account directory"))?;
        Ok(Locked { dir, _lock: lock })
    }

    fn load_state(&self, dir: RawFd, uid: u32) -> Result<State, PublishError> {
        let mut state = self.load_public_state(dir, uid)?;
        if let Ok(file) = sys::open_file_at(dir, PRIVATE_FILE)
            && sys::fstat(file.as_raw_fd()).is_ok_and(|stat| sys::is_regular(stat.mode) && stat.uid == sys::effective_uid())
        {
            let mut bytes = Vec::new();
            let read = io::Read::read_to_end(&mut io::Read::take(&file, MAX_STATE_BYTES + 1), &mut bytes);
            if let Ok(Ok(private)) = read.map(|_| serde_json::from_slice::<PrivateState>(&bytes)) {
                private.apply(&mut state);
            }
        }
        Ok(state)
    }

    fn load_public_state(&self, dir: RawFd, uid: u32) -> Result<State, PublishError> {
        let any_image = [AVATAR_FILE, WALLPAPER_FILE, WALLPAPER_BLUR_FILE]
            .iter()
            .any(|name| sys::open_file_at(dir, name).is_ok());
        let fresh = |base| Ok(State::new(uid, base));
        let file = match sys::open_file_at(dir, PROFILE_FILE) {
            Ok(file) => file,
            // No state: revisions restart at 0, unless images from an earlier life are around, in
            // which case a time based base keeps the revisions ahead of anything a client cached.
            Err(error) if error.kind() == io::ErrorKind::NotFound => return fresh(if any_image { now_seconds() } else { 0 }),
            Err(error) => return Err(PublishError::Io { context: "opening profile.json", error }),
        };
        let mut bytes = Vec::new();
        let read = io::Read::read_to_end(&mut io::Read::take(&file, MAX_STATE_BYTES + 1), &mut bytes);
        match read.map(|_| serde_json::from_slice::<State>(&bytes)) {
            Ok(Ok(state)) if state.version == STATE_VERSION && state.uid == uid && bytes.len() as u64 <= MAX_STATE_BYTES => Ok(state),
            Ok(Ok(state)) if state.version > STATE_VERSION => Err(PublishError::UnsupportedState(state.version)),
            // Unreadable or damaged: start over, but never reuse a revision a client might hold.
            _ => fresh(now_seconds()),
        }
    }

    fn store_state(&self, dir: RawFd, state: &State) -> Result<(), PublishError> {
        let mut text = serde_json::to_string_pretty(state).expect("state serialises");
        text.push('\n');
        let staged = Staged::new(dir, PROFILE_FILE)?;
        io::Write::write_all(&mut &staged.file, text.as_bytes()).map_err(PublishError::io("writing profile.json"))?;
        // The private sidecar first: if we die between the two, the published hashes do not match
        // and the sidecar is ignored (one reconversion), never trusted wrongly.
        let mut private = serde_json::to_string(&PrivateState::of(state)).expect("private state serialises");
        private.push('\n');
        let sidecar = Staged::with_mode(dir, PRIVATE_FILE, 0o600)?;
        io::Write::write_all(&mut &sidecar.file, private.as_bytes()).map_err(PublishError::io("writing the private state"))?;
        sidecar.place(PRIVATE_FILE)?;
        staged.place(PROFILE_FILE)?;
        sys::fsync(dir).map_err(PublishError::io("syncing the account directory"))
    }

    fn published(&self, dir: RawFd, uid: u32, state: &State, warnings: Vec<ImageWarning>) -> Published {
        let exists = |name: &str| sys::open_file_at(dir, name).is_ok();
        let path = |name: &str| self.user_dir(uid).join(name).to_string_lossy().into_owned();
        let shown = state.show_personal_images;
        let avatar = shown && state.avatar.is_some() && exists(AVATAR_FILE);
        let wallpaper = shown && state.wallpaper.is_some() && exists(WALLPAPER_FILE);
        Published {
            uid,
            avatar_path: avatar.then(|| path(AVATAR_FILE)),
            avatar_rev: state.avatar_rev,
            wallpaper_path: wallpaper.then(|| path(WALLPAPER_FILE)),
            wallpaper_blur_path: (wallpaper && exists(WALLPAPER_BLUR_FILE)).then(|| path(WALLPAPER_BLUR_FILE)),
            wallpaper_rev: state.wallpaper_rev,
            show_personal_images: shown,
            appearance: state.appearance.clone(),
            warnings,
        }
    }

    /// The last published state of `uid`, read from disk. Used after a restart, before any
    /// user session has published again. `None` when nothing was ever published.
    pub fn load_cached(&self, uid: u32) -> Option<Published> {
        let dir = self.open_user_dir(uid, false).ok()?;
        let state = self.load_state(dir.as_raw_fd(), uid).ok()?;
        sys::open_file_at(dir.as_raw_fd(), PROFILE_FILE).ok()?;
        Some(self.published(dir.as_raw_fd(), uid, &state, Vec::new()))
    }

    /// Publishes `record` and applies `sources` for account `uid`. The uid comes from the
    /// caller's credentials (the D-Bus peer), never from the record.
    ///
    /// A bad image never fails the call and never removes what was published: it becomes a
    /// warning in the result and the earlier copy stays. A bad record fails the call as a
    /// whole and changes nothing.
    pub fn publish(&self, uid: u32, record: &PublishRecord, sources: Sources) -> Result<Published, PublishError> {
        record.validate()?;
        let locked = self.lock(uid)?;
        let dir = locked.dir.as_raw_fd();
        let mut state = self.load_state(dir, uid)?;
        let mut warnings = Vec::new();
        state.appearance = record.appearance.clone();
        state.show_personal_images = record.show_personal_images;
        if !record.show_personal_images {
            // Opt-out: personal images are removed, whatever was asked, and nothing is added.
            self.remove_images(dir, &mut state)?;
        } else {
            self.apply_update(dir, &mut state, ImageKind::Avatar, sources.avatar, Origin::User, &mut warnings)?;
            self.apply_update(dir, &mut state, ImageKind::Wallpaper, sources.wallpaper, Origin::User, &mut warnings)?;
        }
        self.store_state(dir, &state)?;
        Ok(self.published(dir, uid, &state, warnings))
    }

    /// Fills the avatar and wallpaper of `uid` from the trusted system sources, for accounts
    /// that are not signed in (their own shell cannot hand over a descriptor). A copy that came
    /// from the account's own process ([`Origin::User`]) is never replaced by a system source.
    /// Does nothing for an account that opted out.
    pub fn adopt_system_sources(&self, uid: u32, sources: &SystemSources<'_>) -> Result<Published, PublishError> {
        let locked = self.lock(uid)?;
        let dir = locked.dir.as_raw_fd();
        let mut state = self.load_state(dir, uid)?;
        let mut warnings = Vec::new();
        if state.show_personal_images {
            if state.avatar.as_ref().is_none_or(|avatar| avatar.origin != Origin::User) {
                let update = self.open_account_icon(sources.icon_file);
                self.apply_system(dir, &mut state, ImageKind::Avatar, Origin::AccountIcon, update, &mut warnings)?;
            }
            if state.wallpaper.as_ref().is_none_or(|wallpaper| wallpaper.origin != Origin::User) {
                let update = self.open_snapshot(uid, sources.user_name);
                self.apply_system(dir, &mut state, ImageKind::Wallpaper, Origin::Snapshot, update, &mut warnings)?;
            }
        }
        self.store_state(dir, &state)?;
        Ok(self.published(dir, uid, &state, warnings))
    }

    /// The AccountsService icon, opened by bsm without following any symlink below a trusted root.
    fn open_account_icon(&self, icon: Option<&Path>) -> Result<Option<File>, String> {
        let Some(icon) = icon else { return Ok(None) };
        for root in &self.config.icon_roots {
            let Ok(relative) = icon.strip_prefix(root) else { continue };
            let parts: Option<Vec<&str>> = relative.components().map(|part| part.as_os_str().to_str()).collect();
            let Some(parts) = parts else { return Err("icon path is not UTF-8".into()) };
            return match sys::open_beneath(root, &parts) {
                // The icons directory is root's; files are written by accounts-daemon as root.
                Ok(opened) if opened.owners.iter().all(|owner| self.config.trusted_icon_owners.contains(owner)) => {
                    Ok(Some(opened.file))
                }
                Ok(_) => Err("icon is not owned by a trusted uid".into()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(format!("opening the icon: {error}")),
            };
        }
        Err("icon is not below a trusted icon directory".into())
    }

    /// `<cache>/<user_name>/wallpaper` of `bingux-session export`, accepted only when the
    /// directory and the file belong to `uid` and nothing below the cache root is a symlink.
    fn open_snapshot(&self, uid: u32, user_name: &str) -> Result<Option<File>, String> {
        let plain = !user_name.is_empty()
            && !user_name.starts_with('.')
            && !user_name.bytes().all(|byte| byte.is_ascii_digit())
            && !user_name.contains(['/', '\0']);
        if !plain {
            return Err("account name cannot name a snapshot directory".into());
        }
        match sys::open_beneath(&self.config.cache_dir, &[user_name, WALLPAPER_FILE]) {
            Ok(opened) if opened.owners.iter().all(|owner| *owner == uid) => Ok(Some(opened.file)),
            Ok(_) => Err("snapshot is not owned by the account".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("opening the snapshot: {error}")),
        }
    }

    fn apply_system(
        &self,
        dir: RawFd,
        state: &mut State,
        kind: ImageKind,
        origin: Origin,
        source: Result<Option<File>, String>,
        warnings: &mut Vec<ImageWarning>,
    ) -> Result<(), PublishError> {
        let current = match kind {
            ImageKind::Avatar => &state.avatar,
            ImageKind::Wallpaper => &state.wallpaper,
        };
        match source {
            Ok(Some(file)) => self.apply_update(dir, state, kind, ImageUpdate::Set(file), origin, warnings),
            // The system source went away: drop only a copy that came from it.
            Ok(None) if current.as_ref().is_some_and(|image| image.origin == origin) => {
                self.apply_update(dir, state, kind, ImageUpdate::Clear, origin, warnings)
            }
            Ok(None) => Ok(()),
            Err(error) => {
                warnings.push(ImageWarning { image: kind, error, kept_previous: current.is_some() });
                Ok(())
            }
        }
    }

    fn remove_images(&self, dir: RawFd, state: &mut State) -> Result<(), PublishError> {
        for (kind, names) in [(ImageKind::Avatar, &[AVATAR_FILE][..]), (ImageKind::Wallpaper, &[WALLPAPER_FILE, WALLPAPER_BLUR_FILE][..])] {
            let mut removed = false;
            for name in names {
                removed |= sys::unlink_at(dir, name).map_err(PublishError::io("removing an image"))?;
            }
            match kind {
                ImageKind::Avatar => {
                    removed |= state.avatar.take().is_some();
                    state.avatar_rev += u64::from(removed);
                }
                ImageKind::Wallpaper => {
                    removed |= state.wallpaper.take().is_some();
                    state.wallpaper_rev += u64::from(removed);
                }
            }
        }
        Ok(())
    }

    fn apply_update(
        &self,
        dir: RawFd,
        state: &mut State,
        kind: ImageKind,
        update: ImageUpdate,
        origin: Origin,
        warnings: &mut Vec<ImageWarning>,
    ) -> Result<(), PublishError> {
        match update {
            ImageUpdate::Keep => Ok(()),
            ImageUpdate::Clear => {
                let names: &[&str] = match kind {
                    ImageKind::Avatar => &[AVATAR_FILE],
                    ImageKind::Wallpaper => &[WALLPAPER_FILE, WALLPAPER_BLUR_FILE],
                };
                let mut removed = false;
                for name in names {
                    removed |= sys::unlink_at(dir, name).map_err(PublishError::io("removing an image"))?;
                }
                let (slot, rev) = match kind {
                    ImageKind::Avatar => (&mut state.avatar, &mut state.avatar_rev),
                    ImageKind::Wallpaper => (&mut state.wallpaper, &mut state.wallpaper_rev),
                };
                removed |= slot.take().is_some();
                *rev += u64::from(removed);
                Ok(())
            }
            ImageUpdate::Set(file) => self.convert_and_install(dir, state, kind, &file, origin, warnings),
        }
    }

    fn convert_and_install(
        &self,
        dir: RawFd,
        state: &mut State,
        kind: ImageKind,
        source: &File,
        origin: Origin,
        warnings: &mut Vec<ImageWarning>,
    ) -> Result<(), PublishError> {
        let current = match kind {
            ImageKind::Avatar => state.avatar.clone(),
            ImageKind::Wallpaper => state.wallpaper.clone(),
        };
        let files: &[&str] = match kind {
            ImageKind::Avatar => &[AVATAR_FILE],
            ImageKind::Wallpaper => &[WALLPAPER_FILE, WALLPAPER_BLUR_FILE],
        };
        let complete = files.iter().all(|name| sys::open_file_at(dir, name).is_ok());
        let skip_hash = current.as_ref().filter(|_| complete).map(|image| image.source_sha256.clone());

        let primary = Staged::new(dir, files[0])?;
        let blur = if kind == ImageKind::Wallpaper { Some(Staged::new(dir, files[1])?) } else { None };
        let request = ConvertRequest {
            kind,
            source,
            out: &primary.file,
            blur_out: blur.as_ref().map(|staged| &staged.file),
            if_unchanged: skip_hash.as_deref(),
        };
        let report = match self.config.converter.convert(&request) {
            Ok(report) => report,
            Err(error) => {
                warnings.push(ImageWarning { image: kind, kept_previous: current.is_some(), error: error.to_string() });
                return Ok(());
            }
        };
        if report.unchanged {
            // Same source bytes as the published copy: only the origin may have changed.
            if let Some(image) = match kind {
                ImageKind::Avatar => state.avatar.as_mut(),
                ImageKind::Wallpaper => state.wallpaper.as_mut(),
            } {
                image.origin = origin;
            }
            return Ok(());
        }
        match self.install(dir, kind, primary, blur, &report, current.as_ref(), origin) {
            Ok((image, changed)) => {
                match kind {
                    ImageKind::Avatar => {
                        state.avatar = Some(image);
                        state.avatar_rev += u64::from(changed);
                    }
                    ImageKind::Wallpaper => {
                        state.wallpaper = Some(image);
                        state.wallpaper_rev += u64::from(changed);
                    }
                }
                Ok(())
            }
            Err(InstallError::Rejected(why)) => {
                warnings.push(ImageWarning { image: kind, kept_previous: current.is_some(), error: why });
                Ok(())
            }
            Err(InstallError::Fatal(error)) => Err(error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn install(
        &self,
        dir: RawFd,
        kind: ImageKind,
        primary: Staged,
        blur: Option<Staged>,
        report: &ConvertReport,
        current: Option<&ImageState>,
        origin: Origin,
    ) -> Result<(ImageState, bool), InstallError> {
        let info = report.primary.ok_or_else(|| InstallError::Rejected("converter reported no output".into()))?;
        let bytes = primary
            .read_back(&info, if kind == ImageKind::Avatar { OutputRole::Avatar } else { OutputRole::Wallpaper })
            .map_err(InstallError::Rejected)?;
        let sha256 = sha256_hex(&bytes);
        let (blur_sha256, blur_staged) = match (kind, blur, report.blurred) {
            (ImageKind::Wallpaper, Some(staged), Some(blur_info)) => {
                let blur_bytes = staged.read_back(&blur_info, OutputRole::WallpaperBlur).map_err(InstallError::Rejected)?;
                (Some(sha256_hex(&blur_bytes)), Some(staged))
            }
            (ImageKind::Wallpaper, ..) => return Err(InstallError::Rejected("converter reported no blurred output".into())),
            _ => (None, None),
        };
        let unchanged = current.is_some_and(|image| image.sha256 == sha256 && image.blur_sha256 == blur_sha256)
            && match kind {
                ImageKind::Avatar => sys::open_file_at(dir, AVATAR_FILE).is_ok(),
                ImageKind::Wallpaper => {
                    sys::open_file_at(dir, WALLPAPER_FILE).is_ok() && sys::open_file_at(dir, WALLPAPER_BLUR_FILE).is_ok()
                }
            };
        let image = ImageState {
            origin,
            source_sha256: report.source_sha256.clone(),
            sha256,
            blur_sha256,
            width: info.width,
            height: info.height,
        };
        if unchanged {
            // Identical result (a different source file that decodes to the same pixels, or a
            // changed origin): keep the files and the revision, remember the new source hash.
            return Ok((image, false));
        }
        primary.place(if kind == ImageKind::Avatar { AVATAR_FILE } else { WALLPAPER_FILE }).map_err(InstallError::Fatal)?;
        if let Some(staged) = blur_staged {
            staged.place(WALLPAPER_BLUR_FILE).map_err(InstallError::Fatal)?;
        }
        Ok((image, true))
    }
}

enum InstallError {
    /// The converter's output is not acceptable; keep what is published.
    Rejected(String),
    /// The cache itself failed.
    Fatal(PublishError),
}
