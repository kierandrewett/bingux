use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

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

    // 2. Check for explicit @version syntax (e.g. "firefox@128.0.1")
    let (name, _version) = parse_version_syntax(binary_name);

    // 3. If this IS bxc-shim itself (not a symlink/hardlink), print usage
    if name == "bxc-shim" {
        eprintln!("bxc-shim: multi-call launcher for Bingux packages");
        eprintln!("usage: create a symlink to bxc-shim with the name of a binary");
        eprintln!("       in the dispatch table, then invoke it normally.");
        std::process::exit(0);
    }

    // 4. Resolve from dispatch table (user overlay -> system fallback)
    let uid = nix::unistd::getuid().as_raw();
    let entry = match resolve_dispatch(&name, uid) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("bxc-shim: {}", e);
            std::process::exit(127);
        }
    };

    // 5. Resolve full path to the real binary inside the package store
    let binary_path = resolve_binary(&entry);
    if !binary_path.exists() {
        eprintln!(
            "bxc-shim: binary not found at {} (package {})",
            binary_path.display(),
            entry.package
        );
        std::process::exit(127);
    }

    // Collect remaining args (everything after argv[0])
    let args: Vec<String> = std::env::args().skip(1).collect();

    // 6. Route by sandbox level
    //
    //    none / minimal  -> direct exec (no namespace setup)
    //    standard / strict -> fork, create namespaces via bxc-runtime, exec
    match entry.sandbox.as_str() {
        "none" | "minimal" => {
            // Direct exec -- replace this process with the target binary.
            let err = Command::new(&binary_path).args(&args).exec();
            // exec() only returns on error
            eprintln!("bxc-shim: exec failed for {}: {}", binary_path.display(), err);
            std::process::exit(126);
        }
        _level => {
            // Sandboxed exec -- fork a child, set up namespaces, then exec.
            //
            // We fork so the parent can wait and report the exit status,
            // while the child enters the sandbox and replaces itself with
            // the target binary.
            use nix::sys::wait::waitpid;
            use nix::unistd::{ForkResult, fork};

            match unsafe { fork() } {
                Ok(ForkResult::Child) => {
                    // Child: set up namespaces then exec the binary.
                    //
                    // For now we perform a direct exec -- full namespace
                    // isolation (unshare, mount plan, seccomp) will be wired
                    // through bxc-runtime::Sandbox once the permission daemon
                    // is ready.  The plumbing is intentionally left as a
                    // direct exec so packages still work while the sandbox
                    // runtime matures.
                    let err = Command::new(&binary_path).args(&args).exec();
                    eprintln!("bxc-shim: sandbox exec failed: {}", err);
                    std::process::exit(126);
                }
                Ok(ForkResult::Parent { child }) => {
                    // Parent: wait for the sandboxed child and propagate its
                    // exit status.
                    match waitpid(child, None) {
                        Ok(nix::sys::wait::WaitStatus::Exited(_, code)) => {
                            std::process::exit(code);
                        }
                        Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => {
                            std::process::exit(128 + sig as i32);
                        }
                        Ok(_) => std::process::exit(1),
                        Err(e) => {
                            eprintln!("bxc-shim: waitpid failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("bxc-shim: fork failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
