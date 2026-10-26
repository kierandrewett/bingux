//! The scene and transition engine. Pure logic: no Wayland, no GPU, no clock of
//! its own. The compositor asks it for a [`Plan`] before every rendered frame and
//! tells it when that frame was composed ([`Engine::frame_done`]).
//!
//! What the engine guarantees (and the unit tests below assert):
//!
//! * A session is only ever part of a plan when it is registered, presentable (every
//!   output has a frame presented after its configure was acknowledged) and not
//!   protected. A protected session is never in a plan, whatever else happens.
//! * `Protected` is released only by [`Engine::frame_done`], i.e. after a frame was
//!   planned and composed without the session.
//! * Transitions validate source and target before the first frame and on every
//!   frame. A failure restores the safe scene (opaque cover plus greeter, never a
//!   desktop buffer) and reports `Failed`.
//! * Unlock reveals the session only in the area the greeter has vacated, so no
//!   session pixel can appear while the greeter still covers an output.

use std::collections::HashSet;

use bsm_proto::scene::{SceneEvent, TransitionPhaseEvent};
use bsm_proto::state::{Generation, OperationId, PowerAction, SessionId, TransitionKind};
use bsm_proto::transition::{progress, trajectory};

pub type SessionKey = (SessionId, Generation);

/// What the engine needs to know about the world to validate a frame.
pub trait Probe {
    /// The greeter is connected and has a mapped surface on every output.
    fn greeter_presentable(&self) -> bool;
    /// The session is registered (the registration has not been dropped).
    fn session_exists(&self, key: &SessionKey) -> bool;
    /// The session's host is connected and every current output has a frame that was
    /// presented after its configure was acknowledged. Says nothing about protection.
    fn session_presentable(&self, key: &SessionKey) -> bool;
}

/// The scene when no transition is running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// Opaque cover colour only.
    Cover,
    /// Cover colour and the greeter scene.
    Greeter,
    /// Cover colour and one hosted session.
    Session(SessionKey),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GreeterLayer {
    /// Whole-scene vertical offset in output heights: 0 in place, -1 fully off.
    pub offset_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionLayer {
    pub key: SessionKey,
    pub opacity: f32,
    /// Draw the session only in the area the greeter scene has vacated.
    pub only_vacated: bool,
}

/// What to draw for one frame, identical for every output.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub greeter: Option<GreeterLayer>,
    pub session: Option<SessionLayer>,
    pub session_above_greeter: bool,
    /// Cover colour drawn over everything at this alpha (group fade to cover).
    pub cover_overlay: f32,
    /// Operation and progress when this frame belongs to a transition.
    pub transition: Option<(OperationId, f32)>,
}

