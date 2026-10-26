//! Querying recorded usage: the [`UsageSource`] trait and the SQLite
//! [`Reader`].

use crate::{
    error::{Error, Result},
    kind::{self, Kind},
    schema,
    settings::{self, UsageSettings},
    time::Zone,
};
use rusqlite::{Connection, OptionalExtension};
use std::{
    fmt,
    ops::{Add, AddAssign},
    path::{Path, PathBuf},
};

/// A recorder that has not finished a round for this long is not running.
pub const STALE_AFTER: i64 = 180;

/// Upper bound on the buckets of one [`UsageSource::series`] call.
pub const MAX_BUCKETS: usize = 200_000;

/// Bytes received and sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub rx: u64,
    pub tx: u64,
}

impl Totals {
    /// Received plus sent.
    pub fn total(self) -> u64 {
        self.rx.saturating_add(self.tx)
    }
}

impl Add for Totals {
    type Output = Totals;

    fn add(self, other: Totals) -> Totals {
        Totals {
            rx: self.rx.saturating_add(other.rx),
            tx: self.tx.saturating_add(other.tx),
        }
    }
}

impl AddAssign for Totals {
    fn add_assign(&mut self, other: Totals) {
        *self = *self + other;
    }
}

/// One slot of a series. `start` is a UTC epoch second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bucket {
    pub start: i64,
    pub rx: u64,
    pub tx: u64,
}

/// What a query is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every counted interface except VPNs. VPN traffic also crosses the
    /// physical link underneath it, so adding it would count bytes twice. A
    /// VPN interface stays listed and can be queried with
    /// [`Scope::Interface`] or through its connection.
    All,
    /// One interface by kernel name (`wlan0`).
    Interface(String),
    /// One interface by [`InterfaceInfo::id`].
    InterfaceId(i64),
    /// A connection by NetworkManager uuid, else by name. A name shared by
    /// several recorded connections adds them up.
    Connection(String),
    /// One connection by [`ConnectionInfo::id`].
    ConnectionId(i64),
}

/// The width of a [`Bucket`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Granularity {
    /// One minute; kept 7 days.
    Minute,
    /// One UTC hour; kept 90 days.
    Hour,
    /// One local calendar day; kept forever.
    Day,
}

impl Granularity {
    /// The start of the bucket that contains `timestamp`.
    pub fn floor(self, zone: Zone, timestamp: i64) -> i64 {
        match self {
            Granularity::Minute => timestamp - timestamp.rem_euclid(60),
            Granularity::Hour => timestamp - timestamp.rem_euclid(3600),
            Granularity::Day => zone.day_start(timestamp),
        }
    }

    /// The start of the bucket after the one starting at `start`.
    pub fn next(self, zone: Zone, start: i64) -> i64 {
        match self {
            Granularity::Minute => start + 60,
            Granularity::Hour => start + 3600,
            Granularity::Day => zone.next_day_start(start),
        }
    }

    pub(crate) fn table(self) -> &'static str {
        match self {
            Granularity::Minute => "samples",
            Granularity::Hour => "hourly",
            Granularity::Day => "daily",
        }
    }
}

/// Where the numbers come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// This crate's own SQLite database, written by the recorder.
    Own,
    /// `vnstat --json`.
    Vnstat,
}

