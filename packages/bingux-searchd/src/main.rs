use anyhow::{Context, Result, bail};
use bingux_searchd::{
    config::{SearchCommands, SearchConfig},
    external::{ExternalEvent, ExternalProviders},
    gnoblin::{self, Event as GnoblinEvent},
    protocol::{
        ActivateRequest, DaemonErrorCode, DaemonEvent, DaemonResult, IntegrationState, ProtocolError,
        ProtocolErrorKind, QueryRequest, ShellRequest, encode_daemon_event_lines, parse_shell_request,
        shell_request_id,
    },
    providers::{Activation, Candidate, LocalProviders},
    server::{bind_listener, read_record},
    weather::WeatherProvider,
};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    env,
    ffi::OsStr,
    io::{BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

const CLIENT_QUEUE_CAPACITY: usize = 64;
const EXTERNAL_EVENT_QUEUE_CAPACITY: usize = 256;
const GNOBLIN_EVENT_QUEUE_CAPACITY: usize = 8;
const MAX_CLIENTS: usize = 16;
const QUERY_WORKER_COUNT: usize = 2;
const QUERY_QUEUE_CAPACITY: usize = 64;
const MAX_ACTIVATIONS: usize = 512;
const ACTIVATION_TTL: Duration = Duration::from_secs(120);
const PROTOCOL_ERROR_REQUEST_ID: &str = "protocol-error";

fn main() {
    if let Err(error) = run() {
        eprintln!("[bingux-searchd] {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let config_path = config_path_from_arguments()?;
    let config = SearchConfig::load(&config_path)?;
    let socket_path = search_socket_path()?;
    let local = Arc::new(LocalProviders::new(&config)?);
    let weather = WeatherProvider::start(config.weather.clone());
    let (external_sender, external_receiver) = mpsc::sync_channel(EXTERNAL_EVENT_QUEUE_CAPACITY);
    let external = Arc::new(ExternalProviders::start(
        &config.provider_manifest_paths,
        external_sender,
    )?);
    let runtime = Arc::new(Runtime::new(local, weather, external, config.commands));
    runtime.start_query_workers()?;

    start_external_event_dispatcher(Arc::clone(&runtime), external_receiver);
    start_gnoblin_bridge(Arc::clone(&runtime));

    let listener = bind_listener(&socket_path)
        .with_context(|| format!("could not bind search socket {}", socket_path.display()))?;
    eprintln!("[bingux-searchd] listening on {}", socket_path.display());
    accept_clients(listener, runtime)
}

fn config_path_from_arguments() -> Result<PathBuf> {
    let mut arguments = env::args_os().skip(1);
    let Some(flag) = arguments.next() else {
        bail!("usage: bingux-searchd --config <absolute-path>");
    };
    if flag != OsStr::new("--config") {
        bail!("usage: bingux-searchd --config <absolute-path>");
    }
    let Some(path) = arguments.next() else {
        bail!("usage: bingux-searchd --config <absolute-path>");
    };
    if arguments.next().is_some() {
        bail!("usage: bingux-searchd --config <absolute-path>");
    }

    let path = PathBuf::from(path);
    if !path.is_absolute() {
        bail!("search configuration path must be absolute");
    }
    Ok(path)
}

fn search_socket_path() -> Result<PathBuf> {
    let runtime_directory = env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is not set")?;
    let runtime_directory = PathBuf::from(runtime_directory);
    if !runtime_directory.is_absolute() {
        bail!("XDG_RUNTIME_DIR must be absolute");
    }
    Ok(runtime_directory.join("bingux/search-v1.sock"))
}

struct Runtime {
    local: Arc<LocalProviders>,
    weather: Option<WeatherProvider>,
    external: Arc<ExternalProviders>,
    query_sender: SyncSender<QueryJob>,
    query_receiver: Arc<Mutex<Receiver<QueryJob>>>,
    commands: SearchCommands,
    clients: Mutex<HashMap<u64, SyncSender<DaemonEvent>>>,
    queries: Mutex<HashMap<String, Arc<QueryTracker>>>,
    client_queries: Mutex<HashMap<u64, String>>,
    activations: Mutex<ActivationRegistry>,
    external_activations: Mutex<HashMap<String, ActivationRoute>>,
    gnoblin_ready: AtomicBool,
    next_client_id: AtomicU64,
    next_query_id: AtomicU64,
    next_result_id: AtomicU64,
    next_activation_id: AtomicU64,
}

impl Runtime {
    fn new(
        local: Arc<LocalProviders>,
        weather: Option<WeatherProvider>,
        external: Arc<ExternalProviders>,
        commands: SearchCommands,
    ) -> Self {
        let (query_sender, query_receiver) = mpsc::sync_channel(QUERY_QUEUE_CAPACITY);
        Self {
            local,
            weather,
            external,
            query_sender,
            query_receiver: Arc::new(Mutex::new(query_receiver)),
            commands,
            clients: Mutex::new(HashMap::new()),
            queries: Mutex::new(HashMap::new()),
            client_queries: Mutex::new(HashMap::new()),
            activations: Mutex::new(ActivationRegistry::default()),
            external_activations: Mutex::new(HashMap::new()),
            gnoblin_ready: AtomicBool::new(false),
            next_client_id: AtomicU64::new(1),
            next_query_id: AtomicU64::new(1),
            next_result_id: AtomicU64::new(1),
            next_activation_id: AtomicU64::new(1),
        }
    }

    fn start_query_workers(self: &Arc<Self>) -> Result<()> {
        for worker_index in 0..QUERY_WORKER_COUNT {
            let runtime = Arc::clone(self);
            let receiver = Arc::clone(&self.query_receiver);
            thread::Builder::new()
                .name(format!("bingux-search-query-{worker_index}"))
                .spawn(move || {
                    loop {
                        let job = match receiver.lock() {
                            Ok(receiver) => match receiver.recv() {
                                Ok(job) => job,
                                Err(_) => return,
                            },
                            Err(_) => return,
                        };
                        runtime.run_query(job);
                    }
                })?;
        }
        Ok(())
    }
    fn next_client_id(&self) -> u64 {
        self.next_client_id.fetch_add(1, Ordering::Relaxed)
    }

    fn register_client(&self, client_id: u64, sender: SyncSender<DaemonEvent>) -> bool {
        let Ok(mut clients) = self.clients.lock() else {
            return false;
        };
        if clients.len() >= MAX_CLIENTS {
            return false;
        }
        clients.insert(client_id, sender);
        true
    }

    fn disconnect_client(&self, client_id: u64) {
        if let Ok(mut clients) = self.clients.lock() {
            clients.remove(&client_id);
        }
        self.cancel_client_query(client_id);
        if let Ok(mut activations) = self.activations.lock() {
            activations.remove_client(client_id);
        }
        if let Ok(mut routes) = self.external_activations.lock() {
            routes.retain(|_, route| route.client_id != client_id);
        }
    }

    fn broadcast(&self, event: DaemonEvent) {
        let disconnected = {
            let Ok(clients) = self.clients.lock() else {
                return;
            };
            clients
                .iter()
                .filter_map(|(client_id, sender)| {
                    sender
                        .try_send(event.clone())
                        .is_err()
                        .then_some(*client_id)
                })
                .collect::<Vec<_>>()
        };
        for client_id in disconnected {
            self.disconnect_client(client_id);
        }
    }

    fn enqueue_event(
        &self,
        client_id: u64,
        sender: &SyncSender<DaemonEvent>,
        event: DaemonEvent,
    ) -> bool {
        match sender.try_send(event) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.disconnect_client(client_id);
                false
            }
        }
    }

    fn cancel_client_query(&self, client_id: u64) {
        let previous_query = self
            .client_queries
            .lock()
            .ok()
            .and_then(|mut client_queries| client_queries.remove(&client_id));
        if let Some(query_id) = previous_query {
            self.cancel_query(&query_id, client_id);
        }
    }

    fn cancel_query(&self, query_id: &str, client_id: u64) {
        let tracker = self
            .queries
            .lock()
            .ok()
            .and_then(|mut queries| queries.remove(query_id));
        if let Some(tracker) = tracker {
            self.external
                .cancel_query(query_id, &tracker.provider_ids());
        }
        if let Ok(mut activations) = self.activations.lock() {
            activations.remove_query(client_id, query_id);
        }
    }

    fn enqueue_query_event(
        &self,
        client_id: u64,
        query_id: &str,
        sender: &SyncSender<DaemonEvent>,
        event: DaemonEvent,
    ) -> bool {
        let result = match self.client_queries.lock() {
            Ok(client_queries) => {
                if client_queries
                    .get(&client_id)
                    .is_none_or(|current| current != query_id)
                {
                    return false;
                }
                sender.try_send(event)
            }
            Err(_) => return false,
        };
        match result {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.disconnect_client(client_id);
                false
            }
        }
    }

    fn start_query(
        self: &Arc<Self>,
        client_id: u64,
        generation: &Arc<AtomicU64>,
        sender: SyncSender<DaemonEvent>,
        request: QueryRequest,
    ) {
        let current_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.cancel_client_query(client_id);

        let provider_query_id = format!(
            "p{}-{}",
            client_id,
            self.next_query_id.fetch_add(1, Ordering::Relaxed)
        );
        let tracker = Arc::new(QueryTracker::new(
            client_id,
            request.request_id.clone(),
            usize::from(request.limit),
            sender.clone(),
        ));
        if let Ok(mut queries) = self.queries.lock() {
            queries.insert(provider_query_id.clone(), Arc::clone(&tracker));
        }
        if let Ok(mut client_queries) = self.client_queries.lock() {
            client_queries.insert(client_id, provider_query_id.clone());
        }

        let request_id = request.request_id.clone();
        let job = QueryJob {
            client_id,
            current_generation,
            generation: Arc::clone(generation),
            sender: sender.clone(),
            request,
            provider_query_id: provider_query_id.clone(),
            tracker,
        };
        if self.query_sender.try_send(job).is_err() {
            self.remove_query(&provider_query_id, client_id);
            let _ = self.enqueue_event(
                client_id,
                &sender,
                DaemonEvent::Error {
                    request_id,
                    code: DaemonErrorCode::Unavailable,
                },
            );
        }
    }

    fn run_query(&self, job: QueryJob) {
        let QueryJob {
            client_id,
            current_generation,
            generation,
            sender,
            request,
            provider_query_id,
            tracker,
        } = job;
        if !self.query_is_active(client_id, &provider_query_id)
            || generation.load(Ordering::Acquire) != current_generation
        {
            return;
        }

        let accepted_external = self.external.query(
            provider_query_id.clone(),
            request.query.clone(),
            request.limit,
        );
        tracker.set_provider_ids(accepted_external.clone());
        let mut candidates = self.local.query(&request.query, usize::from(request.limit));
        if let Some(weather) = &self.weather {
            if let Some(result) = weather.query(&request.query) {
                candidates.push(Candidate {
                    provider_id: "weather".to_owned(),
                    result,
                    activation: Activation::None,
                });
            }
        }
        rank_candidates(&mut candidates, usize::from(request.limit));
        if !self.query_is_active(client_id, &provider_query_id)
            || generation.load(Ordering::Acquire) != current_generation
        {
            self.cancel_query(&provider_query_id, client_id);
            return;
        }

        let result_limit = tracker.reserve_result_slots(candidates.len());
        let results =
            self.register_candidates(client_id, &provider_query_id, candidates, result_limit);
        if !self.enqueue_query_event(
            client_id,
            &provider_query_id,
            &sender,
            DaemonEvent::Results {
                request_id: request.request_id.clone(),
                complete: accepted_external.is_empty(),
                elapsed_usec: elapsed_usec(tracker.started),
                results,
            },
        ) {
            self.remove_query(&provider_query_id, client_id);
            return;
        }

        if accepted_external.is_empty() {
            self.remove_query(&provider_query_id, client_id);
            return;
        }
        self.initialise_external_query(tracker);
    }

    fn initialise_external_query(&self, tracker: Arc<QueryTracker>) {
        for event in tracker.initialise() {
            self.route_external_event(event);
        }
    }

    fn remove_query(&self, query_id: &str, client_id: u64) {
        if let Ok(mut queries) = self.queries.lock() {
            queries.remove(query_id);
        }
        if let Ok(mut client_queries) = self.client_queries.lock() {
            if client_queries
                .get(&client_id)
                .is_some_and(|current| current == query_id)
            {
                client_queries.remove(&client_id);
            }
        }
    }

    fn query_is_active(&self, client_id: u64, query_id: &str) -> bool {
        self.client_queries
            .lock()
            .ok()
            .is_some_and(|client_queries| {
                client_queries
                    .get(&client_id)
                    .is_some_and(|current| current == query_id)
            })
    }

    fn route_external_event(&self, event: ExternalEvent) {
        match event {
            ExternalEvent::Results {
                provider_id,
                query_id,
                complete,
                results,
            } => {
                let Some(tracker) = self.query_tracker(&query_id) else {
                    return;
                };
                let query_id_for_removal = query_id.clone();
                let Some(query_complete) = tracker.accept_or_buffer(ExternalEvent::Results {
                    provider_id: provider_id.clone(),
                    query_id,
                    complete,
                    results: results.clone(),
                }) else {
                    return;
                };
                let candidates: Vec<Candidate> = results
                    .into_iter()
                    .filter(|result| result.validate().is_ok())
                    .map(|result| Candidate {
                        activation: Activation::External {
                            provider_id: provider_id.clone(),
                            result_id: result.result_id.clone(),
                        },
                        provider_id: provider_id.clone(),
                        result,
                    })
                    .collect();
                let result_limit = tracker.reserve_result_slots(candidates.len());
                let daemon_results = self.register_candidates(
                    tracker.client_id,
                    &query_id_for_removal,
                    candidates,
                    result_limit,
                );
                if !self.enqueue_query_event(
                    tracker.client_id,
                    &query_id_for_removal,
                    &tracker.sender,
                    DaemonEvent::Results {
                        request_id: tracker.request_id.clone(),
                        complete: query_complete,
                        elapsed_usec: elapsed_usec(tracker.started),
                        results: daemon_results,
                    },
                ) {
                    return;
                }
                if query_complete {
                    self.remove_query(&query_id_for_removal, tracker.client_id);
                }
            }
            ExternalEvent::QueryFailed {
                provider_id,
                query_id,
            } => {
                let Some(tracker) = self.query_tracker(&query_id) else {
                    return;
                };
                let Some(query_complete) = tracker.accept_or_buffer(ExternalEvent::QueryFailed {
                    provider_id,
                    query_id: query_id.clone(),
                }) else {
                    return;
                };
                if !self.enqueue_query_event(
                    tracker.client_id,
                    &query_id,
                    &tracker.sender,
                    DaemonEvent::Error {
                        request_id: tracker.request_id.clone(),
                        code: DaemonErrorCode::ProviderFailed,
                    },
                ) {
                    return;
                }
                if query_complete {
                    let _ = self.enqueue_query_event(
                        tracker.client_id,
                        &query_id,
                        &tracker.sender,
                        DaemonEvent::Results {
                            request_id: tracker.request_id.clone(),
                            complete: true,
                            elapsed_usec: elapsed_usec(tracker.started),
                            results: Vec::new(),
                        },
                    );
                    self.remove_query(&query_id, tracker.client_id);
                }
            }
            ExternalEvent::Activated {
                provider_id,
                activation_id,
            } => {
                self.finish_external_activation(&provider_id, &activation_id, true);
            }
            ExternalEvent::ActivationFailed {
                provider_id,
                activation_id,
            } => {
                self.finish_external_activation(&provider_id, &activation_id, false);
            }
        }
    }

    fn query_tracker(&self, query_id: &str) -> Option<Arc<QueryTracker>> {
        self.queries.lock().ok()?.get(query_id).cloned()
    }

    fn register_candidates(
        &self,
        client_id: u64,
        query_id: &str,
        candidates: Vec<Candidate>,
        limit: usize,
    ) -> Vec<DaemonResult> {
        let mut results = Vec::with_capacity(candidates.len().min(limit));
        let Ok(mut activations) = self.activations.lock() else {
            return results;
        };
        activations.remove_expired();

        for candidate in candidates.into_iter().take(limit) {
            if candidate.result.validate().is_err() {
                continue;
            }
            let result_id = format!("r{}", self.next_result_id.fetch_add(1, Ordering::Relaxed));
            let result = DaemonResult {
                result_id: result_id.clone(),
                provider_id: candidate.provider_id,
                kind: candidate.result.kind,
                title: candidate.result.title,
                subtitle: candidate.result.subtitle,
                icon: candidate.result.icon,
                score: candidate.result.score,
            };
            if result.validate().is_err() {
                continue;
            }
            activations.insert(result_id, client_id, query_id, candidate.activation);
            results.push(result);
        }
        results
    }

    fn start_activation(
        &self,
        client_id: u64,
        sender: SyncSender<DaemonEvent>,
        request: ActivateRequest,
    ) {
        let activation = self
            .activations
            .lock()
            .ok()
            .and_then(|mut activations| activations.take(&request.result_id, client_id));
        let Some(activation) = activation else {
            let _ = self.enqueue_event(
                client_id,
                &sender,
                DaemonEvent::Error {
                    request_id: request.request_id,
                    code: DaemonErrorCode::UnknownResult,
                },
            );
            return;
        };
        self.cancel_client_query(client_id);
        self.run_activation(client_id, sender, request, activation);
    }

    fn run_activation(
        &self,
        client_id: u64,
        sender: SyncSender<DaemonEvent>,
        request: ActivateRequest,
        activation: Activation,
    ) {
        match activation {
            Activation::Spawn { program, arguments } => {
                let event = if launch_program(&program, &arguments).is_ok() {
                    DaemonEvent::Activated {
                        request_id: request.request_id,
                    }
                } else {
                    DaemonEvent::Error {
                        request_id: request.request_id,
                        code: DaemonErrorCode::ProviderFailed,
                    }
                };
                let _ = self.enqueue_event(client_id, &sender, event);
            }
            Activation::Copy { text } => {
                let event = if copy_to_clipboard(&self.commands.clipboard, &text).is_ok() {
                    DaemonEvent::Activated {
                        request_id: request.request_id,
                    }
                } else {
                    DaemonEvent::Error {
                        request_id: request.request_id,
                        code: DaemonErrorCode::ProviderFailed,
                    }
                };
                let _ = self.enqueue_event(client_id, &sender, event);
            }
            Activation::External {
                provider_id,
                result_id,
            } => {
                let external_id = format!(
                    "a{}",
                    self.next_activation_id.fetch_add(1, Ordering::Relaxed)
                );
                let route = ActivationRoute {
                    client_id,
                    provider_id: provider_id.clone(),
                    request_id: request.request_id.clone(),
                    sender: sender.clone(),
                };
                let route_inserted = match self.external_activations.lock() {
                    Ok(mut routes) => {
                        routes.insert(external_id.clone(), route);
                        true
                    }
                    Err(_) => false,
                };
                if !route_inserted
                    || !self
                        .external
                        .activate(&provider_id, external_id.clone(), result_id)
                {
                    if let Ok(mut routes) = self.external_activations.lock() {
                        routes.remove(&external_id);
                    }
                    let _ = self.enqueue_event(
                        client_id,
                        &sender,
                        DaemonEvent::Error {
                            request_id: request.request_id,
                            code: DaemonErrorCode::ProviderFailed,
                        },
                    );
                }
            }
            Activation::None => {
                let _ = self.enqueue_event(
                    client_id,
                    &sender,
                    DaemonEvent::Activated {
                        request_id: request.request_id,
                    },
                );
            }
        }
    }

    fn finish_external_activation(&self, provider_id: &str, activation_id: &str, succeeded: bool) {
        let route = self
            .external_activations
            .lock()
            .ok()
            .and_then(|mut routes| {
                if routes
                    .get(activation_id)
                    .is_some_and(|route| route.provider_id == provider_id)
                {
                    routes.remove(activation_id)
                } else {
                    None
                }
            });
        let Some(route) = route else {
            return;
        };
        let event = if succeeded {
            DaemonEvent::Activated {
                request_id: route.request_id,
            }
        } else {
            DaemonEvent::Error {
                request_id: route.request_id,
                code: DaemonErrorCode::ProviderFailed,
            }
        };
        let _ = self.enqueue_event(route.client_id, &route.sender, event);
    }
}

