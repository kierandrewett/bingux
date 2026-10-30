//! vnStat as a [`UsageSource`]: history from `vnstat --json d` and
//! `vnstat --json h`.
//!
//! The JSON understood here is vnStat 2's `jsonversion` 2 (bytes; entries
//! carry `date` and `time` in the machine's local time and, since vnStat 2.7,
//! a `timestamp`). The parser is tested against fixtures written from the
//! documented format in `man vnstat` (vnStat is not installed on the
//! development machine, so it has not been run against a live daemon).
//!
//! vnStat keeps no per-connection data and no minute history: connection
//! scopes fail with [`Error::Unsupported`], [`Granularity::Minute`] likewise,
//! and a range edge inside an hour is rounded outward to whole hours.
//! Hourly buckets are assumed to start on whole UTC hours.

use crate::{
    cmd::run_capture,
    error::{Error, Result},
    kind::{self, Kind},
    reader::{
        Bucket, ConnectionInfo, Granularity, InterfaceInfo, Scope, Source, Totals, UsageSource,
        bucket_starts, fill, live_since_boot, live_since_boot_total, plan,
    },
    time::{Date, Zone},
};
use parking_lot::Mutex;
use serde::Deserialize;
use std::{
    io,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const VNSTAT_TIMEOUT: Duration = Duration::from_secs(10);
/// A parsed answer is reused this long, so one page render is one `vnstat` run.
const CACHE_TTL: Duration = Duration::from_secs(10);
/// vnStat's daemon saves every few minutes; allow several missed saves.
const LIVE_WITHIN: i64 = 30 * 60;

#[derive(Deserialize)]
struct Document {
    #[serde(default)]
    jsonversion: Option<String>,
    #[serde(default)]
    interfaces: Vec<RawInterface>,
}

#[derive(Deserialize)]
struct RawInterface {
    name: String,
    #[serde(default)]
    updated: Option<RawStamp>,
    #[serde(default)]
    traffic: RawTraffic,
}

#[derive(Deserialize, Default)]
struct RawTraffic {
    #[serde(default, alias = "hours")]
    hour: Vec<RawEntry>,
    #[serde(default, alias = "days")]
    day: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(flatten)]
    when: RawStamp,
    rx: u64,
    tx: u64,
}

#[derive(Deserialize, Default)]
struct RawStamp {
    date: Option<RawDate>,
    time: Option<RawTime>,
    timestamp: Option<i64>,
}

#[derive(Deserialize)]
struct RawDate {
    year: i32,
    month: u8,
    day: u8,
}

#[derive(Deserialize)]
struct RawTime {
    hour: u8,
    minute: u8,
}

type Row = (i64, u64, u64);

/// One interface's parsed history.
#[derive(Clone, Debug, Default)]
struct History {
    name: String,
    hours: Vec<Row>,
    days: Vec<Row>,
    updated: Option<i64>,
}

fn stamp_to_epoch(zone: Zone, stamp: &RawStamp) -> Option<i64> {
    if let Some(timestamp) = stamp.timestamp {
        return Some(timestamp);
    }
    let date = stamp.date.as_ref()?;
    let date = Date::new(date.year, date.month, date.day)?;
    let time = stamp.time.as_ref().map_or(0, |time| {
        i64::from(time.hour) * 3600 + i64::from(time.minute) * 60
    });
    Some(zone.date_start(date) + time)
}

fn parse_document(text: &str, zone: Zone) -> Result<Vec<History>> {
    let document: Document = serde_json::from_str(text)
        .map_err(|error| Error::Command(format!("vnstat printed unreadable JSON: {error}")))?;
    if document.jsonversion.as_deref() == Some("1") {
        return Err(Error::Command(
            "vnstat JSON version 1 (vnStat 1.x) is not supported".into(),
        ));
    }
    Ok(document
        .interfaces
        .into_iter()
        .map(|raw| {
            let hours = raw
                .traffic
                .hour
                .iter()
                .filter_map(|entry| {
                    let start = stamp_to_epoch(zone, &entry.when)?;
                    Some((start - start.rem_euclid(3600), entry.rx, entry.tx))
                })
                .collect();
            // A day is its local date, so DST and a stored timestamp agree.
            let days = raw
                .traffic
                .day
                .iter()
                .filter_map(|entry| {
                    let start = match &entry.when.date {
                        Some(_) => stamp_to_epoch(
                            zone,
                            &RawStamp {
                                date: entry.when.date.as_ref().map(|date| RawDate {
                                    year: date.year,
                                    month: date.month,
                                    day: date.day,
                                }),
                                time: None,
                                timestamp: None,
                            },
                        )?,
                        None => zone.day_start(entry.when.timestamp?),
                    };
                    Some((start, entry.rx, entry.tx))
                })
                .collect();
            History {
                updated: raw
                    .updated
                    .as_ref()
                    .and_then(|stamp| stamp_to_epoch(zone, stamp)),
                name: raw.name,
                hours,
                days,
            }
        })
        .collect())
}