impl Plan {
    pub fn empty() -> Self {
        Self {
            greeter: None,
            session: None,
            session_above_greeter: false,
            cover_overlay: 0.0,
            transition: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunPhase {
    /// Begun and validated; no frame planned yet.
    Prepared,
    /// Frames are being planned.
    Animating,
    /// Final frame composed and the scene swapped; one more frame confirms it.
    Settling,
}

#[derive(Debug, Clone)]
struct Running {
    op: OperationId,
    kind: TransitionKind,
    source: Option<SessionKey>,
    target: Option<SessionKey>,
    duration_ms: u32,
    reduce_motion: bool,
    #[allow(dead_code)]
    power: Option<PowerAction>,
    start_ms: Option<u64>,
    snapshot_base: Base,
    target_was_protected: bool,
    phase: RunPhase,
    /// Progress of the frame planned last.
    last_t: f32,
}

#[derive(Debug)]
pub struct Engine {
    base: Base,
    protected: HashSet<SessionKey>,
    running: Option<Running>,
    /// Sessions protected since the last composed frame.
    pending_protected: Vec<SessionKey>,
    out: Vec<SceneEvent>,
}

fn needs_greeter(kind: TransitionKind) -> bool {
    !matches!(kind, TransitionKind::PowerFade)
}

fn is_lock_family(kind: TransitionKind) -> bool {
    matches!(
        kind,
        TransitionKind::Lock | TransitionKind::SwitchUser | TransitionKind::Suspend
    )
}

fn end_scene(kind: TransitionKind, target: &Option<SessionKey>) -> Base {
    match kind {
        TransitionKind::NewLoginFade | TransitionKind::UnlockSlideUp => match target {
            Some(t) => Base::Session(t.clone()),
            None => Base::Greeter,
        },
        TransitionKind::Lock
        | TransitionKind::SwitchUser
        | TransitionKind::Suspend
        | TransitionKind::LogoutFade => Base::Greeter,
        TransitionKind::PowerFade => Base::Cover,
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// A fresh engine shows the opaque cover only.
    pub fn new() -> Self {
        Self {
            base: Base::Cover,
            protected: HashSet::new(),
            running: None,
            pending_protected: Vec::new(),
            out: Vec::new(),
        }
    }

    pub fn base(&self) -> &Base {
        &self.base
    }

    pub fn is_protected(&self, key: &SessionKey) -> bool {
        self.protected.contains(key)
    }

    pub fn transition_running(&self) -> bool {
        self.running.is_some()
    }

    pub fn running_operation(&self) -> Option<OperationId> {
        self.running.as_ref().map(|r| r.op)
    }

    /// Sessions the running transition will reveal and that therefore must not receive
    /// input yet.
    pub fn transition_target(&self) -> Option<&SessionKey> {
        self.running
            .as_ref()
            .filter(|r| r.phase != RunPhase::Settling)
            .and_then(|r| r.target.as_ref())
    }

    /// The session that is the committed, fully visible foreground: the only session
    /// that may receive input.
    pub fn foreground_session(&self) -> Option<&SessionKey> {
        let committed = match &self.running {
            None => true,
            Some(r) => r.phase == RunPhase::Settling,
        };
        match &self.base {
            Base::Session(k) if committed && !self.protected.contains(k) => Some(k),
            _ => None,
        }
    }

    pub fn drain_events(&mut self) -> Vec<SceneEvent> {
        std::mem::take(&mut self.out)
    }

    fn phase_event(&mut self, op: OperationId, phase: TransitionPhaseEvent) {
        self.out.push(SceneEvent::TransitionPhase { operation: op, phase });
    }

    /// The session left every output and capture. Replies `Protected` once a frame
    /// without it was composed.
    pub fn protect(&mut self, key: &SessionKey) {
        self.protected.insert(key.clone());
        if self.base == Base::Session(key.clone()) {
            self.base = Base::Cover;
        }
        if !self.pending_protected.contains(key) {
            self.pending_protected.push(key.clone());
        }
    }

    pub fn unregister_session(&mut self, key: &SessionKey) {
        self.protected.remove(key);
        self.pending_protected.retain(|k| k != key);
        if self.base == Base::Session(key.clone()) {
            self.base = Base::Cover;
        }
        // A running transition that involves the session fails on its next frame
        // (`session_exists` is false then) and restores the safe scene.
    }

    /// Instant greeter-only scene. A running transition is cancelled.
    pub fn show_greeter_only(&mut self) {
        if let Some(r) = self.running.take() {
            let phase = if r.phase == RunPhase::Settling {
                TransitionPhaseEvent::Completed
            } else {
                TransitionPhaseEvent::Cancelled
            };
            if r.phase != RunPhase::Settling && r.target_was_protected {
                if let Some(t) = &r.target {
                    self.protected.insert(t.clone());
                }
            }
            self.phase_event(r.op, phase);
        }
        self.base = Base::Greeter;
    }

    fn validate(&self, r: &Running, probe: &dyn Probe) -> Result<(), &'static str> {
        if needs_greeter(r.kind) && !probe.greeter_presentable() {
            return Err("greeter not presentable");
        }
        let check = |k: &Option<SessionKey>, what: &'static str| -> Result<(), &'static str> {
            let Some(k) = k else { return Err(what) };
            if !probe.session_exists(k) {
                return Err("session not registered");
            }
            if self.protected.contains(k) {
                return Err("session protected");
            }
            if !probe.session_presentable(k) {
                return Err("session has incomplete outputs");
            }
            Ok(())
        };
        match r.kind {
            TransitionKind::NewLoginFade | TransitionKind::UnlockSlideUp => {
                check(&r.target, "transition has no target session")
            }
            TransitionKind::LogoutFade => check(&r.source, "transition has no source session"),
            _ => Ok(()),
        }
    }

    /// Start a transition. Emits `Prepared`, or `Failed` when it cannot start.
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        &mut self,
        probe: &dyn Probe,
        operation: OperationId,
        kind: TransitionKind,
        source: Option<SessionKey>,
        target: Option<SessionKey>,
        duration_ms: u32,
        reduce_motion: bool,
        power: Option<PowerAction>,
    ) {
        if self.running.is_some() {
            // Only one transition runs at a time; the running one is unaffected.
            self.phase_event(operation, TransitionPhaseEvent::Failed);
            return;
        }
        let target_was_protected = target.as_ref().is_some_and(|t| self.protected.contains(t));
        let mut r = Running {
            op: operation,
            kind,
            source,
            target,
            duration_ms,
            reduce_motion,
            power,
            start_ms: None,
            snapshot_base: self.base.clone(),
            target_was_protected,
            phase: RunPhase::Prepared,
            last_t: 0.0,
        };
        // The target of a reveal is authorised by the manager through this command, so
        // protection is lifted here; the clip in the plan keeps it invisible until the
        // greeter has vacated the area.
        if matches!(kind, TransitionKind::NewLoginFade | TransitionKind::UnlockSlideUp) {
            if let Some(t) = &r.target {
                self.protected.remove(t);
            }
        }
        if let Err(reason) = self.validate(&r, probe) {
            log::warn!("transition {operation} {kind:?} rejected: {reason}");
            if target_was_protected {
                if let Some(t) = &r.target {
                    self.protected.insert(t.clone());
                }
            }
            r.phase = RunPhase::Prepared;
            self.phase_event(operation, TransitionPhaseEvent::Failed);
            self.fail_safe(&r);
            return;
        }
        // Lock, switch and suspend protect the leaving session first if the manager has
        // not done so already.
        if is_lock_family(kind) {
            if let Some(s) = r.source.clone() {
                if !self.protected.contains(&s) {
                    self.protect(&s);
                }
            }
        }
        self.phase_event(operation, TransitionPhaseEvent::Prepared);
        self.running = Some(r);
    }

    /// Put the safe scene in place after a failed transition: cover plus greeter, and
    /// every session the transition touched protected.
    fn fail_safe(&mut self, r: &Running) {
        for k in [&r.source, &r.target].into_iter().flatten() {
            self.protected.insert(k.clone());
        }
        if let Base::Session(k) = &self.base {
            self.protected.insert(k.clone());
        }
        self.base = Base::Greeter;
    }

    /// Abort a transition that has not committed.
    pub fn cancel(&mut self, operation: OperationId) {
        let Some(r) = self.running.as_ref() else { return };
        if r.op != operation || r.phase == RunPhase::Settling {
            return;
        }
        let r = self.running.take().unwrap();
        if r.target_was_protected {
            if let Some(t) = &r.target {
                self.protected.insert(t.clone());
            }
        }
        self.base = r.snapshot_base.clone();
        self.phase_event(operation, TransitionPhaseEvent::Cancelled);
    }

    fn base_plan(&self, probe: &dyn Probe) -> Plan {
        let mut p = Plan::empty();
        match &self.base {
            Base::Cover => {}
            Base::Greeter => {
                if probe.greeter_presentable() {
                    p.greeter = Some(GreeterLayer { offset_y: 0.0 });
                }
            }
            Base::Session(k) => {
                if self.session_ok(k, probe) {
                    p.session = Some(SessionLayer { key: k.clone(), opacity: 1.0, only_vacated: false });
                }
            }
        }
        p
    }

    fn session_ok(&self, k: &SessionKey, probe: &dyn Probe) -> bool {
        !self.protected.contains(k) && probe.session_exists(k) && probe.session_presentable(k)
    }

    /// The plan for the frame about to be composed at monotonic time `now_ms`.
    pub fn plan(&mut self, now_ms: u64, probe: &dyn Probe) -> Plan {
        let Some(mut r) = self.running.take() else {
            return self.base_plan(probe);
        };
        if r.phase == RunPhase::Settling {
            self.running = Some(r);
            return self.base_plan(probe);
        }
        if let Err(reason) = self.validate(&r, probe) {
            log::warn!("transition {} {:?} failed: {reason}", r.op, r.kind);
            self.phase_event(r.op, TransitionPhaseEvent::Failed);
            self.fail_safe(&r);
            return self.base_plan(probe);
        }
        let start = *r.start_ms.get_or_insert(now_ms);
        let elapsed = now_ms.saturating_sub(start);
        let t = progress(elapsed, r.duration_ms, r.reduce_motion);
        r.last_t = t;
        let f = trajectory(r.kind, t);
        let mut p = Plan::empty();
        p.transition = Some((r.op, t));
        p.session_above_greeter = f.session_above_greeter;
        match r.kind {
            TransitionKind::NewLoginFade => {
                p.greeter = Some(GreeterLayer { offset_y: 0.0 });
                p.session = r.target.clone().map(|key| SessionLayer {
                    key,
                    opacity: f.session_opacity,
                    only_vacated: false,
                });
            }
            TransitionKind::UnlockSlideUp => {
                p.greeter = Some(GreeterLayer { offset_y: f.greeter_offset_y });
                p.session = r.target.clone().map(|key| SessionLayer {
                    key,
                    opacity: 1.0,
                    only_vacated: true,
                });
            }
            TransitionKind::Lock | TransitionKind::SwitchUser | TransitionKind::Suspend => {
                p.greeter = Some(GreeterLayer { offset_y: 0.0 });
                p.cover_overlay = 1.0 - f.greeter_opacity;
            }
            TransitionKind::LogoutFade => {
                p.greeter = Some(GreeterLayer { offset_y: 0.0 });
                p.session = r.source.clone().map(|key| SessionLayer {
                    key,
                    opacity: f.session_opacity,
                    only_vacated: false,
                });
            }
            TransitionKind::PowerFade => {
                let base = self.base_plan(probe);
                p.greeter = base.greeter;
                p.session = base.session;
                p.cover_overlay = f.cover_opacity;
            }
        }
        self.running = Some(r);
        p
    }

    /// The frame returned by the last [`Engine::plan`] was composed (and, on hardware,
    /// the flip completed). Releases `Protected` replies and advances transition phases.
    pub fn frame_done(&mut self) {
        for k in std::mem::take(&mut self.pending_protected) {
            self.out.push(SceneEvent::Protected { session: k.0, generation: k.1 });
        }
        let Some(mut r) = self.running.take() else { return };
        match r.phase {
            RunPhase::Prepared => {
                // The frame just composed was the first animated frame.
                r.phase = RunPhase::Animating;
                self.phase_event(r.op, TransitionPhaseEvent::Animating);
                if r.last_t >= 1.0 {
                    self.commit(&mut r);
                }
                self.running = Some(r);
            }
            RunPhase::Animating => {
                if r.last_t >= 1.0 {
                    self.commit(&mut r);
                }
                self.running = Some(r);
            }
            RunPhase::Settling => {
                self.phase_event(r.op, TransitionPhaseEvent::Completed);
            }
        }
    }

    fn commit(&mut self, r: &mut Running) {
        self.base = end_scene(r.kind, &r.target);
        r.phase = RunPhase::Settling;
        self.phase_event(r.op, TransitionPhaseEvent::Committed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        greeter: RefCell<bool>,
        sessions: RefCell<HashMap<SessionKey, bool>>,
    }
    impl Fake {
        fn new(greeter: bool) -> Self {
            let f = Fake::default();
            *f.greeter.borrow_mut() = greeter;
            f
        }
        fn add(&self, k: &SessionKey, presentable: bool) {
            self.sessions.borrow_mut().insert(k.clone(), presentable);
        }
        fn remove(&self, k: &SessionKey) {
            self.sessions.borrow_mut().remove(k);
        }
    }
    impl Probe for Fake {
        fn greeter_presentable(&self) -> bool {
            *self.greeter.borrow()
        }
        fn session_exists(&self, k: &SessionKey) -> bool {
            self.sessions.borrow().contains_key(k)
        }
        fn session_presentable(&self, k: &SessionKey) -> bool {
            self.sessions.borrow().get(k).copied().unwrap_or(false)
        }
    }

    fn key(n: u64) -> SessionKey {
        (format!("s{n:08x}"), n)
    }

    fn phases(e: &mut Engine) -> Vec<TransitionPhaseEvent> {
        e.drain_events()
            .into_iter()
            .filter_map(|ev| match ev {
                SceneEvent::TransitionPhase { phase, .. } => Some(phase),
                _ => None,
            })
            .collect()
    }

    fn run_to_end(e: &mut Engine, p: &Fake, step: u64, max: u64) -> Vec<Plan> {
        let mut plans = Vec::new();
        let mut now = 1000;
        for _ in 0..max {
            plans.push(e.plan(now, p));
            e.frame_done();
            if !e.transition_running() {
                break;
            }
            now += step;
        }
        plans
    }

    #[test]
    fn new_login_keeps_greeter_full_and_raises_session_above() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.show_greeter_only();
        e.begin(&p, 7, TransitionKind::NewLoginFade, None, Some(k.clone()), 420, false, None);
        let plans = run_to_end(&mut e, &p, 20, 100);
        let evs = phases(&mut e);
        assert_eq!(
            evs,
            vec![
                TransitionPhaseEvent::Prepared,
                TransitionPhaseEvent::Animating,
                TransitionPhaseEvent::Committed,
                TransitionPhaseEvent::Completed
            ]
        );
        let first = &plans[0];
        assert_eq!(first.session.as_ref().unwrap().opacity, 0.0);
        let mut prev = -1.0;
        for pl in &plans[..plans.len() - 1] {
            assert_eq!(pl.greeter, Some(GreeterLayer { offset_y: 0.0 }), "greeter constant");
            assert!(pl.session_above_greeter);
            let o = pl.session.as_ref().unwrap().opacity;
            assert!(o >= prev, "opacity monotonic");
            prev = o;
        }
        assert!((prev - 1.0).abs() < 1e-6);
        assert_eq!(e.base(), &Base::Session(k.clone()));
        assert_eq!(e.foreground_session(), Some(&k));
    }

    #[test]
    fn target_cannot_be_foreground_until_commit() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.show_greeter_only();
        e.begin(&p, 1, TransitionKind::NewLoginFade, None, Some(k.clone()), 400, false, None);
        assert_eq!(e.foreground_session(), None);
        assert_eq!(e.transition_target(), Some(&k));
        e.plan(0, &p);
        e.frame_done();
        assert_eq!(e.foreground_session(), None);
    }

    #[test]
    fn unlock_reveals_only_the_vacated_strip_and_unprotects_at_begin() {
        let p = Fake::new(true);
        let k = key(2);
        p.add(&k, true);
        let mut e = Engine::new();
        e.protect(&k);
        e.show_greeter_only();
        assert!(e.is_protected(&k));
        e.begin(&p, 3, TransitionKind::UnlockSlideUp, None, Some(k.clone()), 520, false, None);
        assert!(!e.is_protected(&k));
        let plans = run_to_end(&mut e, &p, 16, 200);
        assert!(plans.len() > 10);
        assert_eq!(plans[0].greeter.unwrap().offset_y, 0.0);
        assert!(plans[0].session.as_ref().unwrap().only_vacated);
        let mut prev = 0.0;
        for pl in &plans[..plans.len() - 1] {
            let y = pl.greeter.unwrap().offset_y;
            assert!(y <= prev + 1e-6, "greeter moves up monotonically");
            prev = y;
            assert!(!pl.session_above_greeter);
        }
        assert!((plans[plans.len() - 2].greeter.unwrap().offset_y + 1.0).abs() < 1e-6);
        assert_eq!(e.base(), &Base::Session(k));
    }

    #[test]
    fn protect_reply_waits_for_a_composed_frame_without_the_session() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.begin(&p, 1, TransitionKind::NewLoginFade, None, Some(k.clone()), 0, true, None);
        e.show_greeter_only();
        // Put the session on screen via a committed transition.
        e.begin(&p, 2, TransitionKind::NewLoginFade, None, Some(k.clone()), 0, true, None);
        run_to_end(&mut e, &p, 16, 10);
        e.drain_events();
        assert!(e.plan(0, &p).session.is_some());
        e.frame_done();
        e.protect(&k);
        assert!(e.drain_events().is_empty(), "no reply before a frame");
        let plan = e.plan(16, &p);
        assert!(plan.session.is_none(), "protected session is never planned");
        e.frame_done();
        assert_eq!(
            e.drain_events(),
            vec![SceneEvent::Protected { session: k.0.clone(), generation: k.1 }]
        );
    }