struct QueryJob {
    client_id: u64,
    current_generation: u64,
    generation: Arc<AtomicU64>,
    sender: SyncSender<DaemonEvent>,
    request: QueryRequest,
    provider_query_id: String,
    tracker: Arc<QueryTracker>,
}

struct QueryTracker {
    client_id: u64,
    request_id: String,
    limit: usize,
    sender: SyncSender<DaemonEvent>,
    started: Instant,
    state: Mutex<QueryTrackerState>,
}

struct QueryTrackerState {
    provider_ids: BTreeSet<String>,
    expected: Option<usize>,
    completed_providers: BTreeSet<String>,
    reserved_results: usize,
    buffered_events: Vec<ExternalEvent>,
}

impl QueryTracker {
    fn new(
        client_id: u64,
        request_id: String,
        limit: usize,
        sender: SyncSender<DaemonEvent>,
    ) -> Self {
        Self {
            client_id,
            request_id,
            limit,
            sender,
            started: Instant::now(),
            state: Mutex::new(QueryTrackerState {
                provider_ids: BTreeSet::new(),
                expected: None,
                completed_providers: BTreeSet::new(),
                reserved_results: 0,
                buffered_events: Vec::new(),
            }),
        }
    }

    fn set_provider_ids(&self, provider_ids: BTreeSet<String>) {
        if let Ok(mut state) = self.state.lock() {
            state.provider_ids = provider_ids;
        }
    }

