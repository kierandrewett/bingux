//! Works out which application made a portal request.
//!
//! Only the desktop portal frontend may ask this backend for a permission
//! decision. The frontend must own `org.freedesktop.portal.Desktop` and must
//! be the direct caller. The request handle names the application that made
//! the request, so that sender must run as the same user as this backend.

use std::path::Path;

use zbus::{Connection, fdo, names::BusName};

pub const PORTAL_DESKTOP_NAME: &str = "org.freedesktop.portal.Desktop";
pub const REQUEST_PATH_PREFIX: &str = "/org/freedesktop/portal/desktop/request/";

/// The identity of the application a request came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    /// The application supplied a non-empty app ID (for example a Flatpak ID).
    AppId(String),
    /// The application has no app ID; its canonical executable path is used.
    HostExe(String),
}

impl Identity {
    /// The full identity string, for example `app-id:org.example.App`.
    pub fn key(&self) -> String {
        match self {
            Identity::AppId(app_id) => format!("app-id:{app_id}"),
            Identity::HostExe(exe) => format!("host-exe:{exe}"),
        }
    }

    pub fn app_id(&self) -> Option<&str> {
        match self {
            Identity::AppId(app_id) => Some(app_id),
            Identity::HostExe(_) => None,
        }
    }

    pub fn exe(&self) -> Option<&str> {
        match self {
            Identity::AppId(_) => None,
            Identity::HostExe(exe) => Some(exe),
        }
    }
}

/// The parts of a portal request handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHandle {
    /// The requesting application's unique bus name, for example `:1.23`.
    pub sender: String,
    /// The sender component of the handle, for example `1_23`.
    pub component: String,
    /// The request token, for example `abc_123`.
    pub token: String,
}

impl RequestHandle {
    /// The capture request ID used with `binguxctl`, for example `1_23-abc`.
    pub fn capture_id(&self) -> String {
        format!("{}-{}", self.component, self.token)
    }
}

/// Parses `/org/freedesktop/portal/desktop/request/<SENDER>/<TOKEN>`.
///
/// `<SENDER>` is a unique bus name with the leading `:` removed and each `.`
/// replaced by `_`. Both halves must be digits. The token is ASCII letters,
/// digits and `_`.
pub fn parse_request_handle(handle: &str) -> Option<RequestHandle> {
    let rest = handle.strip_prefix(REQUEST_PATH_PREFIX)?;
    let (component, token) = rest.split_once('/')?;
    if token.is_empty()
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    let (major, minor) = component.split_once('_')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(major) || !digits(minor) {
        return None;
    }
    Some(RequestHandle {
        sender: format!(":{major}.{minor}"),
        component: component.to_owned(),
        token: token.to_owned(),
    })
}

/// The identity for a non-empty app ID.
pub fn app_identity(app_id: &str) -> Option<Identity> {
    (!app_id.is_empty()).then(|| Identity::AppId(app_id.to_owned()))
}

/// The identity for an executable path. The path must be absolute and name a
/// regular file once symlinks are resolved.
pub fn exe_identity(exe: &Path) -> Option<Identity> {
    if !exe.is_absolute() {
        return None;
    }
    let canonical = std::fs::canonicalize(exe).ok()?;
    if !canonical.is_file() {
        return None;
    }
    Some(Identity::HostExe(canonical.to_str()?.to_owned()))
}

/// Whether `caller` currently owns the bus name `name`.
pub async fn owner_is(connection: &Connection, caller: &str, name: &str) -> bool {
    let Ok(proxy) = fdo::DBusProxy::new(connection).await else {
        return false;
    };
    let Ok(name) = BusName::try_from(name) else {
        return false;
    };
    match proxy.get_name_owner(name).await {
        Ok(owner) => owner.as_str() == caller,
        Err(_) => false,
    }
}

/// Whether `caller` currently owns the desktop portal frontend name.
pub async fn frontend_is(connection: &Connection, caller: &str) -> bool {
    owner_is(connection, caller, PORTAL_DESKTOP_NAME).await
}

/// Resolves the requester identity, or `None` when the request must be refused.
///
/// `caller` is the unique name that called the backend. `uid` is this
/// backend's user ID.
pub async fn resolve(
    connection: &Connection,
    caller: &str,
    app_id: &str,
    handle: &str,
    uid: u32,
) -> Option<Identity> {
    if !frontend_is(connection, caller).await {
        return None;
    }
    let proxy = fdo::DBusProxy::new(connection).await.ok()?;

    let request = parse_request_handle(handle)?;
    let requester = BusName::try_from(request.sender.as_str()).ok()?;
    let requester_uid = proxy
        .get_connection_unix_user(requester.clone())
        .await
        .ok()?;
    if requester_uid != uid {
        return None;
    }

    if let Some(identity) = app_identity(app_id) {
        return Some(identity);
    }

    let pid = proxy.get_connection_unix_process_id(requester).await.ok()?;
    if pid == 0 {
        return None;
    }
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    exe_identity(&exe)
}

/// The UID of this process.
pub fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_handle() {
        let handle =
            parse_request_handle("/org/freedesktop/portal/desktop/request/1_23/abc_9").unwrap();
        assert_eq!(handle.sender, ":1.23");
        assert_eq!(handle.component, "1_23");
        assert_eq!(handle.token, "abc_9");
        assert_eq!(handle.capture_id(), "1_23-abc_9");
    }

    #[test]
    fn rejects_malformed_handles() {
        let bad = [
            "/org/freedesktop/portal/desktop/request/123/abc",
            "/org/freedesktop/portal/desktop/request/1_2_3/abc",
            "/org/freedesktop/portal/desktop/request/a_1/abc",
            "/org/freedesktop/portal/desktop/request/1_/abc",
            "/org/freedesktop/portal/desktop/request/_1/abc",
            "/org/freedesktop/portal/desktop/request/1_2/",
            "/org/freedesktop/portal/desktop/request/1_2/a-b",
            "/org/freedesktop/portal/desktop/request/1_2/a/b",
            "/org/freedesktop/portal/desktop/session/1_2/abc",
            "/org/freedesktop/portal/desktop/request/1_2",
        ];
        for handle in bad {
            assert_eq!(parse_request_handle(handle), None, "{handle}");
        }
    }

    #[test]
    fn identity_keys_and_selectors() {
        let app = Identity::AppId("org.example.App".to_owned());
        assert_eq!(app.key(), "app-id:org.example.App");
        assert_eq!(app.app_id(), Some("org.example.App"));
        assert_eq!(app.exe(), None);

        let exe = Identity::HostExe("/usr/bin/tool".to_owned());
        assert_eq!(exe.key(), "host-exe:/usr/bin/tool");
        assert_eq!(exe.exe(), Some("/usr/bin/tool"));
        assert_eq!(exe.app_id(), None);
    }

    #[test]
    fn empty_app_id_has_no_identity() {
        assert_eq!(app_identity(""), None);
        assert!(app_identity("org.example.App").is_some());
    }

    #[test]
    fn exe_identity_requires_absolute_regular_file() {
        assert_eq!(exe_identity(Path::new("relative/bin")), None);
        assert_eq!(exe_identity(Path::new("/")), None);
        assert_eq!(exe_identity(Path::new("/nonexistent/bingux-test")), None);
        let me = std::env::current_exe().unwrap();
        assert!(matches!(exe_identity(&me), Some(Identity::HostExe(_))));
    }
}
