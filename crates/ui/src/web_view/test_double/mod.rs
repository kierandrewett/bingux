//! `TestDoubleBackend`: an in-memory [`BrowserBackend`] for tests of the
//! browser's UI logic and for its headless snapshots. It is **not** an
//! engine and **never ships**: it exists only with bingux-ui's `testing` and
//! `web-view` features together, which the browser enables for its tests and
//! its `snapshot` build.
//!
//! It keeps a session history per tab, answers a load from a table of host
//! names (`sites.rs`: a page, a certificate error, an unknown host, a crash, a
//! permission prompt ...), draws a wireframe page into a real [`PageSurface`]
//! and records every call, so a test can say what the UI asked of the engine
//! and a snapshot can show the UI in each state without a network.
//!
//! Loads finish at once unless the double is made with
//! [`TestDoubleBackend::manual`], which holds them until
//! [`TestDoubleBackend::finish_loads`]: that is how a test or a snapshot
//! looks at a page that is still loading.

mod paint;
mod sites;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::Waker;

use crate::frame_view::DamageRect;

use crate::web_view::backend::{
    BackendError, BrowserBackend, Capabilities, Done, EngineInfo, EngineSettings, EngineState,
    FindQuery, RemoteSettings, TabConfig,
};
use crate::web_view::events::{BrowserEvent, EngineEvent, FailureCause, NewWindow, TabEvent};
use crate::web_view::ids::{DownloadId, RequestId, TabId};
use crate::web_view::input::{PageInput, PointerKind, WheelUnit};
use crate::web_view::page::{Audio, GoneReason, HistoryEntry, Security};
use crate::web_view::queue::EventQueue;
use crate::web_view::report::{CheckStatus, ReportEntry, SecurityReport};
use crate::web_view::requests::{
    CertError, ContextTarget, DownloadInfo, DownloadState, Request, RequestKind, Response,
};
use crate::web_view::surface::PageSurface;
use crate::web_view::toolbox::Toolbox;

pub use sites::{fix_up, host_of};

use paint::{LINK, LINK_URL, Look, PAGE_HEIGHT};
use sites::{After, Outcome};

/// Something the UI asked of the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    CreateTab(TabConfig),
    AcceptTab(TabId),
    CloseTab(TabId),
    Load(TabId, String),
    Reload(TabId, bool),
    Stop(TabId),
    Back(TabId),
    Forward(TabId),
    GoToHistory(TabId, usize),
    SetSize(TabId, u32, u32, f32),
    SetVisible(TabId, bool, bool),
    SetZoom(TabId, f64),
    SetEnvironment(crate::web_view::environment::PageEnvironment),
    Send(TabId, PageInput),
    Find(TabId, FindQuery),
    FindStop(TabId),
    SetMuted(TabId, bool),
    /// A page printed to a PDF with this file name.
    PrintPdf(TabId, String),
    ClearHistory,
    /// The developer tools of a tab, in a tab of their own.
    OpenDevtools(TabId),
    /// A toolbox the browser closed.
    CloseDevtools(Toolbox),
    Respond(TabId, RequestId, Response),
    CancelDownload(DownloadId),
    PauseDownload(DownloadId),
    ResumeDownload(DownloadId),
    RemoveDownload(DownloadId),
    Restart,
}

struct TabState {
    config: TabConfig,
    accepted: bool,
    history: Vec<HistoryEntry>,
    index: usize,
    /// The address being loaded, until `finish_loads`.
    loading: Option<String>,
    /// The certificate error the person may still proceed past.
    blocked: Option<(RequestId, String)>,
    size: (u32, u32),
    scale: f32,
    zoom: f64,
    scroll: f32,
    visible: bool,
    look: Look,
    hue: f32,
    hovering: bool,
    muted: bool,
    find: Option<(String, u32)>,
    surface: Arc<PageSurface>,
}

#[derive(Default)]
struct Inner {
    /// The toolbox session of each target that has one: the engine allows one
    /// per target, and closes it after the target.
    toolboxes: BTreeMap<TabId, TabId>,
    next_tab: u32,
    next_request: u32,
    next_download: u64,
    tabs: BTreeMap<TabId, TabState>,
    calls: Vec<Call>,
    downloads: Vec<DownloadInfo>,
    /// The engine is gone: every call says so, until `restart`.
    failed: bool,
    /// Why a start (or a restart) does not work yet, until `fix_start`.
    blocked: Option<(FailureCause, String)>,
    /// What the tabs of the engine that failed had: history and where in it.
    lost: BTreeMap<TabId, (Vec<HistoryEntry>, usize)>,
    /// The engine's report, `None` while it has not produced one.
    report: Option<SecurityReport>,
}