    fn provider_ids(&self) -> BTreeSet<String> {
        self.state
            .lock()
            .map(|state| state.provider_ids.clone())
            .unwrap_or_default()
    }

    fn reserve_result_slots(&self, requested: usize) -> usize {
        let Ok(mut state) = self.state.lock() else {
            return 0;
        };
        let granted = requested.min(self.limit.saturating_sub(state.reserved_results));
        state.reserved_results += granted;
        granted
    }

    fn initialise(&self) -> Vec<ExternalEvent> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        state.expected = Some(state.provider_ids.len());
        std::mem::take(&mut state.buffered_events)
    }

    fn accept_or_buffer(&self, event: ExternalEvent) -> Option<bool> {
        let provider_id = match &event {
            ExternalEvent::Results { provider_id, .. }
            | ExternalEvent::QueryFailed { provider_id, .. } => provider_id,
            ExternalEvent::Activated { .. } | ExternalEvent::ActivationFailed { .. } => {
                return None;
            }
        };
        let complete = match &event {
            ExternalEvent::Results { complete, .. } => *complete,
            ExternalEvent::QueryFailed { .. } => true,
            ExternalEvent::Activated { .. } | ExternalEvent::ActivationFailed { .. } => false,
        };
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        if state.expected.is_none() {
            state.buffered_events.push(event);
            return None;
        }
        if !state.provider_ids.contains(provider_id) {
            return None;
        }
        if complete {
            state.completed_providers.insert(provider_id.clone());
        }
        Some(
            state
                .expected
                .is_some_and(|expected| state.completed_providers.len() >= expected),
        )
    }
}

