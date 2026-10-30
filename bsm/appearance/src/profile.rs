//! Building and validating the greeter-facing [`UserProfile`].
//!
//! The greeter is not trusted with arbitrary paths. A profile is acceptable only when
//! every image path is exactly `<cache>/<uid>/avatar`, `<cache>/<uid>/wallpaper` or
//! `<cache>/<uid>/wallpaper-blur` for the profile's own uid, spelled canonically
//! (no `..`, no repeated or trailing slash), and, when checked against the disk, is a
//! regular file that the publisher owns and that is not a symlink.

use std::fmt;
use std::os::fd::AsRawFd;
use std::path::Path;

use bsm_proto::control::UserProfile;
use bsm_proto::state::{SessionAvailability, SessionId};

use crate::accounts::{Account, MAX_DISPLAY_NAME_CHARS, valid_user_name};
use crate::publish::{AVATAR_FILE, Published, WALLPAPER_BLUR_FILE, WALLPAPER_FILE};
use crate::record::parse_accent;
use crate::sys;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    UserName,
    DisplayName,
    Accent(String),
    /// A path field that is not exactly the expected cache file of this uid.
    Path { field: &'static str, expected: String },
    /// The account opted out but the profile still carries image paths.
    OptedOutWithImages,
    /// A blurred wallpaper path without a wallpaper path.
    BlurWithoutWallpaper,
    /// On disk: missing, a symlink, not a regular file, or not owned by the publisher.
    File { field: &'static str, why: String },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserName => f.write_str("user_name is not a valid login name"),
            Self::DisplayName => f.write_str("display_name is empty, too long or has control characters"),
            Self::Accent(accent) => write!(f, "accent {accent:?} is not canonical lower-case #rrggbb"),
            Self::Path { field, expected } => write!(f, "{field} must be exactly {expected}"),
            Self::OptedOutWithImages => f.write_str("account opted out of personal images but the profile has image paths"),
            Self::BlurWithoutWallpaper => f.write_str("wallpaper_blur_path set without wallpaper_path"),
            Self::File { field, why } => write!(f, "{field}: {why}"),
        }
    }
}

impl std::error::Error for ProfileError {}

/// The profile of `account` for the greeter. `published` is `None` for an account nothing was
/// ever published for: it then gets the neutral defaults and no images.
pub fn build_user_profile(
    account: &Account,
    published: Option<&Published>,
    availability: SessionAvailability,
    session: Option<SessionId>,
) -> UserProfile {
    let mut profile = UserProfile {
        uid: account.uid,
        user_name: account.user_name.clone(),
        display_name: account.display_name.clone(),
        avatar_path: None,
        avatar_rev: 0,
        wallpaper_path: None,
        wallpaper_blur_path: None,
        wallpaper_rev: 0,
        show_personal_images: true,
        appearance: Default::default(),
        availability,
        session,
    };
    if let Some(published) = published {
        published.apply_to(&mut profile);
    }
    profile
}

fn expected_path(cache_dir: &Path, uid: u32, name: &str) -> String {
    cache_dir.join(uid.to_string()).join(name).to_string_lossy().into_owned()
}

fn check_path(
    cache_dir: &Path,
    uid: u32,
    field: &'static str,
    actual: &Option<String>,
    name: &str,
) -> Result<(), ProfileError> {
    let Some(actual) = actual else { return Ok(()) };
    let expected = expected_path(cache_dir, uid, name);
    // String equality, not `Path` equality: `Path` ignores repeated slashes and `.` components.
    if *actual == expected { Ok(()) } else { Err(ProfileError::Path { field, expected }) }
}

/// Lexical validation of a profile about to be sent to the greeter.
pub fn validate_user_profile(profile: &UserProfile, cache_dir: &Path) -> Result<(), ProfileError> {
    if !valid_user_name(&profile.user_name) {
        return Err(ProfileError::UserName);
    }
    let name = &profile.display_name;
    if name.trim().is_empty() || name.chars().count() > MAX_DISPLAY_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(ProfileError::DisplayName);
    }
    let accent = &profile.appearance.accent;
    if parse_accent(accent).is_none() || *accent != accent.to_ascii_lowercase() {
        return Err(ProfileError::Accent(accent.clone()));
    }
    let has_images = profile.avatar_path.is_some() || profile.wallpaper_path.is_some() || profile.wallpaper_blur_path.is_some();
    if !profile.show_personal_images && has_images {
        return Err(ProfileError::OptedOutWithImages);
    }
    if profile.wallpaper_blur_path.is_some() && profile.wallpaper_path.is_none() {
        return Err(ProfileError::BlurWithoutWallpaper);
    }
    check_path(cache_dir, profile.uid, "avatar_path", &profile.avatar_path, AVATAR_FILE)?;
    check_path(cache_dir, profile.uid, "wallpaper_path", &profile.wallpaper_path, WALLPAPER_FILE)?;
    check_path(cache_dir, profile.uid, "wallpaper_blur_path", &profile.wallpaper_blur_path, WALLPAPER_BLUR_FILE)
}

