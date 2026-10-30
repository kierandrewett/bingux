//! The recorder: samples kernel counters, writes deltas, rolls them up and
//! prunes old data. It is the only writer of `samples`, `hourly`, `daily`,
//! `counters` and `meta`.

use crate::{
    error::{Error, Result},
    kind::{self, NetInterface},
    nm::{ActiveConnection, ConnectionResolver, NmcliResolver},
    schema,
    time::{Zone, unix_now},
};
use parking_lot::{Condvar, Mutex, MutexGuard};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// A failure is logged at most this often.
const ERROR_LOG_INTERVAL: Duration = Duration::from_secs(60);
/// Pruning runs this often (UTC seconds).
const PRUNE_INTERVAL: i64 = 3600;
/// `connections.metered` is re-read from NetworkManager this often.
const METERED_REFRESH: i64 = 3600;

/// How long data is kept. Daily rows are kept forever.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retention {
    /// Minute samples; default 7 days.
    pub minutes: Duration,
    /// Hourly rollups; default 90 days.
    pub hourly: Duration,
}

impl Default for Retention {
    fn default() -> Self {
        Retention {
            minutes: Duration::from_secs(7 * 86_400),
            hourly: Duration::from_secs(90 * 86_400),
        }
    }
}

/// Everything a [`Recorder`] needs. [`Config::new`] gives the production
/// defaults; the rest are for tests and the dev runner.
pub struct Config {
    /// The SQLite file; its directory is created with mode 0700.
    pub db_path: PathBuf,
    /// Time between samples; default 60 s. Buckets are one minute wide, so
    /// going below that only makes the data more precise in time, not finer.
    pub interval: Duration,
    /// Where `class/net/...` lives; default `/sys`.
    pub sysfs_root: PathBuf,
    /// Where `sys/kernel/random/boot_id` lives; default `/proc`.
    pub proc_root: PathBuf,
    /// Defines where a day starts for the `daily` table; default local time.
    pub zone: Zone,
    pub retention: Retention,
    /// Source of the active connection per device; default `nmcli`.
    pub resolver: Box<dyn ConnectionResolver>,
    /// Where messages go; default stderr with a `[bingux-data-usage]` prefix.
    pub log: Box<dyn Fn(&str) + Send + Sync>,
}

impl Config {
    pub fn new(db_path: impl Into<PathBuf>) -> Config {
        Config {
            db_path: db_path.into(),
            interval: Duration::from_secs(60),
            sysfs_root: PathBuf::from(kind::SYSFS),
            proc_root: PathBuf::from("/proc"),
            zone: Zone::Local,
            retention: Retention::default(),
            resolver: Box::new(NmcliResolver::default()),
            log: Box::new(|message| eprintln!("[bingux-data-usage] {message}")),
        }
    }

    /// A configuration for the default database path (see
    /// [`crate::default_path`]).
    pub fn with_default_path() -> Result<Config> {
        Ok(Config::new(schema::default_path()?))
    }
}

/// A clonable stop signal that also wakes a sleeping [`Recorder::run`].
#[derive(Clone, Default)]
pub struct Stop(Arc<(Mutex<bool>, Condvar)>);

impl Stop {
    pub fn new() -> Stop {
        Stop::default()
    }

    pub fn stop(&self) {
        let (flag, wake) = &*self.0;
        *flag.lock() = true;
        wake.notify_all();
    }

    pub fn is_stopped(&self) -> bool {
        *self.0.0.lock()
    }

    /// Sleeps up to `timeout`; returns `true` as soon as the stop is
    /// signalled.
    pub fn wait(&self, timeout: Duration) -> bool {
        let (flag, wake) = &*self.0;
        let deadline = Instant::now() + timeout;
        let mut stopped = flag.lock();
        while !*stopped {
            if wake.wait_until(&mut stopped, deadline).timed_out() {
                break;
            }
        }
        *stopped
    }
}

/// What one sampling round saw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SampleSummary {
    /// Interfaces that were counted.
    pub interfaces: usize,
    /// Bytes received and sent since the previous round, summed.
    pub rx: u64,
    pub tx: u64,
}

/// How much [`Recorder::prune_at`] removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pruned {
    pub minutes: usize,
    pub hours: usize,
}

/// Records usage into the database. Create one with [`Recorder::new`] and
/// call [`Recorder::run`] on a thread, or drive it with
/// [`Recorder::sample_once`].
pub struct Recorder {
    interval: Duration,
    inner: Mutex<Inner>,
}

struct Inner {
    db_path: PathBuf,
    sysfs_root: PathBuf,
    proc_root: PathBuf,
    zone: Zone,
    retention: Retention,
    resolver: Box<dyn ConnectionResolver>,
    log: Box<dyn Fn(&str) + Send + Sync>,
    connection: Option<Connection>,
    /// A sampling round has completed in this process, so an interface with
    /// no stored counter appeared after we started: its counters began at 0.
    scanned_before: bool,
    last_prune: Option<i64>,
    metered_checked: HashMap<String, i64>,
    last_error: Option<(Instant, String)>,
    resolver_failing: bool,
}

