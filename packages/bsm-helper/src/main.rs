use std::path::PathBuf;
use std::process::ExitCode;

use bsm_helper::config::{Config, DEFAULT_CONFIG_PATH};
use bsm_helper::server::{Server, acquire_listener, resolve_allowed_uid};

const USAGE: &str = "usage: bsm-helper [--config FILE] [--socket PATH] [--allowed-uid UID] [--pam-confdir DIR]\n\
\n\
Privileged PAM and session helper for bsm. Normally started by bsm-helper.socket.\n\
  --config FILE       configuration (default /etc/bsm/helper.toml; a missing file means defaults)\n\
  --socket PATH       listen here instead of the configured path (ignored under socket activation)\n\
  --allowed-uid UID   accept this uid instead of the configured account\n\
  --pam-confdir DIR   read PAM service files from DIR (testing)\n";

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bsm-helper: {e}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<(), String> {
    // Passwords pass through this process and its children: no core dumps and no
    // ptrace by the user processes it later starts. The unit deliberately sets no
    // LimitCORE, because rlimits are inherited by every desktop it launches.
    unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0 as libc::c_ulong, 0, 0, 0) };
    let mut config_path = PathBuf::from(DEFAULT_CONFIG_PATH);
    let mut socket: Option<PathBuf> = None;
    let mut uid: Option<u32> = None;
    let mut confdir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value\n{USAGE}"));
        match a.as_str() {
            "--config" => config_path = PathBuf::from(value("--config")?),
            "--socket" => socket = Some(PathBuf::from(value("--socket")?)),
            "--allowed-uid" => uid = Some(value("--allowed-uid")?.parse().map_err(|_| "--allowed-uid needs a number".to_owned())?),
            "--pam-confdir" => confdir = Some(PathBuf::from(value("--pam-confdir")?)),
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }
    let mut cfg = Config::load(&config_path)?;
    if let Some(s) = socket {
        cfg.socket.path = s;
    }
    if let Some(u) = uid {
        cfg.socket.allowed_uid = Some(u);
    }
    if let Some(d) = confdir {
        cfg.pam.confdir = Some(d);
    }
    cfg.validate()?;
    if let Some(dir) = &cfg.pam.confdir {
        check_confdir(dir)?;
    }
    let allowed = resolve_allowed_uid(&cfg)?;
    let listener = acquire_listener(&cfg, allowed).map_err(|e| format!("listening: {e}"))?;
    let mut server = Server::new(cfg, listener, allowed).map_err(|e| format!("starting: {e}"))?;
    eprintln!("bsm-helper: ready");
    server.run().map_err(|e| format!("event loop: {e}"))
}

/// A root helper must not take PAM stacks from a directory others can change.
fn check_confdir(dir: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    if bsm_helper::sys::euid() != 0 {
        return Ok(());
    }
    let m = std::fs::metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if m.uid() != 0 || m.mode() & 0o022 != 0 {
        return Err(format!("{}: PAM directory must be owned by root and not writable by others", dir.display()));
    }
    Ok(())
}