/// [`validate_user_profile`] plus a look at the disk: every named image must be a regular file,
/// reached without a symlink, owned by this process's uid (the publisher).
pub fn verify_user_profile_files(profile: &UserProfile, cache_dir: &Path) -> Result<(), ProfileError> {
    validate_user_profile(profile, cache_dir)?;
    let uid = profile.uid.to_string();
    let fields = [
        ("avatar_path", &profile.avatar_path, AVATAR_FILE),
        ("wallpaper_path", &profile.wallpaper_path, WALLPAPER_FILE),
        ("wallpaper_blur_path", &profile.wallpaper_blur_path, WALLPAPER_BLUR_FILE),
    ];
    for (field, path, name) in fields {
        if path.is_none() {
            continue;
        }
        let fail = |why: String| ProfileError::File { field, why };
        let opened = sys::open_beneath(cache_dir, &[&uid, name]).map_err(|error| fail(error.to_string()))?;
        let owner = sys::fstat(opened.file.as_raw_fd()).map_err(|error| fail(error.to_string()))?.uid;
        if owner != sys::effective_uid() {
            return Err(fail(format!("owned by uid {owner}, not by the publisher")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::AccountSource;

    fn account() -> Account {
        Account {
            uid: 1000,
            gid: 1000,
            user_name: "alex".into(),
            display_name: "Alex".into(),
            icon_file: None,
            home: "/home/alex".into(),
            shell: "/bin/bash".into(),
            locked: false,
            local: true,
            source: AccountSource::PasswdOnly,
        }
    }

    fn profile() -> UserProfile {
        let mut profile = build_user_profile(&account(), None, SessionAvailability::NotSignedIn, None);
        profile.avatar_path = Some("/var/lib/bingux/greeter/1000/avatar".into());
        profile.wallpaper_path = Some("/var/lib/bingux/greeter/1000/wallpaper".into());
        profile.wallpaper_blur_path = Some("/var/lib/bingux/greeter/1000/wallpaper-blur".into());
        profile
    }

    const CACHE: &str = "/var/lib/bingux/greeter";

    #[test]
    fn neutral_profile_and_canonical_profile_validate() {
        let neutral = build_user_profile(&account(), None, SessionAvailability::NotSignedIn, None);
        assert_eq!(validate_user_profile(&neutral, Path::new(CACHE)), Ok(()));
        assert_eq!(validate_user_profile(&profile(), Path::new(CACHE)), Ok(()));
    }

    #[test]
    fn paths_outside_the_users_cache_directory_are_refused() {
        let bad = [
            "/home/alex/.face",
            "/var/lib/bingux/greeter/1001/avatar",
            "/var/lib/bingux/greeter/1000/../1001/avatar",
            "/var/lib/bingux/greeter/1000/avatar/",
            "/var/lib/bingux/greeter//1000/avatar",
            "/var/lib/bingux/greeter/1000/./avatar",
            "/var/lib/bingux/greeter/1000/wallpaper",
            "/var/lib/bingux/greeter/1000/profile.json",
            "var/lib/bingux/greeter/1000/avatar",
            "/var/lib/bingux/greeter/1000/avatar\0",
            "",
        ];
        for path in bad {
            let mut candidate = profile();
            candidate.avatar_path = Some(path.into());
            assert!(matches!(validate_user_profile(&candidate, Path::new(CACHE)), Err(ProfileError::Path { field: "avatar_path", .. })), "{path:?}");
        }
        let mut swapped = profile();
        swapped.wallpaper_path = Some("/var/lib/bingux/greeter/1000/avatar".into());
        assert!(validate_user_profile(&swapped, Path::new(CACHE)).is_err());
    }

    #[test]
    fn opt_out_may_not_carry_images() {
        let mut candidate = profile();
        candidate.show_personal_images = false;
        assert_eq!(validate_user_profile(&candidate, Path::new(CACHE)), Err(ProfileError::OptedOutWithImages));
        candidate.avatar_path = None;
        candidate.wallpaper_path = None;
        candidate.wallpaper_blur_path = None;
        assert_eq!(validate_user_profile(&candidate, Path::new(CACHE)), Ok(()));
    }

    #[test]
    fn blur_needs_a_wallpaper() {
        let mut candidate = profile();
        candidate.wallpaper_path = None;
        assert_eq!(validate_user_profile(&candidate, Path::new(CACHE)), Err(ProfileError::BlurWithoutWallpaper));
    }

    #[test]
    fn names_and_accents_are_checked() {
        let mut candidate = profile();
        candidate.appearance.accent = "#3584E4".into();
        assert!(matches!(validate_user_profile(&candidate, Path::new(CACHE)), Err(ProfileError::Accent(_))));
        let mut candidate = profile();
        candidate.display_name = "x\u{7}".into();
        assert_eq!(validate_user_profile(&candidate, Path::new(CACHE)), Err(ProfileError::DisplayName));
        let mut candidate = profile();
        candidate.user_name = "../etc".into();
        assert_eq!(validate_user_profile(&candidate, Path::new(CACHE)), Err(ProfileError::UserName));
    }
}