impl Recorder {
    /// Never fails and never touches the disk: the database is opened (and
    /// re-opened) by the sampling rounds, so a full or corrupt disk cannot
    /// stop the host process from starting.
    pub fn new(config: Config) -> Recorder {
        Recorder {
            interval: config.interval.max(Duration::from_secs(1)),
            inner: Mutex::new(Inner {
                db_path: config.db_path,
                sysfs_root: config.sysfs_root,
                proc_root: config.proc_root,
                zone: config.zone,
                retention: config.retention,
                resolver: config.resolver,
                log: config.log,
                connection: None,
                scanned_before: false,
                last_prune: None,
                metered_checked: HashMap::new(),
                last_error: None,
                resolver_failing: false,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock()
    }

    /// Samples every interval until `stop` is signalled. Errors (database
    /// full, corrupt, locked) are logged at most once a minute and the next
    /// round tries again from scratch; this function only returns when
    /// stopped.
    pub fn run(&self, stop: &Stop) {
        while !stop.is_stopped() {
            let started = Instant::now();
            self.lock().tick(unix_now());
            if stop.wait(self.interval.saturating_sub(started.elapsed())) {
                break;
            }
        }
    }

    /// One sampling round at the wall-clock time.
    pub fn sample_once(&self) -> Result<SampleSummary> {
        self.sample_at(unix_now())
    }

    /// One sampling round pretending it is `now` (UTC epoch seconds).
    pub fn sample_at(&self, now: i64) -> Result<SampleSummary> {
        self.lock().sample(now)
    }

    /// Deletes data older than the retention at `now`.
    pub fn prune_at(&self, now: i64) -> Result<Pruned> {
        self.lock().prune(now)
    }

    /// Recomputes `hourly` and `daily` from the minute samples that are still
    /// complete. Rows older than the oldest complete day or hour (minute data
    /// already pruned) are left alone. Use after a crash during development,
    /// a changed time zone or a manual edit.
    pub fn rebuild_rollups(&self) -> Result<()> {
        let mut inner = self.lock();
        let zone = inner.zone;
        let mut connection = inner.open()?;
        let result = rebuild_rollups(&mut connection, zone);
        if result.is_ok() {
            inner.connection = Some(connection);
        }
        result
    }
}

impl Inner {
    fn open(&mut self) -> Result<Connection> {
        match self.connection.take() {
            Some(connection) => Ok(connection),
            None => schema::open_read_write(&self.db_path),
        }
    }

    fn report(&mut self, message: String) {
        let due = match &self.last_error {
            Some((at, _)) => at.elapsed() >= ERROR_LOG_INTERVAL,
            None => true,
        };
        if due {
            (self.log)(&message);
            self.last_error = Some((Instant::now(), message));
        }
    }

    /// A sampling round plus maintenance, with failures contained.
    fn tick(&mut self, now: i64) {
        let outcome = self.sample(now).and_then(|_| {
            if self
                .last_prune
                .is_none_or(|last| now - last >= PRUNE_INTERVAL)
            {
                self.prune(now)?;
            }
            Ok(())
        });
        match outcome {
            Ok(()) => {
                if self.last_error.take().is_some() {
                    (self.log)("recording resumed");
                }
            }
            Err(error) => self.report(format!("recording failed: {error}")),
        }
    }

    fn sample(&mut self, now: i64) -> Result<SampleSummary> {
        let interfaces = kind::scan(&self.sysfs_root)?;
        let boot_id = fs::read_to_string(self.proc_root.join("sys/kernel/random/boot_id"))
            .map(|text| text.trim().to_owned())
            .unwrap_or_else(|_| "unknown".to_owned());

        // Resolve before opening the transaction: nmcli can take seconds and
        // must not hold the write lock.
        let active = match self.resolver.active(now) {
            Ok(active) => {
                self.resolver_failing = false;
                active
            }
            Err(error) => {
                if !self.resolver_failing {
                    self.resolver_failing = true;
                    (self.log)(&format!("connection attribution unavailable: {error}"));
                }
                Vec::new()
            }
        };
        let mut metered = HashMap::new();
        for connection in &active {
            if !interfaces.iter().any(|item| item.name == connection.device) {
                continue;
            }
            let due = self
                .metered_checked
                .get(&connection.uuid)
                .is_none_or(|checked| now - checked >= METERED_REFRESH);
            if due {
                if let Some(value) = self.resolver.metered(&connection.uuid) {
                    metered.insert(connection.uuid.clone(), value);
                }
                self.metered_checked.insert(connection.uuid.clone(), now);
            }
        }

        let mut connection = self.open()?;
        let summary = write_round(
            &mut connection,
            &RoundInput {
                now,
                zone: self.zone,
                interfaces: &interfaces,
                boot_id: &boot_id,
                active: &active,
                metered: &metered,
                created_while_running: self.scanned_before,
            },
        )?;
        // Only a connection whose round committed is kept; after an error it
        // is dropped and reopened, which also recovers from a replaced file.
        self.connection = Some(connection);
        self.scanned_before = true;
        Ok(summary)
    }

    fn prune(&mut self, now: i64) -> Result<Pruned> {
        let mut connection = self.open()?;
        let minute_cutoff = now - self.retention.minutes.as_secs() as i64;
        let minute_cutoff = minute_cutoff - minute_cutoff.rem_euclid(60);
        let hour_cutoff = now - self.retention.hourly.as_secs() as i64;
        let hour_cutoff = hour_cutoff - hour_cutoff.rem_euclid(3600);

        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let minutes = transaction.execute(
            "DELETE FROM samples WHERE bucket_start < ?1",
            [minute_cutoff],
        )?;
        let hours =
            transaction.execute("DELETE FROM hourly WHERE bucket_start < ?1", [hour_cutoff])?;
        let complete_from = meta_i64(&transaction, "samples_complete_from")?.unwrap_or(0);
        set_meta(
            &transaction,
            "samples_complete_from",
            complete_from.max(minute_cutoff),
        )?;
        transaction.commit()?;
        self.connection = Some(connection);
        self.last_prune = Some(now);
        Ok(Pruned { minutes, hours })
    }
}

/// The previous raw counters of an interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredCounter {
    pub rx: u64,
    pub tx: u64,
    pub ifindex: i64,
    pub boot_id: String,
}

/// The bytes to credit for a new reading, or `None` for "baseline only".
///
/// * No stored counter: the interface appeared while the recorder was
///   running (`created_while_running`), so its counters started at zero and
///   count in full; otherwise this is only a baseline.
/// * A different boot id, or a different `ifindex` (the interface was
///   re-created), means the counters restarted from zero: the reading is the
///   delta.
/// * A counter lower than the stored one (wrap, driver reset) is a reset of
///   that direction: the reading is its delta.
/// * Otherwise the delta is the difference. After a recorder restart that
///   includes traffic that flowed while the recorder was down.
pub(crate) fn counter_delta(
    stored: Option<&StoredCounter>,
    rx: u64,
    tx: u64,
    ifindex: i64,
    boot_id: &str,
    created_while_running: bool,
) -> Option<(u64, u64)> {
    let Some(stored) = stored else {
        return created_while_running.then_some((rx, tx));
    };
    let recreated = stored.ifindex != 0 && ifindex != 0 && stored.ifindex != ifindex;
    if stored.boot_id != boot_id || recreated {
        return Some((rx, tx));
    }
    let step = |now: u64, before: u64| if now >= before { now - before } else { now };
    Some((step(rx, stored.rx), step(tx, stored.tx)))
}

struct RoundInput<'a> {
    now: i64,
    zone: Zone,
    interfaces: &'a [NetInterface],
    boot_id: &'a str,
    active: &'a [ActiveConnection],
    metered: &'a HashMap<String, bool>,
    created_while_running: bool,
}

fn write_round(connection: &mut Connection, input: &RoundInput) -> Result<SampleSummary> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let minute = input.now - input.now.rem_euclid(60);
    let hour = input.now - input.now.rem_euclid(3600);
    let day = input.zone.day_start(input.now);
    let mut summary = SampleSummary::default();

