//! Launching the restricted greeter as the `bingux-greeter` account.
//!
//! The greeter gets no PAM session, no D-Bus session bus and a minimal
//! environment. Its HOME and XDG_RUNTIME_DIR are private directories below
//! the runtime root, created per generation and removed when it exits.

use std::collections::BTreeMap;
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use bsm_proto::paths;

use crate::config::GreeterConfig;
use crate::sys::Passwd;

pub struct GreeterRun {
    pub dir: PathBuf,
    pub runtime: PathBuf,
    pub home: PathBuf,
}

/// `path` must be absolute, free of `..` and NUL, and inside `root`.
pub fn path_within(root: &Path, path: &str) -> bool {
    let p = Path::new(path);
    p.is_absolute()
        && !path.contains('\0')
        && !path.contains('\n')
        && p.starts_with(root)
        && p.components().all(|c| !matches!(c, Component::ParentDir | Component::CurDir))
}

/// Create a directory owned by root (mode 0700) and, when `give_to` is set, hand
/// it over through a descriptor opened with `O_NOFOLLOW`. Nothing here resolves a
/// path twice: after `mkdir`, only the descriptor is used.
fn private_dir(path: &Path, give_to: Option<&Passwd>) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    std::fs::DirBuilder::new().mode(0o700).create(path)?;
    let fd = crate::sys::open_dir_nofollow(path)?;
    // mkdir applied the umask; make the mode exact.
    crate::sys::cvt(unsafe { libc::fchmod(fd.as_raw_fd(), 0o700) })?;
    if let Some(pw) = give_to {
        crate::sys::cvt(unsafe { libc::fchown(fd.as_raw_fd(), pw.uid, pw.gid) })?;
    }
    Ok(())
}

/// Create `<root>/<generation>/{rt,home}` owned by the greeter account, mode 0700.
/// `root` itself is created 0755 and stays owned by whoever runs the helper.
///
/// Order matters (review F1): the generation directory is created root-owned and
/// 0700, so nobody else can enter it while its children are made; the children
/// are created inside it and handed over through descriptors that do not follow
/// symlinks; the generation directory itself is handed over last. A greeter
/// process that survived from an earlier generation can therefore never place a
/// symlink where root is about to `chown`.
pub fn prepare_dirs(root: &Path, generation: u64, pw: &Passwd) -> io::Result<GreeterRun> {
    std::fs::create_dir_all(root)?;
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o755))?;
    let dir = root.join(generation.to_string());
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    private_dir(&dir, None)?;
    let runtime = dir.join("rt");
    let home = dir.join("home");
    private_dir(&runtime, Some(pw))?;
    private_dir(&home, Some(pw))?;
    // Last: from here the greeter account can enter and change `dir`.
    let fd = crate::sys::open_dir_nofollow(&dir)?;
    use std::os::fd::AsRawFd;
    crate::sys::cvt(unsafe { libc::fchown(fd.as_raw_fd(), pw.uid, pw.gid) })?;
    Ok(GreeterRun { dir, runtime, home })
}

pub fn remove_dirs(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// Remove leftovers of a previous helper run.
pub fn clean_stale(root: &Path) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let _ = std::fs::remove_dir_all(e.path());
    }
}

pub struct GreeterEnvInputs<'a> {
    pub cfg: &'a GreeterConfig,
    pub pw: &'a Passwd,
    pub run: &'a GreeterRun,
    pub generation: u64,
    pub wayland_socket: &'a str,
    pub control_socket: &'a str,
    pub locale: &'a BTreeMap<String, String>,
}

