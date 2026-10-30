//! The environment of a hosted session, built as pure data so it can be tested.

use std::collections::BTreeMap;

use bsm_proto::paths;

use crate::sys::Passwd;

/// What the helper decides, as opposed to what PAM modules contribute.
pub struct SessionEnvInputs<'a> {
    pub pw: &'a Passwd,
    pub session_id: &'a str,
    pub generation: u64,
    pub path: &'a str,
    pub session_desktop: &'a str,
    pub current_desktop: &'a str,
    pub locale: &'a BTreeMap<String, String>,
    pub extra: &'a BTreeMap<String, String>,
}

/// Names PAM modules may not inject into a desktop: dynamic-linker control,
/// seat identity (hosted sessions are seatless) and the bridge's private socket.
fn rejected_from_pam(name: &str) -> bool {
    name.starts_with("LD_")
        || name == "XDG_SEAT"
        || name == "XDG_VTNR"
        || name == paths::ENV_HOST_SOCKET
        || name == "WAYLAND_DISPLAY"
        || name == "DISPLAY"
        || name == "BASH_ENV"
        || name == "ENV"
}

/// Environment shared by the leader and the bridge. `pam_env` is the list
/// from `pam_getenvlist` after `pam_open_session` (this is where `pam_systemd`
/// puts `XDG_RUNTIME_DIR` and `XDG_SESSION_ID`). The helper's own values win
/// for everything security relevant.
pub fn session_env(inp: &SessionEnvInputs<'_>, pam_env: &[String]) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    env.insert("PATH".into(), inp.path.to_owned());
    for (k, v) in inp.locale {
        env.insert(k.clone(), v.clone());
    }
    for (k, v) in inp.extra {
        if !rejected_from_pam(k) {
            env.insert(k.clone(), v.clone());
        }
    }
    for entry in pam_env {
        if let Some((k, v)) = entry.split_once('=')
            && !k.is_empty()
            && !rejected_from_pam(k)
        {
            env.insert(k.to_owned(), v.to_owned());
        }
    }
    let runtime = env
        .get("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("/run/user/{}", inp.pw.uid));
    let forced = [
        ("HOME", inp.pw.home.display().to_string()),
        ("USER", inp.pw.name.clone()),
        ("LOGNAME", inp.pw.name.clone()),
        ("SHELL", if inp.pw.shell.is_empty() { "/bin/sh".to_owned() } else { inp.pw.shell.clone() }),
        ("XDG_RUNTIME_DIR", runtime.clone()),
        ("XDG_SESSION_TYPE", "wayland".to_owned()),
        ("XDG_SESSION_CLASS", "user".to_owned()),
        ("XDG_SESSION_DESKTOP", inp.session_desktop.to_owned()),
        ("DESKTOP_SESSION", inp.session_desktop.to_owned()),
        ("XDG_CURRENT_DESKTOP", inp.current_desktop.to_owned()),
        (paths::ENV_SESSION_ID, inp.session_id.to_owned()),
        (paths::ENV_GENERATION, inp.generation.to_string()),
        ("DBUS_SESSION_BUS_ADDRESS", format!("unix:path={runtime}/bus")),
    ];
    for (k, v) in forced {
        env.insert(k.to_owned(), v);
    }
    env
}

/// The `NAME=value` pairs handed to PAM before `pam_open_session`, so that
/// `pam_systemd` creates a seatless wayland session of class `user`.
pub fn pam_session_vars(inp: &SessionEnvInputs<'_>) -> Vec<String> {
    vec![
        "XDG_SESSION_TYPE=wayland".to_owned(),
        "XDG_SESSION_CLASS=user".to_owned(),
        format!("XDG_SESSION_DESKTOP={}", inp.session_desktop),
        format!("DESKTOP_SESSION={}", inp.session_desktop),
        format!("XDG_CURRENT_DESKTOP={}", inp.current_desktop),
        format!("{}={}", paths::ENV_SESSION_ID, inp.session_id),
        format!("{}={}", paths::ENV_GENERATION, inp.generation),
    ]
}

/// Locale variables from `/etc/locale.conf` style text (`KEY=value`, optional quotes).
pub fn parse_locale_conf(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim();
        if !(k == "LANG" || k == "LANGUAGE" || k.starts_with("LC_")) {
            continue;
        }
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
        if !v.is_empty() && !v.contains('\0') {
            out.insert(k.to_owned(), v.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn pw() -> Passwd {
        Passwd { name: "alice".into(), uid: 1000, gid: 1000, home: PathBuf::from("/home/alice"), shell: "/bin/zsh".into() }
    }

    fn inputs<'a>(pw: &'a Passwd, loc: &'a BTreeMap<String, String>, extra: &'a BTreeMap<String, String>) -> SessionEnvInputs<'a> {
        SessionEnvInputs {
            pw,
            session_id: "s0000abcd",
            generation: 42,
            path: "/usr/bin",
            session_desktop: "gnoblin",
            current_desktop: "Gnoblin",
            locale: loc,
            extra,
        }
    }

    #[test]
    fn identity_and_session_variables_are_forced() {
        let (p, l, e) = (pw(), BTreeMap::new(), BTreeMap::new());
        let pam = vec![
            "XDG_RUNTIME_DIR=/run/user/1000".to_owned(),
            "XDG_SESSION_ID=3".to_owned(),
            "HOME=/tmp/evil".to_owned(),
            "XDG_SESSION_TYPE=x11".to_owned(),
        ];
        let env = session_env(&inputs(&p, &l, &e), &pam);
        assert_eq!(env["HOME"], "/home/alice");
        assert_eq!(env["USER"], "alice");
        assert_eq!(env["LOGNAME"], "alice");
        assert_eq!(env["SHELL"], "/bin/zsh");
        assert_eq!(env["XDG_SESSION_TYPE"], "wayland");
        assert_eq!(env["XDG_SESSION_CLASS"], "user");
        assert_eq!(env["XDG_SESSION_DESKTOP"], "gnoblin");
        assert_eq!(env["DESKTOP_SESSION"], "gnoblin");
        assert_eq!(env["XDG_CURRENT_DESKTOP"], "Gnoblin");
        assert_eq!(env["BSM_SESSION_ID"], "s0000abcd");
        assert_eq!(env["BSM_GENERATION"], "42");
        assert_eq!(env["XDG_RUNTIME_DIR"], "/run/user/1000");
        assert_eq!(env["XDG_SESSION_ID"], "3");
        assert_eq!(env["DBUS_SESSION_BUS_ADDRESS"], "unix:path=/run/user/1000/bus");
    }

    #[test]
    fn sessions_are_seatless_and_never_carry_the_bridge_socket_or_linker_vars() {
        let (p, l, e) = (pw(), BTreeMap::new(), BTreeMap::new());
        let pam = vec![
            "XDG_SEAT=seat0".to_owned(),
            "XDG_VTNR=1".to_owned(),
            "BSM_HOST_SOCKET=/run/bsm/x".to_owned(),
            "LD_PRELOAD=/tmp/x.so".to_owned(),
            "LD_LIBRARY_PATH=/tmp".to_owned(),
            "WAYLAND_DISPLAY=wayland-0".to_owned(),
            "DISPLAY=:0".to_owned(),
        ];
        let env = session_env(&inputs(&p, &l, &e), &pam);
        for k in ["XDG_SEAT", "XDG_VTNR", "BSM_HOST_SOCKET", "LD_PRELOAD", "LD_LIBRARY_PATH", "WAYLAND_DISPLAY", "DISPLAY"] {
            assert!(!env.contains_key(k), "{k} leaked");
        }
    }

    #[test]
    fn runtime_dir_falls_back_to_the_uid_path() {
        let (p, l, e) = (pw(), BTreeMap::new(), BTreeMap::new());
        let env = session_env(&inputs(&p, &l, &e), &[]);
        assert_eq!(env["XDG_RUNTIME_DIR"], "/run/user/1000");
    }

    #[test]
    fn locale_conf_keeps_only_locale_keys() {
        let m = parse_locale_conf("# c\nLANG=\"en_GB.UTF-8\"\nLC_TIME=en_DK.UTF-8\nPATH=/evil\nLANGUAGE=\n");
        assert_eq!(m.len(), 2);
        assert_eq!(m["LANG"], "en_GB.UTF-8");
        assert_eq!(m["LC_TIME"], "en_DK.UTF-8");
    }

    #[test]
    fn pam_vars_describe_a_seatless_wayland_user_session() {
        let (p, l, e) = (pw(), BTreeMap::new(), BTreeMap::new());
        let v = pam_session_vars(&inputs(&p, &l, &e));
        assert!(v.contains(&"XDG_SESSION_TYPE=wayland".to_owned()));
        assert!(v.contains(&"XDG_SESSION_CLASS=user".to_owned()));
        assert!(!v.iter().any(|s| s.starts_with("XDG_SEAT") || s.starts_with("XDG_VTNR")));
    }
}
