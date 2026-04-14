use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use bingux_common::paths::{SystemPaths, UserPaths};
use bxc_sandbox::SandboxLevel;

use crate::config::SandboxConfig;

/// Flags controlling how a mount entry is applied.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MountFlags {
    /// Mount read-only.
    pub readonly: bool,
    /// Disallow set-uid bits.
    pub nosuid: bool,
    /// Disallow device special files.
    pub nodev: bool,
    /// Disallow program execution.
    pub noexec: bool,
}

/// A single mount entry in the sandbox's filesystem layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountEntry {
    /// Source path on the host (empty for pseudo-filesystems like proc/tmpfs).
    pub source: PathBuf,
    /// Target path inside the sandbox.
    pub target: PathBuf,
    /// Filesystem type: "proc", "tmpfs", "devtmpfs", or None for bind mounts.
    pub fstype: Option<String>,
    /// Mount flags.
    pub flags: MountFlags,
    /// Whether this is a bind mount from the host.
    pub is_bind: bool,
}

/// A complete mount layout for a sandboxed process.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MountPlan {
    pub entries: Vec<MountEntry>,
}

impl MountPlan {
    /// Build the mount plan for the given sandbox configuration.
    ///
    /// The plan varies by sandbox level:
    /// - `None` → empty (no mount namespace)
    /// - `Minimal` / `Standard` / `Strict` → full mount layout
    pub fn build(config: &SandboxConfig) -> Self {
        if config.level == SandboxLevel::None {
            return Self::default();
        }

        let user_paths = UserPaths::new(&config.user, config.uid);
        let mut entries = Vec::new();

        // /system/packages/ → bind-mount (ro) so patchelf'd RUNPATH works
        entries.push(MountEntry {
            source: PathBuf::from(SystemPaths::PACKAGES),
            target: PathBuf::from(SystemPaths::PACKAGES),
            fstype: None,
            flags: MountFlags {
                readonly: true,
                nosuid: true,
                nodev: true,
                noexec: false,
            },
            is_bind: true,
        });

        // /proc → mount proc
        entries.push(MountEntry {
            source: PathBuf::new(),
            target: PathBuf::from("/proc"),
            fstype: Some("proc".into()),
            flags: MountFlags {
                readonly: false,
                nosuid: true,
                nodev: true,
                noexec: true,
            },
            is_bind: false,
        });

        // /dev → minimal device nodes only
        // We mount a tmpfs and then create a handful of device nodes.
        entries.push(MountEntry {
            source: PathBuf::new(),
            target: PathBuf::from("/dev"),
            fstype: Some("tmpfs".into()),
            flags: MountFlags {
                readonly: false,
                nosuid: true,
                nodev: false, // we need device nodes
                noexec: true,
            },
            is_bind: false,
        });

        // Bind-mount essential device nodes from host
        for dev in &["null", "zero", "urandom", "random"] {
            entries.push(MountEntry {
                source: PathBuf::from(format!("/dev/{dev}")),
                target: PathBuf::from(format!("/dev/{dev}")),
                fstype: None,
                flags: MountFlags {
                    readonly: false,
                    nosuid: true,
                    nodev: false,
                    noexec: true,
                },
                is_bind: true,
            });
        }

        // /dev/shm → tmpfs (many apps need this for shared memory)
        entries.push(MountEntry {
            source: PathBuf::new(),
            target: PathBuf::from("/dev/shm"),
            fstype: Some("tmpfs".into()),
            flags: MountFlags {
                readonly: false,
                nosuid: true,
                nodev: true,
                noexec: false,
            },
            is_bind: false,
        });

        // /tmp → fresh tmpfs
        entries.push(MountEntry {
            source: PathBuf::new(),
            target: PathBuf::from("/tmp"),
            fstype: Some("tmpfs".into()),
            flags: MountFlags {
                readonly: false,
                nosuid: true,
                nodev: true,
                noexec: false,
            },
            is_bind: false,
        });

        // Per-package home directory → /users/<user>/ inside sandbox
        let pkg_home = user_paths.package_home(&config.package_id.name);
        entries.push(MountEntry {
            source: pkg_home,
            target: PathBuf::from(SystemPaths::USERS).join(&config.user),
            fstype: None,
            flags: MountFlags {
                readonly: false,
                nosuid: true,
                nodev: true,
                noexec: false,
            },
            is_bind: true,
        });

        Self { entries }
    }

