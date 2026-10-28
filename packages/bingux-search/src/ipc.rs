//! The app search socket: how Spotlight (or any same-user process) searches
//! what an app exposes. Framing and permissions are `bingux-searchd`'s
//! (`search-v1.sock`): newline-delimited UTF-8 JSON, versioned records, a
//! 0700 directory and 0600 sockets. See `design/search.md` 8.4-8.6.
//!
//! A provider ([`serve`]) listens on
//! `$XDG_RUNTIME_DIR/bingux/search/<app-id>.sock` while the app runs and
//! answers `hello`, `search`, `activate` and `subscribe`. Only external
//! indexes and external documents are visible. It also keeps the on-disk
//! snapshots of external indexes up to date.

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::error::Error;
use crate::service::SearchService;
use crate::snapshot;
use crate::text::Cancel;

/// Protocol version carried in every record as `"v"`.
pub const PROTOCOL_VERSION: u64 = 1;
/// The longest record. A provider's response carries rich hits.
pub const MAX_RECORD_BYTES: usize = 1 << 20;
const RUNTIME_DIRECTORY_MODE: u32 = 0o700;
const SOCKET_MODE: u32 = 0o600;
const MAX_CONNECTIONS: usize = 16;
const SUBSCRIBE_RATE: Duration = Duration::from_millis(250);

// ------------------------------------------------------------------ framing

/// Reads one record (the bytes before a newline) of at most `max` bytes.
/// `Ok(None)` at a clean end of stream.
pub fn read_record<R: BufRead>(reader: &mut R, max: usize) -> io::Result<Option<Vec<u8>>> {
    let mut record = Vec::with_capacity(max.min(4096) + 1);
    loop {
        let (consumed, ends_record) = {
            let buffer = reader.fill_buf()?;
            if buffer.is_empty() {
                if record.is_empty() {
                    return Ok(None);
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "peer closed a record before its newline delimiter",
                ));
            }
            let remaining = max + 1 - record.len();
            let newline = buffer.iter().position(|byte| *byte == b'\n');
            let consumed = newline
                .map_or(buffer.len(), |index| index + 1)
                .min(remaining);
            let ends_record = newline.is_some_and(|index| index + 1 == consumed);
            record.extend_from_slice(&buffer[..consumed]);
            (consumed, ends_record)
        };
        reader.consume(consumed);
        if ends_record {
            record.pop();
            return Ok(Some(record));
        }
        if record.len() > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "peer sent a record that exceeds the protocol limit",
            ));
        }
    }
}

/// Makes the directory of `socket_path` (0700, owned by the active user) and
/// removes a stale socket file there. Refuses to replace anything that is not
/// a socket.
pub fn prepare_socket_path(socket_path: &Path) -> io::Result<()> {
    let parent = socket_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "socket path must have a parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let metadata = fs::metadata(parent)?;
    if metadata.uid() != fs::metadata("/proc/self")?.uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "runtime directory is not owned by the active user",
        ));
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(RUNTIME_DIRECTORY_MODE))?;
    match fs::symlink_metadata(socket_path) {
        Ok(metadata) if metadata.file_type().is_socket() => fs::remove_file(socket_path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "refusing to replace a non-socket path",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Binds a unix socket at `socket_path` with mode 0600.
pub fn bind_listener(socket_path: &Path) -> io::Result<UnixListener> {
    prepare_socket_path(socket_path)?;
    let listener = UnixListener::bind(socket_path)?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(SOCKET_MODE))?;
    Ok(listener)
}

fn write_record(stream: &mut UnixStream, value: &Value) -> io::Result<()> {
    let mut line = serde_json::to_vec(value).map_err(io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)
}

/// `$XDG_RUNTIME_DIR/bingux/search`, where app sockets live.
pub fn socket_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR").filter(|p| !p.is_empty())?;
    Some(PathBuf::from(base).join("bingux/search"))
}

fn valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

// ----------------------------------------------------------------- provider

/// What an app tells its provider.
pub struct ProviderConfig {
    /// The app id: the socket and snapshot directory name.
    pub app_id: String,
    pub app_version: String,
    /// Opens `uri` in the app, on its UI thread. Returns whether it was
    /// accepted.
    pub on_activate: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    /// Where the socket goes; `None` is [`socket_dir`].
    pub socket_dir: Option<PathBuf>,
    /// Where snapshots go; `None` is [`snapshot::state_root`], and no
    /// snapshots are written when that is unknown.
    pub state_root: Option<PathBuf>,
    /// Quiet time after a change before the snapshots are written.
    pub snapshot_delay: Duration,
}

impl ProviderConfig {
    pub fn new(
        app_id: &str,
        app_version: &str,
        on_activate: impl Fn(&str) -> bool + Send + Sync + 'static,
    ) -> Self {
        ProviderConfig {
            app_id: app_id.to_string(),
            app_version: app_version.to_string(),
            on_activate: Arc::new(on_activate),
            socket_dir: None,
            state_root: None,
            snapshot_delay: Duration::from_secs(2),
        }
    }
}