impl Source {
    /// A name for the user interface.
    pub fn label(self) -> &'static str {
        match self {
            Source::Own => "Bingux data usage recorder",
            Source::Vnstat => "vnStat",
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceInfo {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    pub mac: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionInfo {
    pub id: i64,
    /// The NetworkManager connection uuid.
    pub uuid: String,
    pub name: String,
    pub kind: Kind,
    pub metered: bool,
}

/// What every history provider answers. Object safe, so a page holds a
/// `Box<dyn UsageSource>` and does not care whether it is the recorder's
/// database or vnStat.
///
/// Time arguments are UTC epoch seconds and ranges are half-open
/// `[from, to)`. A [`Zone`] defines the local days of
/// [`Granularity::Day`]; it must be the zone the data was recorded with.
pub trait UsageSource: Send {
    fn source(&self) -> Source;

    fn zone(&self) -> Zone;

    /// Every interface that has been recorded, by name.
    fn interfaces(&self) -> Result<Vec<InterfaceInfo>>;

    /// Every recorded NetworkManager connection, by name. vnStat knows none:
    /// the list is empty.
    fn connections(&self) -> Result<Vec<ConnectionInfo>>;

    /// Traffic of `scope` in `[from, to)`.
    ///
    /// The answer is exact on the retained data, built from the coarsest
    /// rows that fit (whole local days from `daily`, whole UTC hours from
    /// `hourly`, the ragged edges from the minutes). Minute rows live 7
    /// days and hourly rows 90, so the ragged edge of a range older than
    /// that resolves to the data that is left. A minute bucket that overlaps
    /// the range counts whole.
    fn totals(&self, scope: &Scope, from: i64, to: i64) -> Result<Totals>;

    /// A zero-filled series covering `[from, to)`: one [`Bucket`] for every
    /// slot of `granularity`, the first being the slot that contains `from`.
    /// Fails with [`Error::Invalid`] above [`MAX_BUCKETS`] slots.
    fn series(
        &self,
        scope: &Scope,
        granularity: Granularity,
        from: i64,
        to: i64,
    ) -> Result<Vec<Bucket>>;

    /// Bytes moved by one interface since it was created, which is since
    /// boot for hardware, from the kernel counters. Needs no history, so it
    /// works before the recorder has ever run. `None` if the interface does
    /// not exist and nothing was recorded for this boot.
    fn since_boot(&self, interface: &str) -> Result<Option<Totals>>;

    /// [`UsageSource::since_boot`] summed over every counted interface that
    /// exists right now.
    fn since_boot_total(&self) -> Result<Totals>;

    /// When the last sampling round finished, UTC epoch seconds. `None` if
    /// there never was one.
    fn last_sample_time(&self) -> Result<Option<i64>>;

    /// Whether the data is being kept up to date at `now`: the last round was
    /// less than [`STALE_AFTER`] seconds ago.
    fn is_live(&self, now: i64) -> Result<bool> {
        Ok(self
            .last_sample_time()?
            .is_some_and(|last| now - last < STALE_AFTER))
    }

    /// [`UsageSource::totals`] for every recorded interface, by name.
    fn interface_totals(&self, from: i64, to: i64) -> Result<Vec<(InterfaceInfo, Totals)>> {
        self.interfaces()?
            .into_iter()
            .map(|interface| {
                let totals = self.totals(&Scope::InterfaceId(interface.id), from, to)?;
                Ok((interface, totals))
            })
            .collect()
    }

    /// [`UsageSource::totals`] for every recorded connection, by name.
    fn connection_totals(&self, from: i64, to: i64) -> Result<Vec<(ConnectionInfo, Totals)>> {
        self.connections()?
            .into_iter()
            .map(|connection| {
                let totals = self.totals(&Scope::ConnectionId(connection.id), from, to)?;
                Ok((connection, totals))
            })
            .collect()
    }
}

/// One table to sum over a half-open range.
pub(crate) type Segment = (Granularity, i64, i64);

/// Splits `[from, to)` into the coarsest rows that cover it exactly: whole
/// local days, then whole hours at either end, then minutes.
pub(crate) fn plan(zone: Zone, from: i64, to: i64) -> Vec<Segment> {
    fn hours_and_minutes(out: &mut Vec<Segment>, from: i64, to: i64) {
        if to <= from {
            return;
        }
        let first_hour = (from + 3599).div_euclid(3600) * 3600;
        let last_hour = to.div_euclid(3600) * 3600;
        if first_hour < last_hour {
            minutes(out, from, first_hour);
            out.push((Granularity::Hour, first_hour, last_hour));
            minutes(out, last_hour, to);
        } else {
            minutes(out, from, to);
        }
    }
    fn minutes(out: &mut Vec<Segment>, from: i64, to: i64) {
        if to > from {
            out.push((Granularity::Minute, from, to));
        }
    }

    let mut out = Vec::new();
    if to <= from {
        return out;
    }
    let first_day = {
        let start = zone.day_start(from);
        if start == from {
            from
        } else {
            zone.next_day_start(from)
        }
    };
    let last_day = zone.day_start(to);
    if first_day < last_day {
        hours_and_minutes(&mut out, from, first_day);
        out.push((Granularity::Day, first_day, last_day));
        hours_and_minutes(&mut out, last_day, to);
    } else {
        hours_and_minutes(&mut out, from, to);
    }
    out
}

/// The start of every bucket of `granularity` that overlaps `[from, to)`.
pub(crate) fn bucket_starts(
    granularity: Granularity,
    zone: Zone,
    from: i64,
    to: i64,
) -> Result<Vec<i64>> {
    let mut starts = Vec::new();
    if to <= from {
        return Ok(starts);
    }
    let mut start = granularity.floor(zone, from);
    while start < to {
        if starts.len() >= MAX_BUCKETS {
            return Err(Error::Invalid(format!(
                "a series is limited to {MAX_BUCKETS} buckets; ask for a coarser granularity"
            )));
        }
        starts.push(start);
        start = granularity.next(zone, start);
    }
    Ok(starts)
}

/// Spreads `rows` (any `(start, rx, tx)`) into `starts`: each row lands in
/// the bucket whose span contains its start, so a recorder in another time
/// zone still lands in the right day.
pub(crate) fn fill(starts: &[i64], rows: impl IntoIterator<Item = (i64, u64, u64)>) -> Vec<Bucket> {
    let mut buckets: Vec<Bucket> = starts
        .iter()
        .map(|&start| Bucket {
            start,
            rx: 0,
            tx: 0,
        })
        .collect();
    for (start, rx, tx) in rows {
        let after = starts.partition_point(|&bucket| bucket <= start);
        if after == 0 {
            continue;
        }
        let bucket = &mut buckets[after - 1];
        bucket.rx = bucket.rx.saturating_add(rx);
        bucket.tx = bucket.tx.saturating_add(tx);
    }
    buckets
}

/// Live counters of one interface, if it exists and is countable.
pub(crate) fn live_since_boot(sysfs_root: &Path, interface: &str) -> Option<Totals> {
    kind::classify(sysfs_root, interface)?;
    let (rx, tx) = kind::read_counters(sysfs_root, interface)?;
    Some(Totals { rx, tx })
}

pub(crate) fn live_since_boot_total(sysfs_root: &Path) -> Result<Totals> {
    let mut total = Totals::default();
    // VPN counters are excluded: see [`Scope::All`].
    for interface in kind::scan(sysfs_root)?
        .into_iter()
        .filter(|interface| interface.kind != Kind::Vpn)
    {
        total += Totals {
            rx: interface.rx_bytes,
            tx: interface.tx_bytes,
        };
    }
    Ok(total)
}

/// Reads the recorder's database. Opened read-only: it can run alongside
/// the recorder (WAL) and cannot damage the data.
pub struct Reader {
    connection: Connection,
    zone: Zone,
    sysfs_root: PathBuf,
    proc_root: PathBuf,
}

impl Reader {
    /// Opens the database read-only. Fails with [`Error::NotFound`] if the
    /// recorder has never created it and [`Error::Outdated`] if the recorder
    /// has not yet migrated it; both mean "no data yet".
    pub fn open(path: impl AsRef<Path>) -> Result<Reader> {
        Ok(Reader {
            connection: schema::open_read_only(path.as_ref())?,
            zone: Zone::Local,
            sysfs_root: PathBuf::from(kind::SYSFS),
            proc_root: PathBuf::from("/proc"),
        })
    }

    /// Opens the database at [`crate::default_path`].
    pub fn open_default() -> Result<Reader> {
        Reader::open(schema::default_path()?)
    }

    /// Use another zone for [`Granularity::Day`]; default [`Zone::Local`].
    pub fn with_zone(mut self, zone: Zone) -> Reader {
        self.zone = zone;
        self
    }

    /// Read kernel counters from another sysfs root (tests).
    pub fn with_roots(
        mut self,
        sysfs_root: impl Into<PathBuf>,
        proc_root: impl Into<PathBuf>,
    ) -> Reader {
        self.sysfs_root = sysfs_root.into();
        self.proc_root = proc_root.into();
        self
    }

    /// The effective limit settings: the global rows with the overrides of
    /// `connection` (uuid or name) on top. Writing goes through
    /// [`crate::Settings`].
    pub fn settings(&self, connection: Option<&str>) -> Result<UsageSettings> {
        settings::load(&self.connection, connection)
    }

    /// The SQL restriction for a scope, with ids already resolved.
    fn filter(&self, scope: &Scope) -> Result<String> {
        let unknown = |name: &str| Error::UnknownScope(name.to_owned());
        Ok(match scope {
            Scope::All => {
                " AND interface_id NOT IN (SELECT id FROM interfaces WHERE kind = 'vpn')".to_owned()
            }
            Scope::Interface(name) => {
                let id: i64 = self
                    .connection
                    .query_row("SELECT id FROM interfaces WHERE name = ?1", [name], |row| {
                        row.get(0)
                    })
                    .optional()?
                    .ok_or_else(|| unknown(name))?;
                format!(" AND interface_id = {id}")
            }
            Scope::InterfaceId(id) => {
                self.connection
                    .query_row("SELECT id FROM interfaces WHERE id = ?1", [id], |row| {
                        row.get::<_, i64>(0)
                    })
                    .optional()?
                    .ok_or_else(|| unknown(&id.to_string()))?;
                format!(" AND interface_id = {id}")
            }
            Scope::Connection(key) => {
                // A uuid is unique and wins; only a name can match several.
                let by_uuid: Option<i64> = self
                    .connection
                    .query_row(
                        "SELECT id FROM connections WHERE nm_uuid = ?1",
                        [key],
                        |row| row.get(0),
                    )
                    .optional()?;
                let ids: Vec<i64> = match by_uuid {
                    Some(id) => vec![id],
                    None => {
                        let mut statement = self
                            .connection
                            .prepare("SELECT id FROM connections WHERE name = ?1")?;
                        statement
                            .query_map([key], |row| row.get(0))?
                            .collect::<rusqlite::Result<_>>()?
                    }
                };
                if ids.is_empty() {
                    return Err(unknown(key));
                }
                let list: Vec<String> = ids.iter().map(i64::to_string).collect();
                format!(" AND connection_id IN ({})", list.join(","))
            }
            Scope::ConnectionId(id) => {
                self.connection
                    .query_row("SELECT id FROM connections WHERE id = ?1", [id], |row| {
                        row.get::<_, i64>(0)
                    })
                    .optional()?
                    .ok_or_else(|| unknown(&id.to_string()))?;
                format!(" AND connection_id = {id}")
            }
        })
    }

    fn sum(&self, segment: Segment, filter: &str) -> Result<Totals> {
        let (granularity, from, to) = segment;
        let from = match granularity {
            // A minute bucket is counted whole if it overlaps the range.
            Granularity::Minute => from - from.rem_euclid(60),
            _ => from,
        };
        let (rx, tx): (i64, i64) = self.connection.query_row(
            &format!(
                "SELECT COALESCE(SUM(rx_bytes), 0), COALESCE(SUM(tx_bytes), 0)
                 FROM {} WHERE bucket_start >= ?1 AND bucket_start < ?2{filter}",
                granularity.table()
            ),
            [from, to],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(Totals {
            rx: rx as u64,
            tx: tx as u64,
        })
    }

    fn meta(&self, key: &str) -> Result<Option<i64>> {
        Ok(self
            .connection
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .and_then(|text| text.parse().ok()))
    }

    fn boot_id(&self) -> Option<String> {
        std::fs::read_to_string(self.proc_root.join("sys/kernel/random/boot_id"))
            .ok()
            .map(|text| text.trim().to_owned())
    }
}

fn kind_of(text: String) -> rusqlite::Result<Kind> {
    text.parse().map_err(|()| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown kind {text:?}").into(),
        )
    })
}

impl UsageSource for Reader {
    fn source(&self) -> Source {
        Source::Own
    }

    fn zone(&self) -> Zone {
        self.zone
    }

    fn interfaces(&self) -> Result<Vec<InterfaceInfo>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name, kind, mac FROM interfaces ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            Ok(InterfaceInfo {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: kind_of(row.get(2)?)?,
                mac: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn connections(&self) -> Result<Vec<ConnectionInfo>> {
        let mut statement = self.connection.prepare(
            "SELECT id, nm_uuid, name, kind, metered FROM connections ORDER BY name, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ConnectionInfo {
                id: row.get(0)?,
                uuid: row.get(1)?,
                name: row.get(2)?,
                kind: kind_of(row.get(3)?)?,
                metered: row.get::<_, i64>(4)? != 0,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn totals(&self, scope: &Scope, from: i64, to: i64) -> Result<Totals> {
        let filter = self.filter(scope)?;
        let mut totals = Totals::default();
        for segment in plan(self.zone, from, to) {
            totals += self.sum(segment, &filter)?;
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
        let filter = self.filter(scope)?;
        let starts = bucket_starts(granularity, self.zone, from, to)?;
        let Some(&first) = starts.first() else {
            return Ok(Vec::new());
        };
        let mut statement = self.connection.prepare(&format!(
            "SELECT bucket_start, SUM(rx_bytes), SUM(tx_bytes) FROM {}
             WHERE bucket_start >= ?1 AND bucket_start < ?2{filter}
             GROUP BY bucket_start",
            granularity.table()
        ))?;
        let rows = statement
            .query_map([first, to], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, i64>(2)? as u64,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(fill(&starts, rows))
    }

    fn since_boot(&self, interface: &str) -> Result<Option<Totals>> {
        if let Some(live) = live_since_boot(&self.sysfs_root, interface) {
            return Ok(Some(live));
        }
        // The interface is gone; the recorder's last reading is still the
        // truth for this boot.
        let Some(boot_id) = self.boot_id() else {
            return Ok(None);
        };
        Ok(self
            .connection
            .query_row(
                "SELECT c.rx_raw, c.tx_raw FROM counters c
                 JOIN interfaces i ON i.id = c.interface_id
                 WHERE i.name = ?1 AND c.boot_id = ?2",
                [interface, &boot_id],
                |row| {
                    Ok(Totals {
                        rx: row.get::<_, i64>(0)? as u64,
                        tx: row.get::<_, i64>(1)? as u64,
                    })
                },
            )
            .optional()?)
    }

    fn since_boot_total(&self) -> Result<Totals> {
        live_since_boot_total(&self.sysfs_root)
    }

    fn last_sample_time(&self) -> Result<Option<i64>> {
        self.meta("last_tick")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        kind::fake,
        recorder::{Config, Recorder},
        testutil::TempDir,
        time::Date,
    };
    use std::fs;

    #[test]
    fn plan_uses_coarse_rows_inside_and_fine_rows_at_the_edges() {
        let zone = Zone::Fixed(0);
        let day = |n: i64| n * 86_400;
        // Aligned days only.
        assert_eq!(
            plan(zone, day(10), day(13)),
            [(Granularity::Day, day(10), day(13))]
        );
        // Ragged: 10 d 01:30 to 13 d 02:15.
        let from = day(10) + 5400;
        let to = day(13) + 8100;
        assert_eq!(
            plan(zone, from, to),
            [
                (Granularity::Minute, from, day(10) + 7200),
                (Granularity::Hour, day(10) + 7200, day(11)),
                (Granularity::Day, day(11), day(13)),
                (Granularity::Hour, day(13), day(13) + 7200),
                (Granularity::Minute, day(13) + 7200, to),
            ]
        );
        // Inside one hour.
        assert_eq!(plan(zone, 100, 500), [(Granularity::Minute, 100, 500)]);
        assert!(plan(zone, 500, 500).is_empty());
        assert!(plan(zone, 500, 100).is_empty());
    }

    /// A recorded fixture: a wired and a Wi-Fi interface over a few days.
    struct Fixture {
        dir: TempDir,
        sysfs: PathBuf,
        zone: Zone,
        start: i64,
    }

    /// Records one sample a minute for `minutes` minutes. Each minute, the
    /// wired interface moves 100 down and 10 up; Wi-Fi moves 5 and 1 only in
    /// minutes divisible by 10.
    fn fixture(zone: Zone, minutes: i64) -> Fixture {
        let dir = TempDir::new("reader");
        let sysfs = dir.path().join("sys");
        let proc_root = dir.path().join("proc");
        fs::create_dir_all(proc_root.join("sys/kernel/random")).unwrap();
        fs::write(proc_root.join("sys/kernel/random/boot_id"), "boot-r\n").unwrap();
        fake::interface(
            &sysfs,
            "enp3s0",
            &[("type", "1\n"), ("device/", ""), ("ifindex", "2\n")],
        );
        fake::interface(
            &sysfs,
            "wlan0",
            &[
                ("type", "1\n"),
                ("device/", ""),
                ("wireless/", ""),
                ("ifindex", "3\n"),
            ],
        );
        fake::counters(&sysfs, "enp3s0", 0, 0);
        fake::counters(&sysfs, "wlan0", 0, 0);

        let mut config = Config::new(dir.db());
        config.sysfs_root = sysfs.clone();
        config.proc_root = proc_root;
        config.zone = zone;
        config.resolver = Box::new(crate::nm::NoConnections);
        config.log = Box::new(|_| {});
        let recorder = Recorder::new(config);

        // Begin on a local midnight so the days are predictable: 2026-10-20.
        let start = zone.date_start(Date::new(2026, 10, 20).unwrap());
        recorder.sample_at(start).unwrap();
        let (mut wired_rx, mut wired_tx, mut wifi_rx, mut wifi_tx) = (0, 0, 0, 0);
        for minute in 1..=minutes {
            wired_rx += 100;
            wired_tx += 10;
            if minute % 10 == 0 {
                wifi_rx += 5;
                wifi_tx += 1;
            }
            fake::counters(&sysfs, "enp3s0", wired_rx, wired_tx);
            fake::counters(&sysfs, "wlan0", wifi_rx, wifi_tx);
            recorder.sample_at(start + minute * 60).unwrap();
        }
        Fixture {
            dir,
            sysfs,
            zone,
            start,
        }
    }

    impl Fixture {
        fn reader(&self) -> Reader {
            Reader::open(self.dir.db())
                .unwrap()
                .with_zone(self.zone)
                .with_roots(&self.sysfs, self.dir.path().join("proc"))
        }
    }

    #[test]
    fn totals_agree_across_every_table() {
        let zone = Zone::Fixed(2 * 3600);
        let fixture = fixture(zone, 3 * 1440); // three full local days
        let reader = fixture.reader();
        let start = fixture.start;

        // Per minute across both interfaces: 100 + 10 plus every tenth minute 6.
        let all = reader
            .totals(&Scope::All, start, start + 3 * 86_400 + 1)
            .unwrap();
        assert_eq!(all.rx, 3 * 1440 * 100 + 3 * 144 * 5);
        assert_eq!(all.tx, 3 * 1440 * 10 + 3 * 144);

        // The same range answered from days, from hours and from minutes.
        for granularity in [Granularity::Minute, Granularity::Hour, Granularity::Day] {
            let series = reader
                .series(&Scope::All, granularity, start, start + 3 * 86_400 + 60)
                .unwrap();
            let rx: u64 = series.iter().map(|bucket| bucket.rx).sum();
            let tx: u64 = series.iter().map(|bucket| bucket.tx).sum();
            assert_eq!((rx, tx), (all.rx, all.tx), "{granularity:?}");
        }

        // Sampling began at `start`, so day 0 holds minutes 1 to 1439.
        let wired = reader
            .totals(&Scope::Interface("enp3s0".into()), start, start + 86_400)
            .unwrap();
        assert_eq!(
            wired,
            Totals {
                rx: 143_900,
                tx: 14_390
            }
        );
        let wifi = reader
            .totals(&Scope::InterfaceId(2), start, start + 86_400)
            .unwrap();
        assert_eq!(wifi, Totals { rx: 715, tx: 143 });
        assert_eq!(
            wired + wifi,
            reader.totals(&Scope::All, start, start + 86_400).unwrap()
        );
    }

    #[test]
    fn ragged_ranges_split_into_hours_and_minutes() {
        let zone = Zone::Fixed(0);
        let fixture = fixture(zone, 2 * 1440);
        let reader = fixture.reader();
        let start = fixture.start;
        // 01:30 on day 0 to 02:15 on day 1: 24h45m = 1485 minutes of the
        // wired interface (a minute bucket at 'to' is excluded).
        let from = start + 5400;
        let to = start + 86_400 + 8100;
        let wired = reader
            .totals(&Scope::Interface("enp3s0".into()), from, to)
            .unwrap();
        assert_eq!(wired.rx, 100 * 1485);
        assert_eq!(wired.tx, 10 * 1485);
        // An empty and an inverted range are zero.
        assert_eq!(
            reader.totals(&Scope::All, to, to).unwrap(),
            Totals::default()
        );
        assert_eq!(
            reader.totals(&Scope::All, to, from).unwrap(),
            Totals::default()
        );
    }

    #[test]
    fn series_is_dense_and_aligned() {
        let zone = Zone::Fixed(3600);
        let fixture = fixture(zone, 90);
        let reader = fixture.reader();
        let start = fixture.start;
        let series = reader
            .series(
                &Scope::Interface("enp3s0".into()),
                Granularity::Hour,
                start - 3600,
                start + 4 * 3600,
            )
            .unwrap();
        assert_eq!(series.len(), 5);
        assert_eq!(
            series[0],
            Bucket {
                start: start - 3600,
                rx: 0,
                tx: 0
            }
        );
        // Sampling began at `start`, so the first hour holds 59 deltas.
        assert_eq!(series[1].rx, 59 * 100);
        assert_eq!(series[2].rx, 31 * 100);
        assert_eq!(series[3].rx, 0);
        let minutes = reader
            .series(&Scope::All, Granularity::Minute, start + 30, start + 150)
            .unwrap();
        assert_eq!(
            minutes
                .iter()
                .map(|bucket| bucket.start)
                .collect::<Vec<_>>(),
            [start, start + 60, start + 120]
        );
        let days = reader
            .series(
                &Scope::All,
                Granularity::Day,
                start + 3600,
                start + 3 * 86_400,
            )
            .unwrap();
        assert_eq!(
            days.iter().map(|bucket| bucket.start).collect::<Vec<_>>(),
            [start, start + 86_400, start + 2 * 86_400]
        );
        assert!(
            reader
                .series(&Scope::All, Granularity::Day, 5, 5)
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            reader.series(&Scope::All, Granularity::Minute, 0, 10_000_000_000),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn unknown_scopes_are_reported() {
        let fixture = fixture(Zone::Fixed(0), 5);
        let reader = fixture.reader();
        assert!(matches!(
            reader.totals(&Scope::Interface("nope0".into()), 0, 10),
            Err(Error::UnknownScope(_))
        ));
        assert!(matches!(
            reader.totals(&Scope::Connection("nothing".into()), 0, 10),
            Err(Error::UnknownScope(_))
        ));
        assert!(matches!(
            reader.totals(&Scope::InterfaceId(999), 0, 10),
            Err(Error::UnknownScope(_))
        ));
    }

    #[test]
    fn lists_interfaces_and_per_interface_totals() {
        let fixture = fixture(Zone::Fixed(0), 20);
        let reader = fixture.reader();
        let interfaces = reader.interfaces().unwrap();
        assert_eq!(
            interfaces
                .iter()
                .map(|item| (item.name.as_str(), item.kind))
                .collect::<Vec<_>>(),
            [("enp3s0", Kind::Ethernet), ("wlan0", Kind::Wifi)]
        );
        let totals = reader
            .interface_totals(fixture.start, fixture.start + 86_400)
            .unwrap();
        assert_eq!(totals[0].1.rx, 2000);
        assert_eq!(totals[1].1, Totals { rx: 10, tx: 2 });
        assert!(reader.connections().unwrap().is_empty());
        assert_eq!(reader.source(), Source::Own);
    }

    #[test]
    fn connections_by_uuid_or_name() {
        let fixture = fixture(Zone::Fixed(0), 3);
        {
            let writer = rusqlite::Connection::open(fixture.dir.db()).unwrap();
            writer
                .execute_batch(
                    "INSERT INTO connections (nm_uuid, name, kind, metered) VALUES
                        ('u1', 'Cafe', 'wifi', 1), ('u2', 'Cafe', 'wifi', 0), ('u3', 'Home', 'wifi', 0);
                     INSERT INTO samples VALUES (1, 1, 60, 10, 1), (1, 2, 60, 20, 2), (1, 3, 60, 40, 4), (1, NULL, 60, 80, 8);",
                )
                .unwrap();
        }
        let reader = fixture.reader();
        let (from, to) = (0, 120);
        assert_eq!(
            reader
                .totals(&Scope::Connection("u1".into()), from, to)
                .unwrap(),
            Totals { rx: 10, tx: 1 }
        );
        assert_eq!(
            reader
                .totals(&Scope::Connection("Cafe".into()), from, to)
                .unwrap(),
            Totals { rx: 30, tx: 3 }
        );
        assert_eq!(
            reader.totals(&Scope::ConnectionId(3), from, to).unwrap(),
            Totals { rx: 40, tx: 4 }
        );
        // NULL-connection traffic only shows in unscoped queries.
        assert_eq!(
            reader
                .totals(&Scope::Interface("enp3s0".into()), from, to)
                .unwrap()
                .rx,
            150
        );
        let connections = reader.connections().unwrap();
        assert_eq!(connections.len(), 3);
        assert!(connections[0].metered);
        let per_connection = reader.connection_totals(from, to).unwrap();
        assert_eq!(
            per_connection
                .iter()
                .map(|(_, totals)| totals.rx)
                .sum::<u64>(),
            70
        );
    }

    #[test]
    fn scope_all_leaves_out_vpn_interfaces() {
        let fixture = fixture(Zone::Fixed(0), 5);
        fake::interface(
            &fixture.sysfs,
            "wg0",
            &[
                ("type", "65534\n"),
                ("uevent", "DEVTYPE=wireguard\n"),
                ("ifindex", "9\n"),
            ],
        );
        fake::counters(&fixture.sysfs, "wg0", 0, 0);
        {
            let writer = rusqlite::Connection::open(fixture.dir.db()).unwrap();
            writer
                .execute_batch(
                    "INSERT INTO interfaces (name, kind) VALUES ('wg0', 'vpn');
                     INSERT INTO samples (interface_id, bucket_start, rx_bytes, tx_bytes)
                         SELECT id, 60, 7777, 777 FROM interfaces WHERE name = 'wg0';
                     INSERT INTO hourly (interface_id, bucket_start, rx_bytes, tx_bytes)
                         SELECT id, 0, 7777, 777 FROM interfaces WHERE name = 'wg0';
                     INSERT INTO daily (interface_id, bucket_start, rx_bytes, tx_bytes)
                         SELECT id, 0, 7777, 777 FROM interfaces WHERE name = 'wg0';",
                )
                .unwrap();
        }
        let reader = fixture.reader();
        let (from, to) = (fixture.start, fixture.start + 86_400);
        let all = reader.totals(&Scope::All, from, to).unwrap();
        assert_eq!(all, Totals { rx: 500, tx: 50 }, "the VPN is not added");
        // The VPN is still listed and queryable on its own.
        assert!(
            reader
                .interfaces()
                .unwrap()
                .iter()
                .any(|item| item.name == "wg0")
        );
        let vpn = reader
            .totals(&Scope::Interface("wg0".into()), 0, i64::from(i32::MAX))
            .unwrap();
        assert_eq!(vpn, Totals { rx: 7777, tx: 777 });
        for granularity in [Granularity::Minute, Granularity::Hour, Granularity::Day] {
            let series = reader.series(&Scope::All, granularity, 0, 86_400).unwrap();
            assert_eq!(
                series.iter().map(|bucket| bucket.rx).sum::<u64>(),
                0,
                "{granularity:?}"
            );
        }
        // The live counters skip it too.
        fake::counters(&fixture.sysfs, "wg0", 4000, 400);
        assert_eq!(
            reader.since_boot_total().unwrap(),
            Totals { rx: 500, tx: 50 }
        );
        assert_eq!(
            reader.since_boot("wg0").unwrap(),
            Some(Totals { rx: 4000, tx: 400 })
        );
    }

    #[test]
    fn since_boot_reads_live_counters_without_any_history() {
        let fixture = fixture(Zone::Fixed(0), 5);
        let reader = fixture.reader();
        assert_eq!(
            reader.since_boot("enp3s0").unwrap(),
            Some(Totals { rx: 500, tx: 50 })
        );
        assert_eq!(
            reader.since_boot("wlan0").unwrap(),
            Some(Totals { rx: 0, tx: 0 })
        );
        assert_eq!(
            reader.since_boot_total().unwrap(),
            Totals { rx: 500, tx: 50 }
        );
        assert_eq!(reader.since_boot("lo").unwrap(), None);
        // The interface vanishes: the recorder's last counters stand in for
        // this boot, and not for another one.
        fs::remove_dir_all(fixture.sysfs.join("class/net/enp3s0")).unwrap();
        assert_eq!(
            reader.since_boot("enp3s0").unwrap(),
            Some(Totals { rx: 500, tx: 50 })
        );
        fs::write(
            fixture.dir.path().join("proc/sys/kernel/random/boot_id"),
            "boot-other\n",
        )
        .unwrap();
        assert_eq!(reader.since_boot("enp3s0").unwrap(), None);
    }

    #[test]
    fn last_sample_time_and_liveness() {
        let fixture = fixture(Zone::Fixed(0), 3);
        let reader = fixture.reader();
        let last = fixture.start + 3 * 60;
        assert_eq!(reader.last_sample_time().unwrap(), Some(last));
        assert!(reader.is_live(last + 60).unwrap());
        assert!(!reader.is_live(last + STALE_AFTER).unwrap());
    }

    #[test]
    fn reader_runs_while_the_writer_writes() {
        let dir = TempDir::new("concurrent");
        let sysfs = dir.path().join("sys");
        let proc_root = dir.path().join("proc");
        fs::create_dir_all(proc_root.join("sys/kernel/random")).unwrap();
        fs::write(proc_root.join("sys/kernel/random/boot_id"), "b\n").unwrap();
        fake::interface(
            &sysfs,
            "enp3s0",
            &[("type", "1\n"), ("device/", ""), ("ifindex", "2\n")],
        );
        fake::counters(&sysfs, "enp3s0", 0, 0);
        let mut config = Config::new(dir.db());
        config.sysfs_root = sysfs.clone();
        config.proc_root = proc_root;
        config.zone = Zone::Fixed(0);
        config.resolver = Box::new(crate::nm::NoConnections);
        config.log = Box::new(|_| {});
        let recorder = Recorder::new(config);
        recorder.sample_at(600).unwrap();

        let path = dir.db();
        let writer = std::thread::spawn(move || {
            for step in 1..=300u64 {
                fake::counters(&sysfs, "enp3s0", step * 10, step);
                recorder.sample_at(600 + step as i64 * 60).unwrap();
            }
        });
        let mut last_seen = 0;
        let mut reads = 0;
        while !writer.is_finished() || reads == 0 {
            let reader = Reader::open(&path).unwrap().with_zone(Zone::Fixed(0));
            // One read transaction, so the three queries see the same snapshot.
            reader.connection.execute_batch("BEGIN").unwrap();
            let totals = reader.totals(&Scope::All, 0, i64::from(i32::MAX)).unwrap();
            // Totals only grow, and hourly and minute data agree because each
            // round is one transaction.
            assert!(totals.rx >= last_seen);
            assert_eq!(totals.rx % 10, 0);
            let sum = |granularity| -> u64 {
                reader
                    .series(&Scope::All, granularity, 0, 30_000)
                    .unwrap()
                    .iter()
                    .map(|bucket| bucket.rx)
                    .sum()
            };
            assert_eq!(sum(Granularity::Hour), sum(Granularity::Minute));
            assert_eq!(sum(Granularity::Day), sum(Granularity::Minute));
            reader.connection.execute_batch("COMMIT").unwrap();
            last_seen = totals.rx;
            reads += 1;
        }
        writer.join().unwrap();
        assert!(reads > 0);
        let reader = Reader::open(&path).unwrap().with_zone(Zone::Fixed(0));
        assert_eq!(
            reader
                .totals(&Scope::All, 0, i64::from(i32::MAX))
                .unwrap()
                .rx,
            3000
        );
    }
}