/// The greeter's whole environment. Nothing is inherited from the helper.
pub fn greeter_env(i: &GreeterEnvInputs<'_>) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("PATH".to_owned(), i.cfg.path.clone());
    for (k, v) in i.locale {
        env.insert(k.clone(), v.clone());
    }
    for (k, v) in &i.cfg.extra_env {
        if !k.starts_with("LD_") {
            env.insert(k.clone(), v.clone());
        }
    }
    let home = i.run.home.display().to_string();
    let runtime = i.run.runtime.display().to_string();
    let forced = [
        ("HOME", home.clone()),
        ("USER", i.pw.name.clone()),
        ("LOGNAME", i.pw.name.clone()),
        ("SHELL", if i.pw.shell.is_empty() { "/usr/bin/nologin".to_owned() } else { i.pw.shell.clone() }),
        ("XDG_RUNTIME_DIR", runtime),
        ("XDG_CACHE_HOME", format!("{home}/.cache")),
        ("XDG_CONFIG_HOME", format!("{home}/.config")),
        ("XDG_DATA_HOME", format!("{home}/.local/share")),
        ("XDG_STATE_HOME", format!("{home}/.local/state")),
        ("WAYLAND_DISPLAY", i.wayland_socket.to_owned()),
        ("BSM_CONTROL_SOCKET", i.control_socket.to_owned()),
        (paths::ENV_GENERATION, i.generation.to_string()),
        ("QT_QPA_PLATFORM", "wayland".to_owned()),
        ("XDG_SESSION_TYPE", "wayland".to_owned()),
    ];
    for (k, v) in forced {
        env.insert(k.to_owned(), v);
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pw() -> Passwd {
        let uid = unsafe { libc::getuid() };
        crate::sys::getpwuid(uid).unwrap().unwrap()
    }

    #[test]
    fn socket_paths_must_stay_inside_the_root() {
        let root = Path::new("/run/bsm");
        assert!(path_within(root, "/run/bsm/greeter-wayland.sock"));
        assert!(!path_within(root, "/run/bsm/../etc/x"));
        assert!(!path_within(root, "/run/other/x"));
        assert!(!path_within(root, "/run/bsmx/x"));
        assert!(!path_within(root, "relative"));
        assert!(!path_within(root, "/run/bsm/a\nb"));
    }

    #[test]
    fn directories_are_private_and_replace_stale_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("greeter");
        let stale = root.join("5");
        std::fs::create_dir_all(stale.join("home")).unwrap();
        std::fs::write(stale.join("home/old"), "x").unwrap();
        let run = prepare_dirs(&root, 5, &pw()).unwrap();
        assert!(!run.home.join("old").exists());
        for d in [&run.dir, &run.runtime, &run.home] {
            let mode = std::fs::metadata(d).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", d.display());
        }
        remove_dirs(&run.dir);
        assert!(!run.dir.exists());
    }

    #[test]
    fn environment_is_minimal_and_points_at_the_private_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let p = pw();
        let run = prepare_dirs(tmp.path(), 9, &p).unwrap();
        let mut cfg = GreeterConfig::default();
        cfg.extra_env.insert("LD_PRELOAD".into(), "/x".into());
        cfg.extra_env.insert("QT_LOGGING_RULES".into(), "a=false".into());
        let loc = BTreeMap::new();
        let env = greeter_env(&GreeterEnvInputs {
            cfg: &cfg,
            pw: &p,
            run: &run,
            generation: 9,
            wayland_socket: "/run/bsm/greeter-wayland.sock",
            control_socket: "/run/bsm/greeter-control.sock",
            locale: &loc,
        });
        assert_eq!(env["WAYLAND_DISPLAY"], "/run/bsm/greeter-wayland.sock");
        assert_eq!(env["BSM_CONTROL_SOCKET"], "/run/bsm/greeter-control.sock");
        assert_eq!(env["BSM_GENERATION"], "9");
        assert_eq!(env["XDG_RUNTIME_DIR"], run.runtime.display().to_string());
        assert_eq!(env["HOME"], run.home.display().to_string());
        assert!(!env.contains_key("LD_PRELOAD"));
        assert!(!env.contains_key("DBUS_SESSION_BUS_ADDRESS"));
        assert!(!env.contains_key("BSM_HOST_SOCKET"));
        assert_eq!(env["QT_LOGGING_RULES"], "a=false");
    }

    #[test]
    fn a_planted_symlink_is_never_followed_by_the_root_chown() {
        // Review F1: a process of the previous greeter owns the generation directory's
        // old incarnation and can plant a symlink where root creates a child.
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("victim");
        std::fs::write(&target, "root file").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        let planted = tmp.path().join("rt");
        std::os::unix::fs::symlink(&target, &planted).unwrap();
        assert!(private_dir(&planted, Some(&pw())).is_err(), "must not create through an existing entry");
        assert!(crate::sys::open_dir_nofollow(&planted).is_err(), "must not open through a symlink");
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o644, "target untouched");
    }

    #[test]
    fn the_generation_directory_is_root_only_until_its_children_exist() {
        // The directory is created 0700 and handed over last, so while the children are
        // made nobody else can enter it. Checked through the final state and the order
        // of operations in `prepare_dirs`: children are owned before the parent's owner changes.
        let tmp = tempfile::tempdir().unwrap();
        let p = pw();
        let run = prepare_dirs(tmp.path(), 3, &p).unwrap();
        use std::os::unix::fs::MetadataExt;
        for d in [&run.dir, &run.runtime, &run.home] {
            let m = std::fs::metadata(d).unwrap();
            assert_eq!((m.uid(), m.mode() & 0o777), (p.uid, 0o700), "{}", d.display());
        }
    }
}
