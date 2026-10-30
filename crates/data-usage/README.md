# bingux-data-usage

Network data usage for the Bingux desktop. A **recorder** samples the kernel
byte counters of the interfaces you pay for (Ethernet, Wi-Fi, mobile, VPN) once
a minute into a SQLite database. **Readers** answer "how much today / this week /
this billing cycle, per interface or per connection" for Bingux Settings. It can
also read history from **vnStat** when that is installed.

`bingux-statusd` hosts the recorder on a thread, so there is no extra service:
`bingux.target` already starts it. Nothing here needs root.

```
bingux-statusd ── Recorder ──writes──▶  ~/.local/share/bingux/data-usage.sqlite3
Bingux Settings ─ Reader ────reads───▶  (WAL, read-only connection)
                  Settings ──writes──▶  the `settings` table only
```

## Where the data lives

`$XDG_DATA_HOME/bingux/data-usage.sqlite3`, `~/.local/share/bingux/data-usage.sqlite3`
by default (a relative `XDG_DATA_HOME` is ignored, as the XDG spec says).
The directory is created with mode `0700`. `default_path()` returns it.

The database uses WAL (`journal_mode = WAL`, `synchronous = NORMAL`) and a 5 s
`busy_timeout`, so Settings can read while the recorder writes. The recorder
is the only writer of `samples`, `hourly`, `daily`, `counters` and `meta`.
Settings writes only `settings`, in single-statement transactions.
`PRAGMA user_version` holds the schema version; migrations are in
`schema::MIGRATIONS` and run inside one `BEGIN IMMEDIATE` transaction. A
database from a newer version of this crate is refused (`Error::TooNew`), never
downgraded. A reader that finds an older, not yet migrated database reports
`Error::Outdated` (the recorder or `Settings::open` migrates it).

## Schema (version 1)

```sql
CREATE TABLE interfaces (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('wifi', 'ethernet', 'mobile', 'vpn')),
    mac  TEXT
);

CREATE TABLE connections (
    id      INTEGER PRIMARY KEY,
    nm_uuid TEXT NOT NULL UNIQUE,
    name    TEXT NOT NULL,
    kind    TEXT NOT NULL CHECK (kind IN ('wifi', 'ethernet', 'mobile', 'vpn')),
    metered INTEGER NOT NULL DEFAULT 0 CHECK (metered IN (0, 1))
);

-- Traffic per minute: deltas, not counters. bucket_start is a UTC epoch
-- second divisible by 60.
CREATE TABLE samples (
    interface_id  INTEGER NOT NULL REFERENCES interfaces (id) ON DELETE CASCADE,
    connection_id INTEGER REFERENCES connections (id) ON DELETE CASCADE,
    bucket_start  INTEGER NOT NULL,
    rx_bytes      INTEGER NOT NULL DEFAULT 0 CHECK (rx_bytes >= 0),
    tx_bytes      INTEGER NOT NULL DEFAULT 0 CHECK (tx_bytes >= 0)
);
CREATE UNIQUE INDEX samples_key ON samples (interface_id, COALESCE(connection_id, 0), bucket_start);
CREATE INDEX samples_time ON samples (bucket_start);

-- Same shape. bucket_start is a UTC epoch second divisible by 3600.
CREATE TABLE hourly (
    interface_id  INTEGER NOT NULL REFERENCES interfaces (id) ON DELETE CASCADE,
    connection_id INTEGER REFERENCES connections (id) ON DELETE CASCADE,
    bucket_start  INTEGER NOT NULL,
    rx_bytes      INTEGER NOT NULL DEFAULT 0 CHECK (rx_bytes >= 0),
    tx_bytes      INTEGER NOT NULL DEFAULT 0 CHECK (tx_bytes >= 0)
);
CREATE UNIQUE INDEX hourly_key ON hourly (interface_id, COALESCE(connection_id, 0), bucket_start);
CREATE INDEX hourly_time ON hourly (bucket_start);

-- Same shape. bucket_start is the UTC epoch second of LOCAL midnight.
CREATE TABLE daily (
    interface_id  INTEGER NOT NULL REFERENCES interfaces (id) ON DELETE CASCADE,
    connection_id INTEGER REFERENCES connections (id) ON DELETE CASCADE,
    bucket_start  INTEGER NOT NULL,
    rx_bytes      INTEGER NOT NULL DEFAULT 0 CHECK (rx_bytes >= 0),
    tx_bytes      INTEGER NOT NULL DEFAULT 0 CHECK (tx_bytes >= 0)
);
CREATE UNIQUE INDEX daily_key ON daily (interface_id, COALESCE(connection_id, 0), bucket_start);
CREATE INDEX daily_time ON daily (bucket_start);

-- billing_day, monthly_limit_bytes, warn_percent. A NULL connection_id is
-- the global row; a connection row overrides it for that connection.
CREATE TABLE settings (
    connection_id INTEGER REFERENCES connections (id) ON DELETE CASCADE,
    key           TEXT NOT NULL,
    value         TEXT NOT NULL
);
CREATE UNIQUE INDEX settings_key ON settings (COALESCE(connection_id, 0), key);

-- The last raw kernel counters of each interface, so a recorder restart
-- neither loses nor double counts traffic.
CREATE TABLE counters (
    interface_id INTEGER PRIMARY KEY REFERENCES interfaces (id) ON DELETE CASCADE,
    rx_raw       INTEGER NOT NULL,
    tx_raw       INTEGER NOT NULL,
    ifindex      INTEGER NOT NULL,
    boot_id      TEXT NOT NULL,
    sampled_at   INTEGER NOT NULL
);

-- last_tick: when the recorder last completed a sampling round.
-- samples_complete_from: minute data before this time was pruned.
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
```

