//! Startup milestones and the documented progress model.
//!
//! A milestone is reached only by a real event with evidence (see
//! `docs/bsm/architecture.md`, "Startup protocol"). Nothing in this module
//! reads a clock. [`ProgressModel::fraction`] is a weighted composite whose
//! weights are fixed work units, not a prediction of wall-clock time.

use serde::{Deserialize, Serialize};

use crate::state::{Generation, Revision, SessionId};

/// Ordered startup milestones of a new hosted session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// 1. PAM accepted the credentials and bsm authorised this launch.
    AuthAccepted,
    /// 2. pam_open_session and logind scope succeeded; `XDG_RUNTIME_DIR` exists.
    PamSession,
    /// 3. The session's bridge connected to its brokered socket and passed peer checks.
    HostConnected,
    /// 4. Gnoblin runtime handshake completed: compositor serves session-local clients.
    GnoblinReady,
    /// 5. Virtual outputs configured (counted: n of m) and acknowledged by bsm.
    OutputsConfigured,
    /// 6. Bingux wallpaper and shell processes launched under the user.
    BinguxLaunched,
    /// 7. Required Bingux surfaces mapped on required outputs (counted: n of m).
    SurfacesMapped,
    /// 8. Valid fresh buffers presented on every required output (counted: n of m).
    DesktopReady,
    /// 9. bsm set `ReadyBackground` and the reveal gate is satisfied.
    ReadyToReveal,
}

impl Stage {
    pub const ALL: [Stage; 9] = [
        Stage::AuthAccepted,
        Stage::PamSession,
        Stage::HostConnected,
        Stage::GnoblinReady,
        Stage::OutputsConfigured,
        Stage::BinguxLaunched,
        Stage::SurfacesMapped,
        Stage::DesktopReady,
        Stage::ReadyToReveal,
    ];

    /// Work units. The sum over `ALL` is exactly 100.
    pub fn weight(self) -> u32 {
        match self {
            Stage::AuthAccepted => 5,
            Stage::PamSession => 10,
            Stage::HostConnected => 5,
            Stage::GnoblinReady => 20,
            Stage::OutputsConfigured => 10,
            Stage::BinguxLaunched => 10,
            Stage::SurfacesMapped => 20,
            Stage::DesktopReady => 15,
            Stage::ReadyToReveal => 5,
        }
    }

    /// True for stages that are counted over outputs or surfaces: while in
    /// them the UI shows `done of total` instead of a duration guess.
    pub fn is_counted(self) -> bool {
        matches!(self, Stage::OutputsConfigured | Stage::SurfacesMapped | Stage::DesktopReady)
    }

    /// Honest phase label for the greeter. Shown while the stage before it is
    /// complete and this one is not.
    pub fn label_while_pending(self) -> &'static str {
        match self {
            Stage::AuthAccepted => "Authenticating",
            Stage::PamSession => "Opening your session",
            Stage::HostConnected => "Starting Gnoblin",
            Stage::GnoblinReady => "Starting Gnoblin",
            Stage::OutputsConfigured => "Preparing displays",
            Stage::BinguxLaunched => "Starting Bingux",
            Stage::SurfacesMapped => "Waiting for Bingux surfaces",
            Stage::DesktopReady => "Waiting for the first frame",
            Stage::ReadyToReveal => "Ready",
        }
    }
}

/// Why a launch stopped before readiness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressFailure {
    pub stage: Stage,
    pub code: crate::error::ErrorCode,
    pub message: String,
}

/// The progress model pushed to the greeter. `revision` increases on every change;
/// consumers drop a model whose `(session, generation)` is not the one they track
/// and ignore a revision not greater than the last one applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressModel {
    pub session: SessionId,
    pub generation: Generation,
    pub revision: Revision,
    /// Stages reached, in order of arrival.
    pub reached: Vec<Stage>,
    /// Counts for the stage in progress (`done` of `total`); 0 total when not counted.
    pub counted_stage: Option<Stage>,
    pub done: u32,
    pub total: u32,
    pub failure: Option<ProgressFailure>,
}