/// An in-memory [`BrowserBackend`]. See the module comment.
pub struct TestDoubleBackend {
    inner: Mutex<Inner>,
    queue: EventQueue,
    capabilities: Mutex<Capabilities>,
    auto_finish: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl TestDoubleBackend {
    /// A double whose loads finish at once and which can do everything.
    pub fn new() -> Arc<Self> {
        Self::build(Capabilities::all(), true)
    }

    /// A double that holds every load until [`finish_loads`](Self::finish_loads).
    pub fn manual() -> Arc<Self> {
        Self::build(Capabilities::all(), false)
    }

    /// A double that has only the capabilities given, as the engine does
    /// while the ABI lacks them.
    pub fn with_capabilities(capabilities: Capabilities) -> Arc<Self> {
        Self::build(capabilities, true)
    }

    /// What the engine can do changed: a call it had not answered yet was
    /// taken (find is learned from the first session).
    pub fn set_capabilities(&self, capabilities: Capabilities) {
        *lock(&self.capabilities) = capabilities;
    }

    fn build(capabilities: Capabilities, auto_finish: bool) -> Arc<Self> {
        let double = Arc::new(Self {
            inner: Mutex::new(Inner {
                next_tab: 1,
                next_request: 1,
                next_download: 1,
                report: Some(healthy_report()),
                ..Inner::default()
            }),
            queue: EventQueue::new(),
            capabilities: Mutex::new(capabilities),
            auto_finish,
        });
        double.queue.push(BrowserEvent::Engine(EngineEvent::Ready));
        double
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        lock(&self.inner).calls.clone()
    }

    /// Forget the calls so far.
    pub fn clear_calls(&self) {
        lock(&self.inner).calls.clear();
    }

    /// Queue an event as if the engine had produced it.
    pub fn emit(&self, event: BrowserEvent) {
        self.queue.push(event);
    }

    /// Ask the UI a question as the engine would. Returns its id.
    pub fn ask(&self, tab: TabId, kind: RequestKind) -> RequestId {
        let mut inner = lock(&self.inner);
        let id = RequestId(inner.next_request);
        inner.next_request += 1;
        drop(inner);
        self.queue.push(BrowserEvent::Request {
            tab,
            request: Request { id, kind },
        });
        id
    }

    /// The page opens a window: a new session, as `window.open` makes, with
    /// what the engine sends today of it (nothing about who or how).
    pub fn open_window(&self, opener: TabId, url: &str) -> TabId {
        self.open_window_with(opener, url, NewWindow::default())
    }

    /// The page opens a window and the engine says how (CR-8).
    pub fn open_window_with(&self, opener: TabId, url: &str, window: NewWindow) -> TabId {
        let mut inner = lock(&self.inner);
        let new = TabId(inner.next_tab);
        inner.next_tab += 1;
        let state = Self::new_state(
            TabConfig {
                opener: Some(opener),
                ..TabConfig::default()
            },
            false,
        );
        inner.tabs.insert(new, state);
        drop(inner);
        self.queue.push(BrowserEvent::tab(
            opener,
            TabEvent::NewTab {
                new,
                url: url.to_string(),
                name: String::new(),
                no_opener: false,
                window,
            },
        ));
        new
    }

    /// The engine ends a question by itself: its deadline passed, or the page
    /// went on.
    pub fn close_request(&self, tab: TabId, request: RequestId) {
        self.queue
            .push(BrowserEvent::RequestClosed { tab, request });
    }

    /// The engine process dies: every tab is lost with what it had, and
    /// every call says so until `restart`.
    pub fn fail_engine(&self, cause: FailureCause, message: &str, code: u32, restartable: bool) {
        let mut inner = lock(&self.inner);
        let tabs = std::mem::take(&mut inner.tabs);
        for (id, state) in tabs {
            inner.lost.insert(id, (state.history, state.index));
        }
        inner.failed = true;
        drop(inner);
        self.queue.push(BrowserEvent::Engine(EngineEvent::Failed {
            message: message.to_string(),
            code,
            cause,
            restartable,
        }));
    }

    /// The engine does not start (a profile that is not safe, a sandbox it
    /// cannot reach) until `fix_start`: the backend begins failed.
    pub fn failed_start(cause: FailureCause, message: &str) -> Arc<Self> {
        let double = Self::build(Capabilities::default(), true);
        {
            let mut inner = lock(&double.inner);
            inner.failed = true;
            inner.blocked = Some((cause, message.to_string()));
            inner.report = None;
        }
        // The engine never was ready: the first and only thing it says is why.
        double.queue.drain_into(&mut Vec::new());
        double.queue.push(BrowserEvent::Engine(EngineEvent::Failed {
            message: message.to_string(),
            code: 0,
            cause,
            restartable: true,
        }));
        double
    }

    /// Whatever kept the engine from starting is fixed: the next `restart` works.
    pub fn fix_start(&self) {
        lock(&self.inner).blocked = None;
    }

    /// The engine's report, or `None` while it has not produced one.
    pub fn set_report(&self, report: Option<SecurityReport>) {
        lock(&self.inner).report = report;
        self.queue
            .push(BrowserEvent::Engine(EngineEvent::SecurityChanged));
    }

    /// Finish every load that is waiting.
    pub fn finish_loads(&self) {
        let mut inner = lock(&self.inner);
        let waiting: Vec<TabId> = inner
            .tabs
            .iter()
            .filter(|(_, tab)| tab.loading.is_some())
            .map(|(id, _)| *id)
            .collect();
        for tab in waiting {
            self.complete(&mut inner, tab);
        }
    }

    /// Move every running download on: more bytes, and done when all are in.
    pub fn advance_downloads(&self, bytes: u64) {
        let mut inner = lock(&self.inner);
        let mut changed = Vec::new();
        for download in &mut inner.downloads {
            if download.state != DownloadState::Running {
                continue;
            }
            download.received += bytes;
            if download
                .total
                .is_some_and(|total| download.received >= total)
            {
                download.received = download.total.unwrap_or(download.received);
                download.state = DownloadState::Finished;
                download.speed = 0;
                download.path =
                    Some(PathBuf::from("/home/user/Downloads").join(&download.filename));
            }
            changed.push(download.clone());
        }
        drop(inner);
        for download in changed {
            self.queue.push(BrowserEvent::Download(download));
        }
    }

    fn new_state(config: TabConfig, accepted: bool) -> TabState {
        TabState {
            size: (config.width, config.height),
            scale: config.scale,
            zoom: config.zoom,
            config,
            accepted,
            history: Vec::new(),
            index: 0,
            loading: None,
            blocked: None,
            scroll: 0.0,
            visible: true,
            look: Look::Blank,
            hue: 210.0,
            hovering: false,
            muted: false,
            find: None,
            surface: PageSurface::new(),
        }
    }

    fn with_tab<R>(
        &self,
        tab: TabId,
        call: Call,
        run: impl FnOnce(&mut Inner, TabId) -> R,
    ) -> Result<R, BackendError> {
        let mut inner = lock(&self.inner);
        inner.calls.push(call);
        if inner.failed {
            return Err(BackendError::EngineGone);
        }
        if !inner.tabs.contains_key(&tab) {
            return Err(BackendError::NoSuchTab(tab));
        }
        Ok(run(&mut inner, tab))
    }

    fn history_event(&self, tab: TabId, state: &TabState) {
        self.queue.push(BrowserEvent::tab(
            tab,
            TabEvent::History {
                index: state.index,
                entries: state.history.clone(),
            },
        ));
    }

    fn location_event(&self, tab: TabId, state: &TabState) {
        let url = state
            .history
            .get(state.index)
            .map(|entry| entry.url.clone())
            .unwrap_or_default();
        self.queue.push(BrowserEvent::tab(
            tab,
            TabEvent::Location {
                url,
                can_back: state.index > 0,
                can_forward: state.index + 1 < state.history.len(),
            },
        ));
    }

    /// Start loading `url` in `tab`: into history (`push`) or in place.
    fn start(&self, inner: &mut Inner, tab: TabId, url: &str, push: bool) {
        let url = sites::fix_up(url);
        let Some(state) = inner.tabs.get_mut(&tab) else {
            return;
        };
        if push {
            let keep = if state.history.is_empty() {
                0
            } else {
                state.index + 1
            };
            state.history.truncate(keep);
            state.history.push(HistoryEntry {
                url: url.clone(),
                title: String::new(),
            });
            state.index = state.history.len() - 1;
        } else if let Some(entry) = state.history.get_mut(state.index) {
            entry.url = url.clone();
        }
        state.loading = Some(url);
        state.scroll = 0.0;
        state.blocked = None;
        state.find = None;
        self.queue.push(BrowserEvent::tab(
            tab,
            TabEvent::Load {
                started: true,
                loading: true,
                progress: 0.1,
                error: None,
            },
        ));
        self.location_event(tab, state);
        self.history_event(tab, state);
        if self.auto_finish {
            self.complete(inner, tab);
        }
    }

    fn redraw(state: &mut TabState) {
        let (width, height) = state.size;
        let scale = state.scale * state.zoom as f32;
        let device = (
            (width as f32 * state.scale) as u32,
            (height as f32 * state.scale) as u32,
        );
        if device.0 == 0 || device.1 == 0 {
            return;
        }
        let bytes = paint::page(
            state.look,
            state.hue,
            state.scroll,
            device.0,
            device.1,
            scale,
        );
        let bytes: crate::frame_view::PixelBytes = Arc::new(Bytes(bytes));
        if let Some(frame) = PageSurface::frame_of(
            device.0,
            device.1,
            device.0 as usize * 4,
            state.scale,
            true,
            Some(DamageRect::full(device.0, device.1)),
            bytes,
        ) {
            state.surface.push(frame);
        }
    }

    /// A load ends: the page, or why not.
    fn complete(&self, inner: &mut Inner, tab: TabId) {
        let next_download = inner.next_download;
        let Some(state) = inner.tabs.get_mut(&tab) else {
            return;
        };
        let Some(url) = state.loading.take() else {
            return;
        };
        let outcome = sites::resolve(&url, next_download);
        let stop = |error| {
            BrowserEvent::tab(
                tab,
                TabEvent::Load {
                    started: false,
                    loading: false,
                    progress: 1.0,
                    error,
                },
            )
        };
        match outcome {
            Outcome::Blank => {
                state.look = Look::Blank;
                self.queue
                    .push(BrowserEvent::tab(tab, TabEvent::Title(String::new())));
                self.queue.push(BrowserEvent::tab(
                    tab,
                    TabEvent::Security(Security::default()),
                ));
                self.queue.push(stop(None));
            }
            Outcome::Error(error) => {
                state.look = Look::Blank;
                let host = sites::host_of(&url);
                self.queue
                    .push(BrowserEvent::tab(tab, TabEvent::Title(host)));
                self.queue.push(BrowserEvent::tab(
                    tab,
                    TabEvent::Security(Security::default()),
                ));
                self.queue.push(stop(Some(error)));
            }
            Outcome::Cert(error) => {
                state.look = Look::Blank;
                let id = RequestId(inner.next_request);
                inner.next_request += 1;
                if let Some(state) = inner.tabs.get_mut(&tab) {
                    state.blocked = Some((id, url.clone()));
                }
                self.queue
                    .push(BrowserEvent::tab(tab, TabEvent::Title(error.host.clone())));
                self.queue.push(stop(Some(crate::web_view::page::LoadError {
                    code: 0x805A_1FF3,
                    name: error.code.clone(),
                })));
                self.queue.push(BrowserEvent::Request {
                    tab,
                    request: Request {
                        id,
                        kind: RequestKind::CertError(error),
                    },
                });
            }
            Outcome::Page {
                title,
                security,
                look,
                hue,
                after,
            } => {
                state.look = look;
                state.hue = hue;
                if let Some(entry) = state.history.get_mut(state.index) {
                    entry.title = title.clone();
                }
                let favicon = format!("https://{}/favicon.ico", sites::host_of(&url));
                self.queue
                    .push(BrowserEvent::tab(tab, TabEvent::Title(title)));
                self.queue
                    .push(BrowserEvent::tab(tab, TabEvent::Security(security)));
                self.queue
                    .push(BrowserEvent::tab(tab, TabEvent::Favicon(favicon)));
                self.queue.push(stop(None));
                self.history_event(tab, state);
                Self::redraw(state);
                let muted = state.muted;
                for effect in after {
                    match effect {
                        After::Ask(kind) => {
                            let id = RequestId(inner.next_request);
                            inner.next_request += 1;
                            self.queue.push(BrowserEvent::Request {
                                tab,
                                request: Request { id, kind },
                            });
                        }
                        After::Download(info) => {
                            inner.next_download += 1;
                            inner.downloads.push(info.clone());
                            self.queue.push(BrowserEvent::Download(info));
                        }
                        After::Sound => self.queue.push(BrowserEvent::tab(
                            tab,
                            TabEvent::Audio(Audio {
                                playing: true,
                                muted,
                            }),
                        )),
                        After::Crash => self.queue.push(BrowserEvent::tab(
                            tab,
                            TabEvent::ProcessGone(GoneReason::Crashed),
                        )),
                    }
                }
            }
        }
    }
}

/// A pixel buffer the frame view reads.
struct Bytes(Vec<u8>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl BrowserBackend for TestDoubleBackend {
    fn capabilities(&self) -> Capabilities {
        *lock(&self.capabilities)
    }

    fn info(&self) -> EngineInfo {
        let inner = lock(&self.inner);
        let failed = inner.failed;
        EngineInfo {
            state: if failed {
                EngineState::Failed
            } else {
                EngineState::Ready
            },
            version: if failed {
                String::new()
            } else {
                "Gecko 159.0 (test double)".to_string()
            },
            build_id: if failed {
                String::new()
            } else {
                "20260101000000".to_string()
            },
            user_agent: if failed {
                String::new()
            } else {
                "Mozilla/5.0 (X11; Linux x86_64; rv:159.0) Gecko/20100101 Firefox/159.0".to_string()
            },
            settings: EngineSettings {
                profile_dir: PathBuf::from("/tmp/bingux-browser-test/profile"),
                cache_dir: PathBuf::from("/tmp/bingux-browser-test/cache"),
                download_dir: PathBuf::from("/tmp/bingux-browser-test/downloads"),
                crash_dir: PathBuf::from("/tmp/bingux-browser-test/crashes"),
                remote_settings: RemoteSettings::SecurityOnly,
                remote_settings_server: Some("https://rs.example.org/v1".to_string()),
                content_process_limit: 8,
                locales: "en-GB".to_string(),
                webrtc_ip_policy:
                    "The engine's own fixed policy: no local or TCP addresses are shared"
                        .to_string(),
                sandbox_floor:
                    "Standard: web content level 6, network level 2, seccomp on every process"
                        .to_string(),
            },
            security: inner.report.clone(),
        }
    }

    fn restart(&self) -> Done {
        let mut inner = lock(&self.inner);
        inner.calls.push(Call::Restart);
        if let Some((cause, message)) = inner.blocked.clone() {
            drop(inner);
            // Still no: the person sees the same page again, with the same words.
            self.queue.push(BrowserEvent::Engine(EngineEvent::Failed {
                message: message.clone(),
                code: 0,
                cause,
                restartable: true,
            }));
            return Err(BackendError::Engine(message));
        }
        inner.failed = false;
        inner.report = Some(healthy_report());
        drop(inner);
        self.queue.push(BrowserEvent::Engine(EngineEvent::Ready));
        Ok(())
    }

    fn set_waker(&self, waker: Waker) {
        self.queue.set_waker(waker);
    }

    fn poll_events(&self, out: &mut Vec<BrowserEvent>) {
        self.queue.drain_into(out);
    }

    fn create_tab(&self, config: &TabConfig) -> Result<TabId, BackendError> {
        let mut inner = lock(&self.inner);
        inner.calls.push(Call::CreateTab(config.clone()));
        if inner.failed {
            return Err(BackendError::EngineGone);
        }
        let tab = TabId(inner.next_tab);
        inner.next_tab += 1;
        let mut state = Self::new_state(config.clone(), true);
        // A tab of the engine that failed takes up its history again, and
        // loads the page it was on, not a new one.
        let saved = config.restore_from.and_then(|old| inner.lost.remove(&old));
        if let Some((history, index)) = &saved {
            state.history = history.clone();
            state.index = *index;
        }
        inner.tabs.insert(tab, state);
        self.queue.push(BrowserEvent::tab(tab, TabEvent::Ready));
        if let Some((history, index)) = saved {
            if let Some(entry) = history.get(index) {
                self.start(&mut inner, tab, &entry.url, false);
            }
        } else if !config.url.is_empty() {
            self.start(&mut inner, tab, &config.url, true);
        } else if let Some(state) = inner.tabs.get_mut(&tab) {
            Self::redraw(state);
        }
        Ok(tab)
    }

    fn accept_tab(&self, new: TabId, config: &TabConfig) -> Done {
        let url = config.url.clone();
        self.with_tab(new, Call::AcceptTab(new), |inner, tab| {
            if let Some(state) = inner.tabs.get_mut(&tab) {
                state.accepted = true;
                state.config = config.clone();
            }
            self.queue.push(BrowserEvent::tab(tab, TabEvent::Ready));
            if !url.is_empty() {
                self.start(inner, tab, &url, true);
            }
        })
    }

    fn close_tab(&self, tab: TabId) -> Done {
        self.with_tab(tab, Call::CloseTab(tab), |inner, tab| {
            inner.tabs.remove(&tab);
            self.queue
                .push(BrowserEvent::tab(tab, TabEvent::Closed { error: None }));
            // The engine closes a toolbox after its target, and says why.
            if let Some(toolbox) = inner.toolboxes.remove(&tab) {
                inner.tabs.remove(&toolbox);
                self.queue.push(BrowserEvent::tab(
                    toolbox,
                    TabEvent::Closed {
                        error: Some("TargetClosed".to_string()),
                    },
                ));
            }
        })
    }

    fn surface(&self, tab: TabId) -> Option<Arc<PageSurface>> {
        lock(&self.inner)
            .tabs
            .get(&tab)
            .map(|state| state.surface.clone())
    }

    fn load(&self, tab: TabId, url: &str) -> Done {
        let url = url.to_string();
        self.with_tab(tab, Call::Load(tab, url.clone()), |inner, tab| {
            self.start(inner, tab, &url, true);
        })
    }

    fn reload(&self, tab: TabId, bypass_cache: bool) -> Done {
        self.with_tab(tab, Call::Reload(tab, bypass_cache), |inner, tab| {
            let url = inner
                .tabs
                .get(&tab)
                .and_then(|state| state.history.get(state.index))
                .map(|entry| entry.url.clone());
            if let Some(url) = url {
                self.start(inner, tab, &url, false);
            }
        })
    }

    fn stop(&self, tab: TabId) -> Done {
        self.with_tab(tab, Call::Stop(tab), |inner, tab| {
            if let Some(state) = inner.tabs.get_mut(&tab) {
                if state.loading.take().is_some() {
                    self.queue.push(BrowserEvent::tab(
                        tab,
                        TabEvent::Load {
                            started: false,
                            loading: false,
                            progress: 0.0,
                            error: None,
                        },
                    ));
                }
            }
        })
    }

    fn back(&self, tab: TabId) -> Done {
        self.with_tab(tab, Call::Back(tab), |inner, tab| {
            let url = inner.tabs.get_mut(&tab).and_then(|state| {
                (state.index > 0).then(|| {
                    state.index -= 1;
                    state.history[state.index].url.clone()
                })
            });
            if let Some(url) = url {
                self.start(inner, tab, &url, false);
            }
        })
    }

    fn forward(&self, tab: TabId) -> Done {
        self.with_tab(tab, Call::Forward(tab), |inner, tab| {
            let url = inner.tabs.get_mut(&tab).and_then(|state| {
                (state.index + 1 < state.history.len()).then(|| {
                    state.index += 1;
                    state.history[state.index].url.clone()
                })
            });
            if let Some(url) = url {
                self.start(inner, tab, &url, false);
            }
        })
    }

    fn go_to_history(&self, tab: TabId, index: usize) -> Done {
        self.with_tab(tab, Call::GoToHistory(tab, index), |inner, tab| {
            let url = inner.tabs.get_mut(&tab).and_then(|state| {
                (index < state.history.len()).then(|| {
                    state.index = index;
                    state.history[index].url.clone()
                })
            });
            if let Some(url) = url {
                self.start(inner, tab, &url, false);
            }
        })
    }

    fn set_size(&self, tab: TabId, width: u32, height: u32, scale: f32) -> Done {
        self.with_tab(
            tab,
            Call::SetSize(tab, width, height, scale),
            |inner, tab| {
                if let Some(state) = inner.tabs.get_mut(&tab) {
                    state.size = (width, height);
                    state.scale = scale;
                    Self::redraw(state);
                }
            },
        )
    }

    fn set_visible(&self, tab: TabId, visible: bool, focused: bool) -> Done {
        self.with_tab(
            tab,
            Call::SetVisible(tab, visible, focused),
            |inner, tab| {
                if let Some(state) = inner.tabs.get_mut(&tab) {
                    state.visible = visible;
                }
            },
        )
    }

    fn set_zoom(&self, tab: TabId, zoom: f64) -> Done {
        self.with_tab(tab, Call::SetZoom(tab, zoom), |inner, tab| {
            if let Some(state) = inner.tabs.get_mut(&tab) {
                state.zoom = zoom;
                Self::redraw(state);
            }
        })
    }

    fn set_environment(&self, environment: &crate::web_view::environment::PageEnvironment) -> Done {
        lock(&self.inner)
            .calls
            .push(Call::SetEnvironment(environment.clone()));
        Ok(())
    }

    fn send(&self, tab: TabId, input: PageInput) -> Done {
        let recorded = input.clone();
        let hover_links = lock(&self.capabilities).hover_links;
        let menus = lock(&self.capabilities).context_menus;
        self.with_tab(tab, Call::Send(tab, recorded), |inner, tab| {
            let request_id = RequestId(inner.next_request);
            let Some(state) = inner.tabs.get_mut(&tab) else {
                return;
            };
            let on_link = |x: f64, y: f64, scroll: f32, zoom: f64| {
                let (x, y) = (x as f32 / zoom as f32, y as f32 / zoom as f32 + scroll);
                x >= LINK.0
                    && x <= LINK.0 + LINK.2
                    && y >= LINK.1 - 20.0
                    && y <= LINK.1 + LINK.3 - 18.0
            };
            match input {
                PageInput::Pointer(event) if event.kind == PointerKind::Move && hover_links => {
                    let over = state.look == Look::Site
                        && on_link(event.x, event.y, state.scroll, state.zoom);
                    if over != state.hovering {
                        state.hovering = over;
                        self.queue.push(BrowserEvent::tab(
                            tab,
                            TabEvent::HoverLink(over.then(|| LINK_URL.to_string())),
                        ));
                    }
                }
                PageInput::Pointer(event)
                    if event.kind == PointerKind::Down && event.button == 2 && menus =>
                {
                    let link = (state.look == Look::Site
                        && on_link(event.x, event.y, state.scroll, state.zoom))
                    .then(|| (LINK_URL.to_string(), "More information".to_string()));
                    inner.next_request += 1;
                    self.queue.push(BrowserEvent::Request {
                        tab,
                        request: Request {
                            id: request_id,
                            kind: RequestKind::ContextMenu(ContextTarget {
                                position: (event.x as f32, event.y as f32),
                                link,
                                ..ContextTarget::default()
                            }),
                        },
                    });
                }
                PageInput::Wheel(event) => {
                    let step = match event.unit {
                        WheelUnit::Line => 40.0,
                        WheelUnit::Pixel => 1.0,
                    };
                    let max = (PAGE_HEIGHT - state.size.1 as f32 / state.zoom as f32).max(0.0);
                    state.scroll = (state.scroll + event.dy as f32 * step).clamp(0.0, max);
                    Self::redraw(state);
                }
                _ => {}
            }
        })
    }

    fn find(&self, tab: TabId, query: &FindQuery) -> Done {
        if !lock(&self.capabilities).find {
            return Err(BackendError::Unsupported("find in page"));
        }
        let query = query.clone();
        self.with_tab(tab, Call::Find(tab, query.clone()), |inner, tab| {
            let Some(state) = inner.tabs.get_mut(&tab) else {
                return;
            };
            // The double's pages hold three matches of any word of three letters or more.
            let total = if query.text.chars().count() >= 3 {
                3
            } else {
                0
            };
            let current = match (&state.find, total) {
                (_, 0) => 0,
                (Some((text, current)), _) if *text == query.text && !query.restart => {
                    let next = if query.forward {
                        current % total + 1
                    } else {
                        (current + total - 2) % total + 1
                    };
                    next
                }
                _ => 1,
            };
            state.find = Some((query.text.clone(), current));
            self.queue.push(BrowserEvent::tab(
                tab,
                TabEvent::FindResult { current, total },
            ));
        })
    }

    fn find_stop(&self, tab: TabId) -> Done {
        self.with_tab(tab, Call::FindStop(tab), |inner, tab| {
            if let Some(state) = inner.tabs.get_mut(&tab) {
                state.find = None;
            }
        })
    }

    fn print_pdf(&self, tab: TabId, file_name: &str) -> Done {
        self.with_tab(tab, Call::PrintPdf(tab, file_name.to_string()), |_, tab| {
            // The double has no PDF: it says the file is where a download would be.
            self.queue.push(BrowserEvent::tab(
                tab,
                TabEvent::Printed(Ok(PathBuf::from("/home/user/Downloads").join(file_name))),
            ));
        })
    }

    /// The toolbox is a session of its own, as in the engine: it has a surface
    /// and a size, it is not a tab of the strip, and it ends with its target.
    fn open_devtools(
        &self,
        target: TabId,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Toolbox, BackendError> {
        self.with_tab(target, Call::OpenDevtools(target), |_, _| ())?;
        if lock(&self.inner).toolboxes.contains_key(&target) {
            // The engine answers Busy to a second toolbox for one target.
            return Err(BackendError::Engine("VV_ERR_BUSY".to_string()));
        }
        let session = self.create_tab(&TabConfig {
            url: "about:devtools-toolbox".to_string(),
            width,
            height,
            scale,
            opener: Some(target),
            ..TabConfig::default()
        })?;
        lock(&self.inner).toolboxes.insert(target, session);
        Ok(Toolbox::new(session))
    }

    fn close_devtools(&self, toolbox: Toolbox) -> Done {
        let mut inner = lock(&self.inner);
        inner.calls.push(Call::CloseDevtools(toolbox));
        inner
            .toolboxes
            .retain(|_, session| *session != toolbox.session());
        inner.tabs.remove(&toolbox.session());
        self.queue.push(BrowserEvent::tab(
            toolbox.session(),
            TabEvent::Closed { error: None },
        ));
        Ok(())
    }

    fn clear_history(&self) -> Done {
        let mut inner = lock(&self.inner);
        inner.calls.push(Call::ClearHistory);
        if inner.failed {
            return Err(BackendError::EngineGone);
        }
        self.queue
            .push(BrowserEvent::Engine(EngineEvent::HistoryCleared(Ok(()))));
        Ok(())
    }

    fn set_muted(&self, tab: TabId, muted: bool) -> Done {
        if !lock(&self.capabilities).audio {
            return Err(BackendError::Unsupported("muting a tab"));
        }
        self.with_tab(tab, Call::SetMuted(tab, muted), |inner, tab| {
            if let Some(state) = inner.tabs.get_mut(&tab) {
                state.muted = muted;
            }
            self.queue.push(BrowserEvent::tab(
                tab,
                TabEvent::Audio(Audio {
                    playing: true,
                    muted,
                }),
            ));
        })
    }

    fn respond(&self, tab: TabId, request: RequestId, response: Response) -> Done {
        let answer = response.clone();
        self.with_tab(tab, Call::Respond(tab, request, response), |inner, tab| {
            if let Response::CertError { proceed: true } = answer {
                let blocked = inner.tabs.get_mut(&tab).and_then(|state| {
                    state
                        .blocked
                        .take_if(|(id, _)| *id == request)
                        .map(|(_, url)| url)
                });
                if let Some(url) = blocked {
                    // The person went ahead: the page loads, marked broken.
                    let host = sites::host_of(&url);
                    if let Some(state) = inner.tabs.get_mut(&tab) {
                        state.look = Look::Site;
                        state.hue = 0.0;
                        if let Some(entry) = state.history.get_mut(state.index) {
                            entry.title = host.clone();
                        }
                        Self::redraw(state);
                    }
                    self.queue
                        .push(BrowserEvent::tab(tab, TabEvent::Title(host)));
                    self.queue.push(BrowserEvent::tab(
                        tab,
                        TabEvent::Security(Security {
                            state: crate::web_view::page::SecurityState::Broken,
                            ..Security::default()
                        }),
                    ));
                    self.queue.push(BrowserEvent::tab(
                        tab,
                        TabEvent::Load {
                            started: false,
                            loading: false,
                            progress: 1.0,
                            error: None,
                        },
                    ));
                }
            }
        })
    }

    fn cancel_download(&self, download: DownloadId) -> Done {
        self.change_download(
            Call::CancelDownload(download),
            download,
            DownloadState::Cancelled,
        )
    }

    fn pause_download(&self, download: DownloadId) -> Done {
        self.change_download(
            Call::PauseDownload(download),
            download,
            DownloadState::Paused,
        )
    }

    fn resume_download(&self, download: DownloadId) -> Done {
        self.change_download(
            Call::ResumeDownload(download),
            download,
            DownloadState::Running,
        )
    }

    fn remove_download(&self, download: DownloadId) -> Done {
        let mut inner = lock(&self.inner);
        inner.calls.push(Call::RemoveDownload(download));
        if !lock(&self.capabilities).download_control {
            return Err(BackendError::Unsupported("downloads"));
        }
        // As the engine does, an id it does not hold is no error.
        inner.downloads.retain(|known| known.id != download);
        Ok(())
    }
}

impl TestDoubleBackend {
    fn change_download(&self, call: Call, id: DownloadId, state: DownloadState) -> Done {
        let mut inner = lock(&self.inner);
        inner.calls.push(call);
        if !lock(&self.capabilities).download_control {
            return Err(BackendError::Unsupported("downloads"));
        }
        let Some(download) = inner
            .downloads
            .iter_mut()
            .find(|download| download.id == id)
        else {
            return Err(BackendError::Engine(format!("no download {}", id.0)));
        };
        download.state = state;
        if state != DownloadState::Running {
            download.speed = 0;
        }
        let download = download.clone();
        drop(inner);
        self.queue.push(BrowserEvent::Download(download));
        Ok(())
    }
}

/// What a healthy engine reports, in the engine's own keys: two content
/// processes, the network process, and a graphics process that is not
/// sandboxed (as in Firefox 159).
pub fn healthy_report() -> SecurityReport {
    let entry = |key: &str, value: &str, status, pid: Option<u32>| ReportEntry {
        key: key.to_string(),
        value: value.to_string(),
        status,
        required: pid.is_some(),
        pid,
    };
    let mut entries = vec![
        entry("engine.gecko.version", "159.0", CheckStatus::Info, None),
        entry("sandbox.content.level", "6", CheckStatus::Ok, None),
        entry("sandbox.socket.level", "2", CheckStatus::Ok, None),
        entry("sandbox.rdd", "disabled", CheckStatus::Warn, None),
        entry(
            "sandbox.gpu",
            "none: no GPU process, compositor in the engine process",
            CheckStatus::Info,
            None,
        ),
        entry("fission.enabled", "true", CheckStatus::Ok, None),
        entry("profile.dir.mode", "0700", CheckStatus::Ok, None),
        entry("telemetry", "off", CheckStatus::Ok, None),
        entry("crash.upload", "off", CheckStatus::Ok, None),
    ];
    for (pid, kind) in [(4101, "content"), (4102, "content"), (4103, "socket")] {
        entries.push(entry(
            &format!("process.{pid}.type"),
            kind,
            CheckStatus::Ok,
            Some(pid),
        ));
        entries.push(entry(
            &format!("process.{pid}.seccomp"),
            "2",
            CheckStatus::Ok,
            Some(pid),
        ));
        entries.push(entry(
            &format!("process.{pid}.no_new_privs"),
            "1",
            CheckStatus::Ok,
            Some(pid),
        ));
        entries.push(entry(
            &format!("process.{pid}.cap_eff"),
            "0",
            CheckStatus::Ok,
            Some(pid),
        ));
    }
    SecurityReport::from_entries(CheckStatus::Warn, 1, &entries)
}

impl CertError {
    /// A certificate error for tests, for the host name given.
    pub fn for_test(host: &str) -> Self {
        match sites::resolve(&format!("https://{host}/"), 0) {
            Outcome::Cert(error) => error,
            _ => Self {
                host: host.to_string(),
                code: "SEC_ERROR_UNKNOWN_ISSUER".to_string(),
                problem: crate::web_view::requests::CertProblem::UnknownIssuer,
                issuer: None,
                fingerprint: None,
                valid_until: None,
                overridable: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web_view::input::{Mods, PointerEvent};

    fn drain(double: &TestDoubleBackend) -> Vec<BrowserEvent> {
        let mut out = Vec::new();
        double.poll_events(&mut out);
        out
    }

    #[test]
    fn a_load_reports_location_title_security_and_history_in_order() {
        let double = TestDoubleBackend::new();
        drain(&double);
        let tab = double
            .create_tab(&TabConfig {
                url: "example.com".to_string(),
                ..TabConfig::default()
            })
            .expect("a tab");
        let events = drain(&double);
        let kinds: Vec<&str> = events
            .iter()
            .map(|event| match event {
                BrowserEvent::Tab { event, .. } => match event {
                    TabEvent::Ready => "ready",
                    TabEvent::Load { started: true, .. } => "started",
                    TabEvent::Load { started: false, .. } => "stopped",
                    TabEvent::Location { .. } => "location",
                    TabEvent::History { .. } => "history",
                    TabEvent::Title(_) => "title",
                    TabEvent::Security(_) => "security",
                    TabEvent::Favicon(_) => "favicon",
                    _ => "other",
                },
                _ => "other",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "ready", "started", "location", "history", "title", "security", "favicon",
                "stopped", "history"
            ]
        );
        assert!(events.iter().any(|event| matches!(
            event,
            BrowserEvent::Tab { tab: t, event: TabEvent::Title(title) } if *t == tab && title == "Example Domain"
        )));
    }

    #[test]
    fn back_and_forward_walk_the_history_without_adding_to_it() {
        let double = TestDoubleBackend::new();
        let tab = double.create_tab(&TabConfig::default()).expect("a tab");
        double.load(tab, "example.com").expect("load");
        double.load(tab, "news.bingux.dev").expect("load");
        drain(&double);
        double.back(tab).expect("back");
        let events = drain(&double);
        let location = events.iter().find_map(|event| match event {
            BrowserEvent::Tab {
                event:
                    TabEvent::Location {
                        url,
                        can_back,
                        can_forward,
                    },
                ..
            } => Some((url.clone(), *can_back, *can_forward)),
            _ => None,
        });
        assert_eq!(
            location,
            Some(("https://example.com".to_string(), false, true))
        );
        double.forward(tab).expect("forward");
        let events = drain(&double);
        assert!(events.iter().any(|event| matches!(
            event,
            BrowserEvent::Tab { event: TabEvent::History { index: 1, entries }, .. } if entries.len() == 2
        )));
    }

    #[test]
    fn a_manual_double_holds_a_load_until_it_is_finished() {
        let double = TestDoubleBackend::manual();
        let tab = double.create_tab(&TabConfig::default()).expect("a tab");
        double.load(tab, "example.com").expect("load");
        let events = drain(&double);
        assert!(!events.iter().any(|event| matches!(
            event,
            BrowserEvent::Tab {
                event: TabEvent::Title(_),
                ..
            }
        )));
        double.finish_loads();
        assert!(drain(&double).iter().any(|event| matches!(
            event,
            BrowserEvent::Tab { event: TabEvent::Title(title), .. } if title == "Example Domain"
        )));
    }

    #[test]
    fn every_call_is_recorded_and_a_closed_tab_is_an_error() {
        let double = TestDoubleBackend::new();
        let tab = double.create_tab(&TabConfig::default()).expect("a tab");
        double.close_tab(tab).expect("close");
        assert_eq!(double.reload(tab, false), Err(BackendError::NoSuchTab(tab)));
        let calls = double.calls();
        assert!(matches!(calls[1], Call::CloseTab(t) if t == tab));
        assert!(matches!(calls[2], Call::Reload(t, false) if t == tab));
    }

    #[test]
    fn the_wheel_scrolls_the_drawn_page_and_a_hover_reports_the_link() {
        let double = TestDoubleBackend::new();
        let tab = double
            .create_tab(&TabConfig {
                url: "example.com".to_string(),
                width: 400,
                height: 300,
                ..TabConfig::default()
            })
            .expect("a tab");
        drain(&double);
        let surface = double.surface(tab).expect("a surface");
        let before = surface.stats().pushed;
        double
            .send(
                tab,
                PageInput::Wheel(crate::web_view::input::WheelEvent {
                    x: 10.0,
                    y: 10.0,
                    dx: 0.0,
                    dy: 3.0,
                    unit: WheelUnit::Line,
                    precise: false,
                    modifiers: Mods::default(),
                    phase: crate::frame_view::WheelPhase::None,
                }),
            )
            .expect("wheel");
        assert_eq!(
            surface.stats().pushed,
            before + 1,
            "the page was drawn again"
        );
        double
            .send(
                tab,
                PageInput::Pointer(PointerEvent {
                    kind: PointerKind::Move,
                    x: 60.0,
                    y: 300.0 - 120.0,
                    button: 0,
                    buttons: 0,
                    modifiers: Mods::default(),
                    click_count: 0,
                }),
            )
            .expect("move");
        assert!(drain(&double).iter().any(|event| matches!(
            event,
            BrowserEvent::Tab { event: TabEvent::HoverLink(Some(url)), .. } if url == LINK_URL
        )));
    }
}
