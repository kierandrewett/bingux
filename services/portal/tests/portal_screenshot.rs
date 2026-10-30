//! Drives `xdg-desktop-portal-bingux` on a private session bus.
//!
//! The backend only serves the desktop portal frontend, so the test connection
//! that calls `Screenshot` owns `org.freedesktop.portal.Desktop`. Each
//! application is a separate connection in this process. The request handle
//! names it, so the backend reads its uid and its executable from the bus.
//! A fake `binguxctl` logs each call and replies from a fixed script.
//! Needs `dbus-daemon` on PATH.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use async_io::Timer;
use futures_util::future::join;
use zbus::fdo::DBusProxy;
use zbus::names::BusName;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy, connection};

const BACKEND_NAME: &str = "org.freedesktop.impl.portal.desktop.bingux";
const BACKEND_PATH: &str = "/org/freedesktop/portal/desktop";
const SCREENSHOT_IFACE: &str = "org.freedesktop.impl.portal.Screenshot";
const REQUEST_IFACE: &str = "org.freedesktop.impl.portal.Request";
const FRONTEND_NAME: &str = "org.freedesktop.portal.Desktop";
const REQUEST_PREFIX: &str = "/org/freedesktop/portal/desktop/request";
const ACCESS_DENIED: &str = "org.freedesktop.DBus.Error.AccessDenied";

const RESPONSE_SUCCESS: u32 = 0;
const RESPONSE_CANCELLED: u32 = 1;
const RESPONSE_OTHER: u32 = 2;

const OK: &str = r#"{"ok":true}"#;
const PENDING: &str = r#"{"id":"x","state":"pending"}"#;
const SAVED: &str = r#"{"id":"x","state":"saved","path":"/var/tmp/shot 1.png"}"#;
const CANCELLED: &str = r#"{"id":"x","state":"cancelled"}"#;
const ERROR: &str = r#"{"id":"x","state":"error","message":"no output"}"#;
const BUSY: &str = r#"{"ok":false,"error":"busy"}"#;

type Response = (u32, HashMap<String, OwnedValue>);

static NEXT: AtomicU32 = AtomicU32::new(0);

/// The fake `binguxctl` replies for `capture request` and `capture result`.
struct Script {
    request: &'static str,
    result: &'static str,
}

struct Fixture {
    dir: PathBuf,
    daemon: Child,
    _daemon_stdout: BufReader<ChildStdout>,
    backend: Child,
    log: PathBuf,
    store: PathBuf,
    address: String,
    /// Owns `org.freedesktop.portal.Desktop`, as the real frontend does.
    frontend: Connection,
    /// The requesting application.
    app: Connection,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.backend.kill();
        let _ = self.backend.wait();
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn bus_config(socket: &Path) -> String {
    format!(
        r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow receive_sender="*" eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
        socket.display()
    )
}

/// Forking while another thread holds a freshly written script open fails
/// with ETXTBSY. Each fixture writes its script and spawns children under
/// this lock, so parallel tests cannot interleave the two.
fn spawn_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_ctl(dir: &Path, log: &Path, script: &Option<Script>) -> PathBuf {
    let Some(script) = script else {
        return dir.join("no-such-binguxctl");
    };
    let path = dir.join("binguxctl");
    let text = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\ncase \"$2\" in\n  request) echo '{}' ;;\n  result) echo '{}' ;;\n  *) echo '{}' ;;\nesac\n",
        log.display(),
        script.request,
        script.result,
        OK
    );
    fs::write(&path, text).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn connect(address: &str) -> connection::Builder<'static> {
    connection::Builder::address(address).unwrap()
}

impl Fixture {
    async fn start(script: Option<Script>) -> Self {
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("bingux-portal-it-{}-{id}", std::process::id()));
        fs::create_dir_all(dir.join("config")).unwrap();
        let log = dir.join("binguxctl.log");
        let store = dir.join("config/bingux/portal-permissions.toml");
        let _spawn = spawn_lock();
        let ctl = write_ctl(&dir, &log, &script);

