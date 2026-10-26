//! Hosted sessions: the registry record of one generation, the server side of
//! `bsm_session_host_v1`, and the per-output frame evidence the reveal gate relies on.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use bsm_proto::scene::{HostReport, OutputInfo, SceneEvent, SurfaceEvidence};
use bsm_proto::state::{Generation, InputOwner, SessionId};
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::reexports::calloop::RegistrationToken;
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use smithay::wayland::compositor::give_role;

use crate::engine::Plan;
use crate::protocol::bsm_session_host_v1::{self, BsmSessionHostV1, ReportKind};
use crate::protocol::bsm_session_surface_v1::{self, BsmSessionSurfaceV1};
use crate::roles::{GlobalKind, Role, client_may_see, role_of};
use crate::state::{PendingConn, State};

pub(crate) const SESSION_SURFACE_ROLE: &str = "bsm_session_surface";

pub(crate) struct HostedSurface {
    pub surface: WlSurface,
    pub res: BsmSessionSurfaceV1,
    /// Serial of the latest configure sent.
    pub serial: u32,
    pub width: u32,
    pub height: u32,
    /// The latest configure was acknowledged.
    pub acked: bool,
    /// The attached buffer has exactly the configured size (scale 1, no transform).
    pub valid: bool,
    /// Valid buffers committed after the acknowledgement.
    pub presented_frames: u64,
    /// `presented` events sent, used as the sequence number.
    pub seq: u32,
    /// A valid commit that has not yet been reported with `presented`.
    pub dirty: bool,
}

impl HostedSurface {
    /// True when this output has a frame that may be composed: acknowledged, sized
    /// exactly like the output, and presented at least once since.
    pub fn ready(&self, info: &OutputInfo) -> bool {
        self.acked
            && self.valid
            && self.presented_frames >= 1
            && self.width == info.width
            && self.height == info.height
    }
}

pub(crate) struct SessionRec {
    pub id: SessionId,
    pub generation: Generation,
    pub uid: u32,
    pub scope: String,
    pub host_pid: u32,
    pub host_exe: String,
    pub bound: bool,
    pub socket_path: PathBuf,
    pub listener: Option<RegistrationToken>,
    pub pending: Vec<PendingConn>,
    pub client: Option<ClientId>,
    pub connected: bool,
    pub hosts: Vec<BsmSessionHostV1>,
    pub surfaces: HashMap<String, HostedSurface>,
    pub rejections: u32,
    pub next_serial: u32,
    pub last_input_active: bool,
}

impl SessionRec {
    pub fn evidence(&self) -> Vec<SurfaceEvidence> {
        let mut v: Vec<_> = self
            .surfaces
            .iter()
            .map(|(name, s)| SurfaceEvidence {
                output: name.clone(),
                configure_serial: s.serial,
                acked: s.acked,
                presented_frames: s.presented_frames,
            })
            .collect();
        v.sort_by(|a, b| a.output.cmp(&b.output));
        v
    }
}

pub(crate) struct HostData {
    pub session: SessionId,
    pub generation: Generation,
}

pub(crate) struct SurfData {
    pub session: SessionId,
    pub generation: Generation,
    pub output: String,
}

impl GlobalDispatch<BsmSessionHostV1, ()> for State {
    fn bind(
        state: &mut State,
        _dh: &DisplayHandle,
        client: &Client,
        resource: New<BsmSessionHostV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        let Some(Role::HostedUser { session, generation, uid }) = role_of(client) else {
            // Cannot happen (the global is hidden from every other role); fail closed.
            let res = data_init.init(resource, HostData { session: String::new(), generation: 0 });
            res.closed();
            return;
        };
        let res = data_init.init(resource, HostData { session: session.clone(), generation });
        let Some(rec) = state.sessions.get_mut(&session).filter(|r| r.generation == generation) else {
            res.closed();
            return;
        };
        res.identity(session.clone(), generation.to_string(), uid);
        for o in &state.outputs {
            send_output(&res, &o.info, false);
        }
        let active = state.owner == InputOwner::Session(session.clone());
        res.input_state(active as u32);
        rec.last_input_active = active;
        rec.hosts.push(res);
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        client_may_see(&client, GlobalKind::SessionHost)
    }
}