`NULL` is distinct in SQLite keys, so the uniqueness of
`(interface, connection, bucket)` is enforced with a unique index on
`COALESCE(connection_id, 0)`; real connection ids start at 1. Upserts use that
expression as their conflict target.

### Buckets and time zones

| Table     | `bucket_start`                                   | Rows are          | Kept        |
| --------- | ------------------------------------------------ | ----------------- | ----------- |
| `samples` | UTC epoch second, multiple of 60                 | deltas per minute | 7 days      |
| `hourly`  | UTC epoch second, multiple of 3600               | deltas per UTC hour | 90 days   |
| `daily`   | UTC epoch second of **local midnight**           | deltas per local day | forever  |

The daily rollup uses the **local calendar day** because that is what a
person means by "today" and what a billing day means. The local zone comes
from the C library (`localtime_r`/`mktime`), so `TZ` and daylight saving time are
honoured and a day is 23 or 25 hours long on a DST change. Rows are never
rewritten when the zone changes later: a day already recorded keeps its
boundaries. The `Reader` maps each stored row to the bucket whose span contains its
`bucket_start`, so a different zone shifts the chart by at most a day edge, never
loses data. `Config::zone` / `Reader::with_zone` accept `Zone::Fixed(seconds)` for
deterministic days (tests, servers).

Hourly buckets are UTC hours, so in a zone with a half-hour offset (India) one
hour straddles local midnight; daily rows are always computed from the minutes,
never from the hours, so the day totals stay exact.

Zero deltas are not stored. A missing bucket means no traffic;
`series()` returns it zero-filled.

## Recording

`Recorder::new(Config)` does not touch the disk; `run(&stop)` samples every
`Config::interval` (60 s) until stopped.

1. **Interfaces.** `class/net/<if>` under `Config::sysfs_root` (`/sys`), classified
   by `kind::classify` as a pure function of that directory:

   | Kind     | Rule                                                                              |
   | -------- | --------------------------------------------------------------------------------- |
   | wifi     | `wireless/` dir, `phy80211` link or `DEVTYPE=wlan` (`p2p-dev-*` skipped)            |
   | mobile   | `DEVTYPE=wwan` or a `ww*` name (`wwan0`, `wwp0s20f0u6c3`)                          |
   | vpn      | `DEVTYPE=wireguard`, `DEVTYPE=ppp`/`ppp*`, or a `tun_flags` file (tun/tap)         |
   | ethernet | ARPHRD_ETHER with a backing `device` and nothing virtual about it                  |

   Skipped: `lo`, bridges (`bridge/`, `DEVTYPE=bridge`), veth, docker/podman networks,
   VLANs, bonds, macvlan, dummy, anything Ethernet without a hardware `device`, and
   a tap that is a bridge port (libvirt `vnet*`).