        let config = dir.join("bus.conf");
        fs::write(&config, bus_config(&dir.join("bus.sock"))).unwrap();
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .arg("--nofork")
            .arg("--print-address=1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon must be on PATH for the portal integration test");
        let mut daemon_stdout = BufReader::new(daemon.stdout.take().unwrap());
        let mut address = String::new();
        daemon_stdout.read_line(&mut address).unwrap();
        let address = address.trim().to_owned();

        let backend = Command::new(env!("CARGO_BIN_EXE_xdg-desktop-portal-bingux"))
            .env("DBUS_SESSION_BUS_ADDRESS", &address)
            .env("BINGUXCTL", &ctl)
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(File::create(dir.join("backend.log")).unwrap())
            .spawn()
            .unwrap();
        drop(_spawn);

        let frontend = connect(&address)
            .name(FRONTEND_NAME)
            .unwrap()
            .build()
            .await
            .unwrap();
        let app = connect(&address).build().await.unwrap();

        let fixture = Fixture {
            dir,
            daemon,
            _daemon_stdout: daemon_stdout,
            backend,
            log,
            store,
            address,
            frontend,
            app,
        };
        fixture.wait_for_backend().await;
        fixture
    }

    async fn wait_for_backend(&self) {
        let proxy = DBusProxy::new(&self.frontend).await.unwrap();
        let name = BusName::try_from(BACKEND_NAME).unwrap();
        for _ in 0..100 {
            if proxy.name_has_owner(name.clone()).await.unwrap() {
                return;
            }
            Timer::after(Duration::from_millis(50)).await;
        }
        panic!("the backend never took {BACKEND_NAME}");
    }

    /// A connection that does not own the frontend name.
    async fn impostor(&self) -> Connection {
        connect(&self.address).build().await.unwrap()
    }

    /// The handle component for the application: its unique name with `:`
    /// removed and `.` replaced by `_`.
    fn component(&self) -> String {
        let name = self.app.unique_name().unwrap().as_str();
        name.trim_start_matches(':').replace('.', "_")
    }

    fn handle(&self, token: &str) -> String {
        format!("{REQUEST_PREFIX}/{}/{token}", self.component())
    }

    fn capture_id(&self, token: &str) -> String {
        format!("{}-{token}", self.component())
    }

    fn write_store(&self, text: &str) {
        fs::create_dir_all(self.store.parent().unwrap()).unwrap();
        fs::write(&self.store, text).unwrap();
    }

    fn log_lines(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The `capture request` calls only, without result polls.
    fn requests(&self) -> Vec<String> {
        self.log_lines()
            .into_iter()
            .filter(|line| line.starts_with("capture request "))
            .collect()
    }

    /// Calls `Screenshot` as the frontend, for an application's handle.
    async fn screenshot(
        &self,
        handle: &str,
        app_id: &str,
        options: HashMap<&str, Value<'_>>,
    ) -> Response {
        call_screenshot(&self.frontend, handle, app_id, options).await
    }
}

async fn call_screenshot(
    caller: &Connection,
    handle: &str,
    app_id: &str,
    options: HashMap<&str, Value<'_>>,
) -> Response {
    let proxy = Proxy::new(caller, BACKEND_NAME, BACKEND_PATH, SCREENSHOT_IFACE)
        .await
        .unwrap();
    let handle = ObjectPath::try_from(handle).unwrap();
    proxy
        .call("Screenshot", &(handle, app_id, "", options))
        .await
        .unwrap()
}

fn interactive(value: bool) -> HashMap<&'static str, Value<'static>> {
    HashMap::from([("interactive", Value::from(value))])
}

fn no_options() -> HashMap<&'static str, Value<'static>> {
    HashMap::new()
}

fn uri_of(response: &Response) -> String {
    String::try_from(response.1.get("uri").cloned().expect("no uri result")).unwrap()
}

fn write_rules(fixture: &Fixture, rules: &str) {
    fixture.write_store(&format!("version = 1\n{rules}"));
}

const SELECTOR_ARGS: &str = "--interactive --mode screenshot";

