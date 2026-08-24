use crate::protocol::{
    MAX_RECORD_BYTES, ProviderErrorCorrelation, ProviderManifest, ProviderRequest,
    ProviderResponse, ProviderStartup, encode_provider_request_line, parse_provider_manifest,
    parse_provider_response,
};
use anyhow::{Context, Result, bail};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
const COMMAND_CAPACITY: usize = 64;
const READER_CAPACITY: usize = 32;
const MAX_PENDING: usize = 64;
const MAX_PARTIAL_OUTBOX_EVENTS: usize = MAX_PENDING;
const MAX_TERMINAL_OUTBOX_EVENTS: usize = MAX_PENDING;
const MAX_READER_EVENTS_PER_PROGRESS: usize = READER_CAPACITY;
const INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_millis(50);
const PRIORITY_WEIGHT: f64 = 0.001;
const CANCELLATION_TTL: Duration = Duration::from_secs(11);
const MAX_CANCELLED_REQUESTS: usize = COMMAND_CAPACITY + MAX_PENDING;
const MAX_PROVIDER_MANIFEST_BYTES: u64 = 64 * 1024;

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

#[derive(Debug, Default)]
pub struct QueryDispatch {
    pub accepted: BTreeSet<String>,
    pub rejected: BTreeSet<String>,
}

#[derive(Default)]
struct CancelledRequests {
    queries: BTreeMap<String, Instant>,
    activations: BTreeMap<String, Instant>,
}
impl CancelledRequests {
    fn insert_query(&mut self, query_id: String, now: Instant) {
        self.prune(now);
        if !self.queries.contains_key(&query_id) {
            self.evict_if_full();
        }
        self.queries.insert(query_id, now + CANCELLATION_TTL);
    }

    fn insert_activation(&mut self, activation_id: String, now: Instant) {
        self.prune(now);
        if !self.activations.contains_key(&activation_id) {
            self.evict_if_full();
        }
        self.activations
            .insert(activation_id, now + CANCELLATION_TTL);
    }

    fn take_query(&mut self, query_id: &str, now: Instant) -> bool {
        self.prune(now);
        self.queries.remove(query_id).is_some()
    }

    fn take_activation(&mut self, activation_id: &str, now: Instant) -> bool {
        self.prune(now);
        self.activations.remove(activation_id).is_some()
    }

    fn evict_if_full(&mut self) {
        while self.queries.len() + self.activations.len() >= MAX_CANCELLED_REQUESTS {
            if self.queries.pop_first().is_none() {
                self.activations.pop_first();
            }
        }
    }

    fn prune(&mut self, now: Instant) {
        self.queries.retain(|_, expires_at| *expires_at > now);
        self.activations.retain(|_, expires_at| *expires_at > now);
    }
}

struct ProviderEndpoint {
    commands: SyncSender<WorkerCommand>,
    cancelled_requests: Arc<Mutex<CancelledRequests>>,
}
pub struct ExternalProviders {
    providers: BTreeMap<String, ProviderEndpoint>,
    stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}
impl ExternalProviders {
    pub fn start(manifest_paths: &[PathBuf], events: SyncSender<ExternalEvent>) -> Result<Self> {
        let manifests = load_manifests(manifest_paths)?;
        let stop = Arc::new(AtomicBool::new(false));
        let mut providers: BTreeMap<String, ProviderEndpoint> = BTreeMap::new();
        let mut workers: Vec<JoinHandle<()>> = Vec::with_capacity(manifests.len());
        for manifest in manifests {
            let id = manifest.id.clone();
            let (tx, rx) = mpsc::sync_channel(COMMAND_CAPACITY);
            let cancelled_requests = Arc::new(Mutex::new(CancelledRequests::default()));
            let worker_stop = Arc::clone(&stop);
            let worker_events = events.clone();
            let worker_cancellations = Arc::clone(&cancelled_requests);
            let name = format!("bingux-search-provider-{id}");
            let worker = thread::Builder::new().name(name).spawn(move || {
                Worker::new(
                    manifest,
                    rx,
                    worker_events,
                    worker_stop,
                    worker_cancellations,
                )
                .run()
            });
            let worker = match worker {
                Ok(worker) => worker,
                Err(error) => {
                    stop.store(true, Ordering::Release);
                    for provider in providers.values() {
                        let _ = provider.commands.try_send(WorkerCommand::Shutdown);
                    }
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(error).context("could not start external provider worker");
                }
            };
            providers.insert(
                id,
                ProviderEndpoint {
                    commands: tx,
                    cancelled_requests,
                },
            );
            workers.push(worker);
        }
        Ok(Self {
            providers,
            stop,
            workers,
        })
    }
    pub fn query(&self, query_id: String, query: String, limit: u8) -> QueryDispatch {
        let mut dispatch = QueryDispatch::default();
        for (provider_id, provider) in &self.providers {
            let command = WorkerCommand::Query {
                query_id: query_id.clone(),
                query: query.clone(),
                limit,
            };
            if provider.commands.try_send(command).is_ok() {
                dispatch.accepted.insert(provider_id.clone());
            } else {
                dispatch.rejected.insert(provider_id.clone());
            }
        }
        dispatch
    }