#[derive(Default)]
struct ActivationRegistry {
    entries: HashMap<String, RegisteredActivation>,
    order: VecDeque<String>,
}

struct RegisteredActivation {
    client_id: u64,
    query_id: String,
    activation: Activation,
    expires_at: Instant,
}

impl ActivationRegistry {
    fn insert(
        &mut self,
        result_id: String,
        client_id: u64,
        query_id: &str,
        activation: Activation,
    ) {
        self.remove_expired();
        while self.entries.len() >= MAX_ACTIVATIONS {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
        self.order.push_back(result_id.clone());
        self.entries.insert(
            result_id,
            RegisteredActivation {
                client_id,
                query_id: query_id.to_owned(),
                activation,
                expires_at: Instant::now() + ACTIVATION_TTL,
            },
        );
    }

    fn take(&mut self, result_id: &str, client_id: u64) -> Option<Activation> {
        self.remove_expired();
        if self.entries.get(result_id)?.client_id != client_id {
            return None;
        }
        self.entries.remove(result_id).map(|entry| entry.activation)
    }

    fn remove_query(&mut self, client_id: u64, query_id: &str) {
        self.entries
            .retain(|_, entry| entry.client_id != client_id || entry.query_id.as_str() != query_id);
        self.order
            .retain(|result_id| self.entries.contains_key(result_id));
    }

    fn remove_expired(&mut self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| entry.expires_at > now);
        self.order
            .retain(|result_id| self.entries.contains_key(result_id));
    }

