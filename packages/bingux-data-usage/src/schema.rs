//! The database: file location, schema, migrations and connection set-up.

use crate::error::{Error, Result};
use rusqlite::{Connection, OpenFlags, Transaction};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    time::Duration,
};

/// How long a connection waits for another connection's write lock. The
/// recorder's transactions last milliseconds, so this is generous.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Version 1: the whole schema. The text is documented verbatim in the
/// README (a test keeps the two equal).
///
/// `samples`, `hourly` and `daily` allow a NULL `connection_id` (traffic with
/// no known NetworkManager connection). NULLs are distinct in SQLite keys, so
/// uniqueness is enforced by an index on `COALESCE(connection_id, 0)`; real
/// connection ids start at 1.
pub const SCHEMA_V1: &str = "\
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
";

/// One schema step. `PRAGMA user_version` records how many have run.
pub enum Step {
    Sql(&'static str),
    Code(fn(&Transaction) -> rusqlite::Result<()>),
}

/// The schema history. A new version appends a step; never edit an
/// existing one, databases in the wild have already run it.
pub const MIGRATIONS: &[Step] = &[Step::Sql(SCHEMA_V1)];

/// The `user_version` of a fully migrated database.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// `$XDG_DATA_HOME/bingux/data-usage.sqlite3`, `~/.local/share/...` by
/// default. A relative `XDG_DATA_HOME` is ignored, as the XDG spec says.
pub fn default_path() -> Result<PathBuf> {
    let absolute = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = absolute("XDG_DATA_HOME")
        .or_else(|| absolute("HOME").map(|home| home.join(".local/share")))
        .ok_or_else(|| Error::Invalid("neither XDG_DATA_HOME nor HOME is set".into()))?;
    Ok(base.join("bingux/data-usage.sqlite3"))
}

fn user_version(connection: &Connection) -> rusqlite::Result<i64> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

/// Runs the steps of `steps` that the database has not seen, in one
/// immediate transaction (so two processes migrating at once serialise and
/// the loser finds nothing to do).
pub fn migrate_with(connection: &mut Connection, steps: &[Step]) -> Result<()> {
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let found = user_version(&transaction)?;
    let latest = steps.len() as i64;
    if found > latest {
        return Err(Error::TooNew {
            found,
            supported: latest,
        });
    }
    for step in &steps[found as usize..] {
        match step {
            Step::Sql(sql) => transaction.execute_batch(sql)?,
            Step::Code(code) => code(&transaction)?,
        }
    }
    if found != latest {
        // PRAGMA cannot take a bound parameter; `latest` is a usize.
        transaction.execute_batch(&format!("PRAGMA user_version = {latest}"))?;
    }
    transaction.commit()?;
    Ok(())
}

/// Opens the database for writing, creating the directory (mode 0700), the
/// file and the schema as needed. Used by the recorder and by
/// [`crate::Settings`].
pub fn open_read_write(path: &Path) -> Result<Connection> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    // WAL lets Settings read while the recorder writes. journal_mode answers
    // with a row, so it cannot go through execute_batch.
    let mode: String = connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(Error::Invalid(format!(
            "cannot enable WAL on {} (got {mode})",
            path.display()
        )));
    }
    connection.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
    // Skip the write-locking migration transaction when nothing is pending.
    if user_version(&connection)? != SCHEMA_VERSION {
        migrate_with(&mut connection, MIGRATIONS)?;
    }
    Ok(connection)
}