/// A running provider. Dropping it closes the socket.
pub struct Provider {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    service: Arc<SearchService>,
    shared: Arc<Shared>,
}

struct Shared {
    service: Arc<SearchService>,
    cfg: ProviderConfig,
    connections: AtomicUsize,
}

impl Provider {
    pub fn socket_path(&self) -> &Path {
        &self.path
    }

    /// Writes the snapshot of every external index now.
    pub fn save_snapshots(&self) -> Vec<(String, Result<(), Error>)> {
        save_all(&self.shared)
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wake the accept loop and the watchers so they see the flag.
        let _ = UnixStream::connect(&self.path);
        self.service.poke();
        let _ = fs::remove_file(&self.path);
    }
}

fn save_all(shared: &Shared) -> Vec<(String, Result<(), Error>)> {
    let Some(root) = shared
        .cfg
        .state_root
        .clone()
        .or_else(snapshot::state_root)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in shared.service.names() {
        let Some(index) = shared.service.index(&name) else {
            continue;
        };
        if !index.schema().external {
            continue;
        }
        let path = snapshot::path_for(&root, &shared.cfg.app_id, &name);
        let r = snapshot::save(&index, &shared.cfg.app_id, &shared.cfg.app_version, &path)
            .map(|_| ());
        out.push((name, r));
    }
    out
}

