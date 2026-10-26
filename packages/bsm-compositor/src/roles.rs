//! Connection roles and the global visibility table (docs/bsm/architecture.md,
//! "Connection and role model"). A role is attached to a client by bsm itself when it
//! inserts the accepted stream; a client can neither name nor change its role.

use bsm_proto::state::{Generation, SessionId};
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

/// Per-client data: the role plus Smithay's compositor bookkeeping.
pub struct ClientState {
    pub role: Role,
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
            WlCompositor, WlSubcompositor, WlShm, WlSeat, WlOutput, XdgOutput, LayerShell,
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
