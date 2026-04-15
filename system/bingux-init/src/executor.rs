use std::fs;
use std::os::unix;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::error::InitError;
use crate::plan::{BootPlan, BootStep};

/// Executes boot plan steps sequentially.
pub struct BootExecutor;

impl BootExecutor {
    /// Execute all steps in a boot plan.
    pub fn execute_plan(plan: &BootPlan) -> Result<(), InitError> {
        for (i, step) in plan.steps.iter().enumerate() {
            tracing::info!("boot step {}: {:?}", i + 1, std::mem::discriminant(step));
            Self::execute_step(step)?;
        }
        Ok(())
    }

    fn execute_step(step: &BootStep) -> Result<(), InitError> {
        match step {
            BootStep::MountPersistent {
                source,
                target,
                fstype,
                subvol,
            } => {
                tracing::info!(
                    "mount -t {} {} {} (subvol={:?})",
                    fstype, source, target, subvol
                );
                fs::create_dir_all(target).ok();
                if let Some(sv) = subvol {
                    // Use shell to handle the subvol option
                    let cmd = if subvol.is_some() {
                        format!("mount -t {fstype} -o subvol={sv} {source} {target}")
                    } else {
                        format!("mount -t {fstype} {source} {target}")
                    };
                    let status = Command::new("sh").arg("-c").arg(&cmd).status();
                    match status {
                        Ok(s) if s.success() => return Ok(()),
                        _ => tracing::warn!("mount failed (non-fatal): {cmd}"),
                    }
                    return Ok(());
                }
                let status = Command::new("mount")
                    .arg("-t").arg(fstype)
                    .arg(source)
                    .arg(target)
                    .status();
                match status {
                    Ok(s) if s.success() => {}
                    _ => tracing::warn!("mount failed (non-fatal): {source} -> {target}"),
                }
                Ok(())
            }
            BootStep::MountTmpfs { target, size } => {
                tracing::info!("mount -t tmpfs tmpfs {} (size={:?})", target, size);
                fs::create_dir_all(target).ok();
                let mut cmd = format!("mount -t tmpfs");
                if let Some(s) = size {
                    cmd.push_str(&format!(" -o size={s}"));
                }
                cmd.push_str(&format!(" tmpfs {target}"));
                let status = Command::new("sh").arg("-c").arg(&cmd).status();
                match status {
                    Ok(s) if s.success() => {}
                    _ => tracing::warn!("tmpfs mount failed (non-fatal): {target}"),
                }
                Ok(())
            }
            BootStep::CreateDirectory { path } => {
                tracing::info!("mkdir -p {}", path);
                match fs::create_dir_all(path) {
                    Ok(()) => {}
                    Err(e) => tracing::warn!("mkdir failed (non-fatal): {path}: {e}"),
                }
                Ok(())
            }
            BootStep::CreateSymlink { target, link } => {
                tracing::info!("ln -s {} {}", target, link);
                // Remove existing link/file if present
                if Path::new(link).is_symlink() {
                    fs::remove_file(link).ok();
                }
                if let Some(parent) = Path::new(link).parent() {
                    fs::create_dir_all(parent).ok();
                }
                match unix::fs::symlink(target, link) {
                    Ok(()) => {}
                    Err(e) => tracing::warn!("symlink failed (non-fatal): {target} -> {link}: {e}"),
                }
                Ok(())
            }
            BootStep::ReadConfig { path } => {
                tracing::info!("reading config from {}", path);
                if Path::new(path).exists() {
                    let content = fs::read_to_string(path).map_err(|e| {
                        InitError::StepFailed(format!("read config {path}: {e}"))
                    })?;
                    tracing::info!("config loaded ({} bytes)", content.len());
                } else {
                    tracing::warn!("config not found: {path}");
                }
                Ok(())
            }
            BootStep::GenerateEtc => {
                tracing::info!("generating /etc/ from system config");
                // Generate essential /etc files
                fs::create_dir_all("/etc").ok();
                // /etc/passwd
                if !Path::new("/etc/passwd").exists() {
                    fs::write("/etc/passwd", "root:x:0:0:root:/root:/bin/sh\n").ok();
                }
                // /etc/group
                if !Path::new("/etc/group").exists() {
                    fs::write("/etc/group", "root:x:0:\n").ok();
                }
                // /etc/hostname
                fs::write("/etc/hostname", "bingux\n").ok();
                // /etc/os-release
                fs::write(
                    "/etc/os-release",
                    "NAME=\"Bingux\"\nID=bingux\nVERSION_ID=2\nPRETTY_NAME=\"Bingux v2\"\n",
                ).ok();
                Ok(())
            }
            BootStep::SwitchRoot { new_root, init } => {
                tracing::info!("exec {init} (root={new_root})");
                // In a real initramfs, this would pivot_root + exec.
                // For now, just exec the init directly.
                if Path::new(init).exists() {
                    let err = Command::new(init).exec();
                    // exec() only returns on error
                    return Err(InitError::StepFailed(format!("exec {init}: {err}")));
                }
                tracing::warn!("init binary not found: {init}");
                Ok(())
            }
        }
    }
}