#[test]
fn saved_capture_returns_a_file_uri() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: SAVED,
        }))
        .await;
        let response = f
            .screenshot(&f.handle("tokA"), "org.example.Shot", no_options())
            .await;
        assert_eq!(response.0, RESPONSE_SUCCESS);
        assert_eq!(uri_of(&response), "file:///var/tmp/shot%201.png");
        // No store file means the fallback is ask, so the selector shows.
        assert_eq!(
            f.requests(),
            [format!(
                "capture request {} {SELECTOR_ARGS}",
                f.capture_id("tokA")
            )]
        );
    });
}

#[test]
fn user_cancel_returns_response_one() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: CANCELLED,
        }))
        .await;
        let response = f
            .screenshot(&f.handle("tokB"), "org.example.Shot", no_options())
            .await;
        assert_eq!(response.0, RESPONSE_CANCELLED);
        assert!(response.1.is_empty());
    });
}

#[test]
fn shell_error_returns_response_two() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: ERROR,
        }))
        .await;
        let response = f
            .screenshot(&f.handle("tokE"), "org.example.Shot", no_options())
            .await;
        assert_eq!(response.0, RESPONSE_OTHER);
        assert!(response.1.is_empty());
    });
}

#[test]
fn busy_shell_refuses_at_once() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: BUSY,
            result: PENDING,
        }))
        .await;
        let started = Instant::now();
        let response = f
            .screenshot(&f.handle("tokBusy"), "org.example.Shot", no_options())
            .await;
        assert_eq!(response.0, RESPONSE_OTHER);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "busy reply was slow"
        );
        // Only the request was sent: no polling and no cancel.
        assert_eq!(f.log_lines().len(), 1);
    });
}

#[test]
fn missing_shell_refuses_promptly() {
    async_io::block_on(async {
        let f = Fixture::start(None).await;
        let started = Instant::now();
        let response = f
            .screenshot(&f.handle("tokNone"), "org.example.Shot", no_options())
            .await;
        assert_eq!(response.0, RESPONSE_OTHER);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "missing shell was slow"
        );
        assert!(f.log_lines().is_empty());
    });
}

#[test]
fn deny_rule_refuses_without_calling_the_shell() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: SAVED,
        }))
        .await;
        write_rules(
            &f,
            r#"
[[rule]]
name = "block"
app_id = "org.example.Blocked"
capabilities = ["screenshot"]
level = "deny"
"#,
        );
        let blocked = f
            .screenshot(&f.handle("tokDeny"), "org.example.Blocked", no_options())
            .await;
        assert_eq!(blocked.0, RESPONSE_OTHER);
        assert!(
            f.log_lines().is_empty(),
            "a denied request reached binguxctl"
        );

        // The rule names one app. Another app falls back to ask.
        let other = f
            .screenshot(&f.handle("tokOther"), "org.example.Other", no_options())
            .await;
        assert_eq!(uri_of(&other), "file:///var/tmp/shot%201.png");
        assert_eq!(
            f.requests(),
            [format!(
                "capture request {} {SELECTOR_ARGS}",
                f.capture_id("tokOther")
            )]
        );
    });
}

#[test]
fn allow_and_ask_rules_choose_the_selector() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: SAVED,
        }))
        .await;
        write_rules(
            &f,
            r#"
[[rule]]
name = "quiet"
app_id = "org.example.Quiet"
capabilities = ["screenshot"]
level = "allow"

[[rule]]
name = "forced"
app_id = "org.example.Forced"
capabilities = ["screenshot"]
level = "ask"
"#,
        );

        // allow without the interactive option: no selector.
        let quiet = f
            .screenshot(&f.handle("tq1"), "org.example.Quiet", no_options())
            .await;
        assert_eq!(quiet.0, RESPONSE_SUCCESS);
        // allow with the interactive option: the selector shows.
        let asked = f
            .screenshot(&f.handle("tq2"), "org.example.Quiet", interactive(true))
            .await;
        assert_eq!(asked.0, RESPONSE_SUCCESS);
        // ask forces the selector even when the app did not ask for it.
        let forced = f
            .screenshot(&f.handle("tq3"), "org.example.Forced", interactive(false))
            .await;
        assert_eq!(forced.0, RESPONSE_SUCCESS);

        assert_eq!(
            f.requests(),
            [
                format!("capture request {} --desktop", f.capture_id("tq1")),
                format!("capture request {} {SELECTOR_ARGS}", f.capture_id("tq2")),
                format!("capture request {} {SELECTOR_ARGS}", f.capture_id("tq3")),
            ]
        );
    });
}

