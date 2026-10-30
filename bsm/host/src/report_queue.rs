//! Duplicate suppression and rate limiting for `report` requests (review F6).
//!
//! bsm validates and counts every report, so the bridge never sends the same
//! report twice in a row for a key and never sends more than `RATE` reports
//! per second on average (`BURST` at once). A report that does not fit is kept,
//! replacing an older pending one of the same key (only the newest state of a
//! key matters), and sent by [`ReportQueue::flush`] when budget returns. Pure:
//! the caller passes the time.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::readiness::Report;

/// Sustained reports per second.
pub const RATE: f64 = 20.0;
/// Reports that may go out at once.
pub const BURST: f64 = 20.0;

/// What a report is about: the kind and the output it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    GnoblinReady,
    OutputConfigured(String),
    BinguxLaunched,
    SurfacesMapped(String),
    DesktopReady,
}

pub fn key_of(r: &Report) -> Key {
    match r {
        Report::GnoblinReady => Key::GnoblinReady,
        Report::OutputConfigured { output, .. } => Key::OutputConfigured(output.clone()),
        Report::BinguxLaunched => Key::BinguxLaunched,
        Report::SurfacesMapped { output, .. } => Key::SurfacesMapped(output.clone()),
        Report::DesktopReady { .. } => Key::DesktopReady,
    }
}

#[derive(Debug)]
pub struct ReportQueue {
    last: Vec<(Key, Report)>,
    pending: VecDeque<(Key, Report)>,
    tokens: f64,
    refilled: Option<Instant>,
}

impl Default for ReportQueue {
    fn default() -> Self {
        Self { last: Vec::new(), pending: VecDeque::new(), tokens: BURST, refilled: None }
    }
}

impl ReportQueue {
    fn refill(&mut self, now: Instant) {
        if let Some(t) = self.refilled {
            let dt = now.saturating_duration_since(t).as_secs_f64();
            self.tokens = (self.tokens + dt * RATE).min(BURST);
        }
        self.refilled = Some(now);
    }

    fn last_of(&self, key: &Key) -> Option<&Report> {
        self.last.iter().find(|(k, _)| k == key).map(|(_, r)| r)
    }

    fn set_last(&mut self, key: Key, report: Report) {
        match self.last.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = report,
            None => self.last.push((key, report)),
        }
    }

    /// Offer a report. Returns the reports to send now, in order.
    pub fn submit(&mut self, report: Report, now: Instant) -> Vec<Report> {
        let key = key_of(&report);
        // The newest state of a key replaces a pending older one.
        self.pending.retain(|(k, _)| *k != key);
        if self.last_of(&key) == Some(&report) {
            return Vec::new(); // identical to what bsm already has
        }
        self.pending.push_back((key, report));
        self.flush(now)
    }

    /// Send what the budget allows.
    pub fn flush(&mut self, now: Instant) -> Vec<Report> {
        self.refill(now);
        let mut out = Vec::new();
        while self.tokens >= 1.0 {
            let Some((key, report)) = self.pending.pop_front() else { break };
            self.tokens -= 1.0;
            self.set_last(key, report.clone());
            out.push(report);
        }
        out
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Time until one more report can go out.
    pub fn wait(&self) -> Duration {
        let missing = (1.0 - self.tokens).max(0.0);
        Duration::from_secs_f64(missing / RATE).max(Duration::from_millis(10))
    }

    /// bsm has a new connection and knows nothing: everything may be sent again.
    pub fn reset(&mut self) {
        self.last.clear();
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sm(output: &str, a: u32) -> Report {
        Report::SurfacesMapped { output: output.into(), a, b: 3 }
    }

    #[test]
    fn identical_reports_are_sent_once() {
        let t = Instant::now();
        let mut q = ReportQueue::default();
        assert_eq!(q.submit(sm("A", 1), t), vec![sm("A", 1)]);
        assert!(q.submit(sm("A", 1), t).is_empty());
        assert_eq!(q.submit(sm("A", 2), t), vec![sm("A", 2)]);
        assert_eq!(q.submit(Report::BinguxLaunched, t), vec![Report::BinguxLaunched]);
        assert!(q.submit(Report::BinguxLaunched, t).is_empty());
    }

    #[test]
    fn a_flood_is_capped_and_only_the_newest_state_per_key_survives() {
        let t = Instant::now();
        let mut q = ReportQueue::default();
        let mut sent = 0;
        for i in 0..1000u32 {
            sent += q.submit(sm("A", i), t).len();
        }
        assert_eq!(sent, BURST as usize, "no more than the burst in one instant");
        assert!(q.has_pending());
        // One second later the budget is back; only the newest value is sent.
        let out = q.flush(t + Duration::from_secs(1));
        assert_eq!(out, vec![sm("A", 999)]);
        assert!(!q.has_pending());
    }

    #[test]
    fn the_sustained_rate_is_bounded() {
        let t = Instant::now();
        let mut q = ReportQueue::default();
        let mut sent = 0usize;
        for ms in 0..10_000u32 {
            let now = t + Duration::from_millis(ms as u64);
            for k in 0..5 {
                sent += q.submit(sm(&format!("O{k}"), ms), now).len();
            }
        }
        // 10 s at 20/s plus the initial burst.
        assert!(sent <= 20 * 10 + BURST as usize + 1, "{sent} reports in 10 s");
        assert!(sent >= 20 * 10 - 5, "starved: {sent}");
    }

    #[test]
    fn order_of_distinct_keys_is_kept_and_reset_forgets() {
        let t = Instant::now();
        let mut q = ReportQueue::default();
        q.submit(Report::GnoblinReady, t);
        q.reset();
        assert_eq!(q.submit(Report::GnoblinReady, t), vec![Report::GnoblinReady]);
    }
}