/// Starts the provider of `service`: binds the socket and starts its threads.
pub fn serve(service: Arc<SearchService>, cfg: ProviderConfig) -> io::Result<Provider> {
    if !valid_app_id(&cfg.app_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "app id must be 1-64 of letters, digits, `-`, `_`, `.`",
        ));
    }
    let dir = cfg
        .socket_dir
        .clone()
        .or_else(socket_dir)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))?;
    let path = dir.join(format!("{}.sock", cfg.app_id));
    let listener = bind_listener(&path)?;
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Shared {
        service: service.clone(),
        cfg,
        connections: AtomicUsize::new(0),
    });

    // Accept loop.
    {
        let shared = shared.clone();
        let stop = stop.clone();
        std::thread::Builder::new()
            .name("bingux-search-accept".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    if shared.connections.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
                        shared.connections.fetch_sub(1, Ordering::AcqRel);
                        continue;
                    }
                    let conn_shared = shared.clone();
                    let conn_stop = stop.clone();
                    let spawned = std::thread::Builder::new()
                        .name("bingux-search-conn".into())
                        .spawn(move || {
                            handle_connection(&conn_shared, stream, &conn_stop);
                            conn_shared.connections.fetch_sub(1, Ordering::AcqRel);
                        });
                    if spawned.is_err() {
                        shared.connections.fetch_sub(1, Ordering::AcqRel);
                    }
                }
            })?;
    }

    // Snapshot writer: writes after changes have settled.
    {
        let shared = shared.clone();
        let stop = stop.clone();
        let rx = service.watch();
        std::thread::Builder::new()
            .name("bingux-search-snapshot".into())
            .spawn(move || {
                // The first pass is unconditional: the indexes may have loaded
                // before the provider started, and nothing changes afterwards.
                let mut first = true;
                loop {
                    if !first && rx.recv().is_err() {
                        break;
                    }
                    first = false;
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    // Wait for a quiet period.
                    loop {
                        match rx.recv_timeout(shared.cfg.snapshot_delay) {
                            Ok(_) => {
                                if stop.load(Ordering::Acquire) {
                                    return;
                                }
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    for (name, r) in save_all(&shared) {
                        if let Err(e) = r {
                            eprintln!("bingux-search: could not write snapshot of `{name}`: {e}");
                        }
                    }
                }
            })?;
    }

    Ok(Provider {
        path,
        stop,
        service,
        shared,
    })
}

fn error_record(id: &Value, error: &Error) -> Value {
    let body = error.to_json();
    json!({
        "v": PROTOCOL_VERSION,
        "type": "error",
        "id": id,
        "error": body["error"],
        "status": body["status"],
    })
}

/// Answers one request record. Separate from the socket so it is testable.
pub fn respond(service: &Arc<SearchService>, cfg: &ProviderConfig, request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("v").and_then(Value::as_u64) != Some(PROTOCOL_VERSION) {
        return error_record(
            &id,
            &Error::Request {
                kind: "illegal_argument_exception",
                reason: format!("unsupported protocol version, this is {PROTOCOL_VERSION}"),
            },
        );
    }
    match request.get("type").and_then(Value::as_str) {
        Some("hello") => {
            let mut indexes = Vec::new();
            for name in service.names() {
                let Some(index) = service.index(&name) else { continue };
                if !index.schema().external {
                    continue;
                }
                indexes.push(json!({
                    "name": name,
                    "title": index.schema().title,
                    "documents": index.external_len(),
                    "generation": index.generation(),
                    "updated": index.updated(),
                }));
            }
            json!({
                "v": PROTOCOL_VERSION, "type": "hello", "id": id,
                "app": cfg.app_id, "app_version": cfg.app_version, "indexes": indexes,
            })
        }
        Some("search") => {
            let Some(index) = request.get("index").and_then(Value::as_str) else {
                return error_record(&id, &Error::request("`index` is required"));
            };
            let body = request.get("body").cloned().unwrap_or_else(|| json!({}));
            match service.search_json(Some(index), &body, true, &Cancel::new()) {
                Ok(response) => {
                    let generation = service.index(index).map_or(0, |i| i.generation());
                    json!({
                        "v": PROTOCOL_VERSION, "type": "result", "id": id,
                        "generation": generation, "response": response,
                    })
                }
                Err(body) => json!({
                    "v": PROTOCOL_VERSION, "type": "error", "id": id,
                    "error": body["error"], "status": body["status"],
                }),
            }
        }
        Some("activate") => {
            let (Some(index), Some(doc)) = (
                request.get("index").and_then(Value::as_str),
                request.get("doc").and_then(Value::as_str),
            ) else {
                return error_record(&id, &Error::request("`index` and `doc` are required"));
            };
            let found = service
                .index(index)
                .filter(|i| i.schema().external)
                .and_then(|i| i.get_external(doc));
            let Some(document) = found else {
                return error_record(&id, &Error::UnknownIndex(format!("{index}/{doc}")));
            };
            let Some(uri) = document.fields.get("uri").and_then(Value::as_str) else {
                return error_record(&id, &Error::request("the document has no `uri`"));
            };
            if (cfg.on_activate)(uri) {
                json!({ "v": PROTOCOL_VERSION, "type": "activated", "id": id })
            } else {
                error_record(&id, &Error::request("the app did not accept the address"))
            }
        }
        other => error_record(
            &id,
            &Error::parsing(format!(
                "unknown request type [{}]",
                other.unwrap_or("missing")
            )),
        ),
    }
}

fn handle_connection(shared: &Shared, stream: UnixStream, stop: &AtomicBool) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let Ok(mut writer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        let record = match read_record(&mut reader, MAX_RECORD_BYTES) {
            Ok(Some(r)) => r,
            _ => return,
        };
        let request: Value = match serde_json::from_slice(&record) {
            Ok(v) => v,
            Err(e) => {
                let _ = write_record(
                    &mut writer,
                    &error_record(&Value::Null, &Error::parsing(format!("not JSON: {e}"))),
                );
                return;
            }
        };
        if request.get("type").and_then(Value::as_str) == Some("subscribe")
            && request.get("v").and_then(Value::as_u64) == Some(PROTOCOL_VERSION)
        {
            let _ = write_record(
                &mut writer,
                &json!({ "v": PROTOCOL_VERSION, "type": "subscribed", "id": request.get("id") }),
            );
            subscribe_loop(shared, &mut writer, stop);
            return;
        }
        let reply = respond(&shared.service, &shared.cfg, &request);
        if write_record(&mut writer, &reply).is_err() {
            return;
        }
    }
}

/// Streams `changed` events for external indexes, at most one per index per
/// [`SUBSCRIBE_RATE`].
fn subscribe_loop(shared: &Shared, writer: &mut UnixStream, stop: &AtomicBool) {
    let rx = shared.service.watch();
    while let Ok(first) = rx.recv() {
        if stop.load(Ordering::Acquire) {
            return;
        }
        let mut latest: HashMap<String, u64> = HashMap::new();
        latest.insert(first.index, first.generation);
        let until = Instant::now() + SUBSCRIBE_RATE;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(c) => {
                    latest.insert(c.index, c.generation);
                }
                Err(_) => break,
            }
        }
        for (index, generation) in latest {
            let external = shared
                .service
                .index(&index)
                .is_some_and(|i| i.schema().external);
            if !external {
                continue;
            }
            let record = json!({
                "v": PROTOCOL_VERSION, "type": "changed", "index": index, "generation": generation,
            });
            if write_record(writer, &record).is_err() {
                return;
            }
        }
    }
}

// ------------------------------------------------------------------- client

/// Sends one request to a provider socket and reads its reply, within
/// `timeout` for the whole exchange.
pub fn request(path: &Path, request: &Value, timeout: Duration) -> io::Result<Value> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_write_timeout(Some(timeout))?;
    stream.set_read_timeout(Some(timeout))?;
    write_record(&mut stream, request)?;
    let mut reader = BufReader::new(stream);
    let record = read_record(&mut reader, MAX_RECORD_BYTES)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "provider closed the socket"))?;
    serde_json::from_slice(&record).map_err(io::Error::other)
}

/// The sockets of the running providers: `(app id, path)`. A path that does
/// not accept a connection is skipped and its stale file removed.
pub fn running(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".sock"))
        else {
            continue;
        };
        if !entry.file_type().is_ok_and(|t| t.is_socket()) {
            continue;
        }
        match UnixStream::connect(&path) {
            Ok(_) => out.push((id.to_string(), path)),
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                let _ = fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
    out.sort();
    out
}
