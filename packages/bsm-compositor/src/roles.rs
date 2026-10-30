//! Connection roles and the global visibility table (docs/bsm/architecture.md,
//! "Connection and role model"). A role is attached to a client by bsm itself when it
//! inserts the accepted stream; a client can neither name nor change its role.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use bsm_proto::state::{Generation, SessionId};
use parking_lot::Mutex;
use smithay::wayland::compositor::CompositorClientState;
use wayland_server::Client;
use wayland_server::backend::{ClientData, ClientId, DisconnectReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// The restricted Quickshell greeter.
    Greeter { generation: Generation },
    /// The bridge of one hosted session generation.
    HostedUser { session: SessionId, generation: Generation, uid: u32 },
}

/// Every global bsm creates. The table in [`may_see`] is the only place that decides
/// which role sees which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalKind {
    WlCompositor,
    WlSubcompositor,
    WlShm,
    WlSeat,
    WlOutput,
    XdgOutput,
    LayerShell,
    /// `xdg_wm_base`: Qt's Wayland platform plugin refuses to start without a shell
    /// integration. The greeter may create toplevels and popups, but they are accepted
    /// and never composed: only layer surfaces reach the screen.
    XdgShell,
    Viewporter,
    FractionalScale,
    LinuxDmabuf,
    Presentation,
    SessionHost,
}

/// The documented visibility table. `wl_subcompositor` is an addition to the table in
/// the architecture page: Qt (Quickshell) binds it, it creates no new capability and
/// the hosted role does not get it.
pub fn may_see(role: &Role, kind: GlobalKind) -> bool {
    use GlobalKind::*;
    match role {
        Role::Greeter { .. } => matches!(
            kind,
            WlCompositor
                | WlSubcompositor
                | WlShm
                | WlSeat
                | WlOutput
                | XdgOutput
                | LayerShell
                | XdgShell
                | Viewporter
                | FractionalScale
                | LinuxDmabuf
                | Presentation
        ),
        Role::HostedUser { .. } => {
            matches!(kind, WlCompositor | WlShm | WlSeat | WlOutput | LinuxDmabuf | SessionHost)
        }
    }
}

/// Requests one client may have handled in a single pass of the event loop. The Wayland
/// backend dispatches a client until its socket is empty, so a client that writes faster
/// than we parse would never hand control back; past this many requests it is disconnected.
pub const MAX_REQUESTS_PER_PASS: u64 = 50_000;
/// Live `wl_surface`s per client.
pub const MAX_SURFACES: i64 = 512;
/// `bsm_session_host_v1` objects per connection.
pub const MAX_HOST_BINDS: usize = 4;

