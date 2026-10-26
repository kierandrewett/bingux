//! Brokered connections. No shared Wayland socket exists: the greeter and each hosted
//! session have their own, and a stream only reaches the Wayland display after its
//! peer credentials were checked against what the manager registered. Everything that
//! fails the check is closed before any protocol byte is read.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use bsm_proto::scene::SceneEvent;
use bsm_proto::state::{Generation, SessionId};
use smithay::desktop::layer_map_for_output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, Mode as PollMode, PostAction};
use smithay::reexports::wayland_server::backend::{ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Client;
use smithay::wayland::compositor::{CompositorClientState, with_states};
use smithay::wayland::shell::wlr_layer::LayerSurfaceData;

use crate::host::SessionRec;
use crate::peer::{Expectation, peer_of, verify_greeter, verify_host};
use crate::roles::{ClientState, Role};
use crate::state::{GreeterAuth, Internal, PendingConn, State};

/// How long an unbound connection may wait for its registration.
const PENDING_TTL: Duration = Duration::from_secs(10);
const MAX_PENDING: usize = 4;
const MAX_REJECTION_EVENTS: u32 = 32;

pub(crate) struct RuntimeCfg {
    pub greeter_socket: PathBuf,
    pub greeter_socket_mode: u32,
    pub greeter_socket_gid: Option<u32>,
    pub session_socket_dir: PathBuf,
}

impl RuntimeCfg {
    pub fn from_config(c: &crate::Config) -> Self {
        Self {
            greeter_socket: c.greeter_socket.clone(),
            greeter_socket_mode: c.greeter_socket_mode,
            greeter_socket_gid: c.greeter_socket_gid,
            session_socket_dir: c.session_socket_dir.clone(),
        }
    }
}

fn bind_socket(path: &Path, mode: u32, gid: Option<u32>) -> Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    // A stale socket from a previous run is replaced; a live one belongs to another bsm.
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            anyhow::bail!("{} is already in use", path.display());
        }
        std::fs::remove_file(path).with_context(|| format!("remove stale {}", path.display()))?;
    }
    let l = UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("chmod {}", path.display()))?;
    if let Some(gid) = gid {
        rustix::fs::chown(path, None, Some(rustix::fs::Gid::from_raw(gid)))
            .with_context(|| format!("chown {}", path.display()))?;
    }
    l.set_nonblocking(true)?;
    Ok(l)
}

pub(crate) fn start_greeter_listener(state: &mut State) -> Result<()> {
    let l = bind_socket(
        &state.cfg.greeter_socket,
        state.cfg.greeter_socket_mode,
        state.cfg.greeter_socket_gid,
    )?;
    let token = state
        .handle
        .insert_source(Generic::new(l, Interest::READ, PollMode::Level), |_, l, state| {
            loop {
                match l.accept() {
                    Ok((stream, _)) => state.greeter_accept(stream),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        log::warn!("greeter accept: {e}");
                        break;
                    }
                }
            }
            Ok(PostAction::Continue)
        })
        .map_err(|e| anyhow::anyhow!("greeter listener: {e}"))?;
    state.greeter_listener = Some(token);
    Ok(())
}

impl State {
    /// Insert a verified stream as a Wayland client with exactly this role.
    pub(crate) fn insert_role_client(&mut self, stream: UnixStream, role: Role) -> std::io::Result<Client> {
        let tx = self.internal_tx.clone();
        let data = ClientState {
            role,
            compositor: CompositorClientState::default(),
            on_disconnect: Box::new(move |id, role| {
                let _ = tx.send(Internal::ClientGone { id, role: role.clone() });
            }),
        };
        self.dh.insert_client(stream, Arc::new(data))
    }

    // ----- greeter -----