    for interface in input.interfaces {
        let interface_id = upsert_interface(&transaction, interface)?;
        let stored = load_counter(&transaction, interface_id)?;
        let delta = counter_delta(
            stored.as_ref(),
            interface.rx_bytes,
            interface.tx_bytes,
            interface.ifindex,
            input.boot_id,
            input.created_while_running,
        );
        transaction.execute(
            "INSERT INTO counters (interface_id, rx_raw, tx_raw, ifindex, boot_id, sampled_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (interface_id) DO UPDATE SET
                 rx_raw = excluded.rx_raw, tx_raw = excluded.tx_raw,
                 ifindex = excluded.ifindex, boot_id = excluded.boot_id,
                 sampled_at = excluded.sampled_at",
            params![
                interface_id,
                interface.rx_bytes as i64,
                interface.tx_bytes as i64,
                interface.ifindex,
                input.boot_id,
                input.now
            ],
        )?;
        summary.interfaces += 1;

        let connection_id = match input
            .active
            .iter()
            .find(|item| item.device == interface.name)
        {
            Some(active) => Some(upsert_connection(
                &transaction,
                active,
                interface,
                input.metered.get(&active.uuid).copied(),
            )?),
            None => None,
        };
        let Some((rx, tx)) = delta.filter(|(rx, tx)| rx + tx > 0) else {
            continue;
        };
        summary.rx += rx;
        summary.tx += tx;
        for (table, start) in [("samples", minute), ("hourly", hour), ("daily", day)] {
            add_traffic(
                &transaction,
                table,
                interface_id,
                connection_id,
                start,
                rx,
                tx,
            )?;
        }
    }
    set_meta(&transaction, "last_tick", input.now)?;
    transaction.commit()?;
    Ok(summary)
}

