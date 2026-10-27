//! `/etc/bsm/bsm.toml`. Every key is optional; unknown keys are an error so that a typo
//! never silently leaves a policy at its default.
//!
//! ```toml
//! backend = "drm"                    # drm | winit | headless (the command line wins)
//! runtime_dir = "/run/bsm"
//! state_dir = "/var/lib/bsm"
//! helper_socket = "/run/bsm-helper/helper.sock"
//! greeter_user = "bingux-greeter"
//! greeter_socket_mode = "0660"
//! host_exe = "/usr/libexec/bsm/bsm-host"
//! hidden_users = ["backup"]          # in addition to /etc/bsm/hidden-users
//! reduce_motion = false
//! cover = "#0d1117"                  # the opaque colour behind everything
//! dbus = "system"                    # system | session | none | an address
//! system_services = "logind"         # logind | none (none refuses power actions; development only)
//!
//! [greeter_power]                    # what the greeter may do before anybody signs in
//! suspend = true
//! reboot = true
//! poweroff = true
//!
//! [greeter_power_with_sessions]      # the same, while any account has a running session
//! suspend = true                     # defaults: suspend only; reboot and poweroff would end
//! reboot = false                     # other people's work
//! poweroff = false
//!
//! [timeouts]                         # seconds
//! stage = 120
//! gnoblin_ready = 120
//! auth = 120
//!
//! [transitions]                      # milliseconds
//! new_login_ms = 420
//! unlock_ms = 520
//! lock_ms = 240
//! logout_ms = 320
//! power_ms = 360
//!
//! [boot]
//! hold_first_scene_ms = 0            # DRM: keep a lit display (the boot splash) until the greeter is
//!                                    # ready, at most this long; 0 = off (the unit passes the command
//!                                    # line flag instead)
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use bsm_manager::core::{GreeterPower, Policy};
use bsm_proto::scene::CoverColor;
use bsm_proto::state::TransitionKind;
use serde::Deserialize;

pub const DEFAULT_PATH: &str = "/etc/bsm/bsm.toml";

#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    pub backend: Option<String>,
    pub runtime_dir: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
    pub helper_socket: Option<PathBuf>,
    pub greeter_user: Option<String>,
    pub greeter_socket_mode: Option<String>,
    pub host_exe: Option<String>,
    pub hidden_users: Vec<String>,
    pub reduce_motion: Option<bool>,
    pub cover: Option<String>,
    pub dbus: Option<String>,
    pub system_services: Option<String>,
    pub greeter_power: Option<GreeterPowerFile>,
    pub greeter_power_with_sessions: Option<GreeterPowerBusyFile>,
    pub timeouts: Timeouts,
    pub transitions: Transitions,
    pub boot: Boot,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Boot {
    pub hold_first_scene_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct GreeterPowerFile {
    pub suspend: bool,
    pub reboot: bool,
    pub poweroff: bool,
}

/// Like `GreeterPowerFile` but unset keys stay off, except suspend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct GreeterPowerBusyFile {
    pub suspend: bool,
    pub reboot: bool,
    pub poweroff: bool,
}

impl Default for GreeterPowerBusyFile {
    fn default() -> Self {
        Self { suspend: true, reboot: false, poweroff: false }
    }
}

