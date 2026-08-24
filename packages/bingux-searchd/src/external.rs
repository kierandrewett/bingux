use crate::protocol::{
    MAX_RECORD_BYTES, ProviderErrorCorrelation, ProviderManifest, ProviderRequest,
    ProviderResponse, ProviderStartup, encode_provider_request_line, parse_provider_manifest,
    parse_provider_response,
};
use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
const COMMAND_CAPACITY: usize = 64;
const READER_CAPACITY: usize = 32;
const MAX_PENDING: usize = 64;
const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_millis(50);
const PRIORITY_WEIGHT: f64 = 0.001;

#[derive(Debug, Clone)]
pub enum ExternalEvent {
    Results {
        provider_id: String,
        query_id: String,
        complete: bool,
        results: Vec<crate::protocol::ProviderResult>,
    },
    QueryFailed {
        provider_id: String,
        query_id: String,
    },
    Activated {
        provider_id: String,
        activation_id: String,
    },
    ActivationFailed {
        provider_id: String,
        activation_id: String,
    },
}
pub struct ExternalProviders {
    providers: BTreeMap<String, SyncSender<WorkerCommand>>,
    stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}
impl ExternalProviders {
    pub fn start(manifest_paths: &[PathBuf], events: SyncSender<ExternalEvent>) -> Result<Self> {
        let manifests = load_manifests(manifest_paths)?;
        let stop = Arc::new(AtomicBool::new(false));
        let mut providers: BTreeMap<String, SyncSender<WorkerCommand>> = BTreeMap::new();
        let mut workers: Vec<JoinHandle<()>> = Vec::with_capacity(manifests.len());
        for manifest in manifests {
            let id = manifest.id.clone();
            let (tx, rx) = mpsc::sync_channel(COMMAND_CAPACITY);
            let worker_stop = Arc::clone(&stop);
            let worker_events = events.clone();
            let name = format!("bingux-search-provider-{id}");
            let worker = thread::Builder::new()
                .name(name)
                .spawn(move || Worker::new(manifest, rx, worker_events, worker_stop).run());
            let worker = match worker {
                Ok(worker) => worker,
                Err(error) => {
                    stop.store(true, Ordering::Release);
                    for tx in providers.values() {
                        let _ = tx.try_send(WorkerCommand::Shutdown);
                    }
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(error).context("could not start external provider worker");
                }
            };
            providers.insert(id, tx);
            workers.push(worker);
        }
        Ok(Self {
            providers,
            stop,
            workers,
        })
    }
    pub fn query(&self, query_id: String, query: String, limit: u8) -> usize {
        self.providers
            .values()
            .filter(|tx| {
                tx.try_send(WorkerCommand::Query {
                    query_id: query_id.clone(),
                    query: query.clone(),
                    limit,
                })
                .is_ok()
            })
            .count()
    }
    pub fn activate(&self, provider_id: &str, activation_id: String, result_id: String) -> bool {
        self.providers.get(provider_id).is_some_and(|tx| {
            tx.send(WorkerCommand::Activate {
                activation_id,
                result_id,
            })
            .is_ok()
        })
    }
}
impl Drop for ExternalProviders {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for tx in self.providers.values() {
            let _ = tx.try_send(WorkerCommand::Shutdown);
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[derive(Debug)]
enum WorkerCommand {
    Query {
        query_id: String,
        query: String,
        limit: u8,
    },
    Activate {
        activation_id: String,
        result_id: String,
    },
    Shutdown,
}
#[derive(Clone, Debug)]
enum PendingRequest {
    Query {
        query_id: String,
        query: String,
        limit: u8,
    },
    Activate {
        activation_id: String,
        result_id: String,
    },
}
impl PendingRequest {
    fn request(&self) -> ProviderRequest {
        match self {
            Self::Query {
                query_id,
                query,
                limit,
            } => ProviderRequest::Query {
                query_id: query_id.clone(),
                query: query.clone(),
                limit: *limit,
            },
            Self::Activate {
                activation_id,
                result_id,
            } => ProviderRequest::Activate {
                activation_id: activation_id.clone(),
                result_id: result_id.clone(),
            },
        }
    }
    fn failure(&self, provider_id: &str) -> ExternalEvent {
        match self {
            Self::Query { query_id, .. } => ExternalEvent::QueryFailed {
                provider_id: provider_id.into(),
                query_id: query_id.clone(),
            },
            Self::Activate { activation_id, .. } => ExternalEvent::ActivationFailed {
                provider_id: provider_id.into(),
                activation_id: activation_id.clone(),
            },
        }
    }
}
struct PendingQuery {
    query: String,
    limit: u8,
    deadline: Instant,
    sent: bool,
}
struct PendingActivation {
    result_id: String,
    deadline: Instant,
    sent: bool,
}
#[derive(Default)]
struct Pending {
    queries: BTreeMap<String, PendingQuery>,
    activations: BTreeMap<String, PendingActivation>,
}
impl Pending {
    fn len(&self) -> usize {
        self.queries.len() + self.activations.len()
    }
    fn empty(&self) -> bool {
        self.queries.is_empty() && self.activations.is_empty()
    }
    fn query(&mut self, id: String, query: String, limit: u8, deadline: Instant) -> bool {
        if self.len() >= MAX_PENDING || self.queries.contains_key(&id) {
            return false;
        }
        self.queries.insert(
            id,
            PendingQuery {
                query,
                limit,
                deadline,
                sent: false,
            },
        );
        true
    }
    fn activate(&mut self, id: String, result_id: String, deadline: Instant) -> bool {
        if self.len() >= MAX_PENDING || self.activations.contains_key(&id) {
            return false;
        }
        self.activations.insert(
            id,
            PendingActivation {
                result_id,
                deadline,
                sent: false,
            },
        );
        true
    }
    fn unsent(&self) -> Vec<PendingRequest> {
        let mut requests = Vec::with_capacity(self.len());
        requests.extend(self.queries.iter().filter_map(|(id, p)| {
            (!p.sent).then(|| PendingRequest::Query {
                query_id: id.clone(),
                query: p.query.clone(),
                limit: p.limit,
            })
        }));
        requests.extend(self.activations.iter().filter_map(|(id, p)| {
            (!p.sent).then(|| PendingRequest::Activate {
                activation_id: id.clone(),
                result_id: p.result_id.clone(),
            })
        }));
        requests
    }
    fn sent(&mut self, request: &PendingRequest) {
        match request {
            PendingRequest::Query { query_id, .. } => {
                if let Some(p) = self.queries.get_mut(query_id) {
                    p.sent = true
                }
            }
            PendingRequest::Activate { activation_id, .. } => {
                if let Some(p) = self.activations.get_mut(activation_id) {
                    p.sent = true
                }
            }
        }
    }
    fn remove(&mut self, request: &PendingRequest) {
        match request {
            PendingRequest::Query { query_id, .. } => {
                self.queries.remove(query_id);
            }
            PendingRequest::Activate { activation_id, .. } => {
                self.activations.remove(activation_id);
            }
        }
    }
    fn deadline(&self) -> Option<Instant> {
        self.queries
            .values()
            .map(|p| p.deadline)
            .chain(self.activations.values().map(|p| p.deadline))
            .min()
    }
}
struct Session {
    child: Child,
    stdin: ChildStdin,
    reader: JoinHandle<()>,
    failed: Arc<AtomicBool>,
    generation: u64,
    hello_deadline: Instant,
    ready: bool,
}
struct ReaderEvent {
    generation: u64,
    response: ProviderResponse,
}
struct Worker {
    manifest: ProviderManifest,
    commands: Receiver<WorkerCommand>,
    events: SyncSender<ExternalEvent>,
    stop: Arc<AtomicBool>,
    reader_events: Receiver<ReaderEvent>,
    reader_tx: SyncSender<ReaderEvent>,
    pending: Pending,
    child: Option<Session>,
    next_start: Instant,
    backoff: Duration,
    generation: u64,
}
impl Worker {
    fn new(
        manifest: ProviderManifest,
        commands: Receiver<WorkerCommand>,
        events: SyncSender<ExternalEvent>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let (reader_tx, reader_events) = mpsc::sync_channel(READER_CAPACITY);
        Self {
            manifest,
            commands,
            events,
            stop,
            reader_events,
            reader_tx,
            pending: Pending::default(),
            child: None,
            next_start: Instant::now(),
            backoff: INITIAL_BACKOFF,
            generation: 0,
        }
    }
    fn run(mut self) {
        if matches!(self.manifest.startup, ProviderStartup::Eager) {
            self.progress(Instant::now());
        }
        while !self.stop.load(Ordering::Acquire) {
            self.progress(Instant::now());
            match self.commands.recv_timeout(self.wake(Instant::now())) {
                Ok(WorkerCommand::Query {
                    query_id,
                    query,
                    limit,
                }) => {
                    self.accept_query(query_id, query, limit, Instant::now());
                }
                Ok(WorkerCommand::Activate {
                    activation_id,
                    result_id,
                }) => {
                    self.accept_activation(activation_id, result_id, Instant::now());
                }
                Ok(WorkerCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        self.kill();
    }
    fn progress(&mut self, now: Instant) {
        self.read(now);
        self.expire(now);
        self.start(now);
        self.write_pending(now)
    }
    fn wake(&self, now: Instant) -> Duration {
        let mut wait = TICK;
        if let Some(child) = &self.child {
            if !child.ready {
                wait = wait.min(child.hello_deadline.saturating_duration_since(now));
            }
        } else if self.needed() && self.next_start > now {
            wait = wait.min(self.next_start.duration_since(now));
        }
        if let Some(deadline) = self.pending.deadline() {
            wait = wait.min(deadline.saturating_duration_since(now));
        }
        wait
    }
    fn needed(&self) -> bool {
        matches!(self.manifest.startup, ProviderStartup::Eager) || !self.pending.empty()
    }
    fn accept_query(&mut self, query_id: String, query: String, limit: u8, now: Instant) {
        let deadline = now + Duration::from_millis(self.manifest.timeout_ms.into());
        if !self.pending.query(query_id.clone(), query, limit, deadline) {
            self.emit(ExternalEvent::QueryFailed {
                provider_id: self.manifest.id.clone(),
                query_id,
            });
            return;
        }
        self.progress(now)
    }
    fn accept_activation(&mut self, activation_id: String, result_id: String, now: Instant) {
        let deadline = now + Duration::from_millis(self.manifest.timeout_ms.into());
        if !self
            .pending
            .activate(activation_id.clone(), result_id, deadline)
        {
            self.emit(ExternalEvent::ActivationFailed {
                provider_id: self.manifest.id.clone(),
                activation_id,
            });
            return;
        }
        self.progress(now)
    }
    fn start(&mut self, now: Instant) {
        if self.child.is_some() || !self.needed() || now < self.next_start {
            return;
        }
        let mut command = provider_command(&self.manifest);
        let Ok(mut child) = command.spawn() else {
            self.failed(now);
            return;
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            self.failed(now);
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let failed = Arc::new(AtomicBool::new(false));
        let reader = start_reader(
            stdout,
            self.reader_tx.clone(),
            Arc::clone(&failed),
            self.generation,
        );
        self.child = Some(Session {
            child,
            stdin,
            reader,
            failed,
            generation: self.generation,
            hello_deadline: now + Duration::from_millis(self.manifest.timeout_ms.into()),
            ready: false,
        });
        if self.write(&ProviderRequest::Hello).is_err() {
            self.failed(now)
        }
    }
    fn write_pending(&mut self, now: Instant) {
        if !self.child.as_ref().is_some_and(|child| child.ready) {
            return;
        }
        for request in self.pending.unsent() {
            match self.write(&request.request()) {
                Ok(()) => self.pending.sent(&request),
                Err(WriteError::Encode) => {
                    self.pending.remove(&request);
                    self.emit(request.failure(&self.manifest.id));
                }
                Err(WriteError::Io) => {
                    self.failed(now);
                    return;
                }
            }
        }
    }
    fn write(&mut self, request: &ProviderRequest) -> std::result::Result<(), WriteError> {
        let line = encode_provider_request_line(request).map_err(|_| WriteError::Encode)?;
        let Some(child) = self.child.as_mut() else {
            return Err(WriteError::Io);
        };
        child
            .stdin
            .write_all(&line)
            .and_then(|()| child.stdin.flush())
            .map_err(|_| WriteError::Io)
    }
    fn read(&mut self, now: Instant) {
        let mut corrupt = false;
        loop {
            match self.reader_events.try_recv() {
                Ok(event)
                    if self
                        .child
                        .as_ref()
                        .is_some_and(|child| child.generation == event.generation) =>
                {
                    match event.response {
                        ProviderResponse::Hello => {
                            if let Some(child) = self.child.as_mut() {
                                if child.ready {
                                    corrupt = true
                                } else {
                                    child.ready = true;
                                    self.backoff = INITIAL_BACKOFF
                                }
                            }
                        }
                        response => {
                            if !self.child.as_ref().is_some_and(|child| child.ready) {
                                corrupt = true
                            } else if let Some(mut external) =
                                route(&self.manifest.id, &mut self.pending, response)
                            {
                                prioritize(&mut external, self.manifest.priority);
                                self.backoff = INITIAL_BACKOFF;
                                self.emit(external)
                            } else {
                                corrupt = true
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
            if corrupt {
                break;
            }
        }
        if self.child.as_ref().is_some_and(|child| {
            child.failed.load(Ordering::Acquire) || (!child.ready && child.hello_deadline <= now)
        }) {
            corrupt = true;
        }
        if corrupt {
            self.failed(now)
        }
    }
    fn expire(&mut self, now: Instant) {
        for event in expire(&self.manifest.id, &mut self.pending, now) {
            self.emit(event)
        }
    }
    fn failed(&mut self, now: Instant) {
        self.kill();
        for event in fail_all(&self.manifest.id, &mut self.pending) {
            self.emit(event)
        }
        self.next_start = now + self.backoff;
        self.backoff = self.backoff.saturating_mul(2).min(MAX_BACKOFF)
    }
    fn kill(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        drop(child.stdin);
        let _ = child.child.kill();
        let _ = child.child.wait();
        let _ = child.reader.join();
    }
    fn emit(&self, event: ExternalEvent) {
        let _ = self.events.send(event);
    }
}
#[derive(Clone, Copy)]
enum WriteError {
    Encode,
    Io,
}
fn load_manifests(paths: &[PathBuf]) -> Result<Vec<ProviderManifest>> {
    let mut manifests = Vec::with_capacity(paths.len());
    let mut ids = BTreeMap::new();
    for path in paths {
        let content = fs::read(path)
            .with_context(|| format!("could not read provider manifest {}", path.display()))?;
        let manifest = parse_provider_manifest(&content)
            .with_context(|| format!("could not parse provider manifest {}", path.display()))?;
        if ids.insert(manifest.id.clone(), ()).is_some() {
            bail!(
                "provider manifests contain duplicate provider id {}",
                manifest.id
            )
        }
        manifests.push(manifest)
    }
    Ok(manifests)
}
fn provider_argv(manifest: &ProviderManifest) -> (&str, &[String]) {
    let (program, args) = manifest
        .command
        .split_first()
        .expect("parsed provider manifest has command program");
    (program, args)
}
fn provider_command(manifest: &ProviderManifest) -> Command {
    let (program, args) = provider_argv(manifest);
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}
fn start_reader(
    stdout: ChildStdout,
    events: SyncSender<ReaderEvent>,
    failed: Arc<AtomicBool>,
    generation: u64,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::with_capacity(1024);
        loop {
            match bounded_line(&mut reader, &mut line) {
                Ok(Line::Record) => {
                    line.pop();
                    let Ok(response) = parse_provider_response(&line) else {
                        failed.store(true, Ordering::Release);
                        return;
                    };
                    if events
                        .try_send(ReaderEvent {
                            generation,
                            response,
                        })
                        .is_err()
                    {
                        failed.store(true, Ordering::Release);
                        return;
                    }
                }
                Ok(Line::Eof | Line::Unterminated | Line::TooLong) | Err(_) => {
                    failed.store(true, Ordering::Release);
                    return;
                }
            }
        }
    })
}
#[derive(Clone, Copy)]
enum Line {
    Record,
    Eof,
    Unterminated,
    TooLong,
}
fn bounded_line(reader: &mut impl BufRead, line: &mut Vec<u8>) -> io::Result<Line> {
    line.clear();
    let max = MAX_RECORD_BYTES + 1;
    loop {
        let (consumed, done) = {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                return Ok(if line.is_empty() {
                    Line::Eof
                } else {
                    Line::Unterminated
                });
            }
            let count = buf.len().min(max.saturating_sub(line.len()));
            if let Some(newline) = buf[..count].iter().position(|byte| *byte == b'\n') {
                line.extend_from_slice(&buf[..=newline]);
                (newline + 1, true)
            } else {
                line.extend_from_slice(&buf[..count]);
                (count, false)
            }
        };
        reader.consume(consumed);
        if done {
            return Ok(Line::Record);
        }
        if line.len() == max {
            return Ok(Line::TooLong);
        }
    }
}
fn route(
    provider_id: &str,
    pending: &mut Pending,
    response: ProviderResponse,
) -> Option<ExternalEvent> {
    match response {
        ProviderResponse::Results {
            query_id,
            complete,
            results,
        } => {
            if !pending.queries.contains_key(&query_id) {
                return None;
            }
            if complete {
                pending.queries.remove(&query_id);
            }
            Some(ExternalEvent::Results {
                provider_id: provider_id.into(),
                query_id,
                complete,
                results,
            })
        }
        ProviderResponse::Activated { activation_id } => pending
            .activations
            .remove(&activation_id)
            .map(|_| ExternalEvent::Activated {
                provider_id: provider_id.into(),
                activation_id,
            }),
        ProviderResponse::Error(error) => {
            match error.correlation {
                ProviderErrorCorrelation::Query { query_id } => pending
                    .queries
                    .remove(&query_id)
                    .map(|_| ExternalEvent::QueryFailed {
                        provider_id: provider_id.into(),
                        query_id,
                    }),
                ProviderErrorCorrelation::Activation { activation_id } => pending
                    .activations
                    .remove(&activation_id)
                    .map(|_| ExternalEvent::ActivationFailed {
                        provider_id: provider_id.into(),
                        activation_id,
                    }),
            }
        }
        ProviderResponse::Hello => None,
    }
}
fn prioritize(event: &mut ExternalEvent, priority: u16) {
    let ExternalEvent::Results { results, .. } = event else {
        return;
    };
    let priority = f64::from(priority) / 1000.;
    for result in results {
        result.score = result.score * (1. - PRIORITY_WEIGHT) + priority * PRIORITY_WEIGHT
    }
}
fn expire(provider_id: &str, pending: &mut Pending, now: Instant) -> Vec<ExternalEvent> {
    let queries = pending
        .queries
        .iter()
        .filter_map(|(id, p)| (p.deadline <= now).then(|| id.clone()))
        .collect::<Vec<_>>();
    let activations = pending
        .activations
        .iter()
        .filter_map(|(id, p)| (p.deadline <= now).then(|| id.clone()))
        .collect::<Vec<_>>();
    let mut events = Vec::with_capacity(queries.len() + activations.len());
    for query_id in queries {
        pending.queries.remove(&query_id);
        events.push(ExternalEvent::QueryFailed {
            provider_id: provider_id.into(),
            query_id,
        });
    }
    for activation_id in activations {
        pending.activations.remove(&activation_id);
        events.push(ExternalEvent::ActivationFailed {
            provider_id: provider_id.into(),
            activation_id,
        });
    }
    events
}
fn fail_all(provider_id: &str, pending: &mut Pending) -> Vec<ExternalEvent> {
    let mut events = Vec::with_capacity(pending.len());
    for query_id in pending.queries.keys() {
        events.push(ExternalEvent::QueryFailed {
            provider_id: provider_id.into(),
            query_id: query_id.clone(),
        })
    }
    for activation_id in pending.activations.keys() {
        events.push(ExternalEvent::ActivationFailed {
            provider_id: provider_id.into(),
            activation_id: activation_id.clone(),
        })
    }
    pending.queries.clear();
    pending.activations.clear();
    events
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ProviderResponse, ProviderResult, ResultKind, parse_provider_manifest};
    fn manifest() -> ProviderManifest {
        parse_provider_manifest(br#"{"kind":"bingux.search-provider","protocolVersion":1,"id":"notes","displayName":"Notes","command":["/nix/store/provider/bin/notes","--serve"],"startup":"lazy","priority":100,"timeoutMs":250}"#).expect("valid manifest")
    }
    fn result() -> ProviderResult {
        ProviderResult {
            result_id: "note-1".into(),
            kind: ResultKind::Action,
            title: "First note".into(),
            subtitle: "Notebook".into(),
            icon: "note".into(),
            score: 0.8,
        }
    }
    #[test]
    fn manifest_loading_reports_errors() {
        assert!(
            load_manifests(&[PathBuf::from(
                "/definitely-missing/bingux-search-provider.json"
            )])
            .is_err()
        )
    }
    #[test]
    fn command_uses_argv_without_shell() {
        let manifest = manifest();
        let (program, args) = provider_argv(&manifest);
        assert_eq!(program, "/nix/store/provider/bin/notes");
        assert_eq!(args, ["--serve"])
    }
    #[test]
    fn responses_route_only_for_pending_ids() {
        let now = Instant::now();
        let mut pending = Pending::default();
        assert!(pending.query("query-1".into(), "notes".into(), 10, now));
        assert!(matches!(
            route(
                "notes",
                &mut pending,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: false,
                    results: vec![result()]
                }
            ),
            Some(ExternalEvent::Results {
                complete: false,
                ..
            })
        ));
        assert!(pending.queries.contains_key("query-1"));
        assert!(matches!(
            route(
                "notes",
                &mut pending,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: true,
                    results: vec![]
                }
            ),
            Some(ExternalEvent::Results { complete: true, .. })
        ));
        assert!(
            route(
                "notes",
                &mut pending,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: true,
                    results: vec![]
                }
            )
            .is_none()
        )
    }
    #[test]
    fn timeout_and_failure_clear_pending() {
        let now = Instant::now();
        let mut pending = Pending::default();
        assert!(pending.query("expired".into(), "notes".into(), 10, now));
        assert!(pending.activate("open".into(), "note-1".into(), now));
        assert_eq!(expire("notes", &mut pending, now).len(), 2);
        assert!(pending.empty());
        assert!(pending.query("live".into(), "notes".into(), 10, now));
        assert!(pending.activate("open-live".into(), "note-1".into(), now));
        assert_eq!(fail_all("notes", &mut pending).len(), 2);
        assert!(pending.empty())
    }
    #[test]
    fn priority_changes_result_ordering_score() {
        let mut event = ExternalEvent::Results {
            provider_id: "notes".into(),
            query_id: "query-1".into(),
            complete: true,
            results: vec![result()],
        };
        prioritize(&mut event, 1000);
        let ExternalEvent::Results { results, .. } = event else {
            panic!("results event")
        };
        assert!(results[0].score > 0.8 && results[0].score <= 1.)
    }
}