2. **Deltas.** Counters are `statistics/rx_bytes` and `tx_bytes`; the last raw
   reading of each interface is in `counters`, with the boot id
   (`/proc/sys/kernel/random/boot_id`) and `ifindex`.
   * First reading of an interface: a baseline only, nothing is counted, unless the
     interface appeared while the recorder was already running, in which case its
     counters started at zero and count in full.
   * Boot id changed since the stored counter: the counters restarted at boot, so
     the reading since boot is counted.
   * `ifindex` changed (interface re-created): reset, the reading counts from zero.
   * A counter lower than the stored one (wrap, driver reload): that direction is
     reset, the new value counts from zero.
   * Otherwise the difference. After a recorder restart, this includes traffic
     moved while the recorder was down, credited to the first minute after.

   The same transaction updates `counters`, `samples`, `hourly`, `daily` and
   `meta.last_tick`, so the rollups always equal the sums of the finer data and a
   crash loses nothing and double counts nothing.
3. **Connections.** `nmcli -t -f DEVICE,CONNECTION,CON-UUID device`, at most once
   per 60 s (cached, 3 s timeout, killed on expiry) attributes an interface's
   traffic to its active connection. A new connection's metered flag comes from
   `nmcli -g connection.metered connection show uuid <uuid>` (`yes`/`guess-yes` are
   metered), re-read hourly. When `nmcli` is missing or fails, the connection is
   `NULL`; sampling never waits for or depends on it.
4. **Maintenance.** After each round, once an hour: minute samples older than 7 days
   and hourly rows older than 90 days are deleted; `meta.samples_complete_from`
   records how far back the minutes are complete. `Recorder::rebuild_rollups()`
   recomputes `hourly` and `daily` from the minutes that are still complete (repair
   after a crash during development, a manual edit or a time-zone change), and
   leaves older rows alone.
5. **Failures.** A database error (disk full, corrupt file, locked) drops the
   connection, is logged once a minute at most (`[bingux-data-usage] recording
   failed: ...`) and the next round starts from scratch. `run` returns only when
   stopped. No panic ever reaches the host process.

## Reading

```rust
use bingux_data_usage::{Periods, Reader, Scope, Granularity, UsageSource, Zone, unix_now};

let reader = Reader::open_default()?;           // Err(NotFound) until the recorder has run
let now = unix_now();
let settings = reader.settings(None)?;          // billing_day, monthly_limit_bytes, warn_percent
let periods = Periods::at(Zone::Local, now, settings.billing_day);

let today = reader.totals(&Scope::All, periods.today.0, periods.today.1)?;
let cycle = reader.totals(&Scope::All, periods.cycle.0, periods.cycle.1)?;
println!("today {} B down, {} B up", today.rx, today.tx);

// A dense, zero-filled chart series: hourly for today, daily for the cycle.
let hours = reader.series(&Scope::All, Granularity::Hour, periods.today.0, periods.today.1)?;
let days = reader.series(&Scope::Interface("wlan0".into()), Granularity::Day, periods.cycle.0, now)?;

for (interface, totals) in reader.interface_totals(periods.month.0, periods.month.1)? { /* ... */ }
for (connection, totals) in reader.connection_totals(periods.month.0, periods.month.1)? {
    if connection.metered { /* ... */ }
}
let since_boot = reader.since_boot_total()?;    // live kernel counters: works before any history
let running = reader.is_live(now)?;             // false => "The data usage recorder is not running."
# Ok::<(), bingux_data_usage::Error>(())
```

`UsageSource` is the object-safe trait both readers implement; hold a
`Box<dyn UsageSource>` and call `source()` for the name to show.