fn send_output(res: &BsmSessionHostV1, i: &OutputInfo, changed: bool) {
    if changed {
        res.output_changed(
            i.name.clone(), i.x, i.y, i.width, i.height, i.scale_120, i.refresh_mhz, i.transform,
        );
    } else {
        res.output_added(
            i.name.clone(), i.x, i.y, i.width, i.height, i.scale_120, i.refresh_mhz, i.transform,
        );
    }
}

impl Dispatch<BsmSessionHostV1, HostData> for State {
    fn request(
        state: &mut State,
        client: &Client,
        resource: &BsmSessionHostV1,
        request: bsm_session_host_v1::Request,
        data: &HostData,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        // The role is re-read from the connection on every request: the resource data was
        // set by bsm, but nothing here trusts what the client says about itself.
        let current = match role_of(client) {
            Some(Role::HostedUser { session, generation, .. })
                if session == data.session && generation == data.generation =>
            {
                state.sessions.get(&session).is_some_and(|r| r.generation == generation)
            }
            _ => false,
        };
        match request {
            bsm_session_host_v1::Request::Destroy => {
                if let Some(r) = state.sessions.get_mut(&data.session) {
                    r.hosts.retain(|h| h != resource);
                }
            }
            bsm_session_host_v1::Request::GetSessionSurface { id, surface, output_name } => {
                if !current {
                    resource.post_error(bsm_session_host_v1::Error::StaleGeneration, "stale session");
                    return;
                }
                let Some(out) = state.outputs.iter().find(|o| o.info.name == output_name).map(|o| o.info.clone())
                else {
                    resource.post_error(bsm_session_host_v1::Error::InvalidOutput, "unknown output");
                    return;
                };
                let rec = state.sessions.get_mut(&data.session).expect("checked by current");
                if rec.surfaces.contains_key(&output_name) {
                    resource.post_error(
                        bsm_session_host_v1::Error::AlreadyConstructed,
                        "output already has a session surface",
                    );
                    return;
                }
                if give_role(&surface, SESSION_SURFACE_ROLE).is_err() {
                    resource.post_error(
                        bsm_session_host_v1::Error::AlreadyConstructed,
                        "surface already has a role",
                    );
                    return;
                }
                let res = data_init.init(
                    id,
                    SurfData {
                        session: data.session.clone(),
                        generation: data.generation,
                        output: output_name.clone(),
                    },
                );
                rec.next_serial = rec.next_serial.wrapping_add(1).max(1);
                let serial = rec.next_serial;
                res.configure(serial, out.width, out.height, out.scale_120, out.transform);
                rec.surfaces.insert(
                    output_name.clone(),
                    HostedSurface {
                        surface: surface.clone(),
                        res,
                        serial,
                        width: out.width,
                        height: out.height,
                        acked: false,
                        valid: false,
                        presented_frames: 0,
                        seq: 0,
                        dirty: false,
                    },
                );
                state.surface_index.insert(surface, (data.session.clone(), output_name));
                state.report_evidence(&data.session);
            }
            bsm_session_host_v1::Request::Report { kind, output_name, a, b, generation } => {
                if !current {
                    resource.post_error(bsm_session_host_v1::Error::StaleGeneration, "stale session");
                    return;
                }
                if generation.parse::<u64>().ok() != Some(data.generation) {
                    resource.post_error(
                        bsm_session_host_v1::Error::StaleGeneration,
                        "report carries another generation",
                    );
                    return;
                }
                let known_output = |n: &Option<String>| {
                    n.as_ref().is_some_and(|n| state.outputs.iter().any(|o| &o.info.name == n))
                };
                let report = match kind.into_result() {
                    Ok(ReportKind::GnoblinReady) => HostReport::GnoblinReady,
                    Ok(ReportKind::BinguxLaunched) => HostReport::BinguxLaunched,
                    Ok(ReportKind::DesktopReady) => HostReport::DesktopReady,
                    Ok(ReportKind::OutputConfigured) => {
                        if !known_output(&output_name) {
                            resource.post_error(bsm_session_host_v1::Error::InvalidOutput, "unknown output");
                            return;
                        }
                        HostReport::OutputConfigured { output: output_name.unwrap() }
                    }
                    Ok(ReportKind::SurfacesMapped) => {
                        if !known_output(&output_name) {
                            resource.post_error(bsm_session_host_v1::Error::InvalidOutput, "unknown output");
                            return;
                        }
                        if a > b || b == 0 {
                            resource.post_error(
                                bsm_session_host_v1::Error::InvalidReport,
                                "mapped count exceeds required count",
                            );
                            return;
                        }
                        HostReport::SurfacesMapped { output: output_name.unwrap(), mapped: a, required: b }
                    }
                    Err(_) => {
                        resource.post_error(bsm_session_host_v1::Error::InvalidReport, "unknown report kind");
                        return;
                    }
                };
                state.emit(SceneEvent::SessionReport {
                    session: data.session.clone(),
                    generation: data.generation,
                    report,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<BsmSessionSurfaceV1, SurfData> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        _resource: &BsmSessionSurfaceV1,
        request: bsm_session_surface_v1::Request,
        data: &SurfData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            bsm_session_surface_v1::Request::AckConfigure { serial } => {
                let Some(rec) = state.sessions.get_mut(&data.session).filter(|r| r.generation == data.generation)
                else {
                    return;
                };
                let Some(hs) = rec.surfaces.get_mut(&data.output) else { return };
                // Only the latest configure counts. An older serial leaves the surface
                // unacknowledged, and so unpresentable.
                if serial == hs.serial && !hs.acked {
                    hs.acked = true;
                    state.report_evidence(&data.session);
                }
            }
            bsm_session_surface_v1::Request::Destroy => {
                state.drop_hosted_surface(&data.session, data.generation, &data.output);
            }
            _ => {}
        }
    }

    fn destroyed(state: &mut State, _client: smithay::reexports::wayland_server::backend::ClientId, _resource: &BsmSessionSurfaceV1, data: &SurfData) {
        state.drop_hosted_surface(&data.session, data.generation, &data.output);
    }
}

impl State {
    pub(crate) fn report_evidence(&self, session: &SessionId) {
        if let Some(rec) = self.sessions.get(session) {
            self.emit(SceneEvent::SessionSurfaces {
                session: rec.id.clone(),
                generation: rec.generation,
                surfaces: rec.evidence(),
            });
        }
    }

    pub(crate) fn drop_hosted_surface(&mut self, session: &SessionId, generation: Generation, output: &str) {
        let Some(rec) = self.sessions.get_mut(session).filter(|r| r.generation == generation) else {
            return;
        };
        if let Some(hs) = rec.surfaces.remove(output) {
            self.surface_index.remove(&hs.surface);
            self.report_evidence(session);
        }
    }

    /// A commit on a hosted session surface. Counts evidence, never composes anything by
    /// itself.
    pub(crate) fn host_surface_commit(&mut self, root: &WlSurface) {
        let Some((sid, out)) = self.surface_index.get(root).cloned() else { return };
        let visible = self
            .last_plan
            .session
            .as_ref()
            .is_some_and(|l| l.key.0 == sid);
        let (has_buffer, size, scale, transform) = smithay::backend::renderer::utils::with_renderer_surface_state(
            root,
            |s| (s.buffer().is_some(), s.buffer_size(), s.buffer_scale(), s.buffer_transform()),
        )
        .unwrap_or((false, None, 1, smithay::utils::Transform::Normal));
        let Some(rec) = self.sessions.get_mut(&sid) else { return };
        let Some(hs) = rec.surfaces.get_mut(&out) else { return };
        let valid = has_buffer
            && scale == 1
            && transform == smithay::utils::Transform::Normal
            && size.is_some_and(|s| s.w as u32 == hs.width && s.h as u32 == hs.height);
        hs.valid = valid;
        if !(valid && hs.acked) {
            return;
        }
        let first = hs.presented_frames == 0;
        hs.presented_frames += 1;
        hs.dirty = true;
        if first {
            if !visible {
                // Ready-frame proof: reported without being shown.
                hs.seq = hs.seq.wrapping_add(1);
                hs.dirty = false;
                let (hi, lo, ns) = split_time(self.clock.now_ms());
                hs.res.presented(hs.seq, hi, lo, ns);
            }
            self.report_evidence(&sid);
        }
        self.needs_redraw = true;
    }

    /// After a frame was composed: frame callbacks and `presented` events for what was
    /// actually shown, throttled frame callbacks for what was not.
    pub(crate) fn after_render(&mut self, plan: &Plan, now_ms: u64) {
        let time = Duration::from_millis(now_ms);
        let (hi, lo, ns) = split_time(now_ms);
        let shown = plan.session.as_ref().map(|l| l.key.0.clone());
        for (sid, rec) in self.sessions.iter_mut() {
            let is_shown = shown.as_deref() == Some(sid.as_str());
            for (name, hs) in rec.surfaces.iter_mut() {
                let output = self.outputs.iter().find(|o| &o.info.name == name).map(|o| o.output.clone());
                if is_shown {
                    if hs.dirty {
                        hs.dirty = false;
                        hs.seq = hs.seq.wrapping_add(1);
                        hs.res.presented(hs.seq, hi, lo, ns);
                    }
                    if let Some(output) = output {
                        send_frames_surface_tree(&hs.surface, &output, time, Some(Duration::ZERO), |_, _| {
                            Some(output.clone())
                        });
                    }
                } else if let Some(output) = output {
                    // A session that is not on screen is throttled to a slow tick so it
                    // keeps serving its applications without rendering at full rate.
                    send_frames_surface_tree(&hs.surface, &output, time, Some(Duration::from_millis(250)), |_, _| {
                        Some(output.clone())
                    });
                }
            }
        }
        self.greeter_after_render(time);
    }

    pub(crate) fn host_output_added(&mut self, info: &OutputInfo) {
        for rec in self.sessions.values() {
            for h in &rec.hosts {
                send_output(h, info, false);
            }
        }
    }

    pub(crate) fn host_output_changed(&mut self, info: &OutputInfo) {
        let mut touched = Vec::new();
        for rec in self.sessions.values_mut() {
            for h in &rec.hosts {
                send_output(h, info, true);
            }
            if let Some(hs) = rec.surfaces.get_mut(&info.name) {
                // New geometry: a new configure, and everything proven so far is void.
                rec.next_serial = rec.next_serial.wrapping_add(1).max(1);
                hs.serial = rec.next_serial;
                hs.width = info.width;
                hs.height = info.height;
                hs.acked = false;
                hs.valid = false;
                hs.presented_frames = 0;
                hs.dirty = false;
                hs.res.configure(hs.serial, info.width, info.height, info.scale_120, info.transform);
                touched.push(rec.id.clone());
            }
        }
        for s in touched {
            self.report_evidence(&s);
        }
    }

    pub(crate) fn host_output_removed(&mut self, name: &str) {
        let mut touched = Vec::new();
        for rec in self.sessions.values_mut() {
            for h in &rec.hosts {
                h.output_removed(name.to_string());
            }
            if let Some(hs) = rec.surfaces.remove(name) {
                self.surface_index.remove(&hs.surface);
                touched.push(rec.id.clone());
            }
        }
        for s in touched {
            self.report_evidence(&s);
        }
    }

    /// Tell a session whether it receives physical input.
    pub(crate) fn host_input_state(&mut self, session: &SessionId, active: bool) {
        if let Some(rec) = self.sessions.get_mut(session) {
            if rec.last_input_active != active {
                rec.last_input_active = active;
                for h in &rec.hosts {
                    h.input_state(active as u32);
                }
            }
        }
    }

    /// Hand an accepted, verified stream to the Wayland display as this session's host.
    pub(crate) fn promote_host(&mut self, session: &SessionId, conn: PendingConn) -> bool {
        let Some(rec) = self.sessions.get(session) else { return false };
        let role = Role::HostedUser { session: rec.id.clone(), generation: rec.generation, uid: rec.uid };
        let (gen_, pid) = (rec.generation, conn.peer.pid);
        match self.insert_role_client(conn.stream, role) {
            Ok(client) => {
                let rec = self.sessions.get_mut(session).expect("present");
                rec.client = Some(client.id());
                rec.connected = true;
                self.emit(SceneEvent::SessionConnected { session: session.clone(), generation: gen_, pid });
                true
            }
            Err(e) => {
                log::warn!("insert hosted client failed: {e}");
                false
            }
        }
    }
}

pub(crate) fn split_time(ms: u64) -> (u32, u32, u32) {
    let secs = ms / 1000;
    let nsec = ((ms % 1000) * 1_000_000) as u32;
    ((secs >> 32) as u32, (secs & 0xffff_ffff) as u32, nsec)
}
