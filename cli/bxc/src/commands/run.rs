use std::path::PathBuf;

use crate::output;

use bingux_common::package_id::Arch;
use bingux_common::PackageId;
use bxc_runtime::{Sandbox, SandboxConfig};
use bxc_sandbox::SandboxLevel;
use bxc_shim::dispatch::resolve_dispatch_with_roots;
use bxc_shim::resolver::resolve_binary;
use bxc_shim::version::parse_version_syntax;

fn system_root() -> String {
    std::env::var("BXC_SYSTEM_ROOT").unwrap_or_else(|_| "/system".into())
}

fn home_root() -> String {
    std::env::var("BXC_HOME_ROOT").unwrap_or_else(|_| "/home".into())
}

fn parse_sandbox_level(s: &str) -> SandboxLevel {
    match s.to_lowercase().as_str() {
        "none" => SandboxLevel::None,
        "minimal" => SandboxLevel::Minimal,
        "standard" => SandboxLevel::Standard,
        "strict" => SandboxLevel::Strict,
        _ => SandboxLevel::Standard,
    }
}

/// Execute a binary directly without sandbox isolation.
fn exec_direct(binary_path: &std::path::Path, args: &[String]) -> ! {
    if !binary_path.exists() {
        output::status("error", &format!("binary not found: {}", binary_path.display()));
        std::process::exit(127);
    }
    let mut cmd = std::process::Command::new(binary_path);
    cmd.args(args);
    let path = std::env::var("PATH").unwrap_or_default();
    cmd.env("PATH", &path);
    match cmd.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            output::status("error", &format!("exec failed: {e}"));
            std::process::exit(127);
        }
    }
}

/// Execute a binary inside a sandboxed namespace.
fn exec_sandboxed(
    package_id: &PackageId,
    binary_path: PathBuf,
    args: &[String],
    level: SandboxLevel,
) {
    let uid = nix::unistd::getuid().as_raw();
    let gid = nix::unistd::getgid().as_raw();
    let user = std::env::var("USER").unwrap_or_else(|_| "root".into());

    let config = SandboxConfig {
        package_id: package_id.clone(),
        binary_path: binary_path.clone(),
        args: args.to_vec(),
        level,
        user,
        uid,
        gid,
    };

    let mut sandbox = Sandbox::new(config);
    sandbox.build_mount_plan();

    output::status("run", &format!("sandbox level: {:?}", level));
    output::status("run", &format!("mount entries: {}", sandbox.mount_plan.entries.len()));

    // Fork: child sets up sandbox, parent waits
    match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Child) => {
            // Child: create namespaces, apply mounts, exec
            #[cfg(target_os = "linux")]
            {
                if let Err(e) = sandbox.create_namespaces() {
                    eprintln!("bxc: namespace setup failed: {e}");
                    // Fall through to direct exec
                    exec_direct(&binary_path, args);
                }

                if let Err(e) = sandbox.apply_mounts() {
                    eprintln!("bxc: mount setup failed: {e} (continuing without mounts)");
                }

                if let Err(e) = sandbox.exec_binary() {
                    eprintln!("bxc: exec failed: {e}");
                    std::process::exit(127);
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                exec_direct(&binary_path, args);
            }
        }
        Ok(nix::unistd::ForkResult::Parent { child }) => {
            // Parent: wait for child
            match nix::sys::wait::waitpid(child, None) {
                Ok(nix::sys::wait::WaitStatus::Exited(_, code)) => {
                    std::process::exit(code);
                }
                Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => {
                    std::process::exit(128 + sig as i32);
                }
                Ok(_) => std::process::exit(1),
                Err(e) => {
                    output::status("error", &format!("wait failed: {e}"));
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            output::status("error", &format!("fork failed: {e}"));
            // Fall back to direct exec
            exec_direct(&binary_path, args);
        }
    }
}

pub fn run(package: &str, args: &[String]) {
    let (name, version) = parse_version_syntax(package);

    if let Some(ref ver) = version {
        output::status("run", &format!("explicit version: {name}@{ver}"));
    }

    let uid = nix::unistd::getuid().as_raw();
    let system = system_root();
    let home = home_root();

    match resolve_dispatch_with_roots(&name, uid, &system, &home) {
        Ok(entry) => {
            let binary_path = resolve_binary(&entry);
            output::status("run", &format!("package: {}", entry.package));
            output::status("run", &format!("binary: {}", binary_path.display()));
            output::status("run", &format!("sandbox: {}", entry.sandbox));

            let level = parse_sandbox_level(&entry.sandbox);

            match level {
                SandboxLevel::None | SandboxLevel::Minimal => {
                    exec_direct(&binary_path, args);
                }
                SandboxLevel::Standard | SandboxLevel::Strict => {
                    // Parse package ID from the dispatch entry
                    let pkg_id = entry.package.parse::<PackageId>()
                        .unwrap_or_else(|_| {
                            PackageId::new(&name, "0.0.0", Arch::X86_64Linux).unwrap()
                        });

                    exec_sandboxed(&pkg_id, binary_path, args, level);
                }
            }
        }
        Err(e) => {
            output::status("error", &format!("dispatch failed: {e}"));
        }
    }
}