    fn remove_client(&mut self, client_id: u64) {
        self.entries.retain(|_, entry| entry.client_id != client_id);
        self.order
            .retain(|result_id| self.entries.contains_key(result_id));
    }
}

struct ActivationRoute {
    client_id: u64,
    provider_id: String,
    request_id: String,
    sender: SyncSender<DaemonEvent>,
}

fn start_external_event_dispatcher(runtime: Arc<Runtime>, receiver: Receiver<ExternalEvent>) {
    thread::spawn(move || {
        for event in receiver {
            runtime.route_external_event(event);
        }
    });
}

fn start_gnoblin_bridge(runtime: Arc<Runtime>) {
    let (sender, receiver) = mpsc::sync_channel(GNOBLIN_EVENT_QUEUE_CAPACITY);
    gnoblin::start_super_release_subscriber(sender);
    thread::spawn(move || {
        for event in receiver {
            match event {
                GnoblinEvent::Ready => {
                    runtime.gnoblin_ready.store(true, Ordering::Release);
                    runtime.broadcast(DaemonEvent::IntegrationState {
                        state: IntegrationState::Ready,
                    });
                }
                GnoblinEvent::Unavailable => {
                    runtime.gnoblin_ready.store(false, Ordering::Release);
                    runtime.broadcast(DaemonEvent::IntegrationState {
                        state: IntegrationState::Unavailable,
                    });
                }
                GnoblinEvent::SuperReleased { monotonic_usec } => {
                    runtime.broadcast(DaemonEvent::ShowSearch {
                        monotonic_usec: monotonic_usec.to_string(),
                    })
                }
            }
        }
    });
}

