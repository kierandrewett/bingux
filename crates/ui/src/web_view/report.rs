//! The engine's account of its own security, as the settings page shows it.
//!
//! The engine checks its invariants at start and whenever one can change, and
//! reports them as dotted keys (`sandbox.content.level`,
//! `process.4711.seccomp`, `profile.dir.mode`, ...), each with a status. This
//! module turns that flat list into what a person reads: one row per kind of
//! process with its sandbox, and one row per other check with a name instead
//! of a key. It is the engine's own account and **not** a defence against a
//! compromised engine, which reports what it likes; the page says so.
//!
//! [`ReportEntry`] is the plain shape of one key, so this module needs no
//! engine and its rules are tested without one.

use std::collections::BTreeMap;

/// The result of one check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CheckStatus {
    Ok,
    /// Nothing to act on.
    Info,
    /// Not as strong as it could be.
    Warn,
    /// An invariant does not hold.
    Fail,
    /// The engine could not tell.
    Unknown,
}

impl CheckStatus {
    /// Whether the page shows the check as healthy.
    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Ok | Self::Info)
    }

    /// The worse of two: failed over unknown over warned over healthy.
    fn worst(self, other: Self) -> Self {
        const fn rank(status: CheckStatus) -> u8 {
            match status {
                CheckStatus::Ok => 0,
                CheckStatus::Info => 1,
                CheckStatus::Warn => 2,
                CheckStatus::Unknown => 3,
                CheckStatus::Fail => 4,
            }
        }
        if rank(other) > rank(self) { other } else { self }
    }
}

/// One key of the report as the engine sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportEntry {
    pub key: String,
    pub value: String,
    pub status: CheckStatus,
    /// The start fails when this one does.
    pub required: bool,
    /// The process the entry is about, when it is about one.
    pub pid: Option<u32>,
}

/// One line of the report that is not about a process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityCheck {
    pub label: String,
    pub value: String,
    pub status: CheckStatus,
}

/// The sandbox of one kind of process: what every process of the kind has.
/// A part the engine did not report is `None`, never "off".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessSandbox {
    /// `content`, `socket`, `rdd`, ... as the engine names it.
    pub kind: String,
    /// How many processes of this kind run now.
    pub count: usize,
    /// `6` for a level, or `enabled` / `disabled`.
    pub level: Option<String>,
    /// A seccomp filter is on in every process of the kind.
    pub seccomp: Option<bool>,
    /// `no_new_privs` is set in every process of the kind.
    pub no_new_privs: Option<bool>,
    /// No process of the kind holds a capability.
    pub capabilities_dropped: Option<bool>,
    /// The worst status of the entries the row comes from.
    pub status: CheckStatus,
}

impl ProcessSandbox {
    /// What the row is called.
    pub fn title(&self) -> String {
        let name = kind_label(&self.kind);
        match self.count {
            0 | 1 => name,
            count => format!("{name} ({count} processes)"),
        }
    }

    /// What the row says: the level, then each protection that holds, then
    /// each that was reported missing.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        match self.level.as_deref() {
            Some(level) if level.chars().all(|c| c.is_ascii_digit()) => parts.push(format!("sandbox level {level}")),
            Some(level) => parts.push(format!("sandbox {level}")),
            None => {}
        }
        for (held, name, missing) in [
            (self.seccomp, "seccomp filter", "no seccomp filter"),
            (self.no_new_privs, "no new privileges", "can gain privileges"),
            (self.capabilities_dropped, "no capabilities", "holds capabilities"),
        ] {
            match held {
                Some(true) => parts.push(name.to_string()),
                Some(false) => parts.push(missing.to_string()),
                None => {}
            }
        }
        if parts.is_empty() {
            return "Nothing reported".to_string();
        }
        let mut text = parts.join(", ");
        text[..1].make_ascii_uppercase();
        text
    }
}

/// The report, grouped for the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityReport {
    /// The worst status of any entry.
    pub overall: CheckStatus,
    /// Increases with every change the engine reports.
    pub generation: u64,
    /// One row per kind of process, in the order engines list their processes (web content first).
    pub processes: Vec<ProcessSandbox>,
    /// Every other key, in the order the engine sent them.
    pub checks: Vec<SecurityCheck>,
}

