//! `/etc/bsm/helper.toml`. Every key has a production default, so a missing
//! file is the normal configuration. Unknown keys are an error: a typo in a
//! root service's configuration must not be silently ignored.

use std::collections::BTreeMap;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bsm_proto::helper::TICKET_TTL_SECS;
use bsm_proto::paths;
use serde::Deserialize;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/bsm/helper.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub socket: SocketConfig,
    /// Directory for the helper's own runtime files (session supervisor sockets).
    pub state_dir: PathBuf,
    pub pam: PamConfig,
    pub session: SessionConfig,
    pub greeter: GreeterConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SocketConfig {
    /// Used when systemd did not pass a socket (`LISTEN_FDS`).
    pub path: PathBuf,
    /// Account whose processes may connect. Resolved at startup.
    pub allowed_user: String,
    /// Overrides `allowed_user` (tests).
    pub allowed_uid: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PamConfig {
    pub login_service: String,
    pub unlock_service: String,
    /// Directory with PAM service files. Unset means the system's own `/etc/pam.d`.
    pub confdir: Option<PathBuf>,
    /// Whole-conversation limit.
    pub conversation_timeout_secs: u64,
    /// How long the user may take to answer one prompt.
    pub prompt_timeout_secs: u64,
    /// May only shorten the protocol's `TICKET_TTL_SECS`.
    pub ticket_ttl_secs: u64,
    /// Consecutive failed guesses per account before the helper adds its own
    /// back-off (2, 4, 8 .. 30 s). The distribution's lockout is separate and stays.
    pub free_failures: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionConfig {
    /// Leader command. Placeholders: `{wayland_display}`, `{session_id}`,
    /// `{generation}`, `{runtime_dir}`, `{uid}`, `{user}`.
    pub gnoblin: Vec<String>,
    /// Bridge command, same placeholders.
    pub bridge: Vec<String>,
    /// Accounts below this uid never get a hosted session. Uid 0 never does.
    pub min_uid: u32,
    pub path: String,
    pub session_desktop: String,
    pub current_desktop: String,
    /// Fail the login when PAM did not place the session in a systemd scope.
    pub require_scope: bool,
    /// Fail the login when `/run/user/<uid>` does not appear.
    pub require_user_runtime: bool,
    pub runtime_wait_secs: u64,
    /// Overall limit from `OpenSession` to `SessionOpened`.
    pub start_timeout_secs: u64,
    /// Grace used when the leader exits on its own or the supervisor is signalled.
    pub default_grace_ms: u32,
    /// `socket_path` of `OpenSession` must be inside this directory.
    pub socket_dir: PathBuf,
    pub locale_conf: PathBuf,
    /// Extra environment for both processes (for example a fixed `LANG`).
    pub extra_env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GreeterConfig {
    pub user: String,
    pub command: Vec<String>,
    /// Private HOME and XDG_RUNTIME_DIR are created below this directory.
    pub runtime_root: PathBuf,
    /// Greeter sockets named in `SpawnGreeter` must be inside this directory.
    pub socket_root: PathBuf,
    pub path: String,
    pub extra_env: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            socket: SocketConfig::default(),
            state_dir: PathBuf::from("/run/bsm-helper"),
            pam: PamConfig::default(),
            session: SessionConfig::default(),
            greeter: GreeterConfig::default(),
        }
    }
}

impl Default for SocketConfig {
    fn default() -> Self {
        Self { path: PathBuf::from(paths::HELPER_SOCKET), allowed_user: paths::BSM_USER.into(), allowed_uid: None }
    }
}

impl Default for PamConfig {
    fn default() -> Self {
        Self {
            login_service: "bsm-login".into(),
            unlock_service: "bsm-unlock".into(),
            confdir: None,
            conversation_timeout_secs: 120,
            prompt_timeout_secs: 60,
            ticket_ttl_secs: TICKET_TTL_SECS,
            free_failures: 3,
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            gnoblin: vec![
                "/usr/bin/gnoblin".into(),
                "--hosted".into(),
                "--wayland-display".into(),
                "{wayland_display}".into(),
            ],
            bridge: vec!["/usr/libexec/bsm/bsm-host".into()],
            min_uid: 1000,
            path: "/usr/local/bin:/usr/bin".into(),
            session_desktop: "gnoblin".into(),
            current_desktop: "Gnoblin".into(),
            require_scope: true,
            require_user_runtime: true,
            runtime_wait_secs: 20,
            start_timeout_secs: 60,
            default_grace_ms: 5000,
            socket_dir: PathBuf::from(paths::SESSION_SOCKET_DIR),
            locale_conf: PathBuf::from("/etc/locale.conf"),
            extra_env: BTreeMap::new(),
        }
    }
}

impl Default for GreeterConfig {
    fn default() -> Self {
        Self {
            user: paths::GREETER_USER.into(),
            command: vec!["/usr/bin/bingux-bsm-greeter".into()],
            runtime_root: PathBuf::from("/run/bsm-greeter"),
            socket_root: PathBuf::from(paths::RUNTIME_DIR),
            path: "/usr/local/bin:/usr/bin".into(),
            extra_env: BTreeMap::new(),
        }
    }
}

const TEMPLATE_KEYS: [&str; 6] = ["wayland_display", "session_id", "generation", "runtime_dir", "uid", "user"];

/// Values for the command placeholders.
pub struct Template<'a> {
    pub wayland_display: &'a str,
    pub session_id: &'a str,
    pub generation: u64,
    pub runtime_dir: &'a str,
    pub uid: u32,
    pub user: &'a str,
}