/// Opens an existing, fully migrated database read-only.
pub fn open_read_only(path: &Path) -> Result<Connection> {
    if !path.exists() {
        return Err(Error::NotFound(path.to_owned()));
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    let found = user_version(&connection)?;
    if found < SCHEMA_VERSION {
        return Err(Error::Outdated {
            found,
            expected: SCHEMA_VERSION,
        });
    }
    if found > SCHEMA_VERSION {
        return Err(Error::TooNew {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn tables(connection: &Connection) -> Vec<String> {
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect()
    }

    #[test]
    fn migrates_an_empty_database_from_v0() {
        let dir = TempDir::new("migrate0");
        let mut connection = Connection::open(dir.db()).unwrap();
        assert_eq!(user_version(&connection).unwrap(), 0);
        migrate_with(&mut connection, MIGRATIONS).unwrap();
        assert_eq!(user_version(&connection).unwrap(), SCHEMA_VERSION);
        assert_eq!(
            tables(&connection),
            [
                "connections",
                "counters",
                "daily",
                "hourly",
                "interfaces",
                "meta",
                "samples",
                "settings"
            ]
        );
        // Idempotent.
        migrate_with(&mut connection, MIGRATIONS).unwrap();
        assert_eq!(user_version(&connection).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn v0_database_with_foreign_tables_keeps_them() {
        let dir = TempDir::new("migrate0b");
        let mut connection = Connection::open(dir.db()).unwrap();
        connection
            .execute_batch("CREATE TABLE unrelated (x); INSERT INTO unrelated VALUES (7);")
            .unwrap();
        migrate_with(&mut connection, MIGRATIONS).unwrap();
        let value: i64 = connection
            .query_row("SELECT x FROM unrelated", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, 7);
    }

    #[test]
    fn upgrades_step_by_step_and_keeps_data() {
        fn add_note(transaction: &Transaction) -> rusqlite::Result<()> {
            transaction.execute("ALTER TABLE interfaces ADD COLUMN note TEXT", [])?;
            transaction.execute("UPDATE interfaces SET note = 'migrated'", [])?;
            Ok(())
        }
        let steps = [
            Step::Sql(SCHEMA_V1),
            Step::Code(add_note),
            Step::Sql("CREATE TABLE extra (id INTEGER)"),
        ];
        let dir = TempDir::new("migrate-steps");
        let mut connection = Connection::open(dir.db()).unwrap();
        // A database that is at version 1 and has data.
        migrate_with(&mut connection, &steps[..1]).unwrap();
        connection
            .execute(
                "INSERT INTO interfaces (name, kind) VALUES ('wlan0', 'wifi')",
                [],
            )
            .unwrap();
        assert_eq!(user_version(&connection).unwrap(), 1);

        migrate_with(&mut connection, &steps).unwrap();
        assert_eq!(user_version(&connection).unwrap(), 3);
        let note: String = connection
            .query_row(
                "SELECT note FROM interfaces WHERE name = 'wlan0'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(note, "migrated");
        assert!(tables(&connection).contains(&"extra".to_owned()));
    }

    #[test]
    fn failed_step_rolls_back_everything() {
        let steps = [
            Step::Sql("CREATE TABLE a (x)"),
            Step::Sql("CREATE TABLE a (x)"),
        ];
        let dir = TempDir::new("migrate-fail");
        let mut connection = Connection::open(dir.db()).unwrap();
        assert!(migrate_with(&mut connection, &steps).is_err());
        assert_eq!(user_version(&connection).unwrap(), 0);
        assert!(tables(&connection).is_empty());
    }

    #[test]
    fn newer_database_is_refused() {
        let dir = TempDir::new("toonew");
        let mut connection = Connection::open(dir.db()).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 99")
            .unwrap();
        assert!(matches!(
            migrate_with(&mut connection, MIGRATIONS),
            Err(Error::TooNew { found: 99, .. })
        ));
    }

    #[test]
    fn read_write_open_creates_directory_wal_and_schema() {
        let dir = TempDir::new("open");
        let path = dir.path().join("nested/bingux/data-usage.sqlite3");
        let connection = open_read_write(&path).unwrap();
        let mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let permissions = fs::metadata(path.parent().unwrap()).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(permissions.permissions().mode() & 0o777, 0o700);
        assert_eq!(user_version(&connection).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn read_only_open_reports_missing_and_outdated() {
        let dir = TempDir::new("ro");
        assert!(matches!(open_read_only(&dir.db()), Err(Error::NotFound(_))));
        Connection::open(dir.db()).unwrap();
        assert!(matches!(
            open_read_only(&dir.db()),
            Err(Error::Outdated { found: 0, .. })
        ));
        open_read_write(&dir.db()).unwrap();
        open_read_only(&dir.db()).unwrap();
    }

    #[test]
    fn null_connection_rows_are_unique_through_the_coalesce_index() {
        let dir = TempDir::new("nullkey");
        let connection = open_read_write(&dir.db()).unwrap();
        connection
            .execute(
                "INSERT INTO interfaces (name, kind) VALUES ('wlan0', 'wifi')",
                [],
            )
            .unwrap();
        let insert = "INSERT INTO samples (interface_id, connection_id, bucket_start, rx_bytes, tx_bytes) \
                      VALUES (1, NULL, 60, 1, 1)";
        connection.execute(insert, []).unwrap();
        assert!(
            connection.execute(insert, []).is_err(),
            "duplicate NULL key must fail"
        );
    }

    #[test]
    fn default_path_follows_xdg_data_home_then_home() {
        // Reads the process environment; only asserts the shape.
        let path = default_path().unwrap();
        assert!(path.ends_with("bingux/data-usage.sqlite3"));
        assert!(path.is_absolute());
    }
}