fn accept_clients(listener: UnixListener, runtime: Arc<Runtime>) -> Result<()> {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let client_id = runtime.next_client_id();
                let runtime = Arc::clone(&runtime);
                thread::Builder::new()
                    .name(format!("bingux-search-client-{client_id}"))
                    .spawn(move || handle_client(runtime, client_id, stream))?;
            }
            Err(error) => {
                eprintln!("[bingux-searchd] search socket accept failed: {error}");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

fn client_rejection_event(record: &[u8], error: ProtocolError) -> DaemonEvent {
    let code = match error.kind() {
        ProtocolErrorKind::UnsupportedProtocol => DaemonErrorCode::UnsupportedProtocol,
        _ => DaemonErrorCode::InvalidRequest,
    };
    let request_id =
        shell_request_id(record).unwrap_or_else(|| PROTOCOL_ERROR_REQUEST_ID.to_owned());

    DaemonEvent::Error { request_id, code }
}

fn handle_client(runtime: Arc<Runtime>, client_id: u64, stream: UnixStream) {
    let Ok(writer_stream) = stream.try_clone() else {
        return;
    };
    let (sender, receiver) = mpsc::sync_channel(CLIENT_QUEUE_CAPACITY);
    if !runtime.register_client(client_id, sender.clone()) {
        return;
    }
    let initial_state = if runtime.gnoblin_ready.load(Ordering::Acquire) {
        IntegrationState::Ready
    } else {
        IntegrationState::Unavailable
    };
    if sender
        .try_send(DaemonEvent::IntegrationState {
            state: initial_state,
        })
        .is_err()
    {
        runtime.disconnect_client(client_id);
        return;
    }

    thread::spawn(move || write_client_events(writer_stream, receiver));
    let generation = Arc::new(AtomicU64::new(0));
    let mut reader = BufReader::new(stream);
    loop {
        let record = match read_record(&mut reader) {
            Ok(Some(record)) => record,
            Ok(None) => break,
            Err(error) => {
                eprintln!("[bingux-searchd] rejected client record: {error}");
                runtime.enqueue_event(
                    client_id,
                    &sender,
                    DaemonEvent::Error {
                        request_id: PROTOCOL_ERROR_REQUEST_ID.to_owned(),
                        code: DaemonErrorCode::InvalidRequest,
                    },
                );
                break;
            }
        };
        match parse_shell_request(&record) {
            Ok(ShellRequest::Query(request)) => {
                runtime.start_query(client_id, &generation, sender.clone(), request);
            }
            Ok(ShellRequest::Activate(request)) => {
                runtime.start_activation(client_id, sender.clone(), request);
            }
            Err(error) => {
                eprintln!("[bingux-searchd] rejected client record: {error}");
                if !runtime.enqueue_event(
                    client_id,
                    &sender,
                    client_rejection_event(&record, error),
                ) {
                    break;
                }
            }
        }
    }
    runtime.disconnect_client(client_id);
}

fn write_client_events(stream: UnixStream, receiver: Receiver<DaemonEvent>) {
    let mut writer = std::io::BufWriter::new(stream);
    for event in receiver {
        let records = match encode_daemon_event_lines(&event) {
            Ok(records) => records,
            Err(error) => {
                eprintln!("[bingux-searchd] could not encode daemon event: {error}");
                return;
            }
        };
        for record in records {
            if writer
                .write_all(&record)
                .and_then(|()| writer.flush())
                .is_err()
            {
                return;
            }
        }
    }
}

fn launch_program(program: &str, arguments: &[String]) -> std::io::Result<()> {
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn copy_to_clipboard(command: &[String], text: &str) -> std::io::Result<()> {
    let Some((program, arguments)) = command.split_first() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "clipboard command is empty",
        ));
    };
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let Some(mut stdin) = child.stdin.take() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "clipboard process did not expose standard input",
        ));
    };
    stdin.write_all(text.as_bytes())?;
    drop(stdin);
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("clipboard process failed"))
    }
}

