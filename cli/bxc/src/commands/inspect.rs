use crate::output;

use bxc_sandbox::{SandboxLevel, SeccompProfile};
use bxc_shim::dispatch::resolve_dispatch_with_roots;

fn system_root() -> String {
    std::env::var("BXC_SYSTEM_ROOT").unwrap_or_else(|_| "/system".into())
}

fn home_root() -> String {
    std::env::var("BXC_HOME_ROOT").unwrap_or_else(|_| "/home".into())
}

pub fn run(package: &str) {
    let uid = nix::unistd::getuid().as_raw();
    let system = system_root();
    let home = home_root();

    match resolve_dispatch_with_roots(package, uid, &system, &home) {
        Ok(entry) => {
            output::status("inspect", &format!("package: {}", entry.package));
            output::status("inspect", &format!("binary: {}", entry.binary));
            output::status("inspect", &format!("sandbox level: {}", entry.sandbox));

            // Parse sandbox level and show seccomp profile details.
            let level = match entry.sandbox.as_str() {
                "none" => SandboxLevel::None,
                "minimal" => SandboxLevel::Minimal,
                "standard" => SandboxLevel::Standard,
                "strict" => SandboxLevel::Strict,
                other => {
                    output::status("inspect", &format!("unknown sandbox level: {other}"));
                    return;
                }
            };

            let profile = SeccompProfile::for_level(level);
            if profile.is_empty() {
                output::status("inspect", "seccomp: disabled (no filtering)");
            } else {
                output::status("inspect", &format!(
                    "seccomp: {} allowed, {} notified, {} denied",
                    profile.allow_list.len(),
                    profile.notify_list.len(),
                    profile.deny_list.len(),
                ));
            }
        }
        Err(_) => {
            // No dispatch entry found — show what a default profile would look like.
            output::status("inspect", &format!("{package}: not in dispatch table"));
            output::status("inspect", "default sandbox level: standard");
            let profile = SeccompProfile::for_level(SandboxLevel::Standard);
            output::status("inspect", &format!(
                "seccomp: {} allowed, {} notified, {} denied",
                profile.allow_list.len(),
                profile.notify_list.len(),
                profile.deny_list.len(),
            ));
        }
    }
}