/// How kinds of process are ordered, and what they are called.
const KINDS: &[(&str, &str)] = &[
    ("content", "Web content"),
    ("file", "Local files"),
    ("socket", "Network"),
    ("rdd", "Media decoding"),
    ("utility", "Utility"),
    ("gpu", "Graphics"),
    ("extension", "Extensions"),
    ("gmp", "Media plugins"),
    ("forkserver", "Process launcher"),
];

/// What a kind of process is called for a person.
pub fn kind_label(kind: &str) -> String {
    KINDS
        .iter()
        .find(|(known, _)| *known == kind)
        .map_or_else(|| kind.to_string(), |(_, label)| (*label).to_string())
}

/// The name of a key that is not about a process. A key this list does not
/// know is shown as it is: the engine says an unknown key is informational.
fn check_label(key: &str) -> Option<&'static str> {
    Some(match key {
        "engine.gecko.version" => "Engine version",
        "engine.sdk.version" => "SDK version",
        "engine.sdk.build_age_days" => "SDK age in days",
        "engine.sdk.signature" => "SDK signature",
        "sandbox.floor" => "Sandbox floor",
        "sandbox.gpu" => "Graphics process sandbox",
        "confinement.engine" => "Engine confinement",
        "fission.enabled" => "Site isolation",
        "process.isolation" => "Process isolation",
        "prefs.locked.count" => "Locked preferences",
        "prefs.locked.expected" => "Preferences expected locked",
        "prefs.profile_dropped" => "Profile preferences dropped",
        "profile.dir.owner" => "Profile folder owner",
        "profile.dir.mode" => "Profile folder permissions",
        "profile.dir.symlink" => "Profile folder links",
        "profile.hostile_files" => "Planted files in the profile",
        "env.clean" => "Clean environment",
        "env.overrides" => "Environment overrides",
        "dirs.write_roots" => "Writable folders",
        "files.read_grants" => "Readable folders",
        "network.listeners" => "Network listeners",
        "network.remote_settings.mode" => "Remote Settings mode",
        "network.remote_settings.server" => "Remote Settings server",
        "network.remote_settings.freshness_hours" => "Remote Settings age in hours",
        "network.remote_settings.crlite" => "Revocation data",
        "network.proxy" => "Proxy",
        "network.doh" => "DNS over HTTPS",
        "network.lna" => "Local network access",
        "network.webrtc.ip_policy" => "WebRTC address policy",
        "certs.extra_roots" => "Extra root certificates",
        "devtools.endpoint" => "Developer tools endpoint",
        "automation.endpoint" => "Automation endpoint",
        "telemetry" => "Telemetry",
        "crash.upload" => "Crash report upload",
        "updater" => "Updater",
        "extensions.scopes" => "Extension scopes",
        "extensions.signature_required" => "Extension signatures required",
        "permissions.prompt_service" => "Permission prompts",
        "a11y.active" => "Accessibility",
        _ => return None,
    })
}

/// `process.4711.seccomp` as (4711, `seccomp`).
fn process_key(key: &str) -> Option<(u32, &str)> {
    let rest = key.strip_prefix("process.")?;
    let (pid, field) = rest.split_once('.')?;
    Some((pid.parse().ok()?, field))
}

/// `sandbox.content.level` and `sandbox.rdd` as `content` and `rdd`.
fn sandbox_kind(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("sandbox.")?;
    let kind = rest.strip_suffix(".level").unwrap_or(rest);
    KINDS.iter().any(|(known, _)| *known == kind).then_some(kind)
}

#[derive(Default)]
struct Group {
    pids: Vec<u32>,
    level: Option<String>,
    seccomp: Vec<bool>,
    no_new_privs: Vec<bool>,
    caps_dropped: Vec<bool>,
    status: Option<CheckStatus>,
}

impl Group {
    fn note(&mut self, status: CheckStatus) {
        self.status = Some(self.status.map_or(status, |known| known.worst(status)));
    }
}

fn all(held: &[bool]) -> Option<bool> {
    (!held.is_empty()).then(|| held.iter().all(|held| *held))
}