fn upsert_interface(transaction: &Transaction, interface: &NetInterface) -> Result<i64> {
    Ok(transaction.query_row(
        "INSERT INTO interfaces (name, kind, mac) VALUES (?1, ?2, ?3)
         ON CONFLICT (name) DO UPDATE SET kind = excluded.kind, mac = COALESCE(excluded.mac, mac)
         RETURNING id",
        params![interface.name, interface.kind.as_str(), interface.mac],
        |row| row.get(0),
    )?)
}

fn upsert_connection(
    transaction: &Transaction,
    active: &ActiveConnection,
    interface: &NetInterface,
    metered: Option<bool>,
) -> Result<i64> {
    Ok(transaction.query_row(
        "INSERT INTO connections (nm_uuid, name, kind, metered) VALUES (?1, ?2, ?3, COALESCE(?4, 0))
         ON CONFLICT (nm_uuid) DO UPDATE SET
             name = excluded.name, kind = excluded.kind, metered = COALESCE(?4, metered)
         RETURNING id",
        params![active.uuid, active.name, interface.kind.as_str(), metered],
        |row| row.get(0),
    )?)
}

fn load_counter(transaction: &Transaction, interface_id: i64) -> Result<Option<StoredCounter>> {
    Ok(transaction
        .query_row(
            "SELECT rx_raw, tx_raw, ifindex, boot_id FROM counters WHERE interface_id = ?1",
            [interface_id],
            |row| {
                Ok(StoredCounter {
                    rx: row.get::<_, i64>(0)? as u64,
                    tx: row.get::<_, i64>(1)? as u64,
                    ifindex: row.get(2)?,
                    boot_id: row.get(3)?,
                })
            },
        )
        .optional()?)
}

/// Adds a delta to one bucket of one of the three traffic tables.
fn add_traffic(
    transaction: &Transaction,
    table: &'static str,
    interface_id: i64,
    connection_id: Option<i64>,
    bucket_start: i64,
    rx: u64,
    tx: u64,
) -> Result<()> {
    transaction
        .prepare_cached(&format!(
            "INSERT INTO {table} (interface_id, connection_id, bucket_start, rx_bytes, tx_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (interface_id, COALESCE(connection_id, 0), bucket_start) DO UPDATE SET
             rx_bytes = rx_bytes + excluded.rx_bytes, tx_bytes = tx_bytes + excluded.tx_bytes"
        ))?
        .execute(params![
            interface_id,
            connection_id,
            bucket_start,
            rx as i64,
            tx as i64
        ])?;
    Ok(())
}

fn meta_i64(transaction: &Transaction, key: &str) -> Result<Option<i64>> {
    let text: Option<String> = transaction
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(text.and_then(|text| text.parse().ok()))
}

fn set_meta(transaction: &Transaction, key: &str, value: i64) -> Result<()> {
    transaction.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value.to_string()],
    )?;
    Ok(())
}