/// Merges the `d` and `h` answers by interface name.
fn merge(days: Vec<History>, hours: Vec<History>) -> Vec<History> {
    let mut merged = days;
    for extra in hours {
        match merged.iter_mut().find(|history| history.name == extra.name) {
            Some(history) => {
                history.hours = extra.hours;
                history.updated = history.updated.max(extra.updated);
            }
            None => merged.push(extra),
        }
    }
    merged.sort_by(|left, right| left.name.cmp(&right.name));
    merged
}

struct Snapshot {
    /// Only interfaces we can classify: hardware, Wi-Fi, mobile, VPN.
    interfaces: Vec<(InterfaceInfo, History)>,
}

type Runner = Box<dyn Fn(&str) -> io::Result<String> + Send>;

pub struct VnstatReader {
    zone: Zone,
    sysfs_root: PathBuf,
    runner: Runner,
    cache: Mutex<Option<(Instant, Arc<Snapshot>)>>,
}

impl VnstatReader {
    /// A reader backed by the `vnstat` on `PATH`, if it is there, works and
    /// has history for at least one interface. Costs one `vnstat` run.
    pub fn detect(zone: Zone) -> Option<VnstatReader> {
        let reader = VnstatReader::with_runner(
            zone,
            Box::new(|mode| run_capture("vnstat", &["--json", mode], VNSTAT_TIMEOUT)),
        );
        let snapshot = reader.snapshot().ok()?;
        snapshot
            .interfaces
            .iter()
            .any(|(_, history)| !history.days.is_empty() || !history.hours.is_empty())
            .then_some(reader)
    }

    /// A reader over fixed `vnstat --json d` and `vnstat --json h` output.
    /// For tests and for callers that run vnStat themselves.
    pub fn from_json(days_json: &str, hours_json: &str, zone: Zone) -> Result<VnstatReader> {
        let (days_json, hours_json) = (days_json.to_owned(), hours_json.to_owned());
        let reader = VnstatReader::with_runner(
            zone,
            Box::new(move |mode| {
                Ok(if mode == "d" {
                    days_json.clone()
                } else {
                    hours_json.clone()
                })
            }),
        );
        reader.snapshot()?;
        Ok(reader)
    }

    fn with_runner(zone: Zone, runner: Runner) -> VnstatReader {
        VnstatReader {
            zone,
            sysfs_root: PathBuf::from(kind::SYSFS),
            runner,
            cache: Mutex::new(None),
        }
    }

    /// Classify interfaces against another sysfs root (tests).
    pub fn with_sysfs_root(mut self, root: impl Into<PathBuf>) -> VnstatReader {
        self.sysfs_root = root.into();
        *self.cache.lock() = None;
        self
    }

    fn snapshot(&self) -> Result<Arc<Snapshot>> {
        let mut cache = self.cache.lock();
        if let Some((at, snapshot)) = &*cache {
            if at.elapsed() < CACHE_TTL {
                return Ok(snapshot.clone());
            }
        }
        let days = (self.runner)("d")
            .map_err(|error| Error::Command(format!("vnstat: {error}")))
            .and_then(|text| parse_document(&text, self.zone))?;
        // Without hourly history vnStat can still answer by day.
        let hours = (self.runner)("h")
            .map_err(|error| Error::Command(format!("vnstat: {error}")))
            .and_then(|text| parse_document(&text, self.zone))
            .unwrap_or_default();
        let interfaces = merge(days, hours)
            .into_iter()
            .filter_map(|history| {
                let kind = kind::classify(&self.sysfs_root, &history.name)
                    .or_else(|| Kind::guess_from_name(&history.name))?;
                Some((history, kind))
            })
            .enumerate()
            .map(|(index, (history, kind))| {
                let info = InterfaceInfo {
                    id: index as i64 + 1,
                    name: history.name.clone(),
                    kind,
                    mac: None,
                };
                (info, history)
            })
            .collect();
        let snapshot = Arc::new(Snapshot { interfaces });
        *cache = Some((Instant::now(), snapshot.clone()));
        Ok(snapshot)
    }

