use std::path::{Path, PathBuf};

/// Well-known filesystem paths for the Bingux system layout.
///
/// All paths are absolute and correspond to the persistent `/system/`
/// and `/users/` hierarchy described in the architecture plan.
pub struct SystemPaths;

impl SystemPaths {
    // ── /system/ (persistent, root-owned) ──────────────────────

    /// Root of the Bingux system hierarchy.
    pub const SYSTEM: &str = "/system";

    /// The package store — every installed package lives here as its own directory.
    /// Layout: `/system/packages/<name>-<version>-<arch>/`
    pub const PACKAGES: &str = "/system/packages";

    /// System-level profile generations.
    /// `current` is a symlink to the active generation.
    pub const PROFILES: &str = "/system/profiles";

    /// Symlink to the active system generation.
    pub const PROFILE_CURRENT: &str = "/system/profiles/current";

    /// System configuration (git-backed).
    /// Contains `system.toml` and optional `permissions/` directory.
    pub const CONFIG: &str = "/system/config";

    /// The primary system configuration file.
    pub const SYSTEM_TOML: &str = "/system/config/system.toml";

    /// Recipe repository (git-managed).
    pub const RECIPES: &str = "/system/recipes";

    // ── /system/state/ ───────────────────────────────────────────

    /// Root of all system state.
    pub const STATE: &str = "/system/state";

    /// Persistent state — survives reboots (package DB, build locks).
    pub const STATE_PERSISTENT: &str = "/system/state/persistent";

    /// SQLite package database.
    pub const DB: &str = "/system/state/persistent/db.sqlite";

    /// Build lock directory.
    pub const LOCKS: &str = "/system/state/persistent/locks";

    /// Ephemeral state — tmpfs, cleared on reboot (sockets, PIDs, runtime).
    pub const STATE_EPHEMERAL: &str = "/system/state/ephemeral";

    /// Bingux daemon runtime (sockets, PID files).
    pub const STATE_EPHEMERAL_BINGUX: &str = "/system/state/ephemeral/bingux";

    /// System volatile package state (cleared on reboot).
    pub const VOLATILE_TOML: &str = "/system/state/ephemeral/bingux/volatile.toml";

    // ── /system/boot/ and /system/modules/ ─────────────────────

    /// Kernel and initramfs.
    pub const BOOT: &str = "/system/boot";

    /// Kernel modules and firmware.
    pub const MODULES: &str = "/system/modules";

    // ── /users/ (persistent, per-user) ─────────────────────────

    /// Top-level user home directories.
    pub const USERS: &str = "/users";

    /// Per-package metadata directory name inside each package.
    pub const BPKG_META_DIR: &str = ".bpkg";

    /// Package manifest filename.
    pub const MANIFEST_FILENAME: &str = "manifest.toml";

    /// File integrity list filename.
    pub const FILES_FILENAME: &str = "files.txt";

    /// Patchelf log filename.
    pub const PATCHELF_LOG_FILENAME: &str = "patchelf.log";

    /// Dispatch table filename inside a generation directory.
    pub const DISPATCH_FILENAME: &str = ".dispatch.toml";
}

/// Per-user paths derived from a username.
pub struct UserPaths {
    /// The user's home directory: `/users/<username>/`
    pub home: PathBuf,
    /// Bingux managed state: `~/.config/bingux/`
    pub bingux_config: PathBuf,
    /// User config (git-backed): `~/.config/bingux/config/`
    pub config: PathBuf,
    /// User's home.toml: `~/.config/bingux/config/home.toml`
    pub home_toml: PathBuf,
    /// User profile generations: `~/.config/bingux/profiles/`
    pub profiles: PathBuf,
    /// Active user profile: `~/.config/bingux/profiles/current`
    pub profile_current: PathBuf,
    /// Per-package permission grants: `~/.config/bingux/permissions/`
    pub permissions: PathBuf,
    /// Per-package sandboxed home directories: `~/.config/bingux/state/`
    pub state: PathBuf,
    /// Volatile user state (cleared on reboot): `/system/state/ephemeral/user/<uid>/`
    pub run_volatile: PathBuf,
}

impl UserPaths {
    pub fn new(username: &str, uid: u32) -> Self {
        let home = PathBuf::from(SystemPaths::USERS).join(username);
        let bingux_config = home.join(".config/bingux");
        let config = bingux_config.join("config");

        Self {
            home_toml: config.join("home.toml"),
            config,
            profiles: bingux_config.join("profiles"),
            profile_current: bingux_config.join("profiles/current"),
            permissions: bingux_config.join("permissions"),
            state: bingux_config.join("state"),
            run_volatile: PathBuf::from(SystemPaths::STATE_EPHEMERAL)
                .join("user")
                .join(uid.to_string()),
            home,
            bingux_config,
        }
    }

    /// Path to the per-package sandboxed home for a given package name.
    /// This is name-keyed (not version-keyed) by default.
    pub fn package_home(&self, package_name: &str) -> PathBuf {
        self.state.join(package_name).join("home")
    }

    /// Path to the permission TOML for a given package.
    pub fn permission_file(&self, package_name: &str) -> PathBuf {
        self.permissions.join(format!("{package_name}.toml"))
    }
}

/// Resolve the package directory path from a package ID string.
pub fn package_dir(package_id: &str) -> PathBuf {
    Path::new(SystemPaths::PACKAGES).join(package_id)
}

/// Resolve the metadata directory inside a package.
pub fn package_meta_dir(package_id: &str) -> PathBuf {
    package_dir(package_id).join(SystemPaths::BPKG_META_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_paths_layout() {
        let paths = UserPaths::new("kieran", 1000);
        assert_eq!(paths.home, PathBuf::from("/users/kieran"));
        assert_eq!(
            paths.home_toml,
            PathBuf::from("/users/kieran/.config/bingux/config/home.toml")
        );
        assert_eq!(
            paths.profile_current,
            PathBuf::from("/users/kieran/.config/bingux/profiles/current")
        );
        assert_eq!(
            paths.package_home("firefox"),
            PathBuf::from("/users/kieran/.config/bingux/state/firefox/home")
        );
        assert_eq!(
            paths.permission_file("firefox"),
            PathBuf::from("/users/kieran/.config/bingux/permissions/firefox.toml")
        );
        assert_eq!(
            paths.run_volatile,
            PathBuf::from("/system/state/ephemeral/user/1000")
        );
    }

    #[test]
    fn package_dir_from_id() {
        assert_eq!(
            package_dir("firefox-128.0.1-x86_64-linux"),
            PathBuf::from("/system/packages/firefox-128.0.1-x86_64-linux")
        );
    }
}