impl SecurityReport {
    /// Groups the entries of a report. A report with no entry at all is one
    /// the engine has not produced: the caller shows "not reported yet"
    /// rather than an empty healthy one.
    pub fn from_entries(overall: CheckStatus, generation: u64, entries: &[ReportEntry]) -> Self {
        let mut groups: BTreeMap<&str, Group> = BTreeMap::new();
        let mut kind_of_pid: BTreeMap<u32, &str> = BTreeMap::new();
        // The kind of each process comes first: its other keys can arrive before it.
        for entry in entries {
            if let Some((pid, "type")) = process_key(&entry.key) {
                kind_of_pid.insert(pid, entry.value.as_str());
                let group = groups.entry(entry.value.as_str()).or_default();
                group.pids.push(pid);
                group.note(entry.status);
            }
        }
        let mut checks = Vec::new();
        for entry in entries {
            if let Some((pid, field)) = process_key(&entry.key) {
                let Some(group) = kind_of_pid.get(&pid).and_then(|kind| groups.get_mut(kind)) else {
                    // A process whose type was never reported is a check of its own.
                    checks.push(SecurityCheck {
                        label: format!("Process {pid} {field}"),
                        value: entry.value.clone(),
                        status: entry.status,
                    });
                    continue;
                };
                let on = entry.value.trim();
                match field {
                    "seccomp" => group.seccomp.push(on != "0" && !on.is_empty()),
                    "no_new_privs" => group.no_new_privs.push(on == "1"),
                    "cap_eff" => group.caps_dropped.push(on.chars().all(|c| c == '0')),
                    _ => {}
                }
                if field != "type" {
                    group.note(entry.status);
                }
            } else if let Some(group) = sandbox_kind(&entry.key).and_then(|kind| groups.get_mut(kind)) {
                group.level = Some(entry.value.clone());
                group.note(entry.status);
            } else {
                let label = check_label(&entry.key).map_or_else(
                    || sandbox_kind(&entry.key).map_or_else(|| entry.key.clone(), |kind| format!("{} sandbox", kind_label(kind))),
                    str::to_string,
                );
                checks.push(SecurityCheck {
                    label,
                    value: entry.value.clone(),
                    status: entry.status,
                });
            }
        }
        let mut processes: Vec<ProcessSandbox> = groups
            .into_iter()
            .map(|(kind, group)| ProcessSandbox {
                kind: kind.to_string(),
                count: group.pids.len(),
                level: group.level.clone(),
                seccomp: all(&group.seccomp),
                no_new_privs: all(&group.no_new_privs),
                capabilities_dropped: all(&group.caps_dropped),
                status: group.status.unwrap_or(CheckStatus::Unknown),
            })
            .collect();
        let rank = |kind: &str| KINDS.iter().position(|(known, _)| *known == kind).unwrap_or(KINDS.len());
        processes.sort_by_key(|process| rank(&process.kind));
        Self {
            overall,
            generation,
            processes,
            checks,
        }
    }