fn elapsed_usec(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn rank_candidates(candidates: &mut Vec<Candidate>, limit: usize) {
    candidates.sort_by(|left, right| {
        right
            .result
            .score
            .partial_cmp(&left.result.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.provider_id.cmp(&right.provider_id))
            .then_with(|| left.result.result_id.cmp(&right.result.result_id))
    });
    candidates.truncate(limit);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_cancellation_removes_only_that_queries_activations() {
        let mut activations = ActivationRegistry::default();
        activations.insert("old".into(), 1, "q-old", Activation::None);
        activations.insert("current".into(), 1, "q-current", Activation::None);

        activations.remove_query(1, "q-old");

        assert!(activations.take("old", 1).is_none());
        assert!(matches!(
            activations.take("current", 1),
            Some(Activation::None)
        ));
    }

    #[test]
    fn rejected_client_record_returns_a_correlated_invalid_request_error() {
        let record =
            br#"{"protocolVersion":1,"type":"query","requestId":"q-01","query":123,"limit":20}"#;
        let error = parse_shell_request(record).expect_err("query must be text");

        assert_eq!(
            client_rejection_event(record, error),
            DaemonEvent::Error {
                request_id: "q-01".into(),
                code: DaemonErrorCode::InvalidRequest,
            }
        );
    }

    #[test]
    fn malformed_client_record_uses_the_protocol_error_request_id() {
        let record = b"{invalid json";
        let error = parse_shell_request(record).expect_err("record is not JSON");

        assert_eq!(
            client_rejection_event(record, error),
            DaemonEvent::Error {
                request_id: PROTOCOL_ERROR_REQUEST_ID.into(),
                code: DaemonErrorCode::InvalidRequest,
            }
        );
    }


    #[test]
    fn unsupported_client_protocol_returns_the_compatibility_error_code() {
        let record =
            br#"{"protocolVersion":2,"type":"query","requestId":"q-01","query":"firefox","limit":20}"#;
        let error = parse_shell_request(record).expect_err("protocol version two is unsupported");

        assert_eq!(
            client_rejection_event(record, error),
            DaemonEvent::Error {
                request_id: "q-01".into(),
                code: DaemonErrorCode::UnsupportedProtocol,
            }
        );
    }
    #[test]
    fn query_tracker_reserves_the_result_limit_across_batches() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let tracker = QueryTracker::new(1, "q-01".into(), 3, sender);

        assert_eq!(tracker.reserve_result_slots(2), 2);
        assert_eq!(tracker.reserve_result_slots(2), 1);
        assert_eq!(tracker.reserve_result_slots(1), 0);
    }
}
