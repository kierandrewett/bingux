//! Which biometric PAM methods can serve a user for a PAM service.
//!
//! The answer only changes the prompt text. PAM itself decides what runs;
//! this never claims a device or starts verification.

use serde::Serialize;
use std::{fs, path::Path, time::Duration};
use zbus::{blocking::Connection, proxy, zvariant::OwnedObjectPath};

const PAM_DIRS: [&str; 2] = ["/etc/pam.d", "/usr/lib/pam.d"];
const MAX_INCLUDE_DEPTH: usize = 4;
const DBUS_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Biometrics {
    /// `pam_fprintd` is in the stack and fprintd reports an enrolled finger.
    pub fingerprint: bool,
    /// `pam_howdy` is in the stack. Howdy keeps its models readable by root
    /// only, so enrolment cannot be checked from here.
    pub face: bool,
}

#[proxy(
    interface = "net.reactivated.Fprint.Manager",
    default_service = "net.reactivated.Fprint",
    default_path = "/net/reactivated/Fprint/Manager"
)]
trait Manager {
    fn get_default_device(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "net.reactivated.Fprint.Device",
    default_service = "net.reactivated.Fprint"
)]
trait Device {
    fn list_enrolled_fingers(&self, username: &str) -> zbus::Result<Vec<String>>;
}

pub fn probe(user: &str, service: &str) -> Biometrics {
    let stack = PamStack::system();
    Biometrics {
        fingerprint: stack.mentions(service, "pam_fprintd") && fingerprint_enrolled(user),
        face: stack.mentions(service, "pam_howdy"),
    }
}

fn fingerprint_enrolled(user: &str) -> bool {
    let Ok(connection) = Connection::system() else {
        return false;
    };
    let Ok(manager) = ManagerProxyBlocking::new(&connection) else {
        return false;
    };
    let Ok(device_path) = manager.get_default_device() else {
        return false;
    };
    let Ok(device) = DeviceProxyBlocking::builder(&connection)
        .path(device_path)
        .and_then(|builder| Ok(builder.cache_properties(zbus::proxy::CacheProperties::No)))
        .and_then(|builder| builder.build())
    else {
        return false;
    };
    let _ = DBUS_TIMEOUT;
    device
        .list_enrolled_fingers(user)
        .is_ok_and(|fingers| !fingers.is_empty())
}

/// Reads PAM service files, following `include` and `substack` lines so a
/// module referenced from `system-auth` counts for `bingux-lock`.
pub struct PamStack {
    read: fn(&str) -> Option<String>,
}

impl PamStack {
    pub fn system() -> Self {
        Self {
            read: read_system_service,
        }
    }

    #[cfg(test)]
    pub fn with_reader(read: fn(&str) -> Option<String>) -> Self {
        Self { read }
    }

    pub fn mentions(&self, service: &str, module: &str) -> bool {
        self.mentions_at(service, module, 0)
    }

    fn mentions_at(&self, service: &str, module: &str, depth: usize) -> bool {
        let Some(text) = (self.read)(service) else {
            return false;
        };
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            let Some((facility, rest)) = words.split_first() else {
                continue;
            };
            if facility.trim_start_matches('-') != "auth" {
                continue;
            }
            match rest {
                [control, target, ..] if *control == "include" || *control == "substack" => {
                    if depth < MAX_INCLUDE_DEPTH && self.mentions_at(target, module, depth + 1) {
                        return true;
                    }
                }
                [_, module_path, ..] => {
                    let file = Path::new(module_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("");
                    if file.strip_suffix(".so").unwrap_or(file) == module {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
}

fn read_system_service(service: &str) -> Option<String> {
    if service.contains('/') {
        return None;
    }
    PAM_DIRS
        .iter()
        .find_map(|directory| fs::read_to_string(Path::new(directory).join(service)).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(service: &str) -> Option<String> {
        match service {
            "bingux-lock" => Some("auth include system-auth\n".to_string()),
            "system-auth" => Some("auth required pam_env.so\n-auth sufficient pam_fprintd.so timeout=10\nauth sufficient pam_unix.so\nsession optional pam_howdy.so\n".to_string()),
            "loop" => Some("auth include loop\n".to_string()),
            _ => None,
        }
    }

    #[test]
    fn follows_includes_and_only_counts_auth_lines() {
        let stack = PamStack::with_reader(fixture);
        assert!(stack.mentions("bingux-lock", "pam_fprintd"));
        assert!(stack.mentions("system-auth", "pam_unix"));
        assert!(!stack.mentions("bingux-lock", "pam_howdy"));
        assert!(!stack.mentions("missing", "pam_unix"));
        assert!(!stack.mentions("loop", "pam_unix"));
    }
}