fn rebuild_rollups(connection: &mut Connection, zone: Zone) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let complete_from = meta_i64(&transaction, "samples_complete_from")?.unwrap_or(0);
    let hour_from = (complete_from + 3599).div_euclid(3600) * 3600;
    let day_from = {
        let start = zone.day_start(complete_from);
        if start == complete_from {
            start
        } else {
            zone.next_day_start(complete_from)
        }
    };

    transaction.execute("DELETE FROM hourly WHERE bucket_start >= ?1", [hour_from])?;
    transaction.execute(
        "INSERT INTO hourly (interface_id, connection_id, bucket_start, rx_bytes, tx_bytes)
         SELECT interface_id, connection_id, bucket_start - bucket_start % 3600,
                SUM(rx_bytes), SUM(tx_bytes)
         FROM samples WHERE bucket_start >= ?1
         GROUP BY interface_id, connection_id, bucket_start - bucket_start % 3600",
        [hour_from],
    )?;

    // Local midnight is not expressible in SQL, so days are grouped here.
    let mut days: BTreeMap<(i64, Option<i64>, i64), (i64, i64)> = BTreeMap::new();
    {
        let mut statement = transaction.prepare(
            "SELECT interface_id, connection_id, bucket_start, rx_bytes, tx_bytes
             FROM samples WHERE bucket_start >= ?1",
        )?;
        let mut rows = statement.query([day_from])?;
        while let Some(row) = rows.next()? {
            let key = (row.get(0)?, row.get(1)?, zone.day_start(row.get(2)?));
            let entry = days.entry(key).or_default();
            entry.0 += row.get::<_, i64>(3)?;
            entry.1 += row.get::<_, i64>(4)?;
        }
    }
    transaction.execute("DELETE FROM daily WHERE bucket_start >= ?1", [day_from])?;
    for ((interface_id, connection_id, day), (rx, tx)) in days {
        transaction.execute(
            "INSERT INTO daily (interface_id, connection_id, bucket_start, rx_bytes, tx_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![interface_id, connection_id, day, rx, tx],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

impl From<Error> for std::io::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Io(error) => error,
            other => std::io::Error::other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{kind::fake, testutil::TempDir};
    use std::path::Path;

    /// Attribution from a fixed list.
    #[derive(Clone)]
    struct Stub {
        active: Arc<Mutex<Result<Vec<ActiveConnection>>>>,
        metered: Arc<Mutex<HashMap<String, bool>>>,
    }

    impl Default for Stub {
        fn default() -> Self {
            Stub {
                active: Arc::new(Mutex::new(Ok(Vec::new()))),
                metered: Arc::default(),
            }
        }
    }

    impl ConnectionResolver for Stub {
        fn active(&mut self, _now: i64) -> Result<Vec<ActiveConnection>> {
            match &*self.active.lock() {
                Ok(list) => Ok(list.clone()),
                Err(error) => Err(Error::Command(error.to_string())),
            }
        }

        fn metered(&mut self, uuid: &str) -> Option<bool> {
            self.metered.lock().get(uuid).copied()
        }
    }

    pub(crate) struct Rig {
        pub dir: TempDir,
        pub sysfs: PathBuf,
        pub proc_root: PathBuf,
        pub logs: Arc<Mutex<Vec<String>>>,
    }

    impl Rig {
        pub fn new() -> Rig {
            let dir = TempDir::new("rec");
            let sysfs = dir.path().join("sys");
            let proc_root = dir.path().join("proc");
            fs::create_dir_all(proc_root.join("sys/kernel/random")).unwrap();
            let rig = Rig {
                dir,
                sysfs,
                proc_root,
                logs: Arc::default(),
            };
            rig.set_boot("boot-1");
            rig
        }

        pub fn set_boot(&self, id: &str) {
            fs::write(
                self.proc_root.join("sys/kernel/random/boot_id"),
                format!("{id}\n"),
            )
            .unwrap();
        }

        pub fn counters(&self, name: &str, rx: u64, tx: u64) {
            fake::counters(&self.sysfs, name, rx, tx);
        }

        pub fn db(&self) -> PathBuf {
            self.dir.db()
        }

        pub fn config(&self, zone: Zone) -> Config {
            let logs = self.logs.clone();
            let mut config = Config::new(self.db());
            config.sysfs_root = self.sysfs.clone();
            config.proc_root = self.proc_root.clone();
            config.zone = zone;
            config.resolver = Box::new(crate::nm::NoConnections);
            config.log = Box::new(move |message| logs.lock().push(message.to_owned()));
            config
        }

        pub fn recorder(&self, zone: Zone) -> Recorder {
            Recorder::new(self.config(zone))
        }

        /// One wired interface starting at `rx`/`tx`.
        pub fn wired(&self, rx: u64, tx: u64) {
            fake::interface(
                &self.sysfs,
                "enp3s0",
                &[("type", "1\n"), ("device/", ""), ("ifindex", "2\n")],
            );
            self.counters("enp3s0", rx, tx);
        }
    }

    fn sum(path: &Path, table: &str) -> (i64, i64) {
        let connection = Connection::open(path).unwrap();
        connection
            .query_row(
                &format!(
                    "SELECT COALESCE(SUM(rx_bytes),0), COALESCE(SUM(tx_bytes),0) FROM {table}"
                ),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    const T0: i64 = 1_760_000_040; // a minute boundary

    #[test]
    fn first_sample_is_only_a_baseline_then_deltas_count() {
        let rig = Rig::new();
        rig.wired(1_000_000, 500_000);
        let recorder = rig.recorder(Zone::Fixed(0));

        let first = recorder.sample_at(T0).unwrap();
        assert_eq!(
            first,
            SampleSummary {
                interfaces: 1,
                rx: 0,
                tx: 0
            }
        );
        assert_eq!(sum(&rig.db(), "samples"), (0, 0));

        rig.counters("enp3s0", 1_000_300, 500_100);
        let second = recorder.sample_at(T0 + 60).unwrap();
        assert_eq!((second.rx, second.tx), (300, 100));
        assert_eq!(sum(&rig.db(), "samples"), (300, 100));
        assert_eq!(sum(&rig.db(), "hourly"), (300, 100));
        assert_eq!(sum(&rig.db(), "daily"), (300, 100));
    }

    #[test]
    fn two_rounds_in_one_minute_share_a_bucket() {
        let rig = Rig::new();
        rig.wired(0, 0);
        let recorder = rig.recorder(Zone::Fixed(0));
        recorder.sample_at(T0).unwrap();
        rig.counters("enp3s0", 10, 1);
        recorder.sample_at(T0 + 5).unwrap();
        rig.counters("enp3s0", 30, 2);
        recorder.sample_at(T0 + 10).unwrap();
        let connection = Connection::open(rig.db()).unwrap();
        let rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM samples", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        assert_eq!(sum(&rig.db(), "samples"), (30, 2));
    }

    #[test]
    fn a_counter_going_backwards_is_a_reset() {
        let rig = Rig::new();
        rig.wired(5_000, 5_000);
        let recorder = rig.recorder(Zone::Fixed(0));
        recorder.sample_at(T0).unwrap();
        rig.counters("enp3s0", 5_500, 5_200);
        recorder.sample_at(T0 + 60).unwrap();
        // Driver reload: the counters start again from nothing.
        rig.counters("enp3s0", 70, 9);
        recorder.sample_at(T0 + 120).unwrap();
        assert_eq!(sum(&rig.db(), "samples"), (500 + 70, 200 + 9));
    }

    #[test]
    fn a_recorder_restart_neither_loses_nor_double_counts() {
        let rig = Rig::new();
        rig.wired(1_000, 1_000);
        rig.recorder(Zone::Fixed(0)).sample_at(T0).unwrap();
        rig.counters("enp3s0", 1_400, 1_100);
        rig.recorder(Zone::Fixed(0)).sample_at(T0 + 60).unwrap();
        // Same counters again from a third process: nothing to add.
        rig.recorder(Zone::Fixed(0)).sample_at(T0 + 120).unwrap();
        assert_eq!(sum(&rig.db(), "samples"), (400, 100));
    }

    #[test]
    fn a_new_boot_counts_everything_since_boot() {
        let rig = Rig::new();
        rig.wired(9_000_000, 9_000_000);
        rig.recorder(Zone::Fixed(0)).sample_at(T0).unwrap();
        // Rebooted: boot id changed, counters restarted and reached 700/300.
        rig.set_boot("boot-2");
        rig.counters("enp3s0", 700, 300);
        let summary = rig.recorder(Zone::Fixed(0)).sample_at(T0 + 3600).unwrap();
        assert_eq!((summary.rx, summary.tx), (700, 300));
        assert_eq!(sum(&rig.db(), "samples"), (700, 300));
    }

    #[test]
    fn an_interface_created_while_running_counts_from_zero() {
        let rig = Rig::new();
        rig.wired(100, 100);
        let recorder = rig.recorder(Zone::Fixed(0));
        recorder.sample_at(T0).unwrap();
        // A VPN comes up between rounds with 40/60 bytes already moved.
        fake::interface(
            &rig.sysfs,
            "wg0",
            &[
                ("type", "65534\n"),
                ("uevent", "DEVTYPE=wireguard\n"),
                ("ifindex", "9\n"),
            ],
        );
        rig.counters("wg0", 40, 60);
        recorder.sample_at(T0 + 60).unwrap();
        assert_eq!(sum(&rig.db(), "samples"), (40, 60));
    }

    #[test]
    fn a_recreated_interface_is_a_reset_even_if_the_counter_is_higher() {
        let rig = Rig::new();
        rig.wired(100, 100);
        let recorder = rig.recorder(Zone::Fixed(0));
        recorder.sample_at(T0).unwrap();
        // Same name, new ifindex, and it already moved more than the old one.
        fake::interface(&rig.sysfs, "enp3s0", &[("ifindex", "11\n")]);
        rig.counters("enp3s0", 500, 400);
        recorder.sample_at(T0 + 60).unwrap();
        assert_eq!(sum(&rig.db(), "samples"), (500, 400));
    }

    #[test]
    fn counter_delta_table() {
        let stored = |rx, tx| StoredCounter {
            rx,
            tx,
            ifindex: 2,
            boot_id: "a".into(),
        };
        assert_eq!(counter_delta(None, 5, 6, 2, "a", false), None);
        assert_eq!(counter_delta(None, 5, 6, 2, "a", true), Some((5, 6)));
        assert_eq!(
            counter_delta(Some(&stored(10, 10)), 15, 12, 2, "a", false),
            Some((5, 2))
        );
        assert_eq!(
            counter_delta(Some(&stored(10, 10)), 3, 12, 2, "a", false),
            Some((3, 2))
        );
        assert_eq!(
            counter_delta(Some(&stored(10, 10)), 15, 12, 2, "b", false),
            Some((15, 12))
        );
        assert_eq!(
            counter_delta(Some(&stored(10, 10)), 15, 12, 3, "a", false),
            Some((15, 12))
        );
        assert_eq!(
            counter_delta(Some(&stored(10, 10)), 15, 12, 0, "a", false),
            Some((5, 2))
        );
        assert_eq!(
            counter_delta(Some(&stored(10, 10)), 10, 10, 2, "a", false),
            Some((0, 0))
        );
    }

    #[test]
    fn traffic_is_attributed_to_the_active_connection() {
        let rig = Rig::new();
        rig.wired(0, 0);
        let stub = Stub::default();
        *stub.active.lock() = Ok(vec![ActiveConnection {
            device: "enp3s0".into(),
            uuid: "uuid-1".into(),
            name: "Office".into(),
        }]);
        stub.metered.lock().insert("uuid-1".into(), true);
        let mut config = rig.config(Zone::Fixed(0));
        config.resolver = Box::new(stub.clone());
        let recorder = Recorder::new(config);
        recorder.sample_at(T0).unwrap();
        rig.counters("enp3s0", 100, 50);
        recorder.sample_at(T0 + 60).unwrap();

        let connection = Connection::open(rig.db()).unwrap();
        let row: (String, String, i64, i64, i64) = connection
            .query_row(
                "SELECT c.name, c.kind, c.metered, s.rx_bytes, s.tx_bytes
                 FROM samples s JOIN connections c ON c.id = s.connection_id",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row, ("Office".into(), "ethernet".into(), 1, 100, 50));

        // The connection changes: later traffic is a different row.
        *stub.active.lock() = Ok(vec![ActiveConnection {
            device: "enp3s0".into(),
            uuid: "uuid-2".into(),
            name: "Lab".into(),
        }]);
        rig.counters("enp3s0", 160, 60);
        recorder.sample_at(T0 + 120).unwrap();
        let split: i64 = connection
            .query_row(
                "SELECT COUNT(DISTINCT connection_id) FROM samples",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(split, 2);
        assert_eq!(sum(&rig.db(), "samples"), (160, 60));
    }

    #[test]
    fn failing_attribution_leaves_the_connection_null_and_logs_once() {
        let rig = Rig::new();
        rig.wired(0, 0);
        let stub = Stub::default();
        *stub.active.lock() = Err(Error::Command("nmcli: not installed".into()));
        let mut config = rig.config(Zone::Fixed(0));
        config.resolver = Box::new(stub);
        let recorder = Recorder::new(config);
        recorder.sample_at(T0).unwrap();
        rig.counters("enp3s0", 10, 10);
        recorder.sample_at(T0 + 60).unwrap();
        rig.counters("enp3s0", 20, 20);
        recorder.sample_at(T0 + 120).unwrap();

        let connection = Connection::open(rig.db()).unwrap();
        let attributed: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM samples WHERE connection_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attributed, 0);
        assert_eq!(sum(&rig.db(), "samples"), (20, 20));
        assert_eq!(rig.logs.lock().len(), 1);
    }

    /// Writes traffic across local midnight and checks that every coarser
    /// table equals the sums of the finer one.
    #[test]
    fn rollups_match_the_minutes_across_local_midnight() {
        // UTC+2: local midnight is 22:00 UTC.
        let zone = Zone::Fixed(2 * 3600);
        let rig = Rig::new();
        rig.wired(0, 0);
        let recorder = rig.recorder(zone);
        let midnight = zone.date_start(zone.date_of(T0).add_days(1));
        let begin = midnight - 5 * 60;
        let mut raw = 0;
        recorder.sample_at(begin).unwrap();
        for step in 1..=12 {
            raw += 1000 + step;
            rig.counters("enp3s0", raw, raw / 2);
            recorder.sample_at(begin + step as i64 * 60).unwrap();
        }
        let connection = Connection::open(rig.db()).unwrap();
        let total = sum(&rig.db(), "samples");
        assert_eq!(sum(&rig.db(), "hourly"), total);
        assert_eq!(sum(&rig.db(), "daily"), total);

        let days: Vec<(i64, i64)> = connection
            .prepare("SELECT bucket_start, rx_bytes FROM daily ORDER BY bucket_start")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(days.len(), 2, "traffic spans two local days");
        assert_eq!(days[1].0, midnight);
        assert_eq!(days[0].0, midnight - 86_400);
        let before: i64 = connection
            .query_row(
                "SELECT SUM(rx_bytes) FROM samples WHERE bucket_start < ?1",
                [midnight],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(days[0].1, before);
        assert_eq!(days[0].1 + days[1].1, total.0);
    }

    #[test]
    fn rebuild_repairs_damaged_rollups() {
        let zone = Zone::Fixed(-3 * 3600);
        let rig = Rig::new();
        rig.wired(0, 0);
        let recorder = rig.recorder(zone);
        recorder.sample_at(T0).unwrap();
        for step in 1..=200u64 {
            rig.counters("enp3s0", step * 100, step * 10);
            recorder.sample_at(T0 + step as i64 * 60).unwrap();
        }
        let good_hourly = sum(&rig.db(), "hourly");
        let good_daily = sum(&rig.db(), "daily");
        assert_eq!(good_hourly, sum(&rig.db(), "samples"));
        {
            let connection = Connection::open(rig.db()).unwrap();
            connection
                .execute("UPDATE hourly SET rx_bytes = rx_bytes + 7", [])
                .unwrap();
            connection.execute("DELETE FROM daily", []).unwrap();
        }
        assert_ne!(sum(&rig.db(), "hourly"), good_hourly);
        recorder.rebuild_rollups().unwrap();
        assert_eq!(sum(&rig.db(), "hourly"), good_hourly);
        assert_eq!(sum(&rig.db(), "daily"), good_daily);
    }

    #[test]
    fn prune_removes_old_data_by_tier_and_rebuild_keeps_the_rest() {
        let rig = Rig::new();
        rig.wired(0, 0);
        let recorder = rig.recorder(Zone::Fixed(0));
        let now = T0 + 100 * 86_400;
        // Data 100, 30, 8, 6 and 0 days ago.
        let mut raw = 0;
        recorder.sample_at(T0 - 3600).unwrap();
        for days_ago in [100i64, 30, 8, 6, 0] {
            raw += 1000;
            rig.counters("enp3s0", raw, raw);
            recorder.sample_at(now - days_ago * 86_400).unwrap();
        }
        // The first baseline sample was at T0-3600, i.e. 100 days before `now`
        // minus an hour; only the five deltas count.
        assert_eq!(sum(&rig.db(), "samples").0, 5 * 1000);
        let pruned = recorder.prune_at(now).unwrap();
        // Minute samples keep only the last 7 days: 6 and 0 days ago.
        assert_eq!(sum(&rig.db(), "samples").0, 2000);
        assert_eq!(pruned.minutes, 3);
        // Hourly keeps 90 days: drops the 100-day-old hour.
        assert_eq!(sum(&rig.db(), "hourly").0, 4000);
        assert_eq!(pruned.hours, 1);
        // Daily is forever.
        assert_eq!(sum(&rig.db(), "daily").0, 5000);

        // A rebuild must not shrink the history whose minutes are gone.
        recorder.rebuild_rollups().unwrap();
        assert_eq!(sum(&rig.db(), "hourly").0, 4000);
        assert_eq!(sum(&rig.db(), "daily").0, 5000);
    }

    #[test]
    fn database_failure_is_contained_logged_once_and_retried() {
        let rig = Rig::new();
        rig.wired(0, 0);
        // The database's parent is a file: opening fails until it is fixed.
        let blocker = rig.dir.path().join("blocker");
        fs::write(&blocker, "not a directory").unwrap();
        let mut config = rig.config(Zone::Fixed(0));
        config.db_path = blocker.join("data-usage.sqlite3");
        let recorder = Recorder::new(config);

        recorder.lock().tick(T0);
        recorder.lock().tick(T0 + 60);
        recorder.lock().tick(T0 + 120);
        let logs = rig.logs.lock().clone();
        assert_eq!(logs.len(), 1, "{logs:?}");
        assert!(logs[0].starts_with("recording failed"));

        // The disk comes back: the same recorder recovers by itself.
        fs::remove_file(&blocker).unwrap();
        recorder.lock().tick(T0 + 180);
        rig.counters("enp3s0", 5, 5);
        recorder.lock().tick(T0 + 240);
        assert_eq!(sum(&blocker.join("data-usage.sqlite3"), "samples"), (5, 5));
        assert_eq!(rig.logs.lock().last().unwrap(), "recording resumed");
    }

    #[test]
    fn missing_sysfs_is_an_error_not_a_panic() {
        let rig = Rig::new();
        let recorder = rig.recorder(Zone::Fixed(0));
        assert!(matches!(recorder.sample_at(T0), Err(Error::Io(_))));
    }

    #[test]
    fn run_stops_promptly_and_records_last_tick() {
        let rig = Rig::new();
        rig.wired(0, 0);
        let mut config = rig.config(Zone::Fixed(0));
        config.interval = Duration::from_secs(3600);
        let recorder = Arc::new(Recorder::new(config));
        let stop = Stop::new();
        let handle = {
            let (recorder, stop) = (recorder.clone(), stop.clone());
            std::thread::spawn(move || recorder.run(&stop))
        };
        // Wait for the first round to land.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !rig.db().exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let started = Instant::now();
        stop.stop();
        handle.join().unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "run must wake from its sleep"
        );
        let connection = Connection::open(rig.db()).unwrap();
        let tick: String = connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'last_tick'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(tick.parse::<i64>().unwrap() > 1_700_000_000);
    }
}