impl Default for GreeterPowerFile {
    fn default() -> Self {
        Self { suspend: true, reboot: true, poweroff: true }
    }
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Timeouts {
    pub stage: Option<u64>,
    pub gnoblin_ready: Option<u64>,
    pub auth: Option<u64>,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Transitions {
    pub new_login_ms: Option<u32>,
    pub unlock_ms: Option<u32>,
    pub lock_ms: Option<u32>,
    pub logout_ms: Option<u32>,
    pub power_ms: Option<u32>,
}

#[derive(Debug)]
pub enum ConfigError {
    Read(PathBuf, std::io::Error),
    Parse(PathBuf, String),
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(p, e) => write!(f, "cannot read {}: {e}", p.display()),
            Self::Parse(p, e) => write!(f, "{}: {e}", p.display()),
            Self::Invalid(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl FileConfig {
    pub fn parse(text: &str, origin: &Path) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|e| ConfigError::Parse(origin.to_path_buf(), e.to_string()))
    }

    /// Load `path`. A missing file is the defaults only when `required` is false.
    pub fn load(path: &Path, required: bool) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(t) => Self::parse(&t, path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => Ok(Self::default()),
            Err(e) => Err(ConfigError::Read(path.to_path_buf(), e)),
        }
    }

    pub fn cover_color(&self) -> Result<Option<CoverColor>, ConfigError> {
        let Some(c) = &self.cover else { return Ok(None) };
        parse_color(c).map(Some).ok_or_else(|| ConfigError::Invalid(format!("cover must be #rrggbb, not {c:?}")))
    }

    pub fn socket_mode(&self) -> Result<u32, ConfigError> {
        match &self.greeter_socket_mode {
            None => Ok(0o660),
            Some(s) => u32::from_str_radix(s.trim_start_matches("0o"), 8)
                .ok()
                .filter(|m| *m <= 0o777)
                .ok_or_else(|| ConfigError::Invalid(format!("greeter_socket_mode must be octal like \"0660\", not {s:?}"))),
        }
    }

    /// Apply the policy keys on top of the defaults of `Policy`.
    pub fn apply(&self, mut p: Policy) -> Policy {
        if let Some(h) = &self.host_exe {
            p.host_exe = h.clone();
        }
        if let Some(r) = self.reduce_motion {
            p.reduce_motion = r;
        }
        if let Some(g) = &self.greeter_power {
            p.greeter_power = GreeterPower { suspend: g.suspend, reboot: g.reboot, poweroff: g.poweroff };
        }
        if let Some(s) = self.timeouts.stage {
            p.stage_timeout = Duration::from_secs(s);
        }
        if let Some(s) = self.timeouts.gnoblin_ready {
            p.gnoblin_ready_timeout = Duration::from_secs(s);
        }
        if let Some(s) = self.timeouts.auth {
            p.auth_timeout = Duration::from_secs(s);
        }
        if let Some(g) = &self.greeter_power_with_sessions {
            p.greeter_power_with_sessions = GreeterPower { suspend: g.suspend, reboot: g.reboot, poweroff: g.poweroff };
        }
        let t = &self.transitions;
        for (kind, ms) in [
            (TransitionKind::NewLoginFade, t.new_login_ms),
            (TransitionKind::UnlockSlideUp, t.unlock_ms),
            (TransitionKind::Lock, t.lock_ms),
            (TransitionKind::SwitchUser, t.lock_ms),
            (TransitionKind::LogoutFade, t.logout_ms),
            (TransitionKind::PowerFade, t.power_ms),
        ] {
            if let Some(ms) = ms {
                p.duration_overrides.push((kind, ms));
            }
        }
        p
    }
}

pub fn parse_color(s: &str) -> Option<CoverColor> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map(|v| v as f32 / 255.0).ok();
    Some(CoverColor { r: c(0)?, g: c(2)?, b: c(4)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documented_example_parses() {
        let text = r##"
backend = "drm"
runtime_dir = "/run/bsm"
greeter_user = "bingux-greeter"
greeter_socket_mode = "0660"
hidden_users = ["backup"]
cover = "#0d1117"
dbus = "system"
[greeter_power]
suspend = false
[timeouts]
stage = 30
[transitions]
unlock_ms = 600
[boot]
hold_first_scene_ms = 20000
"##;
        let c = FileConfig::parse(text, Path::new("t")).unwrap();
        assert_eq!(c.socket_mode().unwrap(), 0o660);
        let p = c.apply(Policy::default());
        assert!(!p.greeter_power.suspend && p.greeter_power.reboot);
        assert_eq!(p.stage_timeout, Duration::from_secs(30));
        assert_eq!(p.duration_ms(TransitionKind::UnlockSlideUp), 600);
        assert_eq!(p.duration_ms(TransitionKind::NewLoginFade), 420);
        let cover = c.cover_color().unwrap().unwrap();
        assert!((cover.r - 13.0 / 255.0).abs() < 1e-6);
        assert_eq!(c.boot.hold_first_scene_ms, Some(20000));
    }

    #[test]
    fn a_typo_is_an_error_not_a_silent_default() {
        assert!(FileConfig::parse("bakend = \"drm\"", Path::new("t")).is_err());
        assert!(FileConfig::parse("[timeouts]\nstages = 3", Path::new("t")).is_err());
        assert!(FileConfig::parse("[boot]\nhold_first_scene = 3", Path::new("t")).is_err());
    }

    #[test]
    fn bad_values_are_refused() {
        let c = FileConfig { cover: Some("red".into()), greeter_socket_mode: Some("0999".into()), ..Default::default() };
        assert!(c.cover_color().is_err());
        assert!(c.socket_mode().is_err());
        assert!(parse_color("#12345").is_none());
    }

    #[test]
    fn a_missing_file_is_the_defaults_unless_required() {
        let p = Path::new("/nonexistent/bsm.toml");
        assert_eq!(FileConfig::load(p, false).unwrap(), FileConfig::default());
        assert!(FileConfig::load(p, true).is_err());
    }
}

#[cfg(test)]
mod busy_tests {
    use super::*;

    #[test]
    fn power_with_sessions_is_strict_unless_opened_up() {
        let p = FileConfig::default().apply(Policy::default());
        assert!(p.greeter_power_with_sessions.suspend && !p.greeter_power_with_sessions.reboot && !p.greeter_power_with_sessions.poweroff);
        let c = FileConfig::parse("[greeter_power_with_sessions]\nreboot = true\n", Path::new("t")).unwrap();
        let p = c.apply(Policy::default());
        assert!(p.greeter_power_with_sessions.reboot && !p.greeter_power_with_sessions.poweroff);
    }
}
