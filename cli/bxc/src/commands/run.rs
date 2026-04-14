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
            if !args.is_empty() {
                output::status("run", &format!("args: {}", args.join(" ")));
            }
            output::status("run", "would launch sandbox now");
        }
        Err(e) => {
            output::status("error", &format!("dispatch failed: {e}"));
        }
    }
}
