use crate::output;

use bxc_shim::dispatch::resolve_dispatch_with_roots;
use bxc_shim::resolver::resolve_binary;
use bxc_shim::version::parse_version_syntax;

fn system_root() -> String {
    std::env::var("BXC_SYSTEM_ROOT").unwrap_or_else(|_| "/system".into())
}

fn home_root() -> String {
    std::env::var("BXC_HOME_ROOT").unwrap_or_else(|_| "/home".into())
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

            // For sandbox=none or minimal, exec the binary directly
            let sandbox_lower = entry.sandbox.to_lowercase();
            if sandbox_lower == "none" || sandbox_lower == "minimal" {
                // Direct execution — no sandbox
                if binary_path.exists() {
                    let mut cmd = std::process::Command::new(&binary_path);
                    cmd.args(args);

                    // Set up environment
                    let path = std::env::var("PATH").unwrap_or_default();
                    cmd.env("PATH", &path);

                    match cmd.status() {
                        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
                        Err(e) => {
                            output::status("error", &format!("exec failed: {e}"));
                            std::process::exit(127);
                        }
                    }
                } else {
                    output::status("error", &format!("binary not found: {}", binary_path.display()));
                    std::process::exit(127);
                }
            } else {
                // Standard/strict sandbox — would set up namespaces
                output::status("run", "sandbox isolation requires bingux-gated daemon");
                output::status("run", "executing directly (sandbox not yet active)...");

                if binary_path.exists() {
                    let mut cmd = std::process::Command::new(&binary_path);
                    cmd.args(args);
                    match cmd.status() {
                        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
                        Err(e) => {
                            output::status("error", &format!("exec failed: {e}"));
                            std::process::exit(127);
                        }
                    }
                } else {
                    output::status("error", &format!("binary not found: {}", binary_path.display()));
                }
            }
        }
        Err(e) => {
            output::status("error", &format!("dispatch failed: {e}"));
        }
    }
}
