use std::path::Path;

use bxc_shim::dispatch::resolve_dispatch;
use bxc_shim::resolver::resolve_binary;
use bxc_shim::version::parse_version_syntax;

fn main() {
    // 1. Get binary name from argv[0]
    let argv0 = std::env::args().next().unwrap_or_default();
    let binary_name = Path::new(&argv0)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&argv0);

    // 2. Check for explicit @version syntax
    let (name, _version) = parse_version_syntax(binary_name);

    // 3. If this IS bxc-shim itself (not a hardlink), handle subcommands
    if name == "bxc-shim" {
        eprintln!("bxc-shim: multi-call launcher for Bingux packages");
        std::process::exit(0);
    }

    // 4. Resolve from dispatch table (user -> system fallback)
    let uid = nix::unistd::getuid().as_raw();
    let entry = match resolve_dispatch(&name, uid) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("bxc-shim: {}", e);
            std::process::exit(127);
        }
    };

    // 5. Resolve binary path
    let binary_path = resolve_binary(&entry);

    // 6. Route by sandbox level
    match entry.sandbox.as_str() {
        "none" => {
            // Direct exec -- no sandbox
            // In real impl: std::os::unix::process::CommandExt::exec()
            println!("exec: {}", binary_path.display());
        }
        level => {
            // Launch via sandbox
            println!("sandbox({}): {}", level, binary_path.display());
            // In real impl: create SandboxConfig, launch via bxc-runtime
        }
    }
}
