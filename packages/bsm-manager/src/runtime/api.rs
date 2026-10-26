//! Types shared by the runtime's workers and the traits that connect it to the
//! system. Real implementations live in this crate (`system`, `dbus`) and in the
//! `bsm` binary (accounts and appearance, which call `bsm-appearance`); tests
//! provide their own doubles.

use std::sync::mpsc::SyncSender;

use bsm_proto::error::BsmError;
use bsm_proto::scene::SceneCommand;
use bsm_proto::state::PowerAction;

use crate::core::{AccountEntry, Caller, Inhibitor, Input, Method, PublishRequest, PublishedView, Reply};

/// Everything the driver thread receives.
pub enum RtInput {
    Core(Input),
    /// A D-Bus method call. The reply goes back through `reply`; the D-Bus task bounds
    /// its own wait, the driver bounds it with `CALL_DEADLINE` as well.
    Call(DbusCall),
    /// Stop the driver (SIGTERM). Sessions are not closed: bsm restarting must not end them.
    Shutdown,
}

pub struct DbusCall {
    pub caller: Caller,
    pub method: Method,
    pub reply: async_channel::Sender<Result<Reply, BsmError>>,
}

/// Sends commands to the compositor. Returns false when the compositor is gone.
pub type SceneSink = Box<dyn Fn(SceneCommand) -> bool + Send>;

/// Handle workers use to feed the driver. Bounded: a stuck driver pushes back instead
/// of growing without limit.
pub type DriverTx = SyncSender<RtInput>;

/// Eligible accounts as bsm-appearance lists them, with their published appearance.
pub trait AccountSource: Send + Sync {
    fn list(&self) -> Result<Vec<AccountEntry>, BsmError>;
}

/// Validates, converts and publishes an appearance record. The request carries the
/// uid from the bus; the file descriptors were opened by that user.
pub trait AppearanceBackend: Send + Sync {
    fn publish(&self, uid: u32, request: PublishRequest) -> Result<PublishedView, BsmError>;
}

/// Authorisation and system actions: polkit, logind inhibitors, logind power calls.
pub trait SystemServices: Send + Sync {
    /// polkit `CheckAuthorization` for the D-Bus caller, no interaction.
    fn polkit_check(&self, caller: &Caller, action_id: &str) -> Result<bool, BsmError>;
    /// logind `ListInhibitors`.
    fn list_inhibitors(&self) -> Result<Vec<Inhibitor>, BsmError>;
    /// logind `Suspend`, `Reboot` or `PowerOff` (non-interactive). Returns on acceptance.
    fn power(&self, action: PowerAction) -> Result<(), BsmError>;
    /// All sessions are protected: release the sleep delay inhibitor. The implementation
    /// takes a new one after the system resumed.
    fn sleep_ready(&self);
}