| Method                                  | Returns                                                                            |
| --------------------------------------- | ---------------------------------------------------------------------------------- |
| `source()`                              | `Source::Own` or `Source::Vnstat` (`label()` for the UI)                           |
| `interfaces()` / `connections()`        | `InterfaceInfo { id, name, kind, mac }` / `ConnectionInfo { id, uuid, name, kind, metered }` |
| `totals(&Scope, from, to)`              | `Totals { rx, tx }` for `[from, to)`, built from the coarsest rows that fit exactly |
| `series(&Scope, Granularity, from, to)` | dense `Vec<Bucket { start, rx, tx }>`, one per slot (max 200 000)                    |
| `since_boot(name)` / `since_boot_total()` | `Totals` from the live kernel counters                                           |
| `last_sample_time()` / `is_live(now)`   | when the recorder last finished a round; live = under 180 s ago                    |
| `interface_totals(from, to)` / `connection_totals(from, to)` | per-scope totals for the "By interface/connection" lists  |

`Scope` is `All`, `Interface(name)`, `InterfaceId(id)`, `Connection(uuid or name)`,
`ConnectionId(id)`. Traffic with no known NetworkManager connection is in
`All` and in its interface, never in a connection scope. `All` (and
`since_boot_total()`) leaves out VPN interfaces, whose traffic also crosses the
physical link and would be counted twice; they stay listed and are queryable by
`Interface`/`Connection`. An unknown scope is
`Error::UnknownScope`.

Totals are exact on the retained data: whole local days come from `daily`, whole
UTC hours from `hourly`, and only the ragged edges from minutes (a minute that
overlaps the range counts whole). Ranges older than the minute retention resolve
to hours, and older than the hourly retention to days.

### Periods

`Periods::at(zone, now, billing_day)` gives half-open `(from, to)` ranges for
`today`, `week` (starts Monday 00:00 local), `month` and the billing `cycle`
(`cycle.1` is when the next one starts). The pure helpers
`billing_cycle(today, billing_day) -> (Date, Date)`, `week_start(Date)` and
`month_start(Date)` are what it is built from. A billing day a month is too short
for (31 in April) starts the cycle on that month's last day.

### Settings

```rust
let settings = bingux_data_usage::Settings::open_default()?;   // creates the database if missing
settings.set_billing_day(15, None)?;
settings.set_monthly_limit(Some(50_000_000_000), None)?;       // None switches the limit off
settings.set_warn_percent(90, Some("Home"))?;                  // override for one connection
let effective = settings.load(Some("Home"))?;                  // global rows + that connection's overrides
# Ok::<(), bingux_data_usage::Error>(())
```

Keys: `billing_day` (1 to 31, default 1), `monthly_limit_bytes` (0 or absent = no
limit), `warn_percent` (1 to 100, default 80). A connection override needs the
connection to have been recorded.

## vnStat

`VnstatReader::detect(zone)` returns a reader when `vnstat` is on `PATH` and
`vnstat --json d` prints history. It implements `UsageSource` from
`vnstat --json d` (days) and `vnstat --json h` (hours), cached for 10 s.
`open_best()` prefers it and falls back to the own database.

vnStat has no per-connection and no per-minute history: `Scope::Connection*` and
`Granularity::Minute` fail with `Error::Unsupported`, `connections()` is empty, and a
range edge inside an hour is rounded outward to whole hours. Only interfaces that are
countable (by `/sys`, else by name: `wlan*`, `en*`, `ww*`, `wg*`, `tun*`, `ppp*`) are
listed, so `docker0` and friends are left out of totals as they are in the recorder.
The parser targets vnStat 2's `jsonversion` 2 and is tested against fixtures written
from the documented format (`tests/fixtures/vnstat-*.json`); vnStat was not
installed on the development machine, so it has not been run against a live one.

## Development

```
cargo test                                   # unit and integration tests, temp dirs only
cargo run --example record -- --db /tmp/usage.sqlite3 --interval 5 --duration 60
```

`examples/record.rs` runs the recorder against the real `/sys` and prints what the reader
sees. Without `--db` it writes the real database, the one `bingux-statusd` uses.
