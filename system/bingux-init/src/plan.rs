/// A single step in the boot sequence.
#[derive(Debug, Clone)]
pub enum BootStep {
    /// Mount a persistent filesystem (e.g. btrfs subvolume).
    MountPersistent {
        source: String,
        target: String,
        fstype: String,
        subvol: Option<String>,
    },
    /// Mount a tmpfs filesystem.
    MountTmpfs {
        target: String,
        size: Option<String>,
    },
    /// Create a directory.
    CreateDirectory {
        path: String,
    },
    /// Create a symbolic link.
    CreateSymlink {
        target: String,
        link: String,
    },
    /// Read a configuration file.
    ReadConfig {
        path: String,
    },
    /// Generate /etc/ from system configuration.
    GenerateEtc,
    /// Pivot to the real root and exec init.
    SwitchRoot {
        new_root: String,
        init: String,
    },
}

/// A complete boot plan consisting of ordered steps.
#[derive(Debug, Clone)]
pub struct BootPlan {
    pub steps: Vec<BootStep>,
}

impl BootPlan {
    /// Build the standard Bingux boot plan.
    pub fn standard() -> Self {
        Self {
            steps: vec![
                // 1. Mount persistent filesystems
                BootStep::MountPersistent {
                    source: "/dev/root".into(),
                    target: "/system".into(),
                    fstype: "btrfs".into(),
                    subvol: Some("@system".into()),
                },
                BootStep::MountPersistent {
                    source: "/dev/root".into(),
                    target: "/users".into(),
                    fstype: "btrfs".into(),
                    subvol: Some("@users".into()),
                },
                // 2. Mount ephemeral tmpfs
                BootStep::MountTmpfs {
                    target: "/etc".into(),
                    size: Some("50M".into()),
                },
                BootStep::MountTmpfs {
                    target: "/run".into(),
                    size: None,
                },
                BootStep::MountTmpfs {
                    target: "/tmp".into(),
                    size: None,
                },
                // 3. Create runtime directories
                BootStep::CreateDirectory {
                    path: "/run/bingux".into(),
                },
                BootStep::CreateDirectory {
                    path: "/run/bingux/system".into(),
                },
                // 4. Read system configuration
                BootStep::ReadConfig {
                    path: "/system/config/system.toml".into(),
                },
                // 5. Generate /etc/ from config
                BootStep::GenerateEtc,
                // 6. Create compatibility symlinks
                BootStep::CreateSymlink {
                    target: "/system/profiles/current/bin".into(),
                    link: "/bin".into(),
                },
                BootStep::CreateSymlink {
                    target: "/system/profiles/current/lib".into(),
                    link: "/lib".into(),
                },
                BootStep::CreateSymlink {
                    target: "/users".into(),
                    link: "/home".into(),
                },
                // 7. Switch root and exec systemd
                BootStep::SwitchRoot {
                    new_root: "/".into(),
                    init: "/system/profiles/current/bin/systemd".into(),
                },
            ],
        }
    }
}