    pub fn cancel_query(&self, query_id: &str, provider_ids: &BTreeSet<String>) {
        for provider_id in provider_ids {
            self.cancel_query_for_provider(provider_id, query_id);
        }
    }

    pub fn cancel_activation(&self, provider_id: &str, activation_id: &str) {
        let Some(provider) = self.providers.get(provider_id) else {
            return;
        };
        if let Ok(mut cancelled) = provider.cancelled_requests.lock() {
            cancelled.insert_activation(activation_id.to_owned(), Instant::now());
        }
    }

    fn cancel_query_for_provider(&self, provider_id: &str, query_id: &str) {
        let Some(provider) = self.providers.get(provider_id) else {
            return;
        };
        if let Ok(mut cancelled) = provider.cancelled_requests.lock() {
            cancelled.insert_query(query_id.to_owned(), Instant::now());
        }
    }

    pub fn activate(&self, provider_id: &str, activation_id: String, result_id: String) -> bool {
        self.providers.get(provider_id).is_some_and(|provider| {
            provider
                .commands
                .try_send(WorkerCommand::Activate {
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
        for provider in self.providers.values() {
            let _ = provider.commands.try_send(WorkerCommand::Shutdown);
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
    result_count: usize,
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
                result_count: 0,
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
        requests.extend(
            self.queries
                .iter()
                .filter(|&(_, pending)| !pending.sent)
                .map(|(id, pending)| PendingRequest::Query {
                    query_id: id.clone(),
                    query: pending.query.clone(),
                    limit: pending.limit,
                }),
        );
        requests.extend(
            self.activations
                .iter()
                .filter(|&(_, pending)| !pending.sent)
                .map(|(id, pending)| PendingRequest::Activate {
                    activation_id: id.clone(),
                    result_id: pending.result_id.clone(),
                }),
        );
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

    fn cancel_query(&mut self, query_id: &str) -> bool {
        self.queries.remove(query_id).is_some()
    }

    fn cancel_activation(&mut self, activation_id: &str) -> bool {
        self.activations.remove(activation_id).is_some()
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
    writer_tx: SyncSender<Vec<u8>>,
    writer: JoinHandle<()>,
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

// Partial result batches can be shed under backpressure. The worker stops command acceptance
// while it drains this FIFO, so one terminal slot per pending request keeps completions bounded.
#[derive(Default)]
struct Outbox {
    events: VecDeque<ExternalEvent>,
    partial_count: usize,
    terminal_count: usize,
}

impl Outbox {
    fn enqueue(&mut self, event: ExternalEvent) -> bool {
        if is_terminal_event(&event) {
            if self.terminal_count >= MAX_TERMINAL_OUTBOX_EVENTS {
                return false;
            }
            self.terminal_count += 1;
        } else {
            if self.partial_count >= MAX_PARTIAL_OUTBOX_EVENTS {
                return false;
            }
            self.partial_count += 1;
        }
        self.events.push_back(event);
        true
    }

    fn pop_front(&mut self) -> Option<ExternalEvent> {
        let event = self.events.pop_front()?;
        if is_terminal_event(&event) {
            self.terminal_count -= 1;
        } else {
            self.partial_count -= 1;
        }
        Some(event)
    }

    fn push_front(&mut self, event: ExternalEvent) {
        if is_terminal_event(&event) {
            self.terminal_count += 1;
        } else {
            self.partial_count += 1;
        }
        self.events.push_front(event);
    }

    fn clear(&mut self) {
        self.events.clear();
        self.partial_count = 0;
        self.terminal_count = 0;
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

struct Worker {
    manifest: ProviderManifest,
    commands: Receiver<WorkerCommand>,
    events: SyncSender<ExternalEvent>,
    stop: Arc<AtomicBool>,
    cancelled_requests: Arc<Mutex<CancelledRequests>>,
    reader_events: Receiver<ReaderEvent>,
    reader_tx: SyncSender<ReaderEvent>,
    pending: Pending,
    outbox: Outbox,
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
        cancelled_requests: Arc<Mutex<CancelledRequests>>,
    ) -> Self {
        let (reader_tx, reader_events) = mpsc::sync_channel(READER_CAPACITY);
        Self {
            manifest,
            commands,
            events,
            stop,
            cancelled_requests,
            reader_events,
            reader_tx,
            pending: Pending::default(),
            outbox: Outbox::default(),
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
            if !self.outbox.is_empty() {
                thread::sleep(TICK);
                continue;
            }
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
        self.flush_events();
        self.cancel_pending_requests(now);
        self.expire(now);
        self.read(now);
        self.start(now);
        self.write_pending(now);
        self.flush_events();
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
        if self.take_query_cancellation(&query_id, now) {
            return;
        }
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

    fn take_query_cancellation(&self, query_id: &str, now: Instant) -> bool {
        self.cancelled_requests
            .lock()
            .ok()
            .is_some_and(|mut cancelled| cancelled.take_query(query_id, now))
    }

    fn take_activation_cancellation(&self, activation_id: &str, now: Instant) -> bool {
        self.cancelled_requests
            .lock()
            .ok()
            .is_some_and(|mut cancelled| cancelled.take_activation(activation_id, now))
    }

    fn cancel_pending_requests(&mut self, now: Instant) {
        let Ok(mut cancelled) = self.cancelled_requests.lock() else {
            return;
        };
        let cancelled_queries = self
            .pending
            .queries
            .keys()
            .filter(|query_id| cancelled.take_query(query_id, now))
            .cloned()
            .collect::<Vec<_>>();
        let cancelled_activations = self
            .pending
            .activations
            .keys()
            .filter(|activation_id| cancelled.take_activation(activation_id, now))
            .cloned()
            .collect::<Vec<_>>();
        for query_id in cancelled_queries {
            self.pending.cancel_query(&query_id);
        }
        for activation_id in cancelled_activations {
            self.pending.cancel_activation(&activation_id);
        }
    }

    fn accept_activation(&mut self, activation_id: String, result_id: String, now: Instant) {
        if self.take_activation_cancellation(&activation_id, now) {
            return;
        }
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
            stop_process_group(&mut child);
            self.failed(now);
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let failed = Arc::new(AtomicBool::new(false));
        let (writer_tx, writer_rx) = mpsc::sync_channel(COMMAND_CAPACITY);
        let Ok(writer) = start_writer(stdin, writer_rx, Arc::clone(&failed)) else {
            stop_process_group(&mut child);
            self.failed(now);
            return;
        };
        let Ok(reader) = start_reader(
            stdout,
            self.reader_tx.clone(),
            Arc::clone(&failed),
            self.generation,
        ) else {
            drop(writer_tx);
            stop_process_group(&mut child);
            drop(writer);
            self.failed(now);
            return;
        };
        self.child = Some(Session {
            child,
            writer_tx,
            writer,
            reader,
            failed,
            generation: self.generation,
            hello_deadline: now + Duration::from_millis(self.manifest.timeout_ms.into()),
            ready: false,
        });
        if self.write(&ProviderRequest::Hello).is_err() {
            self.failed(now);
        }
    }
    fn write_pending(&mut self, now: Instant) {
        if !self.child.as_ref().is_some_and(|child| child.ready) {
            return;
        }
        for request in self.pending.unsent() {
            match self.write(&request.request()) {
                Ok(()) => self.pending.sent(&request),
                Err(WriteError::Full) => return,
                Err(WriteError::Encode) => {
                    self.pending.remove(&request);
                    self.emit(request.failure(&self.manifest.id));
                }
                Err(WriteError::Disconnected) => {
                    self.failed(now);
                    return;
                }
            }
        }
    }

    fn write(&mut self, request: &ProviderRequest) -> std::result::Result<(), WriteError> {
        let line = encode_provider_request_line(request).map_err(|_| WriteError::Encode)?;
        let Some(child) = self.child.as_ref() else {
            return Err(WriteError::Disconnected);
        };
        child.writer_tx.try_send(line).map_err(|error| match error {
            TrySendError::Full(_) => WriteError::Full,
            TrySendError::Disconnected(_) => WriteError::Disconnected,
        })
    }
    fn read(&mut self, now: Instant) {
        let mut corrupt = false;
        for _ in 0..MAX_READER_EVENTS_PER_PROGRESS {
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
                                    corrupt = true;
                                } else {
                                    child.ready = true;
                                    self.backoff = INITIAL_BACKOFF;
                                }
                            }
                        }
                        response => {
                            if !self.child.as_ref().is_some_and(|child| child.ready) {
                                corrupt = true;
                            } else {
                                match route(&self.manifest.id, &mut self.pending, now, response) {
                                    Route::Event(mut external) => {
                                        prioritize(&mut external, self.manifest.priority);
                                        self.backoff = INITIAL_BACKOFF;
                                        self.emit(external);
                                    }
                                    Route::Ignored => {}
                                    Route::Corrupt => corrupt = true,
                                }
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
            self.failed(now);
        }
    }

    fn expire(&mut self, now: Instant) {
        let expired = expire(&self.manifest.id, &mut self.pending, now);
        if expired.is_empty() {
            return;
        }
        for event in expired {
            self.emit(event);
        }
        self.failed(now);
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
        drop(child.writer_tx);
        stop_process_group(&mut child.child);
        drop(child.reader);
        drop(child.writer);
    }
    fn emit(&mut self, event: ExternalEvent) {
        if self.outbox.enqueue(event) {
            self.flush_events();
        }
    }

    fn flush_events(&mut self) {
        while let Some(event) = self.outbox.pop_front() {
            match self.events.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Full(event)) => {
                    self.outbox.push_front(event);
                    return;
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.outbox.clear();
                    return;
                }
            }
        }
    }
}

fn is_terminal_event(event: &ExternalEvent) -> bool {
    match event {
        ExternalEvent::Results { complete, .. } => *complete,
        ExternalEvent::QueryFailed { .. }
        | ExternalEvent::Activated { .. }
        | ExternalEvent::ActivationFailed { .. } => true,
    }
}

#[derive(Clone, Copy)]
enum WriteError {
    Encode,
    Full,
    Disconnected,
}
fn read_bounded_file(path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut content = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut content)?;
    if content.len() as u64 > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file exceeds configured size limit",
        ));
    }
    Ok(content)
}

fn load_manifests(paths: &[PathBuf]) -> Result<Vec<ProviderManifest>> {
    let mut manifests = Vec::with_capacity(paths.len());
    let mut ids = BTreeMap::new();
    for path in paths {
        let content = read_bounded_file(path, MAX_PROVIDER_MANIFEST_BYTES)
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
        .stderr(Stdio::null())
        .process_group(0);
    command
}

fn stop_process_group(child: &mut Child) {
    let Ok(process_group) = i32::try_from(child.id()) else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    };
    // SAFETY: process_group is the positive PID of a child started in its own process group.
    let _ = unsafe { libc::kill(-process_group, libc::SIGKILL) };
    let _ = child.kill();
    let _ = child.wait();
}

fn start_writer(
    mut stdin: ChildStdin,
    records: Receiver<Vec<u8>>,
    failed: Arc<AtomicBool>,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("bingux-search-provider-writer".into())
        .spawn(move || {
            for record in records {
                if stdin
                    .write_all(&record)
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    failed.store(true, Ordering::Release);
                    return;
                }
            }
        })
}

fn start_reader(
    stdout: ChildStdout,
    events: SyncSender<ReaderEvent>,
    failed: Arc<AtomicBool>,
    generation: u64,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("bingux-search-provider-reader".into())
        .spawn(move || {
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
                            .send(ReaderEvent {
                                generation,
                                response,
                            })
                            .is_err()
                        {
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
enum Route {
    Event(ExternalEvent),
    Ignored,
    Corrupt,
}

fn route(
    provider_id: &str,
    pending: &mut Pending,
    now: Instant,
    response: ProviderResponse,
) -> Route {
    match response {
        ProviderResponse::Results {
            query_id,
            complete,
            mut results,
        } => {
            let Some(query) = pending.queries.get_mut(&query_id) else {
                return Route::Ignored;
            };
            if !query.sent {
                return Route::Corrupt;
            }
            if query.deadline <= now {
                return Route::Ignored;
            }
            let remaining = usize::from(query.limit).saturating_sub(query.result_count);
            results.truncate(remaining);
            query.result_count += results.len();
            if complete {
                pending.queries.remove(&query_id);
            }
            if results.is_empty() && !complete {
                Route::Ignored
            } else {
                Route::Event(ExternalEvent::Results {
                    provider_id: provider_id.into(),
                    query_id,
                    complete,
                    results,
                })
            }
        }
        ProviderResponse::Activated { activation_id } => {
            let Some(activation) = pending.activations.get(&activation_id) else {
                return Route::Ignored;
            };
            if !activation.sent {
                return Route::Corrupt;
            }
            if activation.deadline <= now {
                return Route::Ignored;
            }
            pending.activations.remove(&activation_id);
            Route::Event(ExternalEvent::Activated {
                provider_id: provider_id.into(),
                activation_id,
            })
        }
        ProviderResponse::Error(error) => match error.correlation {
            ProviderErrorCorrelation::Query { query_id } => {
                let Some(query) = pending.queries.get(&query_id) else {
                    return Route::Ignored;
                };
                if !query.sent {
                    return Route::Corrupt;
                }
                if query.deadline <= now {
                    return Route::Ignored;
                }
                pending.queries.remove(&query_id);
                Route::Event(ExternalEvent::QueryFailed {
                    provider_id: provider_id.into(),
                    query_id,
                })
            }
            ProviderErrorCorrelation::Activation { activation_id } => {
                let Some(activation) = pending.activations.get(&activation_id) else {
                    return Route::Ignored;
                };
                if !activation.sent {
                    return Route::Corrupt;
                }
                if activation.deadline <= now {
                    return Route::Ignored;
                }
                pending.activations.remove(&activation_id);
                Route::Event(ExternalEvent::ActivationFailed {
                    provider_id: provider_id.into(),
                    activation_id,
                })
            }
        },
        ProviderResponse::Hello => Route::Corrupt,
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
        .filter(|&(_, pending)| pending.deadline <= now)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let activations = pending
        .activations
        .iter()
        .filter(|&(_, pending)| pending.deadline <= now)
        .map(|(id, _)| id.clone())
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
    fn responses_route_only_for_sent_pending_ids() {
        let now = Instant::now();
        let mut pending = Pending::default();
        assert!(pending.query(
            "query-1".into(),
            "notes".into(),
            10,
            now + Duration::from_secs(1),
        ));
        let request = PendingRequest::Query {
            query_id: "query-1".into(),
            query: "notes".into(),
            limit: 10,
        };
        pending.sent(&request);
        assert!(matches!(
            route(
                "notes",
                &mut pending,
                now,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: false,
                    results: vec![result()]
                }
            ),
            Route::Event(ExternalEvent::Results {
                complete: false,
                ..
            })
        ));
        assert!(pending.queries.contains_key("query-1"));
        assert!(matches!(
            route(
                "notes",
                &mut pending,
                now,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: true,
                    results: vec![]
                }
            ),
            Route::Event(ExternalEvent::Results { complete: true, .. })
        ));
        assert!(matches!(
            route(
                "notes",
                &mut pending,
                now,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: true,
                    results: vec![]
                }
            ),
            Route::Ignored
        ));
    }

    #[test]
    fn rejects_responses_for_unsent_requests() {
        let now = Instant::now();
        let mut pending = Pending::default();
        assert!(pending.query("query-1".into(), "notes".into(), 10, now));

        assert!(matches!(
            route(
                "notes",
                &mut pending,
                now,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: false,
                    results: vec![result()]
                }
            ),
            Route::Corrupt
        ));
    }

    #[test]
    fn caps_results_across_partial_provider_records() {
        let now = Instant::now();
        let mut pending = Pending::default();
        assert!(pending.query(
            "query-1".into(),
            "notes".into(),
            1,
            now + Duration::from_secs(1),
        ));
        pending.sent(&PendingRequest::Query {
            query_id: "query-1".into(),
            query: "notes".into(),
            limit: 1,
        });

        let Route::Event(ExternalEvent::Results { results, .. }) = route(
            "notes",
            &mut pending,
            now,
            ProviderResponse::Results {
                query_id: "query-1".into(),
                complete: false,
                results: vec![result(), result()],
            },
        ) else {
            panic!("first result batch must route");
        };
        assert_eq!(results.len(), 1);
        assert!(matches!(
            route(
                "notes",
                &mut pending,
                now,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: false,
                    results: vec![result()]
                }
            ),
            Route::Ignored
        ));
        assert!(matches!(
            route(
                "notes",
                &mut pending,
                now,
                ProviderResponse::Results {
                    query_id: "query-1".into(),
                    complete: true,
                    results: vec![result()]
                }
            ),
            Route::Event(ExternalEvent::Results {
                complete: true,
                results,
                ..
            }) if results.is_empty()
        ));
    }

    #[test]
    fn cancellation_removes_unfinished_queries_and_activations() {
        let now = Instant::now();
        let mut pending = Pending::default();
        assert!(pending.query("query-1".into(), "notes".into(), 10, now));
        assert!(pending.activate("activation-1".into(), "note-1".into(), now));

        assert!(pending.cancel_query("query-1"));
        assert!(pending.cancel_activation("activation-1"));
        assert!(pending.unsent().is_empty());
    }

    #[test]
    fn cancellation_markers_are_typed_expire_and_stay_bounded() {
        let now = Instant::now();
        let mut cancelled = CancelledRequests::default();

        cancelled.insert_query("expired".into(), now);
        assert!(!cancelled.take_query("expired", now + CANCELLATION_TTL));

        cancelled.insert_query("shared-id".into(), now);
        cancelled.insert_activation("shared-id".into(), now);
        assert!(cancelled.take_query("shared-id", now));
        assert!(cancelled.take_activation("shared-id", now));

        for index in 0..=MAX_CANCELLED_REQUESTS {
            cancelled.insert_query(format!("query-{index}"), now);
        }
        assert_eq!(
            cancelled.queries.len() + cancelled.activations.len(),
            MAX_CANCELLED_REQUESTS
        );
    }

    #[test]
    fn cancellation_drops_queued_and_pending_activations() {
        let now = Instant::now();
        let cancellations = Arc::new(Mutex::new(CancelledRequests::default()));
        let (_commands, command_receiver) = mpsc::sync_channel(1);
        let (events, event_receiver) = mpsc::sync_channel(1);
        let mut worker = Worker::new(
            manifest(),
            command_receiver,
            events,
            Arc::new(AtomicBool::new(false)),
            Arc::clone(&cancellations),
        );

        cancellations
            .lock()
            .expect("cancellation lock")
            .insert_activation("queued".into(), now);
        worker.accept_activation("queued".into(), "note-1".into(), now);
        assert!(worker.pending.empty());

        assert!(worker.pending.activate(
            "pending".into(),
            "note-1".into(),
            now + Duration::from_secs(1)
        ));
        cancellations
            .lock()
            .expect("cancellation lock")
            .insert_activation("pending".into(), now);
        worker.cancel_pending_requests(now);
        assert!(worker.pending.empty());
        assert!(event_receiver.try_recv().is_err());
    }

    #[test]
    fn query_reports_a_full_provider_command_queue() {
        let (commands, _receiver) = mpsc::sync_channel(0);
        let external = ExternalProviders {
            providers: BTreeMap::from([(
                "notes".into(),
                ProviderEndpoint {
                    commands,
                    cancelled_requests: Arc::new(Mutex::new(CancelledRequests::default())),
                },
            )]),
            stop: Arc::new(AtomicBool::new(false)),
            workers: Vec::new(),
        };

        let dispatch = external.query("query-1".into(), "notes".into(), 10);

        assert!(dispatch.accepted.is_empty());
        assert_eq!(dispatch.rejected, BTreeSet::from(["notes".into()]));
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

    #[test]
    fn outbox_reserves_terminal_space_when_partial_queue_is_full() {
        let mut outbox = Outbox::default();
        for index in 0..MAX_PARTIAL_OUTBOX_EVENTS {
            assert!(outbox.enqueue(ExternalEvent::Results {
                provider_id: "notes".into(),
                query_id: format!("query-{index}"),
                complete: false,
                results: vec![result()],
            }));
        }

        assert!(outbox.enqueue(ExternalEvent::Results {
            provider_id: "notes".into(),
            query_id: "query-complete".into(),
            complete: true,
            results: vec![],
        }));

        assert_eq!(outbox.partial_count, MAX_PARTIAL_OUTBOX_EVENTS);
        assert!(outbox.events.iter().any(|event| {
            matches!(
                event,
                ExternalEvent::Results {
                    query_id,
                    complete: true,
                    ..
                } if query_id == "query-complete"
            )
        }));
    }

    #[test]
    fn outbox_uses_fixed_capacity_queues() {
        let mut outbox = Outbox::default();
        for index in 0..MAX_PARTIAL_OUTBOX_EVENTS {
            assert!(outbox.enqueue(ExternalEvent::Results {
                provider_id: "notes".into(),
                query_id: format!("partial-{index}"),
                complete: false,
                results: vec![result()],
            }));
        }
        assert!(!outbox.enqueue(ExternalEvent::Results {
            provider_id: "notes".into(),
            query_id: "partial-overflow".into(),
            complete: false,
            results: vec![result()],
        }));

        for index in 0..MAX_TERMINAL_OUTBOX_EVENTS {
            assert!(outbox.enqueue(ExternalEvent::QueryFailed {
                provider_id: "notes".into(),
                query_id: format!("terminal-{index}"),
            }));
        }
        assert!(!outbox.enqueue(ExternalEvent::QueryFailed {
            provider_id: "notes".into(),
            query_id: "terminal-overflow".into(),
        }));

        assert_eq!(outbox.partial_count, MAX_PARTIAL_OUTBOX_EVENTS);
        assert_eq!(outbox.terminal_count, MAX_TERMINAL_OUTBOX_EVENTS);
    }

    #[test]
    fn outbox_keeps_partial_and_terminal_events_in_arrival_order() {
        let mut outbox = Outbox::default();
        assert!(outbox.enqueue(ExternalEvent::Results {
            provider_id: "notes".into(),
            query_id: "first".into(),
            complete: false,
            results: vec![result()],
        }));
        assert!(outbox.enqueue(ExternalEvent::QueryFailed {
            provider_id: "notes".into(),
            query_id: "second".into(),
        }));
        assert!(outbox.enqueue(ExternalEvent::Results {
            provider_id: "notes".into(),
            query_id: "third".into(),
            complete: false,
            results: vec![result()],
        }));

        assert!(matches!(
            outbox.pop_front(),
            Some(ExternalEvent::Results { query_id, .. }) if query_id == "first"
        ));
        assert!(matches!(
            outbox.pop_front(),
            Some(ExternalEvent::QueryFailed { query_id, .. }) if query_id == "second"
        ));
        assert!(matches!(
            outbox.pop_front(),
            Some(ExternalEvent::Results { query_id, .. }) if query_id == "third"
        ));
        assert!(outbox.is_empty());
    }
}