    /// An invariant does not hold, in the report as a whole or in a kind of
    /// process. A warning is not a failure.
    pub fn has_failure(&self) -> bool {
        self.overall == CheckStatus::Fail || self.processes.iter().any(|process| process.status == CheckStatus::Fail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, value: &str, status: CheckStatus, pid: Option<u32>) -> ReportEntry {
        ReportEntry {
            key: key.to_string(),
            value: value.to_string(),
            status,
            required: pid.is_some(),
            pid,
        }
    }

    fn healthy() -> Vec<ReportEntry> {
        let mut entries = vec![
            entry("engine.gecko.version", "159.0", CheckStatus::Info, None),
            entry("sandbox.content.level", "6", CheckStatus::Ok, None),
            entry("sandbox.socket.level", "2", CheckStatus::Ok, None),
            entry("sandbox.rdd", "disabled", CheckStatus::Warn, None),
            entry("sandbox.gpu", "none: no GPU process, compositor in the engine process", CheckStatus::Info, None),
            entry("profile.dir.mode", "0700", CheckStatus::Ok, None),
            entry("some.future.key", "x", CheckStatus::Info, None),
        ];
        for (pid, kind, seccomp) in [(11, "content", "2"), (12, "content", "2"), (13, "socket", "2")] {
            entries.push(entry(&format!("process.{pid}.type"), kind, CheckStatus::Ok, Some(pid)));
            entries.push(entry(&format!("process.{pid}.seccomp"), seccomp, CheckStatus::Ok, Some(pid)));
            entries.push(entry(&format!("process.{pid}.no_new_privs"), "1", CheckStatus::Ok, Some(pid)));
            entries.push(entry(&format!("process.{pid}.cap_eff"), "0", CheckStatus::Ok, Some(pid)));
        }
        entries
    }

    #[test]
    fn processes_are_grouped_by_kind_with_the_sandbox_level_of_the_kind() {
        let report = SecurityReport::from_entries(CheckStatus::Warn, 3, &healthy());
        let kinds: Vec<(&str, usize)> = report.processes.iter().map(|p| (p.kind.as_str(), p.count)).collect();
        assert_eq!(kinds, [("content", 2), ("socket", 1)]);
        let content = &report.processes[0];
        assert_eq!(content.title(), "Web content (2 processes)");
        assert_eq!(content.summary(), "Sandbox level 6, seccomp filter, no new privileges, no capabilities");
        assert_eq!(report.processes[1].title(), "Network");
        assert!(!report.has_failure(), "a warning is not a failure");
    }

    #[test]
    fn a_sandbox_key_of_a_kind_with_no_process_is_a_check_with_a_name() {
        let report = SecurityReport::from_entries(CheckStatus::Warn, 1, &healthy());
        let rdd = report.checks.iter().find(|check| check.label == "Media decoding sandbox").expect("rdd row");
        assert_eq!((rdd.value.as_str(), rdd.status), ("disabled", CheckStatus::Warn));
        let gpu = report.checks.iter().find(|check| check.label == "Graphics process sandbox").expect("gpu row");
        assert_eq!(gpu.status, CheckStatus::Info);
    }

    #[test]
    fn keys_have_names_and_an_unknown_key_stays_as_it_is() {
        let report = SecurityReport::from_entries(CheckStatus::Ok, 1, &healthy());
        assert!(report.checks.iter().any(|check| check.label == "Profile folder permissions" && check.value == "0700"));
        assert!(report.checks.iter().any(|check| check.label == "some.future.key"), "unknown keys are informational");
        assert!(!report.checks.iter().any(|check| check.label.starts_with("process.")), "process keys are not rows of their own");
    }

    #[test]
    fn one_process_without_seccomp_is_reported_for_its_kind() {
        let mut entries = healthy();
        entries.retain(|entry| entry.key != "process.12.seccomp");
        entries.push(entry("process.12.seccomp", "0", CheckStatus::Fail, Some(12)));
        let report = SecurityReport::from_entries(CheckStatus::Fail, 2, &entries);
        let content = &report.processes[0];
        assert_eq!(content.seccomp, Some(false), "a kind has seccomp only when every process of it has");
        assert_eq!(content.status, CheckStatus::Fail);
        assert!(content.summary().contains("no seccomp filter"));
        assert!(report.has_failure());
    }

    #[test]
    fn what_the_engine_did_not_report_is_not_off() {
        let entries = [
            entry("process.7.type", "content", CheckStatus::Ok, Some(7)),
            entry("process.7.seccomp", "2", CheckStatus::Ok, Some(7)),
        ];
        let report = SecurityReport::from_entries(CheckStatus::Ok, 1, &entries);
        let content = &report.processes[0];
        assert_eq!((content.level.as_deref(), content.no_new_privs, content.capabilities_dropped), (None, None, None));
        assert_eq!(content.summary(), "Seccomp filter");
    }

    #[test]
    fn a_process_of_unknown_kind_is_still_a_row_and_one_without_a_type_is_a_check() {
        let entries = [
            entry("process.5.type", "weird", CheckStatus::Ok, Some(5)),
            entry("process.9.seccomp", "2", CheckStatus::Ok, Some(9)),
        ];
        let report = SecurityReport::from_entries(CheckStatus::Ok, 1, &entries);
        assert_eq!(report.processes[0].title(), "weird");
        assert_eq!(report.checks[0].label, "Process 9 seccomp");
    }
}