impl Template<'_> {
    fn value(&self, key: &str) -> Option<String> {
        Some(match key {
            "wayland_display" => self.wayland_display.to_owned(),
            "session_id" => self.session_id.to_owned(),
            "generation" => self.generation.to_string(),
            "runtime_dir" => self.runtime_dir.to_owned(),
            "uid" => self.uid.to_string(),
            "user" => self.user.to_owned(),
            _ => return None,
        })
    }

    pub fn expand(&self, argv: &[String]) -> Vec<String> {
        argv.iter().map(|a| self.expand_one(a)).collect()
    }

    fn expand_one(&self, arg: &str) -> String {
        let mut out = String::with_capacity(arg.len());
        let mut rest = arg;
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('}').and_then(|end| self.value(&after[..end]).map(|v| (v, end))) {
                Some((v, end)) => {
                    out.push_str(&v);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('{');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

fn check_argv(name: &str, argv: &[String], templated: bool) -> Result<(), String> {
    let first = argv.first().ok_or_else(|| format!("{name}: command is empty"))?;
    if !first.starts_with('/') {
        return Err(format!("{name}: the executable must be an absolute path, got {first:?}"));
    }
    if argv.iter().any(|a| a.contains('\0')) {
        return Err(format!("{name}: argument contains NUL"));
    }
    if templated {
        for arg in argv {
            let mut rest = arg.as_str();
            while let Some(start) = rest.find('{') {
                let after = &rest[start + 1..];
                let Some(end) = after.find('}') else { break };
                let key = &after[..end];
                if !TEMPLATE_KEYS.contains(&key) {
                    return Err(format!("{name}: unknown placeholder {{{key}}}"));
                }
                rest = &after[end + 1..];
            }
        }
    }
    Ok(())
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        let cfg: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Load `path`. A missing file yields the defaults. When the helper runs as
    /// root the file must be owned by root and not writable by others, because
    /// it names the executables the helper starts.
    pub fn load(path: &Path) -> Result<Config, String> {
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let cfg = Config::default();
                cfg.validate()?;
                return Ok(cfg);
            }
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        if crate::sys::euid() == 0 && (meta.uid() != 0 || meta.mode() & 0o022 != 0) {
            return Err(format!("{}: must be owned by root and not writable by group or others", path.display()));
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn validate(&self) -> Result<(), String> {
        check_argv("session.gnoblin", &self.session.gnoblin, true)?;
        check_argv("session.bridge", &self.session.bridge, true)?;
        check_argv("greeter.command", &self.greeter.command, false)?;
        for service in [&self.pam.login_service, &self.pam.unlock_service] {
            let ok = !service.is_empty()
                && service.len() <= 64
                && service.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            if !ok {
                return Err(format!("pam service name {service:?} is not a plain file name"));
            }
        }
        if self.pam.login_service == self.pam.unlock_service {
            return Err("pam.login_service and pam.unlock_service must differ".into());
        }
        if self.pam.ticket_ttl_secs == 0 {
            return Err("pam.ticket_ttl_secs must be positive".into());
        }
        if !self.session.socket_dir.is_absolute() || !self.greeter.runtime_root.is_absolute() {
            return Err("session.socket_dir and greeter.runtime_root must be absolute".into());
        }
        Ok(())
    }

    pub fn ticket_ttl(&self) -> Duration {
        Duration::from_secs(self.pam.ticket_ttl_secs.min(TICKET_TTL_SECS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_name_the_documented_services() {
        let c = Config::default();
        c.validate().unwrap();
        assert_eq!(c.pam.login_service, "bsm-login");
        assert_eq!(c.pam.unlock_service, "bsm-unlock");
        assert_eq!(c.socket.path, PathBuf::from(paths::HELPER_SOCKET));
        assert!(c.session.require_scope);
    }

    #[test]
    fn unknown_keys_and_bad_values_are_rejected() {
        assert!(Config::parse("[pam]\nlogin_servise = \"x\"\n").is_err());
        assert!(Config::parse("[session]\ngnoblin = [\"gnoblin\"]\n").is_err());
        assert!(Config::parse("[session]\ngnoblin = [\"/bin/x\", \"{nope}\"]\n").is_err());
        assert!(Config::parse("[pam]\nlogin_service = \"../login\"\n").is_err());
        assert!(Config::parse("[pam]\nlogin_service = \"a\"\nunlock_service = \"a\"\n").is_err());
    }

    #[test]
    fn ticket_ttl_can_only_be_shortened() {
        let mut c = Config::default();
        c.pam.ticket_ttl_secs = 3600;
        assert_eq!(c.ticket_ttl(), Duration::from_secs(TICKET_TTL_SECS));
        c.pam.ticket_ttl_secs = 2;
        assert_eq!(c.ticket_ttl(), Duration::from_secs(2));
    }

    #[test]
    fn templates_expand_known_keys_only() {
        let t = Template {
            wayland_display: "bsm-s1",
            session_id: "s1",
            generation: 9,
            runtime_dir: "/run/user/1000",
            uid: 1000,
            user: "kieran",
        };
        let out = t.expand(&["/x".into(), "--d={wayland_display}".into(), "{generation}/{uid}".into(), "{odd".into()]);
        assert_eq!(out, ["/x", "--d=bsm-s1", "9/1000", "{odd"]);
    }
}
