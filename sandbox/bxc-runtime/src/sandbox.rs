use std::path::Path;

use anyhow::{Context, Result};

use crate::config::SandboxConfig;
use crate::mounts::MountPlan;

/// A sandbox instance managing the lifecycle of a confined process.
///
/// The sandbox holds the configuration, the computed mount plan, and
/// (once launched) the PID of the child process.
#[derive(Debug)]
pub struct Sandbox {
    /// The configuration this sandbox was created from.
    pub config: SandboxConfig,
    /// The mount layout to apply inside the namespace.
    pub mount_plan: MountPlan,
    /// PID of the sandboxed child process (set after launch).
    pub pid: Option<u32>,
}

impl Sandbox {
    /// Create a new sandbox from configuration. The mount plan is not
    /// built yet — call `build_mount_plan()` before launching.
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            mount_plan: MountPlan::default(),
            pid: None,
        }
    }

    /// Compute the mount plan based on the sandbox configuration.
    pub fn build_mount_plan(&mut self) -> &MountPlan {
        self.mount_plan = MountPlan::build(&self.config);
        &self.mount_plan
    }

    /// Create Linux namespaces for this sandbox.
    ///
    /// Calls `unshare(2)` with the appropriate namespace flags based
    /// on the sandbox level:
    /// - Minimal/Standard: `CLONE_NEWNS` (mount namespace only)
    /// - Strict: `CLONE_NEWNS | CLONE_NEWPID | CLONE_NEWNET`
    #[cfg(target_os = "linux")]
    pub fn create_namespaces(&self) -> Result<()> {
        use bxc_sandbox::SandboxLevel;
        use nix::sched::{CloneFlags, unshare};

        if self.config.level == SandboxLevel::None {
            return Ok(());
        }

        let mut flags = CloneFlags::CLONE_NEWNS;

        if self.config.level == SandboxLevel::Strict {
            flags |= CloneFlags::CLONE_NEWPID;
            flags |= CloneFlags::CLONE_NEWNET;
        }

        unshare(flags).context("failed to unshare namespaces")?;
        Ok(())
    }

    /// Apply the mount plan inside the current (already-unshared) namespace.
    ///
    /// Iterates over `self.mount_plan.entries` and performs the corresponding
    /// `mount(2)` calls — bind mounts for host paths, pseudo-fs mounts for
    /// proc/tmpfs/devtmpfs.
    #[cfg(target_os = "linux")]
    pub fn apply_mounts(&self) -> Result<()> {
        use nix::mount::{MsFlags, mount};

        for entry in &self.mount_plan.entries {
            let mut flags = MsFlags::empty();
            if entry.flags.readonly {
                flags |= MsFlags::MS_RDONLY;
            }
            if entry.flags.nosuid {
                flags |= MsFlags::MS_NOSUID;
            }
            if entry.flags.nodev {
                flags |= MsFlags::MS_NODEV;
            }
            if entry.flags.noexec {
                flags |= MsFlags::MS_NOEXEC;
            }
            if entry.is_bind {
                flags |= MsFlags::MS_BIND;
            }

            let source: Option<&Path> = if entry.is_bind || entry.fstype.is_some() {
                if entry.source.as_os_str().is_empty() {
                    None
                } else {
                    Some(&entry.source)
                }
            } else {
                None
            };

            let fstype: Option<&str> = entry.fstype.as_deref();

            mount(source, &entry.target, fstype, flags, None::<&str>)
                .with_context(|| format!("mount failed for {:?}", entry.target))?;

            // For bind mounts that need to be read-only, we must remount
            // because MS_BIND|MS_RDONLY is silently ignored on the first call.
            if entry.is_bind && entry.flags.readonly {
                flags |= MsFlags::MS_REMOUNT;
                mount(source, &entry.target, fstype, flags, None::<&str>)
                    .with_context(|| format!("remount ro failed for {:?}", entry.target))?;
            }
        }

        Ok(())
    }

    /// Replace the current process image with the sandboxed binary.
    ///
    /// This is the final step — after namespaces are created and mounts
    /// are applied, we `execve()` into the target binary.
    #[cfg(target_os = "linux")]
    pub fn exec_binary(&self) -> Result<()> {
        use std::ffi::CString;

        use nix::unistd::execve;

        let binary = CString::new(
            self.config
                .binary_path
                .to_str()
                .context("binary path is not valid UTF-8")?,
        )
        .context("binary path contains null bytes")?;

        let mut argv = vec![binary.clone()];
        for arg in &self.config.args {
            argv.push(CString::new(arg.as_str()).context("arg contains null bytes")?);
        }

        // Inherit the current environment — the shim will have already
        // set up the sandbox-specific env vars.
        let env: Vec<CString> = std::env::vars()
            .filter_map(|(k, v)| CString::new(format!("{k}={v}")).ok())
            .collect();

        execve(&binary, &argv, &env).context("execve failed")?;

        unreachable!("execve should not return on success");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bingux_common::package_id::Arch;
    use bingux_common::PackageId;
    use bxc_sandbox::SandboxLevel;
    use std::path::PathBuf;

    fn test_config(level: SandboxLevel) -> SandboxConfig {
        SandboxConfig {
            package_id: PackageId::new("firefox", "128.0.1", Arch::X86_64Linux).unwrap(),
            binary_path: PathBuf::from("/system/packages/firefox-128.0.1-x86_64-linux/bin/firefox"),
            args: vec!["--headless".into()],
            level,
            user: "kieran".into(),
            uid: 1000,
            gid: 1000,
        }
    }

    #[test]
    fn new_sandbox_has_no_pid() {
        let sb = Sandbox::new(test_config(SandboxLevel::Standard));
        assert!(sb.pid.is_none());
    }

    #[test]
    fn build_mount_plan_populates_entries() {
        let mut sb = Sandbox::new(test_config(SandboxLevel::Standard));
        assert!(sb.mount_plan.entries.is_empty());
        sb.build_mount_plan();
        assert!(!sb.mount_plan.entries.is_empty());
    }

    #[test]
    fn none_level_has_empty_mount_plan() {
        let mut sb = Sandbox::new(test_config(SandboxLevel::None));
        sb.build_mount_plan();
        assert!(sb.mount_plan.entries.is_empty());
    }
}