    /// Find a mount entry by its target path.
    pub fn entry_for_target(&self, target: &str) -> Option<&MountEntry> {
        self.entries
            .iter()
            .find(|e| e.target == PathBuf::from(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_common::package_id::Arch;
    use bingux_common::PackageId;

    fn test_config(level: SandboxLevel) -> SandboxConfig {
        SandboxConfig {
            package_id: PackageId::new("firefox", "128.0.1", Arch::X86_64Linux).unwrap(),
            binary_path: PathBuf::from("/system/packages/firefox-128.0.1-x86_64-linux/bin/firefox"),
            args: vec![],
            level,
            user: "kieran".into(),
            uid: 1000,
            gid: 1000,
        }
    }

    #[test]
    fn none_level_produces_empty_plan() {
        let plan = MountPlan::build(&test_config(SandboxLevel::None));
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn standard_has_expected_mounts() {
        let plan = MountPlan::build(&test_config(SandboxLevel::Standard));

        // /system/packages (bind, ro)
        let pkg = plan.entry_for_target("/system/packages").unwrap();
        assert!(pkg.is_bind);
        assert!(pkg.flags.readonly);

        // /proc
        let proc_mount = plan.entry_for_target("/proc").unwrap();
        assert_eq!(proc_mount.fstype.as_deref(), Some("proc"));
        assert!(!proc_mount.is_bind);

        // /dev
        let dev = plan.entry_for_target("/dev").unwrap();
        assert_eq!(dev.fstype.as_deref(), Some("tmpfs"));

        // /dev/null, /dev/zero, /dev/urandom, /dev/random
        for name in &["null", "zero", "urandom", "random"] {
            let entry = plan
                .entry_for_target(&format!("/dev/{name}"))
                .unwrap_or_else(|| panic!("/dev/{name} missing"));
            assert!(entry.is_bind);
        }

        // /dev/shm
        let shm = plan.entry_for_target("/dev/shm").unwrap();
        assert_eq!(shm.fstype.as_deref(), Some("tmpfs"));

        // /tmp
        let tmp = plan.entry_for_target("/tmp").unwrap();
        assert_eq!(tmp.fstype.as_deref(), Some("tmpfs"));

        // per-package home
        let home = plan.entry_for_target("/users/kieran").unwrap();
        assert!(home.is_bind);
        assert!(!home.flags.readonly);
        assert_eq!(
            home.source,
            PathBuf::from("/users/kieran/.config/bingux/state/firefox/home")
        );
    }

    #[test]
    fn minimal_has_same_mounts_as_standard() {
        // Minimal still gets a mount namespace, just no seccomp.
        let minimal = MountPlan::build(&test_config(SandboxLevel::Minimal));
        let standard = MountPlan::build(&test_config(SandboxLevel::Standard));
        assert_eq!(minimal.entries.len(), standard.entries.len());
    }

    #[test]
    fn proc_flags_correct() {
        let plan = MountPlan::build(&test_config(SandboxLevel::Standard));
        let proc_mount = plan.entry_for_target("/proc").unwrap();
        assert!(proc_mount.flags.nosuid);
        assert!(proc_mount.flags.nodev);
        assert!(proc_mount.flags.noexec);
    }

    #[test]
    fn tmp_flags_correct() {
        let plan = MountPlan::build(&test_config(SandboxLevel::Standard));
        let tmp = plan.entry_for_target("/tmp").unwrap();
        assert!(tmp.flags.nosuid);
        assert!(tmp.flags.nodev);
        assert!(!tmp.flags.noexec);
    }
}
