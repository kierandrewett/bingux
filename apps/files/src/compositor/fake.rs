//! A fake Gnoblin control socket for tests, and the window list it answers
//! with. It speaks the wire as `src/control/gnoblin-control-connection.c`
//! (the greeting), `gnoblin-control-wire.c` (`encode_response`: an `event` of
//! `reply` with `id` and `result`, or `error` with `id` and `message`) and
//! `gnoblin-control-requests.c` (a request is one JSON object per line) have it.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{Gnoblin, Locator};

/// The `result` of `windows.list` as Gnoblin writes it: the Lua window
/// records (`lua_window_record` in `gnoblin-control-publish.c`, the field
/// names of `window_property_names`), here on two monitors, the second at
/// scale 1.5 and left of the first so its coordinates are negative.
pub(crate) fn windows_list() -> serde_json::Value {
    serde_json::json!([
        {
            "id": "12", "title": "Home – Files", "app_id": "bingux-files", "gtk_app_id": "",
            "wm_class": "bingux-files", "rule_app_id": "bingux-files", "focused": true,
            "workspace_id": "workspace-1", "workspace_number": 1, "monitor_index": 0,
            "monitor_id": "DP-1", "above": false, "sticky": false, "minimized": false,
            "maximized": false, "fullscreen": false, "modal": false,
            "frame": {"x": 100, "y": 60, "width": 600, "height": 400}, "revision": 90
        },
        {
            "id": "13", "title": "Documents – Files (2)", "app_id": "bingux-files",
            "workspace_id": "workspace-1", "sticky": false, "minimized": false,
            "frame": {"x": -1280, "y": -120, "width": 600, "height": 400}, "revision": 90
        },
        {
            "id": "14", "title": "Notes", "app_id": "org.example.Editor.desktop",
            "workspace_id": "workspace-2", "sticky": false, "minimized": true,
            "frame": {"x": 5, "y": 6, "width": 7, "height": 8}, "revision": 90
        },
        {
            "id": "15", "title": "Panel helper", "app_id": "bingux-files",
            "workspace_id": "workspace-2", "sticky": true, "minimized": false,
            "frame": {"x": 0, "y": 0, "width": 10, "height": 10}, "revision": 90
        },
        { "id": "16", "title": "No frame", "app_id": "bingux-files", "revision": 90 }
    ])
}

/// A control socket: one greeting line on connect, then a `reply` or `error`
/// line for each request, ids echoed.
pub(crate) struct Fake {
    pub path: PathBuf,
    /// Connections accepted.
    pub accepted: Arc<AtomicUsize>,
    /// Every request line received, parsed.
    pub requests: Arc<Mutex<Vec<serde_json::Value>>>,
    /// Set to make the server answer requests with this error.
    pub refuse: Arc<Mutex<Option<&'static str>>>,
    /// The `result` of every reply.
    pub result: Arc<Mutex<serde_json::Value>>,
    /// The greeting's `api_minor` and `methods`.
    pub hello: Arc<Mutex<(u64, Vec<&'static str>)>>,
    /// Close each connection after this many requests (0: keep it).
    pub close_after: Arc<AtomicUsize>,
    /// Send an event line and another request's reply before each reply.
    pub chatty: Arc<Mutex<bool>>,
    /// Connections closed by the client.
    pub hung_up: Arc<AtomicUsize>,
}

static SOCKETS: AtomicUsize = AtomicUsize::new(0);

type Shared = (
    Arc<Mutex<Vec<serde_json::Value>>>,
    Arc<Mutex<Option<&'static str>>>,
    Arc<Mutex<serde_json::Value>>,
    Arc<Mutex<(u64, Vec<&'static str>)>>,
    Arc<AtomicUsize>,
    Arc<Mutex<bool>>,
    Arc<AtomicUsize>,
);

impl Fake {
    pub(crate) fn start(result: serde_json::Value) -> Fake {
        let path = std::env::temp_dir().join(format!(
            "bingux-files-fake-gnoblin-{}-{}.sock",
            std::process::id(),
            SOCKETS.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("bind the fake control socket");
        let fake = Fake {
            path,
            accepted: Arc::default(),
            requests: Arc::default(),
            refuse: Arc::default(),
            result: Arc::new(Mutex::new(result)),
            hello: Arc::new(Mutex::new((86, vec!["ping", "windows.list", "workspaces.list"]))),
            close_after: Arc::default(),
            chatty: Arc::default(),
            hung_up: Arc::default(),
        };
        let accepted = fake.accepted.clone();
        let shared: Shared = (
            fake.requests.clone(),
            fake.refuse.clone(),
            fake.result.clone(),
            fake.hello.clone(),
            fake.close_after.clone(),
            fake.chatty.clone(),
            fake.hung_up.clone(),
        );
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                accepted.fetch_add(1, Ordering::Relaxed);
                let shared = shared.clone();
                std::thread::spawn(move || serve(stream, &shared));
            }
        });
        fake
    }

    pub(crate) fn locator(&self) -> Locator {
        Locator::Gnoblin(Gnoblin::new(self.path.clone()))
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn serve(stream: UnixStream, shared: &Shared) {
    let (requests, refuse, result, hello, close_after, chatty, hung_up) = shared;
    let mut writer = stream.try_clone().expect("clone");
    let (minor, methods) = hello.lock().expect("hello").clone();
    let greeting = serde_json::json!({
        "event": "hello", "version": 1, "api_major": 1, "api_minor": minor,
        "state_revision": 3, "revision_scope": "window-workspace-monitor-layer-input-device-source-capability-state",
        "revision_semantics": "monotonic-per-session", "methods": methods, "events": [], "capabilities": [], "features": [],
    });
    if writeln!(writer, "{greeting}").is_err() {
        return;
    }
    let mut served = 0;
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let request: serde_json::Value = serde_json::from_str(&line).expect("a request is one JSON object per line");
        requests.lock().expect("requests").push(request.clone());
        let id = request["id"].as_str().unwrap_or_default().to_string();
        if *chatty.lock().expect("chatty") {
            let _ = writeln!(writer, r#"{{"event":"gnoblin.window.changed","revision":4}}"#);
            let _ = writeln!(writer, r#"{{"event":"reply","id":"someone-else","result":[]}}"#);
        }
        let reply = match *refuse.lock().expect("refuse") {
            Some(message) => serde_json::json!({"event": "error", "id": id, "message": message}),
            None => serde_json::json!({"event": "reply", "id": id, "result": result.lock().expect("result").clone()}),
        };
        if writeln!(writer, "{reply}").is_err() {
            return;
        }
        served += 1;
        let limit = close_after.load(Ordering::Relaxed);
        if limit != 0 && served >= limit {
            return;
        }
    }
    hung_up.fetch_add(1, Ordering::Relaxed);
}