/// What a request costs against a client's budget.
#[derive(Clone, Copy)]
pub enum Cost {
    /// Any request: counts against the per-pass limit.
    Pass,
    /// A request that reaches the manager or creates scene state (`report`,
    /// `get_session_surface`, `ack_configure`, registry binds): also a token bucket,
    /// 64 burst, 20 per second sustained.
    Control,
    /// A buffer commit: token bucket, 2000 burst, 1000 per second sustained (several outputs
    /// at a high refresh rate fit comfortably).
    Commit,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl Bucket {
    fn take(&mut self, capacity: f64, per_sec: f64) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * per_sec).min(capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Per-client resource accounting (review F3, F4, F11).
pub struct ClientBudget {
    limit: u64,
    pass: AtomicU64,
    surfaces: AtomicI64,
    control: Mutex<Bucket>,
    commits: Mutex<Bucket>,
}

impl Default for ClientBudget {
    fn default() -> Self {
        Self::new(MAX_REQUESTS_PER_PASS)
    }
}

impl ClientBudget {
    pub fn new(limit: u64) -> Self {
        let now = Instant::now();
        Self {
            limit,
            pass: AtomicU64::new(0),
            surfaces: AtomicI64::new(0),
            control: Mutex::new(Bucket { tokens: 64.0, last: now }),
            commits: Mutex::new(Bucket { tokens: 2000.0, last: now }),
        }
    }

    /// A new pass of the event loop starts.
    pub fn new_pass(&self) {
        self.pass.store(0, Ordering::Relaxed);
    }

    /// Charge one request. False means the client is over budget and must go.
    pub fn charge(&self, cost: Cost) -> bool {
        if self.pass.fetch_add(1, Ordering::Relaxed) + 1 > self.limit {
            return false;
        }
        match cost {
            Cost::Pass => true,
            Cost::Control => self.control.lock().take(64.0, 20.0),
            Cost::Commit => self.commits.lock().take(2000.0, 1000.0),
        }
    }

    /// One more live surface; false when over the cap.
    pub fn surface_created(&self) -> bool {
        self.surfaces.fetch_add(1, Ordering::Relaxed) + 1 <= MAX_SURFACES
    }

    pub fn surface_destroyed(&self) {
        self.surfaces.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Per-client data: the role plus Smithay's compositor bookkeeping.
pub struct ClientState {
    pub role: Role,
    pub budget: std::sync::Arc<ClientBudget>,
    pub compositor: CompositorClientState,
    /// Notified when the client goes away so the state can react (greeter crash,
    /// session host gone). Runs on the compositor thread through the loop.
    pub on_disconnect: Box<dyn Fn(ClientId, &Role) + Send + Sync>,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, client_id: ClientId, _reason: DisconnectReason) {
        (self.on_disconnect)(client_id, &self.role);
    }
}

/// The budget of a client, if bsm inserted it.
pub fn budget_of(client: &Client) -> Option<std::sync::Arc<ClientBudget>> {
    client.get_data::<ClientState>().map(|d| d.budget.clone())
}

/// The role of a client, or `None` for anything bsm did not insert itself.
pub fn role_of(client: &Client) -> Option<Role> {
    client.get_data::<ClientState>().map(|d| d.role.clone())
}

/// Role check used by every `can_view`: unknown clients see nothing.
pub fn client_may_see(client: &Client, kind: GlobalKind) -> bool {
    role_of(client).is_some_and(|r| may_see(&r, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn greeter() -> Role {
        Role::Greeter { generation: 1 }
    }
    fn hosted() -> Role {
        Role::HostedUser { session: "s00000001".into(), generation: 1, uid: 1000 }
    }

    #[test]
    fn hosted_sees_exactly_the_documented_set() {
        use GlobalKind::*;
        let seen: Vec<_> = [
            WlCompositor, WlSubcompositor, WlShm, WlSeat, WlOutput, XdgOutput, LayerShell, XdgShell,
            Viewporter, FractionalScale, LinuxDmabuf, Presentation, SessionHost,
        ]
        .into_iter()
        .filter(|k| may_see(&hosted(), *k))
        .collect();
        assert_eq!(seen, vec![WlCompositor, WlShm, WlSeat, WlOutput, LinuxDmabuf, SessionHost]);
    }

    #[test]
    fn greeter_never_sees_the_session_host_global() {
        assert!(!may_see(&greeter(), GlobalKind::SessionHost));
        assert!(may_see(&greeter(), GlobalKind::LayerShell));
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn a_pass_allows_exactly_the_limit_and_the_next_pass_starts_fresh() {
        let b = ClientBudget::new(10);
        assert!((0..10).all(|_| b.charge(Cost::Pass)));
        assert!(!b.charge(Cost::Pass), "the 11th request of the pass is over budget");
        b.new_pass();
        assert!(b.charge(Cost::Pass));
    }

    #[test]
    fn the_control_bucket_allows_a_burst_then_the_sustained_rate() {
        let b = ClientBudget::new(u64::MAX);
        let ok = (0..200).filter(|_| b.charge(Cost::Control)).count();
        assert!((64..=66).contains(&ok), "burst of 64 then nothing (got {ok})");
    }

    #[test]
    fn surfaces_are_capped() {
        let b = ClientBudget::default();
        assert!((0..MAX_SURFACES).all(|_| b.surface_created()));
        assert!(!b.surface_created());
        b.surface_destroyed();
        b.surface_destroyed();
        assert!(b.surface_created());
    }
}