    /// The histories a scope selects.
    fn select<'a>(&self, snapshot: &'a Snapshot, scope: &Scope) -> Result<Vec<&'a History>> {
        let matching = |test: &dyn Fn(&InterfaceInfo) -> bool, what: String| {
            let found: Vec<&History> = snapshot
                .interfaces
                .iter()
                .filter(|(info, _)| test(info))
                .map(|(_, history)| history)
                .collect();
            if found.is_empty() {
                Err(Error::UnknownScope(what))
            } else {
                Ok(found)
            }
        };
        match scope {
            // VPN traffic also crosses the physical link: see `Scope::All`.
            Scope::All => Ok(snapshot
                .interfaces
                .iter()
                .filter(|(info, _)| info.kind != Kind::Vpn)
                .map(|(_, history)| history)
                .collect()),
            Scope::Interface(name) => matching(&|info| info.name == *name, name.clone()),
            Scope::InterfaceId(id) => matching(&|info| info.id == *id, id.to_string()),
            Scope::Connection(_) | Scope::ConnectionId(_) => {
                Err(Error::Unsupported("vnStat has no per-connection history"))
            }
        }
    }
}

fn sum_rows(rows: &[Row], from: i64, to: i64) -> Totals {
    let mut totals = Totals::default();
    for &(start, rx, tx) in rows {
        if start >= from && start < to {
            totals += Totals { rx, tx };
        }
    }
    totals
}

impl UsageSource for VnstatReader {
    fn source(&self) -> Source {
        Source::Vnstat
    }

    fn zone(&self) -> Zone {
        self.zone
    }

    fn interfaces(&self) -> Result<Vec<InterfaceInfo>> {
        Ok(self
            .snapshot()?
            .interfaces
            .iter()
            .map(|(info, _)| info.clone())
            .collect())
    }

    fn connections(&self) -> Result<Vec<ConnectionInfo>> {
        Ok(Vec::new())
    }

    fn totals(&self, scope: &Scope, from: i64, to: i64) -> Result<Totals> {
        let snapshot = self.snapshot()?;
        let histories = self.select(&snapshot, scope)?;
        let mut totals = Totals::default();
        for (granularity, a, b) in plan(self.zone, from, to) {
            for history in &histories {
                totals += match granularity {
                    Granularity::Day => sum_rows(&history.days, a, b),
                    Granularity::Hour => sum_rows(&history.hours, a, b),
                    // No minutes in vnStat: take the hours that overlap.
                    Granularity::Minute => sum_rows(
                        &history.hours,
                        a - a.rem_euclid(3600),
                        (b + 3599).div_euclid(3600) * 3600,
                    ),
                };
            }
        }
        Ok(totals)
    }

    fn series(
        &self,
        scope: &Scope,
        granularity: Granularity,
        from: i64,
        to: i64,
    ) -> Result<Vec<Bucket>> {
        if granularity == Granularity::Minute {
            return Err(Error::Unsupported("vnStat has no per-minute history"));
        }
        let snapshot = self.snapshot()?;
        let histories = self.select(&snapshot, scope)?;
        let starts = bucket_starts(granularity, self.zone, from, to)?;
        let rows = histories.iter().flat_map(|history| match granularity {
            Granularity::Day => history.days.iter().copied(),
            _ => history.hours.iter().copied(),
        });
        let (first, last) = match (starts.first(), starts.last()) {
            (Some(&first), Some(&last)) => (first, last),
            _ => return Ok(Vec::new()),
        };
        let _ = last;
        Ok(fill(
            &starts,
            rows.filter(|&(start, _, _)| start >= first && start < to),
        ))
    }

