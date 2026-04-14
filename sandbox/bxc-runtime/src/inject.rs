use std::path::Path;

use anyhow::Result;

/// Inject a bind mount into a running sandbox's mount namespace.
///
/// This is used by the permission daemon (bingux-gated) when it grants a
/// file-access permission at runtime — for example, the user approves
/// access to `/dev/dri` for GPU, so we inject a bind mount of that path
/// into the sandbox.
///
/// Implementation outline (requires CAP_SYS_ADMIN):
/// 1. Open `/proc/<sandbox_pid>/ns/mnt`
/// 2. `setns(fd, CLONE_NEWNS)` to enter the sandbox's mount namespace
/// 3. Perform the bind `mount(2)` of `source` → `target`
/// 4. If `readonly`, remount with `MS_RDONLY | MS_REMOUNT | MS_BIND`
/// 5. Return to the original namespace (saved beforehand)
///
/// # Safety
///
/// This function manipulates mount namespaces of another process. It must
/// be called from a privileged context (the gated daemon running as root
/// or with `CAP_SYS_ADMIN`).
#[cfg(target_os = "linux")]
pub fn inject_mount(sandbox_pid: u32, source: &Path, target: &Path, readonly: bool) -> Result<()> {
    use std::fs::File;

    use anyhow::Context;
    use nix::mount::{MsFlags, mount};
    use nix::sched::{CloneFlags, setns};

    // Save our current mount namespace so we can return to it.
    let self_ns = File::open("/proc/self/ns/mnt").context("open own mount namespace")?;

    // Enter the sandbox's mount namespace.
    let target_ns_path = format!("/proc/{sandbox_pid}/ns/mnt");
    let target_ns = File::open(&target_ns_path)
        .with_context(|| format!("open sandbox mount namespace at {target_ns_path}"))?;
    setns(target_ns, CloneFlags::CLONE_NEWNS)
        .context("setns into sandbox mount namespace")?;

    // Perform the bind mount.
    let flags = MsFlags::MS_BIND;
    mount(Some(source), target, None::<&str>, flags, None::<&str>)
        .with_context(|| format!("bind mount {source:?} -> {target:?}"))?;

    // Optionally make it read-only.
    if readonly {
        let ro_flags = MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY;
        mount(Some(source), target, None::<&str>, ro_flags, None::<&str>)
            .with_context(|| format!("remount read-only {target:?}"))?;
    }

    // Return to our original namespace.
    setns(self_ns, CloneFlags::CLONE_NEWNS)
        .context("setns back to original mount namespace")?;

    Ok(())
}

/// Non-Linux stub — mount injection is only supported on Linux.
#[cfg(not(target_os = "linux"))]
pub fn inject_mount(
    _sandbox_pid: u32,
    _source: &Path,
    _target: &Path,
    _readonly: bool,
) -> Result<()> {
    anyhow::bail!("mount injection is only supported on Linux")
}