impl ProgressModel {
    pub fn new(session: SessionId, generation: Generation) -> Self {
        Self {
            session,
            generation,
            revision: 0,
            reached: Vec::new(),
            counted_stage: None,
            done: 0,
            total: 0,
            failure: None,
        }
    }

    pub fn has(&self, stage: Stage) -> bool {
        self.reached.contains(&stage)
    }

    /// The first stage not yet reached, or `None` when all are.
    pub fn pending(&self) -> Option<Stage> {
        Stage::ALL.into_iter().find(|s| !self.has(*s))
    }

    /// Mark a stage reached. Stages must arrive in order; an out-of-order or
    /// duplicate stage returns false and changes nothing, so a stale or
    /// replayed event cannot advance progress.
    pub fn reach(&mut self, stage: Stage) -> bool {
        if self.failure.is_some() || self.pending() != Some(stage) {
            return false;
        }
        self.reached.push(stage);
        self.counted_stage = None;
        self.done = 0;
        self.total = 0;
        self.revision += 1;
        true
    }

    /// Record `done` of `total` for a counted pending stage.
    pub fn count(&mut self, stage: Stage, done: u32, total: u32) -> bool {
        if self.failure.is_some() || self.pending() != Some(stage) || !stage.is_counted() || done > total {
            return false;
        }
        if self.counted_stage == Some(stage) && self.done == done && self.total == total {
            return false;
        }
        self.counted_stage = Some(stage);
        self.done = done;
        self.total = total;
        self.revision += 1;
        true
    }

    pub fn fail(&mut self, failure: ProgressFailure) {
        self.failure = Some(failure);
        self.revision += 1;
    }

    /// Weighted composite, 0..=100. Reached stages count fully; the stage in
    /// progress counts proportionally only when it is a counted stage with a
    /// known total. Uncounted pending stages contribute nothing until reached,
    /// which makes the bar hold still (indeterminate) instead of inventing motion.
    pub fn fraction(&self) -> u32 {
        let reached: u32 = self.reached.iter().map(|s| s.weight()).sum();
        let partial = match (self.counted_stage, self.total) {
            (Some(stage), total) if total > 0 && self.pending() == Some(stage) => stage.weight() * self.done / total,
            _ => 0,
        };
        reached + partial
    }

    /// True while the pending stage has an unknown duration (no counts).
    pub fn is_indeterminate(&self) -> bool {
        self.failure.is_none() && self.pending().is_some() && self.total == 0
    }

    /// Label for the greeter.
    pub fn label(&self) -> &'static str {
        match self.pending() {
            Some(stage) => stage.label_while_pending(),
            None => "Ready",
        }
    }

    pub fn is_complete(&self) -> bool {
        self.pending().is_none() && self.failure.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_sum_to_100() {
        assert_eq!(Stage::ALL.iter().map(|s| s.weight()).sum::<u32>(), 100);
    }

    #[test]
    fn stages_only_advance_in_order() {
        let mut m = ProgressModel::new("s00000001".into(), 7);
        assert!(!m.reach(Stage::PamSession));
        assert!(m.reach(Stage::AuthAccepted));
        assert!(!m.reach(Stage::AuthAccepted));
        assert!(m.reach(Stage::PamSession));
        assert_eq!(m.fraction(), 15);
    }

    #[test]
    fn counted_stage_is_proportional_and_bounded() {
        let mut m = ProgressModel::new("s00000001".into(), 7);
        for s in [Stage::AuthAccepted, Stage::PamSession, Stage::HostConnected, Stage::GnoblinReady] {
            assert!(m.reach(s));
        }
        assert!(m.is_indeterminate());
        assert!(!m.count(Stage::OutputsConfigured, 3, 2));
        assert!(m.count(Stage::OutputsConfigured, 1, 2));
        assert_eq!(m.fraction(), 40 + 5);
        assert!(!m.is_indeterminate());
    }

    #[test]
    fn failure_freezes_progress() {
        let mut m = ProgressModel::new("s00000001".into(), 7);
        m.reach(Stage::AuthAccepted);
        m.fail(ProgressFailure {
            stage: Stage::PamSession,
            code: crate::error::ErrorCode::PamFailure,
            message: "x".into(),
        });
        assert!(!m.reach(Stage::PamSession));
        assert!(!m.is_complete());
    }
}