    fn since_boot(&self, interface: &str) -> Result<Option<Totals>> {
        Ok(live_since_boot(&self.sysfs_root, interface))
    }

    fn since_boot_total(&self) -> Result<Totals> {
        live_since_boot_total(&self.sysfs_root)
    }

    fn last_sample_time(&self) -> Result<Option<i64>> {
        Ok(self
            .snapshot()?
            .interfaces
            .iter()
            .filter_map(|(_, history)| history.updated)
            .max())
    }

    fn is_live(&self, now: i64) -> Result<bool> {
        Ok(self
            .last_sample_time()?
            .is_some_and(|last| now - last < LIVE_WITHIN))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::fake;
    use crate::testutil::TempDir;

    const DAYS: &str = include_str!("../tests/fixtures/vnstat-d.json");
    const HOURS: &str = include_str!("../tests/fixtures/vnstat-h.json");
    const UTC: Zone = Zone::Fixed(0);
    /// 2026-10-22 00:00 UTC.
    const OCT_22: i64 = 1_792_627_200;

    fn reader() -> (TempDir, VnstatReader) {
        let dir = TempDir::new("vnstat");
        fake::interface(dir.path(), "eth0", &[("type", "1\n"), ("device/", "")]);
        fake::interface(dir.path(), "wlan0", &[("type", "1\n"), ("wireless/", "")]);
        fake::counters(dir.path(), "eth0", 123, 45);
        fake::counters(dir.path(), "wlan0", 6, 7);
        // docker0 is deliberately absent from sysfs and not guessable.
        let reader = VnstatReader::from_json(DAYS, HOURS, UTC)
            .unwrap()
            .with_sysfs_root(dir.path());
        (dir, reader)
    }

    #[test]
    fn parses_interfaces_and_drops_virtual_ones() {
        let (_dir, reader) = reader();
        let interfaces = reader.interfaces().unwrap();
        assert_eq!(
            interfaces
                .iter()
                .map(|item| (item.name.as_str(), item.kind))
                .collect::<Vec<_>>(),
            [("eth0", Kind::Ethernet), ("wlan0", Kind::Wifi)]
        );
        assert_eq!(reader.source(), Source::Vnstat);
        assert!(reader.connections().unwrap().is_empty());
    }

    #[test]
    fn day_totals_per_interface_and_overall() {
        let (_dir, reader) = reader();
        let day = 86_400;
        assert_eq!(
            reader
                .totals(
                    &Scope::Interface("eth0".into()),
                    OCT_22 - 2 * day,
                    OCT_22 + day
                )
                .unwrap(),
            Totals {
                rx: 9_000_000,
                tx: 900_000
            }
        );
        assert_eq!(
            reader.totals(&Scope::All, OCT_22 - day, OCT_22).unwrap(),
            Totals {
                rx: 4_000_500,
                tx: 400_050
            },
            "eth0 + wlan0 on the 21st; docker0 is not counted"
        );
        assert_eq!(
            reader
                .totals(&Scope::InterfaceId(2), OCT_22, OCT_22 + day)
                .unwrap(),
            Totals { rx: 100, tx: 10 }
        );
    }

    #[test]
    fn hour_totals_and_series_use_timestamp_or_date() {
        let (_dir, reader) = reader();
        // eth0 has timestamps, wlan0 only date/time: both land on 00:00 and 01:00.
        let first_two = reader.totals(&Scope::All, OCT_22, OCT_22 + 7200).unwrap();
        assert_eq!(
            first_two,
            Totals {
                rx: 1_000_000 + 600_000 + 60 + 40,
                tx: 100_000 + 60_000 + 6 + 4
            }
        );
        let series = reader
            .series(
                &Scope::Interface("wlan0".into()),
                Granularity::Hour,
                OCT_22,
                OCT_22 + 4 * 3600,
            )
            .unwrap();
        assert_eq!(series.len(), 4);
        assert_eq!(
            series[0],
            Bucket {
                start: OCT_22,
                rx: 60,
                tx: 6
            }
        );
        assert_eq!(
            series[1],
            Bucket {
                start: OCT_22 + 3600,
                rx: 40,
                tx: 4
            }
        );
        assert_eq!(series[2].rx, 0);
        let days = reader
            .series(
                &Scope::All,
                Granularity::Day,
                OCT_22 - 2 * 86_400,
                OCT_22 + 1,
            )
            .unwrap();
        assert_eq!(
            days.iter().map(|bucket| bucket.rx).collect::<Vec<_>>(),
            [3_000_000, 4_000_500, 2_000_100]
        );
    }

    #[test]
    fn a_range_edge_inside_an_hour_rounds_outward() {
        let (_dir, reader) = reader();
        // 00:30 to 01:30 overlaps the 00:00 and 01:00 hours.
        let totals = reader
            .totals(
                &Scope::Interface("eth0".into()),
                OCT_22 + 1800,
                OCT_22 + 5400,
            )
            .unwrap();
        assert_eq!(totals.rx, 1_600_000);
    }

    #[test]
    fn unsupported_queries_say_so() {
        let (_dir, reader) = reader();
        assert!(matches!(
            reader.totals(&Scope::Connection("Home".into()), 0, 10),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            reader.series(&Scope::All, Granularity::Minute, 0, 600),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            reader.totals(&Scope::Interface("nope0".into()), 0, 10),
            Err(Error::UnknownScope(_))
        ));
    }

    #[test]
    fn scope_all_leaves_out_vpn_interfaces() {
        let days = r#"{"jsonversion":"2","interfaces":[
            {"name":"eth0","traffic":{"day":[{"date":{"year":2026,"month":10,"day":22},"rx":100,"tx":10}]}},
            {"name":"wg0","traffic":{"day":[{"date":{"year":2026,"month":10,"day":22},"rx":90,"tx":9}]}}]}"#;
        let reader = VnstatReader::from_json(days, "{}", UTC)
            .unwrap()
            .with_sysfs_root("/nonexistent-sysfs");
        assert_eq!(reader.interfaces().unwrap().len(), 2);
        assert_eq!(
            reader.totals(&Scope::All, OCT_22, OCT_22 + 86_400).unwrap(),
            Totals { rx: 100, tx: 10 }
        );
        assert_eq!(
            reader
                .totals(&Scope::Interface("wg0".into()), OCT_22, OCT_22 + 86_400)
                .unwrap(),
            Totals { rx: 90, tx: 9 }
        );
        let series = reader
            .series(&Scope::All, Granularity::Day, OCT_22, OCT_22 + 1)
            .unwrap();
        assert_eq!(series[0].rx, 100);
    }

    #[test]
    fn since_boot_comes_from_the_kernel_counters() {
        let (_dir, reader) = reader();
        assert_eq!(
            reader.since_boot("eth0").unwrap(),
            Some(Totals { rx: 123, tx: 45 })
        );
        assert_eq!(
            reader.since_boot_total().unwrap(),
            Totals { rx: 129, tx: 52 }
        );
    }

    #[test]
    fn liveness_follows_vnstats_updated_stamp() {
        let (_dir, reader) = reader();
        // updated: 2026-10-22 02:30.
        let updated = OCT_22 + 2 * 3600 + 1800;
        assert_eq!(reader.last_sample_time().unwrap(), Some(updated));
        assert!(reader.is_live(updated + 600).unwrap());
        assert!(!reader.is_live(updated + 3 * 3600).unwrap());
    }

    #[test]
    fn garbage_and_old_versions_are_errors() {
        assert!(VnstatReader::from_json("not json", HOURS, UTC).is_err());
        assert!(
            VnstatReader::from_json(r#"{"jsonversion":"1","interfaces":[]}"#, HOURS, UTC).is_err()
        );
        let empty =
            VnstatReader::from_json(r#"{"jsonversion":"2","interfaces":[]}"#, "{}", UTC).unwrap();
        assert!(empty.interfaces().unwrap().is_empty());
        assert_eq!(
            empty.totals(&Scope::All, 0, 100).unwrap(),
            Totals::default()
        );
    }

    #[test]
    fn missing_hourly_history_still_answers_by_day() {
        let reader = VnstatReader::from_json(DAYS, "garbage", UTC)
            .unwrap()
            .with_sysfs_root("/nonexistent-sysfs");
        // Names are classified by the name guess without sysfs.
        let totals = reader.totals(&Scope::All, OCT_22, OCT_22 + 86_400).unwrap();
        assert_eq!(totals.rx, 2_000_100);
    }
}