    fn greeter_accept(&mut self, stream: UnixStream) {
        let _ = stream.set_nonblocking(true);
        let peer = match peer_of(&stream) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("greeter peer credentials unavailable: {e}");
                return;
            }
        };
        let conn = PendingConn { stream, peer, since: Instant::now() };
        if self.greeter.auth.is_some() {
            self.try_promote_greeter(conn);
        } else if self.greeter.pending.len() < MAX_PENDING {
            // The helper reports the greeter pid to the manager, which registers it here.
            // The process can connect first; hold it outside the display until then.
            self.greeter.pending.push(conn);
        }
    }

    fn try_promote_greeter(&mut self, conn: PendingConn) {
        let Some(auth) = self.greeter.auth.as_ref() else { return };
        let (generation, uid, pid) = (auth.generation, auth.uid, auth.pid);
        if let Err(why) = verify_greeter(&conn.peer, uid, pid) {
            log::warn!("greeter connection rejected: {why}");
            return;
        }
        if self.greeter.client.is_some() {
            log::warn!("second greeter connection rejected: one connection per generation");
            return;
        }
        match self.insert_role_client(conn.stream, Role::Greeter { generation }) {
            Ok(c) => {
                self.greeter.client = Some(c.id());
                self.greeter.connected = Some((generation, conn.peer.pid));
                self.emit(SceneEvent::GreeterConnected { generation, pid: conn.peer.pid });
                self.needs_redraw = true;
            }
            Err(e) => log::warn!("insert greeter client: {e}"),
        }
    }

    fn retry_pending_greeters(&mut self) {
        let now = Instant::now();
        let pending = std::mem::take(&mut self.greeter.pending);
        for c in pending {
            if now.duration_since(c.since) > PENDING_TTL {
                continue;
            }
            if self.greeter.auth.is_some() {
                self.try_promote_greeter(c);
            } else {
                self.greeter.pending.push(c);
            }
        }
    }

    pub(crate) fn register_greeter(&mut self, generation: Generation, uid: u32, pid: u32) {
        // A new generation replaces the previous greeter, which is disconnected.
        if let Some((g, _)) = self.greeter.connected {
            if g != generation {
                self.kill_greeter_client();
            }
        }
        self.greeter.auth = Some(GreeterAuth { generation, uid, pid });
        self.retry_pending_greeters();
    }

    pub(crate) fn unregister_greeter(&mut self, generation: Generation) {
        if self.greeter.auth.as_ref().is_some_and(|a| a.generation == generation) {
            self.greeter.auth = None;
            if self.greeter.connected.is_some_and(|(g, _)| g == generation) {
                self.kill_greeter_client();
            }
        }
    }

    fn kill_greeter_client(&mut self) {
        if let Some(id) = self.greeter.client.clone() {
            self.dh
                .backend_handle()
                .kill_client(id, DisconnectReason::ConnectionClosed);
        }
    }

    // ----- sessions -----

    pub(crate) fn register_session(
        &mut self,
        session: SessionId,
        generation: Generation,
        uid: u32,
        scope: String,
        host_pid: u32,
        host_exe: String,
    ) {
        if let Some(old) = self.sessions.get(&session) {
            if old.generation == generation {
                // Idempotent: tell the manager where the socket is again.
                self.emit(SceneEvent::SessionSocketReady {
                    session,
                    generation,
                    path: old.socket_path.display().to_string(),
                });
                return;
            }
            let g = old.generation;
            self.unregister_session(&session, g);
        }
        let path = self
            .cfg
            .session_socket_dir
            .join(format!("{session}-{generation}.sock"));
        let listener = match bind_socket(&path, 0o666, None) {
            Ok(l) => l,
            Err(e) => {
                self.emit(SceneEvent::SessionConnectionRejected {
                    session,
                    generation,
                    reason: format!("cannot create the session socket: {e:#}"),
                });
                return;
            }
        };
        let (sid, gen_) = (session.clone(), generation);
        let token = self
            .handle
            .insert_source(Generic::new(listener, Interest::READ, PollMode::Level), move |_, l, state| {
                loop {
                    match l.accept() {
                        Ok((stream, _)) => state.session_accept(&sid, gen_, stream),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => {
                            log::warn!("session accept: {e}");
                            break;
                        }
                    }
                }
                Ok(PostAction::Continue)
            })
            .ok();
        let bound = host_pid != 0 && !scope.is_empty();
        self.sessions.insert(
            session.clone(),
            SessionRec {
                id: session.clone(),
                generation,
                uid,
                scope,
                host_pid,
                host_exe,
                bound,
                socket_path: path.clone(),
                listener: token,
                pending: Vec::new(),
                client: None,
                connected: false,
                hosts: Vec::new(),
                surfaces: HashMap::new(),
                rejections: 0,
                next_serial: 0,
                last_input_active: false,
            },
        );
        self.emit(SceneEvent::SessionSocketReady {
            session,
            generation,
            path: path.display().to_string(),
        });
    }

    pub(crate) fn bind_host(&mut self, session: &SessionId, generation: Generation, scope: String, host_pid: u32) {
        let Some(rec) = self.sessions.get_mut(session).filter(|r| r.generation == generation) else {
            return;
        };
        if rec.bound || rec.connected || host_pid == 0 || scope.is_empty() {
            // Bound once; a second binding or a degenerate one changes nothing.
            return;
        }
        rec.scope = scope;
        rec.host_pid = host_pid;
        rec.bound = true;
        let pending = std::mem::take(&mut rec.pending);
        for c in pending {
            self.verify_and_promote(session, c);
        }
    }

    fn session_accept(&mut self, session: &SessionId, generation: Generation, stream: UnixStream) {
        let _ = stream.set_nonblocking(true);
        let Some(rec) = self.sessions.get(session).filter(|r| r.generation == generation) else {
            return;
        };
        if rec.connected {
            // Single use; the listener is normally gone by now, this covers a race.
            return;
        }
        let peer = match peer_of(&stream) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("session peer credentials unavailable: {e}");
                return;
            }
        };
        if let Err(why) = crate::peer::check_uid(&peer, rec.uid) {
            self.reject_session(session, why);
            return;
        }
        let conn = PendingConn { stream, peer, since: Instant::now() };
        if rec.bound {
            self.verify_and_promote(session, conn);
        } else {
            let rec = self.sessions.get_mut(session).expect("present");
            if rec.pending.len() < MAX_PENDING {
                rec.pending.push(conn);
            }
        }
    }

    fn verify_and_promote(&mut self, session: &SessionId, conn: PendingConn) {
        let Some(rec) = self.sessions.get(session) else { return };
        let want = Expectation {
            uid: rec.uid,
            pid: rec.host_pid,
            scope: rec.scope.clone(),
            exe: rec.host_exe.clone(),
        };
        if let Err(why) = verify_host(&conn.peer, &want) {
            self.reject_session(session, why);
            return; // `conn` drops here and closes the stream
        }
        if self.promote_host(session, conn) {
            // Single use: the socket goes away the moment the host is in.
            self.close_session_listener(session);
        }
    }

    fn reject_session(&mut self, session: &SessionId, reason: String) {
        let Some(rec) = self.sessions.get_mut(session) else { return };
        rec.rejections += 1;
        log::warn!("session {session}: connection rejected: {reason}");
        if rec.rejections <= MAX_REJECTION_EVENTS {
            let generation = rec.generation;
            self.emit(SceneEvent::SessionConnectionRejected {
                session: session.clone(),
                generation,
                reason,
            });
        }
    }

    fn close_session_listener(&mut self, session: &SessionId) {
        if let Some(rec) = self.sessions.get_mut(session) {
            if let Some(t) = rec.listener.take() {
                self.handle.remove(t);
            }
            let _ = std::fs::remove_file(&rec.socket_path);
            rec.pending.clear();
        }
    }

    pub(crate) fn unregister_session(&mut self, session: &SessionId, generation: Generation) {
        if !self.sessions.get(session).is_some_and(|r| r.generation == generation) {
            return;
        }
        self.close_session_listener(session);
        let Some(rec) = self.sessions.remove(session) else { return };
        for h in &rec.hosts {
            h.closed();
        }
        for hs in rec.surfaces.values() {
            self.surface_index.remove(&hs.surface);
        }
        if let Some(id) = rec.client.clone() {
            self.dh
                .backend_handle()
                .kill_client(id, DisconnectReason::ConnectionClosed);
        }
        let key = (session.clone(), generation);
        self.engine.unregister_session(&key);
        if matches!(&self.owner, bsm_proto::state::InputOwner::Session(s) if s == session) {
            self.set_owner_now(bsm_proto::state::InputOwner::None);
        }
        self.needs_redraw = true;
    }

    // ----- disconnects -----

    pub(crate) fn client_gone(&mut self, id: ClientId, role: Role) {
        match role {
            Role::Greeter { generation } => {
                if self.greeter.client.as_ref() == Some(&id) {
                    self.greeter.client = None;
                    self.greeter.connected = None;
                    self.greeter_kbd_focus = None;
                    // A greeter that vanished cannot be re-used: the next one must be
                    // registered again by the manager (new generation, new pid).
                    if self.greeter.auth.as_ref().is_some_and(|a| a.generation == generation) {
                        self.greeter.auth = None;
                    }
                    self.emit(SceneEvent::GreeterGone { generation });
                    self.refresh_focus();
                    self.needs_redraw = true;
                }
            }
            Role::HostedUser { session, generation, .. } => {
                let matches = self
                    .sessions
                    .get(&session)
                    .is_some_and(|r| r.generation == generation && r.client.as_ref() == Some(&id));
                if matches {
                    let rec = self.sessions.get_mut(&session).expect("present");
                    rec.connected = false;
                    rec.client = None;
                    rec.hosts.clear();
                    let surfaces: Vec<_> = rec.surfaces.drain().map(|(_, s)| s.surface).collect();
                    for s in surfaces {
                        self.surface_index.remove(&s);
                    }
                    self.emit(SceneEvent::SessionGone { session: session.clone(), generation });
                    self.refresh_focus();
                    self.needs_redraw = true;
                }
            }
        }
    }

    // ----- greeter surfaces -----

    /// A commit on something that is not a hosted surface: a greeter layer surface.
    pub(crate) fn greeter_surface_commit(&mut self, root: &WlSurface) {
        let Some(output) = self.output_of_layer_surface(root) else { return };
        let initial_sent = with_states(root, |states| {
            states
                .data_map
                .get::<LayerSurfaceData>()
                .map(|d| d.lock().unwrap().initial_configure_sent)
        })
        .unwrap_or(true);
        let mut map = layer_map_for_output(&output);
        map.arrange();
        if !initial_sent {
            if let Some(l) = map.layer_for_surface(root, smithay::desktop::WindowSurfaceType::TOPLEVEL) {
                l.layer_surface().send_configure();
            }
        }
        drop(map);
        self.refresh_focus();
    }

    pub(crate) fn greeter_after_render(&mut self, time: Duration) {
        for rec in &self.outputs {
            let map = layer_map_for_output(&rec.output);
            for l in map.layers() {
                l.send_frame(&rec.output, time, Some(Duration::ZERO), |_, _| Some(rec.output.clone()));
            }
        }
    }

    /// Drop connections that waited too long for their registration.
    pub(crate) fn expire_pending(&mut self) {
        let now = Instant::now();
        self.greeter.pending.retain(|c| now.duration_since(c.since) <= PENDING_TTL);
        for rec in self.sessions.values_mut() {
            rec.pending.retain(|c| now.duration_since(c.since) <= PENDING_TTL);
        }
    }
}
