//! Runtime paths, bus names and account names. One module so that no other
//! crate hardcodes them.

/// System D-Bus well-known name and object path of the bsm service.
pub const BUS_NAME: &str = "org.bingux.Session1";
pub const BUS_PATH: &str = "/org/bingux/Session1";
pub const BUS_INTERFACE: &str = "org.bingux.Session1";

/// Directory owned by the `bsm` account. Mode 0755.
pub const RUNTIME_DIR: &str = "/run/bsm";
/// Greeter Wayland socket. Mode 0660, group `bingux-greeter`. Peer credentials
/// are checked on accept: uid must be the greeter account and the pid the
/// greeter process that bsm launched.
pub const GREETER_WAYLAND_SOCKET: &str = "/run/bsm/greeter-wayland.sock";
/// Greeter control socket (newline-delimited JSON, see `control`). Same access rules.
pub const GREETER_CONTROL_SOCKET: &str = "/run/bsm/greeter-control.sock";
/// Directory for per-session hosted Wayland sockets: `<dir>/<session-id>-<generation>.sock`.
/// Mode 0711; each socket is mode 0666 with a random name, but accept() verifies
/// uid, cgroup scope and `/proc/<pid>/exe` before the connection is promoted to a
/// session surface role.
pub const SESSION_SOCKET_DIR: &str = "/run/bsm/sessions";
/// Root-owned socket of `bsm-helper`, accepting only the `bsm` account. Mode 0600 owner `bsm`.
pub const HELPER_SOCKET: &str = "/run/bsm-helper/helper.sock";
/// State directory: last user, sanitised appearance cache, session registry metadata.
pub const STATE_DIR: &str = "/var/lib/bsm";
/// Published greeter appearance cache, one directory per uid. Written only by bsm.
pub const GREETER_CACHE_DIR: &str = "/var/lib/bingux/greeter";

/// System accounts created by sysusers.
pub const BSM_USER: &str = "bsm";
pub const GREETER_USER: &str = "bingux-greeter";

/// Inside a hosted user session (`$XDG_RUNTIME_DIR`-relative).
pub const SESSION_READY_SOCKET: &str = "bsm/ready.sock";

/// Environment exported into a hosted session by the helper.
pub const ENV_SESSION_ID: &str = "BSM_SESSION_ID";
pub const ENV_GENERATION: &str = "BSM_GENERATION";
pub const ENV_HOST_SOCKET: &str = "BSM_HOST_SOCKET";