    #[test]
    fn reduced_motion_is_one_secure_frame() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.show_greeter_only();
        e.begin(&p, 1, TransitionKind::UnlockSlideUp, None, Some(k.clone()), 520, true, None);
        let first = e.plan(0, &p);
        assert_eq!(first.greeter.unwrap().offset_y, -1.0);
        e.frame_done();
        let evs = phases(&mut e);
        assert_eq!(
            evs,
            vec![
                TransitionPhaseEvent::Prepared,
                TransitionPhaseEvent::Animating,
                TransitionPhaseEvent::Committed
            ]
        );
        e.plan(16, &p);
        e.frame_done();
        assert_eq!(phases(&mut e), vec![TransitionPhaseEvent::Completed]);
    }

    #[test]
    fn vanishing_target_fails_to_cover_and_greeter() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.show_greeter_only();
        e.begin(&p, 1, TransitionKind::NewLoginFade, None, Some(k.clone()), 400, false, None);
        e.plan(0, &p);
        e.frame_done();
        p.remove(&k);
        let plan = e.plan(100, &p);
        assert!(plan.session.is_none());
        assert_eq!(plan.greeter, Some(GreeterLayer { offset_y: 0.0 }));
        assert_eq!(plan.cover_overlay, 0.0);
        assert!(!e.transition_running());
        let evs = phases(&mut e);
        assert_eq!(evs.last(), Some(&TransitionPhaseEvent::Failed));
        assert_eq!(e.base(), &Base::Greeter);
        assert!(e.is_protected(&k), "a touched session stays hidden after a failure");
    }

    #[test]
    fn unprepared_target_fails_at_begin_and_nothing_animates() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, false);
        let mut e = Engine::new();
        e.protect(&k);
        e.show_greeter_only();
        e.begin(&p, 1, TransitionKind::UnlockSlideUp, None, Some(k.clone()), 520, false, None);
        assert!(!e.transition_running());
        assert_eq!(phases(&mut e), vec![TransitionPhaseEvent::Failed]);
        assert!(e.is_protected(&k), "protection is restored when begin fails");
        assert!(e.plan(0, &p).session.is_none());
    }

    #[test]
    fn cancel_before_commit_restores_previous_scene_and_protection() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.protect(&k);
        e.show_greeter_only();
        e.begin(&p, 9, TransitionKind::UnlockSlideUp, None, Some(k.clone()), 520, false, None);
        e.plan(0, &p);
        e.frame_done();
        e.plan(100, &p);
        e.frame_done();
        e.cancel(9);
        assert!(e.is_protected(&k));
        assert_eq!(e.base(), &Base::Greeter);
        let plan = e.plan(200, &p);
        assert!(plan.session.is_none());
        assert_eq!(plan.greeter.unwrap().offset_y, 0.0);
        assert_eq!(phases(&mut e).last(), Some(&TransitionPhaseEvent::Cancelled));
    }

    #[test]
    fn lock_protects_source_and_fades_greeter_over_cover() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.begin(&p, 1, TransitionKind::NewLoginFade, None, Some(k.clone()), 0, true, None);
        e.show_greeter_only();
        e.begin(&p, 1, TransitionKind::NewLoginFade, None, Some(k.clone()), 0, true, None);
        run_to_end(&mut e, &p, 16, 10);
        e.drain_events();
        e.begin(&p, 2, TransitionKind::Lock, Some(k.clone()), None, 240, false, None);
        assert!(e.is_protected(&k));
        let plans = run_to_end(&mut e, &p, 16, 100);
        assert!(plans.iter().all(|pl| pl.session.is_none()));
        assert!((plans[0].cover_overlay - 1.0).abs() < 1e-6, "starts fully covered");
        let last = &plans[plans.len() - 2];
        assert!(last.cover_overlay.abs() < 1e-6);
        assert_eq!(e.base(), &Base::Greeter);
    }

    #[test]
    fn power_fade_keeps_the_visible_scene_and_covers_it() {
        let p = Fake::new(true);
        let mut e = Engine::new();
        e.show_greeter_only();
        e.begin(&p, 4, TransitionKind::PowerFade, None, None, 360, false, Some(PowerAction::PowerOff));
        let plans = run_to_end(&mut e, &p, 40, 100);
        assert!(plans[0].cover_overlay.abs() < 1e-6);
        assert!(plans[0].greeter.is_some());
        assert!((plans[plans.len() - 2].cover_overlay - 1.0).abs() < 1e-6);
        assert_eq!(e.base(), &Base::Cover);
    }

    #[test]
    fn greeter_loss_shows_cover_only() {
        let p = Fake::new(false);
        let mut e = Engine::new();
        e.show_greeter_only();
        let plan = e.plan(0, &p);
        assert!(plan.greeter.is_none() && plan.session.is_none());
    }

    #[test]
    fn second_transition_is_refused_while_one_runs() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.show_greeter_only();
        e.begin(&p, 1, TransitionKind::NewLoginFade, None, Some(k.clone()), 400, false, None);
        e.begin(&p, 2, TransitionKind::PowerFade, None, None, 400, false, None);
        assert!(e.transition_running());
        assert_eq!(e.running_operation(), Some(1));
        let evs = e.drain_events();
        assert!(evs.contains(&SceneEvent::TransitionPhase { operation: 2, phase: TransitionPhaseEvent::Failed }));
    }

    #[test]
    fn unregister_of_foreground_session_drops_to_cover() {
        let p = Fake::new(true);
        let k = key(1);
        p.add(&k, true);
        let mut e = Engine::new();
        e.show_greeter_only();
        e.begin(&p, 1, TransitionKind::NewLoginFade, None, Some(k.clone()), 0, true, None);
        run_to_end(&mut e, &p, 16, 10);
        assert_eq!(e.foreground_session(), Some(&k));
        e.unregister_session(&k);
        p.remove(&k);
        assert_eq!(e.foreground_session(), None);
        assert!(e.plan(0, &p).session.is_none());
    }
}