#[test]
fn exe_rule_matches_an_app_without_an_app_id() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: SAVED,
        }))
        .await;
        // With no app ID the backend reads the requester's executable, which is
        // this test binary.
        let exe = fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
        write_rules(
            &f,
            &format!(
                r#"
[[rule]]
name = "host"
exe = "{}"
capabilities = ["screenshot"]
level = "deny"
"#,
                exe.display()
            ),
        );
        let host = f.screenshot(&f.handle("th1"), "", no_options()).await;
        assert_eq!(host.0, RESPONSE_OTHER);
        assert!(f.log_lines().is_empty());

        // An app ID takes precedence over the executable, so the rule no longer applies.
        let flatpak = f
            .screenshot(&f.handle("th2"), "org.example.Free", no_options())
            .await;
        assert_eq!(flatpak.0, RESPONSE_SUCCESS);
    });
}

#[test]
fn frontend_close_cancels_the_capture() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: PENDING,
        }))
        .await;
        let handle = f.handle("tokClose");
        let shot = f.screenshot(&handle, "org.example.Shot", no_options());

        let close = async {
            Timer::after(Duration::from_millis(500)).await;

            // A connection that is not the frontend cannot close the request.
            let impostor = f.impostor().await;
            let request = Proxy::new(&impostor, BACKEND_NAME, handle.as_str(), REQUEST_IFACE)
                .await
                .unwrap();
            let refused: zbus::Result<()> = request.call("Close", &()).await;
            match refused {
                Err(zbus::Error::MethodError(name, _, _)) => {
                    assert_eq!(name.as_str(), ACCESS_DENIED);
                }
                other => panic!("non-frontend Close was not refused: {other:?}"),
            }

            // The frontend that made the request may close it.
            let request = Proxy::new(&f.frontend, BACKEND_NAME, handle.as_str(), REQUEST_IFACE)
                .await
                .unwrap();
            let closed: zbus::Result<()> = request.call("Close", &()).await;
            closed.unwrap();
        };

        let (response, ()) = join(shot, close).await;
        assert_eq!(response.0, RESPONSE_CANCELLED);
        assert!(response.1.is_empty());
        let cancel = format!("capture cancel-request {}", f.capture_id("tokClose"));
        assert!(
            f.log_lines().contains(&cancel),
            "close did not cancel the shell request"
        );
    });
}

#[test]
fn caller_that_is_not_the_frontend_is_refused() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: SAVED,
        }))
        .await;
        let impostor = f.impostor().await;
        let response = call_screenshot(
            &impostor,
            &f.handle("tokImp"),
            "org.example.Shot",
            no_options(),
        )
        .await;
        assert_eq!(response.0, RESPONSE_OTHER);
        assert!(f.log_lines().is_empty(), "an impostor reached binguxctl");
    });
}

#[test]
fn handle_that_names_no_requester_is_refused() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: SAVED,
        }))
        .await;
        // Component 9_999999 names no connection on this bus.
        let handle = format!("{REQUEST_PREFIX}/9_999999/tokGhost");
        let response = f
            .screenshot(&handle, "org.example.Shot", no_options())
            .await;
        assert_eq!(response.0, RESPONSE_OTHER);
        assert!(f.log_lines().is_empty());
    });
}

#[test]
fn invalid_store_denies_every_request() {
    async_io::block_on(async {
        let f = Fixture::start(Some(Script {
            request: OK,
            result: SAVED,
        }))
        .await;
        f.write_store("version = 9\n");
        let response = f
            .screenshot(&f.handle("tokBad"), "org.example.Shot", no_options())
            .await;
        assert_eq!(response.0, RESPONSE_OTHER);
        assert!(f.log_lines().is_empty());
    });
}
