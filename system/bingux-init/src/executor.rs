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
                    fstype,
                    source,
                    target,
                    subvol
                );
                #[cfg(target_os = "linux")]
                {
                    // Real implementation would call nix::mount::mount() here
                    let _ = (source, target, fstype, subvol);
                }
                Ok(())
            }
            BootStep::MountTmpfs { target, size } => {
                tracing::info!("mount -t tmpfs tmpfs {} (size={:?})", target, size);
                #[cfg(target_os = "linux")]
                {
                    let _ = (target, size);
                }
                Ok(())
            }
            BootStep::CreateDirectory { path } => {
                tracing::info!("mkdir -p {}", path);
                #[cfg(target_os = "linux")]
                {
                    let _ = path;
                }
                Ok(())
            }
            BootStep::CreateSymlink { target, link } => {
                tracing::info!("ln -s {} {}", target, link);
                #[cfg(target_os = "linux")]
                {
                    let _ = (target, link);
                }
                Ok(())
            }
            BootStep::ReadConfig { path } => {
                tracing::info!("reading config from {}", path);
                // Will read and parse system.toml in real implementation
                let _ = path;
                Ok(())
            }
            BootStep::GenerateEtc => {
                tracing::info!("generating /etc/ from system config");
                // Will use bsys-config::EtcGenerator after integration
                Ok(())
            }
            BootStep::SwitchRoot { new_root, init } => {
                tracing::info!("switch_root {} {}", new_root, init);
                #[cfg(target_os = "linux")]
                {
                    // Real implementation would call pivot_root + exec
                    let _ = (new_root, init);
                }
                Ok(())
            }
        }
    }
}
