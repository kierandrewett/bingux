use anyhow::{Context, Result, bail};
use bingux_searchd::{
    ai::{AiProvider, ChatHistory},
    config::{SearchCommands, SearchConfig},
    external::{ExternalEvent, ExternalProviders},
    gnoblin::{self, Event as GnoblinEvent},
    protocol::{
        ActivateRequest, DaemonErrorCode, DaemonEvent, DaemonResult, IntegrationState,
        ProtocolError, ProtocolErrorKind, QueryRequest, ShellRequest, encode_daemon_event_lines,
        parse_shell_request, shell_request_id,
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
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError},
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
const MAX_CONCURRENT_CHAT_REQUESTS: usize = 4;
const CHAT_WORKER_COUNT: usize = MAX_CONCURRENT_CHAT_REQUESTS;
const CHAT_QUEUE_CAPACITY: usize = 16;
const ACTIVATION_TTL: Duration = Duration::from_secs(120);
const PROTOCOL_ERROR_REQUEST_ID: &str = "protocol-error";
const MAX_BUFFERED_EXTERNAL_EVENTS: usize = 64;

const MAX_REAPED_PROGRAMS: usize = 128;
const PROGRAM_REAPER_POLL_INTERVAL: Duration = Duration::from_millis(100);
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
    let ai = config.ai.clone().map(AiProvider::new).transpose()?;
    let weather = WeatherProvider::start(config.weather.clone());
    let (external_sender, external_receiver) = mpsc::sync_channel(EXTERNAL_EVENT_QUEUE_CAPACITY);
    let external = Arc::new(ExternalProviders::start(
        &config.provider_manifest_paths,
        external_sender,
    )?);
    let runtime = Arc::new(Runtime::new(local, weather, external, config.commands, ai)?);
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

struct ChatWork {
    runtime: Arc<Runtime>,
    ai: AiProvider,
    history: ChatHistory,
    prompt: String,
    activation_id: String,
}

struct Runtime {
    local: Arc<LocalProviders>,
    weather: Option<WeatherProvider>,
    external: Arc<ExternalProviders>,
    query_sender: SyncSender<QueryJob>,
    query_receiver: Arc<Mutex<Receiver<QueryJob>>>,
    chat_sender: SyncSender<ChatWork>,
    chat_receiver: Arc<Mutex<Receiver<ChatWork>>>,
    commands: SearchCommands,
    program_reaper: ProgramReaper,
    ai: Option<AiProvider>,
    chat_work_limiter: ChatWorkLimiter,
    active_clients: AtomicUsize,
    clients: Mutex<HashMap<u64, SyncSender<DaemonEvent>>>,
    queries: Mutex<HashMap<String, Arc<QueryTracker>>>,
    client_queries: Mutex<HashMap<u64, String>>,
    activations: Mutex<ActivationRegistry>,
    external_activations: Mutex<HashMap<String, ActivationRoute>>,
    chat_activations: Mutex<HashMap<String, ChatActivationRoute>>,
    chat_histories: Mutex<HashMap<u64, ChatHistory>>,
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
        ai: Option<AiProvider>,
    ) -> Result<Self> {
        let (query_sender, query_receiver) = mpsc::sync_channel(QUERY_QUEUE_CAPACITY);
        let (chat_sender, chat_receiver) = mpsc::sync_channel(CHAT_QUEUE_CAPACITY);
        Ok(Self {
            local,
            weather,
            external,
            query_sender,
            query_receiver: Arc::new(Mutex::new(query_receiver)),
            chat_sender,
            chat_receiver: Arc::new(Mutex::new(chat_receiver)),
            commands,
            program_reaper: ProgramReaper::new()?,
            ai,
            chat_work_limiter: ChatWorkLimiter::new(),
            active_clients: AtomicUsize::new(0),
            clients: Mutex::new(HashMap::new()),
            queries: Mutex::new(HashMap::new()),
            client_queries: Mutex::new(HashMap::new()),
            activations: Mutex::new(ActivationRegistry::default()),
            external_activations: Mutex::new(HashMap::new()),
            chat_activations: Mutex::new(HashMap::new()),
            chat_histories: Mutex::new(HashMap::new()),
            gnoblin_ready: AtomicBool::new(false),
            next_client_id: AtomicU64::new(1),
            next_query_id: AtomicU64::new(1),
            next_result_id: AtomicU64::new(1),
            next_activation_id: AtomicU64::new(1),
        })
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
        for worker_index in 0..CHAT_WORKER_COUNT {
            let receiver = Arc::clone(&self.chat_receiver);
            thread::Builder::new()
                .name(format!("bingux-search-chat-{worker_index}"))
                .spawn(move || {
                    loop {
                        let work = match receiver.lock() {
                            Ok(receiver) => match receiver.recv() {
                                Ok(work) => work,
                                Err(_) => return,
                            },
                            Err(_) => return,
                        };
                        if !work.runtime.chat_activation_is_active(&work.activation_id) {
                            continue;
                        }
                        let completion = work.ai.complete(&work.history, &work.prompt);
                        work.runtime.finish_chat_activation(
                            &work.activation_id,
                            work.prompt,
                            completion,
                        );
                    }
                })?;
        }
        Ok(())
    }
    fn next_client_id(&self) -> u64 {
        self.next_client_id.fetch_add(1, Ordering::Relaxed)
    }

    fn try_reserve_client_slot(&self) -> bool {
        self.active_clients
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_CLIENTS).then_some(active + 1)
            })
            .is_ok()
    }
    fn release_client_slot(&self) {
        // The socket admission path reserves before registration. Direct test
        // callers may register without a reservation, so release is saturating.
        let _ = self
            .active_clients
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                active.checked_sub(1)
            });
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
        let removed = self
            .clients
            .lock()
            .map(|mut clients| clients.remove(&client_id).is_some())
            .unwrap_or(false);
        if removed {
            self.release_client_slot();
        }
        self.cancel_client_query(client_id);
        if let Ok(mut activations) = self.activations.lock() {
            activations.remove_client(client_id);
        }
        self.cancel_matching_external_activations(|route| route.client_id == client_id);
        if let Ok(mut routes) = self.chat_activations.lock() {
            cancel_matching_chat_activations(&mut routes, |route| route.client_id == client_id);
        }
        if let Ok(mut histories) = self.chat_histories.lock() {
            histories.remove(&client_id);
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

    fn cancel_matching_external_activations(&self, matches: impl Fn(&ActivationRoute) -> bool) {
        let cancelled = self
            .external_activations
            .lock()
            .map(|mut routes| take_matching_external_activation_routes(&mut routes, matches))
            .unwrap_or_default();
        for (provider_id, activation_id) in cancelled {
            self.external
                .cancel_activation(&provider_id, &activation_id);
        }
    }

    fn cancel_request(&self, client_id: u64, request_id: &str) {
        let active_query = self
            .client_queries
            .lock()
            .ok()
            .and_then(|client_queries| client_queries.get(&client_id).cloned());
        let active_query_matches = active_query.is_some_and(|query_id| {
            self.queries
                .lock()
                .ok()
                .and_then(|queries| queries.get(&query_id).cloned())
                .is_some_and(|tracker| tracker.request_id == request_id)
        });
        if active_query_matches {
            self.cancel_client_query(client_id);
        }
        self.cancel_matching_external_activations(|route| {
            route.client_id == client_id && route.request_id == request_id
        });
        if let Ok(mut routes) = self.chat_activations.lock() {
            cancel_matching_chat_activations(&mut routes, |route| {
                route.client_id == client_id && route.request_id == request_id
            });
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
            self.remove_query(&provider_query_id, client_id);
            return;
        }

        let dispatch = self.external.query(
            provider_query_id.clone(),
            request.query.clone(),
            request.limit,
        );
        tracker.configure_providers(dispatch.accepted.clone());

        if !self.query_is_active(client_id, &provider_query_id)
            || generation.load(Ordering::Acquire) != current_generation
        {
            self.external
                .cancel_query(&provider_query_id, &dispatch.accepted);
            self.remove_query(&provider_query_id, client_id);
            return;
        }

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
            self.remove_query(&provider_query_id, client_id);
            return;
        }

        let result_limit = tracker.reserve_result_slots(candidates.len());
        if !self.query_is_active(client_id, &provider_query_id)
            || generation.load(Ordering::Acquire) != current_generation
        {
            self.remove_query(&provider_query_id, client_id);
            return;
        }
        let results =
            self.register_candidates(client_id, &provider_query_id, candidates, result_limit);
        if !self.query_is_active(client_id, &provider_query_id)
            || generation.load(Ordering::Acquire) != current_generation
        {
            self.remove_query(&provider_query_id, client_id);
            return;
        }
        if !self.enqueue_query_event(
            client_id,
            &provider_query_id,
            &sender,
            DaemonEvent::Results {
                request_id: request.request_id.clone(),
                complete: dispatch.accepted.is_empty(),
                elapsed_usec: elapsed_usec(tracker.started),
                results,
            },
        ) {
            self.remove_query(&provider_query_id, client_id);
            return;
        }

        if !dispatch.rejected.is_empty()
            && !self.enqueue_query_event(
                client_id,
                &provider_query_id,
                &sender,
                DaemonEvent::Error {
                    request_id: request.request_id.clone(),
                    code: DaemonErrorCode::ProviderFailed,
                },
            )
        {
            self.remove_query(&provider_query_id, client_id);
            return;
        }

        if dispatch.accepted.is_empty() {
            self.remove_query(&provider_query_id, client_id);
            return;
        }
        self.release_external_query(tracker);
    }

    fn release_external_query(&self, tracker: Arc<QueryTracker>) {
        let mut events = tracker.release_external();
        loop {
            for event in events {
                self.route_external_event(event, true);
            }
            let Some(next_events) = tracker.finish_external_replay() else {
                return;
            };
            events = next_events;
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
        if let Ok(mut activations) = self.activations.lock() {
            activations.remove_query(client_id, query_id);
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

    fn route_external_event(&self, event: ExternalEvent, replaying: bool) {
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
                if !self.query_is_active(tracker.client_id, &query_id) {
                    return;
                }
                let query_id_for_removal = query_id.clone();
                let query_complete = tracker.accept_external_event(
                    ExternalEvent::Results {
                        provider_id: provider_id.clone(),
                        query_id,
                        complete,
                        results: results.clone(),
                    },
                    replaying,
                );
                let Some(query_complete) = query_complete else {
                    return;
                };
                if !self.query_is_active(tracker.client_id, &query_id_for_removal) {
                    return;
                }
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
                if !self.query_is_active(tracker.client_id, &query_id_for_removal) {
                    self.remove_query(&query_id_for_removal, tracker.client_id);
                    return;
                }
                let daemon_results = self.register_candidates(
                    tracker.client_id,
                    &query_id_for_removal,
                    candidates,
                    result_limit,
                );
                if !self.query_is_active(tracker.client_id, &query_id_for_removal) {
                    self.remove_query(&query_id_for_removal, tracker.client_id);
                    return;
                }
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
                    self.remove_query(&query_id_for_removal, tracker.client_id);
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
                if !self.query_is_active(tracker.client_id, &query_id) {
                    return;
                }
                let query_complete = tracker.accept_external_event(
                    ExternalEvent::QueryFailed {
                        provider_id,
                        query_id: query_id.clone(),
                    },
                    replaying,
                );
                let Some(query_complete) = query_complete else {
                    return;
                };
                if !self.query_is_active(tracker.client_id, &query_id) {
                    self.remove_query(&query_id, tracker.client_id);
                    return;
                }
                if !self.enqueue_query_event(
                    tracker.client_id,
                    &query_id,
                    &tracker.sender,
                    DaemonEvent::Error {
                        request_id: tracker.request_id.clone(),
                        code: DaemonErrorCode::ProviderFailed,
                    },
                ) {
                    self.remove_query(&query_id, tracker.client_id);
                    return;
                }
                if query_complete {
                    if !self.query_is_active(tracker.client_id, &query_id) {
                        self.remove_query(&query_id, tracker.client_id);
                        return;
                    }
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
            if !self.query_is_active(client_id, query_id) {
                break;
            }
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
        self: &Arc<Self>,
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
        self: &Arc<Self>,
        client_id: u64,
        sender: SyncSender<DaemonEvent>,
        request: ActivateRequest,
        activation: Activation,
    ) {
        match activation {
            Activation::Spawn { program, arguments } => {
                let event = if launch_program(&program, &arguments, &self.program_reaper).is_ok() {
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
            Activation::Chat { prompt } => {
                self.start_chat_activation(client_id, sender, request, prompt);
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

    fn start_chat_activation(
        self: &Arc<Self>,
        client_id: u64,
        sender: SyncSender<DaemonEvent>,
        request: ActivateRequest,
        prompt: String,
    ) {
        let Some(ai) = self.ai.clone() else {
            let _ = self.enqueue_event(
                client_id,
                &sender,
                DaemonEvent::Error {
                    request_id: request.request_id,
                    code: DaemonErrorCode::ProviderFailed,
                },
            );
            return;
        };
        let history = match self.chat_histories.lock() {
            Ok(histories) => histories.get(&client_id).cloned().unwrap_or_default(),
            Err(_) => {
                let _ = self.enqueue_event(
                    client_id,
                    &sender,
                    DaemonEvent::Error {
                        request_id: request.request_id,
                        code: DaemonErrorCode::ProviderFailed,
                    },
                );
                return;
            }
        };
        let chat_activation_id = format!(
            "chat-{}",
            self.next_activation_id.fetch_add(1, Ordering::Relaxed)
        );
        let request_id = request.request_id;
        let Some(reservation) = self.chat_work_limiter.reserve() else {
            let _ = self.enqueue_event(
                client_id,
                &sender,
                DaemonEvent::Error {
                    request_id,
                    code: DaemonErrorCode::Unavailable,
                },
            );
            return;
        };
        let route = ChatActivationRoute {
            client_id,
            request_id: request_id.clone(),
            sender: sender.clone(),
            _reservation: Some(reservation),
        };
        let admission = match self.chat_activations.lock() {
            Ok(mut routes) => {
                if can_admit_chat_activation(&routes, client_id) {
                    routes.insert(chat_activation_id.clone(), route);
                    ChatAdmission::Admitted
                } else {
                    ChatAdmission::Unavailable
                }
            }
            Err(_) => ChatAdmission::Failed,
        };
        if !matches!(admission, ChatAdmission::Admitted) {
            let code = match admission {
                ChatAdmission::Unavailable => DaemonErrorCode::Unavailable,
                ChatAdmission::Failed => DaemonErrorCode::ProviderFailed,
                ChatAdmission::Admitted => unreachable!("admitted chat activation returned early"),
            };
            let _ = self.enqueue_event(client_id, &sender, DaemonEvent::Error { request_id, code });
            return;
        }
        if !self.client_is_live(client_id) {
            self.abandon_chat_activation(&chat_activation_id);
            return;
        }

        let work = ChatWork {
            runtime: Arc::clone(self),
            ai,
            history,
            prompt,
            activation_id: chat_activation_id.clone(),
        };
        match self.chat_sender.try_send(work) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.abandon_chat_activation(&chat_activation_id);
                let _ = self.enqueue_event(
                    client_id,
                    &sender,
                    DaemonEvent::Error {
                        request_id,
                        code: DaemonErrorCode::Unavailable,
                    },
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                self.abandon_chat_activation(&chat_activation_id);
                let _ = self.enqueue_event(
                    client_id,
                    &sender,
                    DaemonEvent::Error {
                        request_id,
                        code: DaemonErrorCode::ProviderFailed,
                    },
                );
            }
        }
    }

    fn finish_chat_activation(
        &self,
        chat_activation_id: &str,
        prompt: String,
        completion: Result<String>,
    ) {
        let (client_id, sent, history) = {
            let Ok(mut routes) = self.chat_activations.lock() else {
                return;
            };
            let Some(route) = routes.get(chat_activation_id) else {
                return;
            };

            let (event, history) = match completion {
                Ok(message) => {
                    let message: Arc<str> = message.into();
                    (
                        DaemonEvent::ChatResponse {
                            request_id: route.request_id.clone(),
                            message: Arc::clone(&message),
                        },
                        Some((prompt, message)),
                    )
                }
                Err(_) => (
                    DaemonEvent::Error {
                        request_id: route.request_id.clone(),
                        code: DaemonErrorCode::ProviderFailed,
                    },
                    None,
                ),
            };
            let sent = route.sender.try_send(event).is_ok();
            let client_id = route.client_id;
            routes.remove(chat_activation_id);
            (client_id, sent, history)
        };

        if sent {
            if let Some((prompt, message)) = history {
                if let Ok(mut histories) = self.chat_histories.lock() {
                    histories
                        .entry(client_id)
                        .or_default()
                        .record(prompt, message);
                }
            }
        } else {
            self.disconnect_client(client_id);
        }
    }

    fn abandon_chat_activation(&self, chat_activation_id: &str) {
        if let Ok(mut routes) = self.chat_activations.lock() {
            routes.remove(chat_activation_id);
        }
    }

    fn chat_activation_is_active(&self, chat_activation_id: &str) -> bool {
        self.chat_activations
            .lock()
            .map(|routes| routes.contains_key(chat_activation_id))
            .unwrap_or(false)
    }

    fn client_is_live(&self, client_id: u64) -> bool {
        self.clients
            .lock()
            .map(|clients| clients.contains_key(&client_id))
            .unwrap_or(false)
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
    external_released: bool,
    // Keeps buffered events ordered before events that arrive during release.
    external_replay_pending: bool,
    completed_providers: BTreeSet<String>,
    reserved_results: usize,
    buffered_events: VecDeque<ExternalEvent>,
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
                external_released: false,
                external_replay_pending: false,
                completed_providers: BTreeSet::new(),
                reserved_results: 0,
                buffered_events: VecDeque::new(),
            }),
        }
    }

    fn configure_providers(&self, provider_ids: BTreeSet<String>) {
        if let Ok(mut state) = self.state.lock() {
            state.expected = Some(provider_ids.len());
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

    fn release_external(&self) -> Vec<ExternalEvent> {
        let Ok(mut state) = self.state.lock() else {
            return Vec::new();
        };
        state.external_released = true;
        state.external_replay_pending = true;
        state.buffered_events.drain(..).collect()
    }

    fn finish_external_replay(&self) -> Option<Vec<ExternalEvent>> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        if state.buffered_events.is_empty() {
            state.external_replay_pending = false;
            return None;
        }
        Some(state.buffered_events.drain(..).collect())
    }

    fn accept_external_event(&self, event: ExternalEvent, replaying: bool) -> Option<bool> {
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
        if state.expected.is_none()
            || !state.external_released
            || (!replaying && state.external_replay_pending)
        {
            if !replaying {
                buffer_external_event(&mut state, event);
            }
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

fn buffer_external_event(state: &mut QueryTrackerState, event: ExternalEvent) {
    if state.buffered_events.len() >= MAX_BUFFERED_EXTERNAL_EVENTS {
        if !external_event_is_terminal(&event) {
            return;
        }
        let Some(position) = state
            .buffered_events
            .iter()
            .position(|buffered| !external_event_is_terminal(buffered))
        else {
            return;
        };
        state.buffered_events.remove(position);
    }
    state.buffered_events.push_back(event);
}

fn external_event_is_terminal(event: &ExternalEvent) -> bool {
    matches!(
        event,
        ExternalEvent::Results { complete: true, .. } | ExternalEvent::QueryFailed { .. }
    )
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

fn take_matching_external_activation_routes(
    routes: &mut HashMap<String, ActivationRoute>,
    matches: impl Fn(&ActivationRoute) -> bool,
) -> Vec<(String, String)> {
    let mut cancelled = Vec::new();
    routes.retain(|activation_id, route| {
        if matches(route) {
            cancelled.push((route.provider_id.clone(), activation_id.clone()));
            false
        } else {
            true
        }
    });
    cancelled
}

struct ChatActivationRoute {
    client_id: u64,
    request_id: String,
    sender: SyncSender<DaemonEvent>,
    _reservation: Option<ChatWorkReservation>,
}

fn cancel_matching_chat_activations(
    routes: &mut HashMap<String, ChatActivationRoute>,
    matches: impl Fn(&ChatActivationRoute) -> bool,
) {
    routes.retain(|_, route| !matches(route));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChatAdmission {
    Admitted,
    Unavailable,
    Failed,
}

struct ChatWorkLimiter {
    active: Arc<AtomicUsize>,
}

struct ChatWorkReservation {
    active: Arc<AtomicUsize>,
}

impl Drop for ChatWorkReservation {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

impl ChatWorkLimiter {
    fn new() -> Self {
        Self {
            active: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn reserve(&self) -> Option<ChatWorkReservation> {
        let mut active = self.active.load(Ordering::Acquire);
        loop {
            if active >= MAX_CONCURRENT_CHAT_REQUESTS {
                return None;
            }
            match self.active.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(ChatWorkReservation {
                        active: Arc::clone(&self.active),
                    });
                }
                Err(current) => active = current,
            }
        }
    }
}

fn can_admit_chat_activation(
    routes: &HashMap<String, ChatActivationRoute>,
    client_id: u64,
) -> bool {
    !routes.values().any(|route| route.client_id == client_id)
}

fn start_external_event_dispatcher(runtime: Arc<Runtime>, receiver: Receiver<ExternalEvent>) {
    thread::spawn(move || {
        for event in receiver {
            runtime.route_external_event(event, false);
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
                if !runtime.try_reserve_client_slot() {
                    continue;
                }
                let client_id = runtime.next_client_id();
                let runtime_for_thread = Arc::clone(&runtime);
                let runtime_for_error = Arc::clone(&runtime);
                if let Err(error) = thread::Builder::new()
                    .name(format!("bingux-search-client-{client_id}"))
                    .spawn(move || handle_client(runtime_for_thread, client_id, stream))
                {
                    runtime_for_error.release_client_slot();
                    return Err(error.into());
                }
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
        runtime.release_client_slot();
        return;
    };
    let (sender, receiver) = mpsc::sync_channel(CLIENT_QUEUE_CAPACITY);
    if !runtime.register_client(client_id, sender.clone()) {
        runtime.release_client_slot();
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

    let writer_runtime = Arc::clone(&runtime);
    thread::spawn(move || {
        write_client_events(writer_stream, receiver);
        writer_runtime.disconnect_client(client_id);
    });
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
            Ok(ShellRequest::Cancel(request)) => {
                runtime.cancel_request(client_id, &request.request_id);
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

fn encoding_failure_event(event: &DaemonEvent) -> DaemonEvent {
    let request_id = match event {
        DaemonEvent::Results { request_id, .. }
        | DaemonEvent::Activated { request_id }
        | DaemonEvent::ChatResponse { request_id, .. }
        | DaemonEvent::Error { request_id, .. } => request_id.as_str(),
        DaemonEvent::ShowSearch { .. } | DaemonEvent::IntegrationState { .. } => {
            PROTOCOL_ERROR_REQUEST_ID
        }
    };
    let candidate = DaemonEvent::Error {
        request_id: request_id.to_owned(),
        code: DaemonErrorCode::ProviderFailed,
    };
    if encode_daemon_event_lines(&candidate).is_ok() {
        candidate
    } else {
        DaemonEvent::Error {
            request_id: PROTOCOL_ERROR_REQUEST_ID.to_owned(),
            code: DaemonErrorCode::ProviderFailed,
        }
    }
}

fn write_client_events(stream: UnixStream, receiver: Receiver<DaemonEvent>) {
    let mut writer = std::io::BufWriter::new(stream);
    for event in receiver {
        let records = match encode_daemon_event_lines(&event) {
            Ok(records) => records,
            Err(error) => {
                eprintln!("[bingux-searchd] could not encode daemon event: {error}");
                let fallback = encoding_failure_event(&event);
                match encode_daemon_event_lines(&fallback) {
                    Ok(records) => records,
                    Err(fallback_error) => {
                        eprintln!(
                            "[bingux-searchd] could not encode daemon error: {fallback_error}"
                        );
                        return;
                    }
                }
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

fn launch_program(
    program: &str,
    arguments: &[String],
    reaper: &ProgramReaper,
) -> std::io::Result<()> {
    let reservation = reaper.reserve().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "too many launched programs are awaiting reaping",
        )
    })?;
    let child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    reaper.submit(child, reservation)
}

struct ProgramReaper {
    sender: Sender<ReapedChild>,
    active: Arc<AtomicUsize>,
}

struct ReapedChild {
    child: Child,
    _reservation: ProgramReservation,
}

struct ProgramReservation {
    active: Arc<AtomicUsize>,
}

impl Drop for ProgramReservation {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

impl ProgramReaper {
    fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let active = Arc::new(AtomicUsize::new(0));
        thread::Builder::new()
            .name("bingux-search-program-reaper".to_owned())
            .spawn(move || reap_launched_programs(receiver))
            .context("could not start launched program reaper")?;
        Ok(Self { sender, active })
    }

    fn reserve(&self) -> Option<ProgramReservation> {
        let mut active = self.active.load(Ordering::Acquire);
        loop {
            if active >= MAX_REAPED_PROGRAMS {
                return None;
            }
            match self.active.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(ProgramReservation {
                        active: Arc::clone(&self.active),
                    });
                }
                Err(current) => active = current,
            }
        }
    }

    fn submit(&self, child: Child, reservation: ProgramReservation) -> std::io::Result<()> {
        match self.sender.send(ReapedChild {
            child,
            _reservation: reservation,
        }) {
            Ok(()) => Ok(()),
            Err(error) => {
                let mut reaped_child = error.0;
                let _ = reaped_child.child.wait();
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "launched program reaper stopped",
                ))
            }
        }
    }

    #[cfg(test)]
    fn active_count(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }
}

fn reap_launched_programs(receiver: Receiver<ReapedChild>) {
    let mut children = Vec::with_capacity(MAX_REAPED_PROGRAMS);
    let mut disconnected = false;
    loop {
        if disconnected {
            thread::sleep(PROGRAM_REAPER_POLL_INTERVAL);
        } else {
            match receiver.recv_timeout(PROGRAM_REAPER_POLL_INTERVAL) {
                Ok(child) => children.push(child),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => disconnected = true,
            }
        }
        children.extend(receiver.try_iter());
        children.retain_mut(|child| match child.child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(_) => {
                let _ = child.child.wait();
                false
            }
        });
        if disconnected && children.is_empty() {
            return;
        }
    }
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

    fn test_search_config() -> SearchConfig {
        SearchConfig {
            protocol_version: 1,
            commands: SearchCommands {
                application_launcher: vec!["/bin/true".to_owned()],
                file_opener: vec!["/bin/true".to_owned()],
                clipboard: vec!["/bin/true".to_owned()],
            },
            file_roots: Vec::new(),
            provider_manifest_paths: Vec::new(),
            sqlite_sources: Vec::new(),
            weather: None,
            ai: None,
        }
    }

    fn test_runtime() -> Arc<Runtime> {
        let config = test_search_config();
        let local = Arc::new(
            LocalProviders::new_without_index_workers(&config).expect("start local providers"),
        );
        let (external_sender, _external_receiver) = mpsc::sync_channel(1);
        let external = Arc::new(
            ExternalProviders::start(&[], external_sender).expect("start empty external providers"),
        );
        let runtime = Arc::new(
            Runtime::new(local, None, external, config.commands.clone(), None)
                .expect("start test runtime"),
        );
        runtime.start_query_workers().expect("start query workers");
        runtime
    }

    fn read_socket_event(reader: &mut BufReader<UnixStream>) -> serde_json::Value {
        let record = read_record(reader)
            .expect("read daemon event")
            .expect("daemon event before socket closes");
        serde_json::from_slice(&record).expect("decode daemon event")
    }

    #[test]
    fn serves_a_calculation_query_and_rejects_an_unknown_activation_over_a_socket() {
        let runtime = test_runtime();
        let (daemon_stream, mut shell_stream) = UnixStream::pair().expect("create socket pair");
        shell_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set socket read timeout");
        let client = thread::spawn(move || handle_client(runtime, 1, daemon_stream));
        let mut reader = BufReader::new(shell_stream.try_clone().expect("clone shell socket"));

        let initial_state = read_socket_event(&mut reader);
        assert_eq!(initial_state["type"], "integration-state");
        assert_eq!(initial_state["state"], "unavailable");

        shell_stream
            .write_all(
                br#"{"protocolVersion":1,"type":"query","requestId":"q-01","query":"1 + 1","limit":20}"#,
            )
            .and_then(|()| shell_stream.write_all(b"\n"))
            .expect("send query request");

        let results = read_socket_event(&mut reader);
        assert_eq!(results["type"], "results");
        assert_eq!(results["requestId"], "q-01");
        assert_eq!(results["complete"], true);
        assert!(
            results["results"]
                .as_array()
                .expect("results array")
                .iter()
                .any(|result| result["kind"] == "calculation" && result["title"] == "2"),
        );

        shell_stream
            .write_all(
                br#"{"protocolVersion":1,"type":"activate","requestId":"a-01","resultId":"missing"}"#,
            )
            .and_then(|()| shell_stream.write_all(b"\n"))
            .expect("send activation request");

        let activation_error = read_socket_event(&mut reader);
        assert_eq!(activation_error["type"], "error");
        assert_eq!(activation_error["requestId"], "a-01");
        assert_eq!(activation_error["code"], "unknown-result");

        drop(reader);
        drop(shell_stream);
        client.join().expect("join socket client");
    }

    #[test]
    #[ignore = "machine-specific warm-query benchmark"]
    fn measures_warm_socket_query_latency() {
        const SAMPLE_COUNT: usize = 200;

        let runtime = test_runtime();
        let (daemon_stream, mut shell_stream) = UnixStream::pair().expect("create socket pair");
        shell_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set socket read timeout");
        let client = thread::spawn(move || handle_client(runtime, 1, daemon_stream));
        let mut reader = BufReader::new(shell_stream.try_clone().expect("clone shell socket"));
        let _initial_state = read_socket_event(&mut reader);

        shell_stream
            .write_all(
                br#"{"protocolVersion":1,"type":"query","requestId":"q-warm","query":"12345 * 6789","limit":20}"#,
            )
            .and_then(|()| shell_stream.write_all(b"\n"))
            .expect("send warm query");
        let warm_result = read_socket_event(&mut reader);
        assert_eq!(warm_result["type"], "results");
        assert_eq!(warm_result["complete"], true);

        let mut samples = Vec::with_capacity(SAMPLE_COUNT);
        for index in 0..SAMPLE_COUNT {
            let request_id = format!("q-benchmark-{index}");
            let request = format!(
                r#"{{"protocolVersion":1,"type":"query","requestId":"{request_id}","query":"12345 * 6789","limit":20}}"#
            );
            let started = Instant::now();
            shell_stream
                .write_all(request.as_bytes())
                .and_then(|()| shell_stream.write_all(b"\n"))
                .expect("send benchmark query");
            let result = read_socket_event(&mut reader);
            samples.push(started.elapsed());
            assert_eq!(result["type"], "results");
            assert_eq!(result["requestId"], request_id);
            assert_eq!(result["complete"], true);
        }
        samples.sort_unstable();

        let p95_index = (samples.len() * 95).div_ceil(100) - 1;
        let p95 = samples[p95_index];
        let maximum = samples.last().expect("benchmark samples");
        eprintln!(
            "[benchmark] warm socket query: p95={}ns max={}ns samples={SAMPLE_COUNT}",
            p95.as_nanos(),
            maximum.as_nanos(),
        );

        drop(reader);
        drop(shell_stream);
        client.join().expect("join socket client");
    }

    #[test]
    fn admits_one_chat_route_per_client() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let mut routes = HashMap::new();
        routes.insert(
            "chat-1".into(),
            ChatActivationRoute {
                client_id: 1,
                request_id: "a-1".into(),
                sender,
                _reservation: None,
            },
        );

        assert!(!can_admit_chat_activation(&routes, 1));
        assert!(can_admit_chat_activation(&routes, 2));
    }

    #[test]
    fn oversized_event_emits_one_bounded_provider_error_before_close() {
        let event = DaemonEvent::Results {
            request_id: "q-01".into(),
            complete: true,
            elapsed_usec: 1,
            results: vec![DaemonResult {
                result_id: "r-01".into(),
                provider_id: "provider".into(),
                kind: bingux_searchd::protocol::ResultKind::Action,
                title: "invalid\nresult".into(),
                subtitle: String::new(),
                icon: String::new(),
                score: 0.5,
            }],
        };
        let (daemon_stream, shell_stream) = UnixStream::pair().expect("create socket pair");
        let (sender, receiver) = mpsc::channel();
        let writer = thread::spawn(move || write_client_events(daemon_stream, receiver));
        sender.send(event).expect("queue malformed event");
        drop(sender);

        let mut reader = BufReader::new(shell_stream);
        let error = read_socket_event(&mut reader);
        assert_eq!(error["type"], "error");
        assert_eq!(error["requestId"], "q-01");
        assert_eq!(error["code"], "provider-failed");
        writer.join().expect("join client writer");
    }

    #[test]
    fn launched_program_reaper_releases_completed_process_reservations() {
        let reaper = ProgramReaper::new().expect("start program reaper");
        let reservation = reaper.reserve().expect("reserve program reaper slot");
        let child = Command::new(env::current_exe().expect("locate test executable"))
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start completed child process");

        reaper
            .submit(child, reservation)
            .expect("submit child process for reaping");

        let deadline = Instant::now() + Duration::from_secs(1);
        while reaper.active_count() != 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(reaper.active_count(), 0);
    }

    #[test]
    fn launched_program_reaper_bounds_process_reservations() {
        let reaper = ProgramReaper::new().expect("start program reaper");
        let reservations = (0..MAX_REAPED_PROGRAMS)
            .map(|_| reaper.reserve().expect("reserve program reaper slot"))
            .collect::<Vec<_>>();

        assert!(reaper.reserve().is_none());

        drop(reservations);

        assert!(reaper.reserve().is_some());
    }

    #[test]
    fn chat_work_limiter_bounds_in_flight_requests() {
        let limiter = ChatWorkLimiter::new();
        let reservations = (0..MAX_CONCURRENT_CHAT_REQUESTS)
            .map(|_| limiter.reserve().expect("reserve chat request"))
            .collect::<Vec<_>>();

        assert!(limiter.reserve().is_none());

        drop(reservations);

        assert!(limiter.reserve().is_some());
    }

    #[test]
    fn cancelled_chat_request_releases_limiter_capacity_immediately() {
        let limiter = ChatWorkLimiter::new();
        let mut reservations = (0..MAX_CONCURRENT_CHAT_REQUESTS)
            .map(|_| limiter.reserve().expect("reserve chat request"))
            .collect::<Vec<_>>();
        let (sender, _receiver) = mpsc::sync_channel(1);
        let mut routes = HashMap::from([(
            "chat-1".into(),
            ChatActivationRoute {
                client_id: 1,
                request_id: "a-1".into(),
                sender,
                _reservation: Some(reservations.pop().expect("route reservation")),
            },
        )]);

        assert!(limiter.reserve().is_none());
        cancel_matching_chat_activations(&mut routes, |route| {
            route.client_id == 1 && route.request_id == "a-1"
        });
        assert!(limiter.reserve().is_some());
        drop(reservations);
    }

    #[test]
    fn cancelled_chat_request_releases_route_admission() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let mut routes = HashMap::from([(
            "chat-1".into(),
            ChatActivationRoute {
                client_id: 1,
                request_id: "a-1".into(),
                sender,
                _reservation: None,
            },
        )]);

        cancel_matching_chat_activations(&mut routes, |route| {
            route.client_id == 1 && route.request_id == "a-1"
        });

        assert!(can_admit_chat_activation(&routes, 1));
        assert!(!routes.contains_key("chat-1"));
    }

    #[test]
    fn external_activation_cancellation_scopes_routes_to_client_request() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let mut routes = HashMap::from([
            (
                "provider-1".into(),
                ActivationRoute {
                    client_id: 1,
                    provider_id: "notes".into(),
                    request_id: "a-1".into(),
                    sender: sender.clone(),
                },
            ),
            (
                "provider-2".into(),
                ActivationRoute {
                    client_id: 1,
                    provider_id: "notes".into(),
                    request_id: "a-2".into(),
                    sender: sender.clone(),
                },
            ),
            (
                "provider-3".into(),
                ActivationRoute {
                    client_id: 2,
                    provider_id: "notes".into(),
                    request_id: "a-1".into(),
                    sender,
                },
            ),
        ]);

        let cancelled = take_matching_external_activation_routes(&mut routes, |route| {
            route.client_id == 1 && route.request_id == "a-1"
        });

        assert_eq!(cancelled, vec![("notes".into(), "provider-1".into())]);
        assert_eq!(routes.len(), 2);
        assert!(routes.contains_key("provider-2"));
        assert!(routes.contains_key("provider-3"));
    }
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
    fn query_removal_cleans_registered_activations() {
        let runtime = test_runtime();
        let (sender, _receiver) = mpsc::sync_channel(1);
        let tracker = Arc::new(QueryTracker::new(1, "q-01".into(), 2, sender));
        runtime
            .queries
            .lock()
            .expect("query registry lock")
            .insert("q-01".into(), tracker);
        runtime
            .client_queries
            .lock()
            .expect("client query registry lock")
            .insert(1, "q-01".into());
        runtime
            .activations
            .lock()
            .expect("activation registry lock")
            .insert("r-01".into(), 1, "q-01", Activation::None);

        runtime.remove_query("q-01", 1);

        assert!(!runtime.query_is_active(1, "q-01"));
        assert!(runtime.query_tracker("q-01").is_none());
        assert!(
            runtime
                .activations
                .lock()
                .expect("activation registry lock")
                .take("r-01", 1)
                .is_none()
        );
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

    #[test]
    fn buffers_terminal_events_until_pre_release_events_are_replayed() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let tracker = QueryTracker::new(1, "q-01".into(), 20, sender);
        tracker.configure_providers(BTreeSet::from(["provider".into()]));

        assert_eq!(
            tracker.accept_external_event(
                ExternalEvent::Results {
                    provider_id: "provider".into(),
                    query_id: "provider-query".into(),
                    complete: false,
                    results: Vec::new(),
                },
                false,
            ),
            None
        );
        let mut first_batch = tracker.release_external();
        assert_eq!(first_batch.len(), 1);
        assert_eq!(
            tracker.accept_external_event(
                ExternalEvent::Results {
                    provider_id: "provider".into(),
                    query_id: "provider-query".into(),
                    complete: true,
                    results: Vec::new(),
                },
                false,
            ),
            None
        );
        assert_eq!(
            tracker.accept_external_event(first_batch.remove(0), true),
            Some(false)
        );
        let mut terminal_batch = tracker
            .finish_external_replay()
            .expect("terminal event buffers while replay is pending");
        assert_eq!(terminal_batch.len(), 1);
        assert_eq!(
            tracker.accept_external_event(terminal_batch.remove(0), true),
            Some(true)
        );
        assert!(tracker.finish_external_replay().is_none());
    }

    #[test]
    fn buffers_external_events_until_the_local_result_is_sent() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let tracker = QueryTracker::new(1, "q-01".into(), 20, sender);
        tracker.configure_providers(BTreeSet::from(["failed".into(), "healthy".into()]));

        assert_eq!(
            tracker.accept_external_event(
                ExternalEvent::QueryFailed {
                    provider_id: "failed".into(),
                    query_id: "provider-query".into(),
                },
                false,
            ),
            None
        );
        assert_eq!(
            tracker.accept_external_event(
                ExternalEvent::Results {
                    provider_id: "healthy".into(),
                    query_id: "provider-query".into(),
                    complete: true,
                    results: Vec::new(),
                },
                false,
            ),
            None
        );
        let mut buffered = tracker.release_external();
        assert_eq!(buffered.len(), 2);
        assert_eq!(
            tracker.accept_external_event(buffered.remove(0), true),
            Some(false)
        );
        assert_eq!(
            tracker.accept_external_event(buffered.remove(0), true),
            Some(true)
        );
        assert!(tracker.finish_external_replay().is_none());
    }
}
