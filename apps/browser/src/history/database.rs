//! `browser.sqlite`: opening it, its schema and what is read from and written to it.

use std::fs::OpenOptions;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags, TransactionBehavior, params};

use super::{Change, Page};
use crate::store::files::Problem;
use crate::store::keeping::NotSaved;

/// How long the store waits for another holder of the file's write lock.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The schema, one step per version. A new version appends a step; a step
/// is never edited, files in the wild have run it.
const STEPS: &[&str] = &["\
CREATE TABLE pages (
    id          INTEGER PRIMARY KEY,
    url         TEXT NOT NULL UNIQUE,
    title       TEXT NOT NULL DEFAULT '',
    visit_count INTEGER NOT NULL,
    last_visit  INTEGER NOT NULL
);
CREATE TABLE visits (
    id      INTEGER PRIMARY KEY,
    page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
    at      INTEGER NOT NULL
);
CREATE INDEX visits_at ON visits(at);
CREATE INDEX pages_last_visit ON pages(last_visit);
"];

/// The statements that empty the history.
const CLEAR_ALL: &str = include_str!("clear.sql");

/// The open history file. Only the store's thread holds one.
pub struct Database(Connection);

impl Database {
    /// The schema version this build reads and writes.
    pub const VERSION: u32 = STEPS.len() as u32;

    /// The connection, for a test that sets the file's limits.
    #[cfg(test)]
    pub fn connection(&self) -> &Connection {
        &self.0
    }

    /// What a database error means for the person.
    pub fn not_saved(error: &rusqlite::Error) -> NotSaved {
        let code = error.sqlite_error_code();
        match code {
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => NotSaved::Damaged {
                detail: error.to_string(),
            },
            Some(ErrorCode::DiskFull) => NotSaved::DiskFull,
            Some(ErrorCode::ReadOnly | ErrorCode::CannotOpen | ErrorCode::PermissionDenied) => {
                NotSaved::ReadOnly
            }
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => NotSaved::Busy,
            _ => NotSaved::Failed(format!("browser.sqlite: {error}")),
        }
    }

    fn io_not_saved(error: &std::io::Error) -> NotSaved {
        match Problem::of(error) {
            Problem::DiskFull => NotSaved::DiskFull,
            Problem::ReadOnly => NotSaved::ReadOnly,
            Problem::Other(text) => NotSaved::Failed(format!("browser.sqlite: {text}")),
        }
    }

    fn user_version(connection: &Connection) -> rusqlite::Result<u32> {
        connection.query_row("PRAGMA user_version", [], |row| row.get(0))
    }

    /// Opens `path` for the store: creates it (0600) when absent, refuses a file
    /// it cannot read or one from a newer browser without writing a byte to it,
    /// checks it, and brings its schema up to date.
    pub fn open(path: &Path) -> Result<Self, NotSaved> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map_err(|error| Self::io_not_saved(&error))?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| Self::io_not_saved(&error))?;
        drop(file);
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| Self::not_saved(&error))?;
        let setup = |connection: &mut Connection| -> Result<(), NotSaved> {
            let fail = |error: rusqlite::Error| Self::not_saved(&error);
            connection.busy_timeout(BUSY_TIMEOUT).map_err(fail)?;
            // Reading the version reads the header: a file that is not a
            // database fails here, and a newer one is refused before anything
            // (the WAL switch included) writes to it.
            let found = Self::user_version(connection).map_err(fail)?;
            if found > Self::VERSION {
                return Err(NotSaved::TooNew {
                    found,
                    supported: Self::VERSION,
                });
            }
            let checked: String = connection
                .query_row("PRAGMA quick_check", [], |row| row.get(0))
                .map_err(fail)?;
            if checked != "ok" {
                return Err(NotSaved::Damaged { detail: checked });
            }
            let mode: String = connection
                .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
                .map_err(fail)?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(NotSaved::Failed(format!(
                    "browser.sqlite: the journal is {mode}, not WAL"
                )));
            }
            connection
                .execute_batch(
                    "PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON;",
                )
                .map_err(fail)?;
            if found < Self::VERSION {
                Self::migrate(connection).map_err(fail)?;
            }
            Ok(())
        };
        setup(&mut connection)?;
        Ok(Self(connection))
    }

    /// Runs the steps the file has not seen, in one immediate transaction.
    fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let found = Self::user_version(&transaction)? as usize;
        for step in STEPS.iter().skip(found) {
            transaction.execute_batch(step)?;
        }
        transaction.execute_batch(&format!("PRAGMA user_version = {}", Self::VERSION))?;
        transaction.commit()
    }

    /// Removes all but the `cap` pages visited last, and their visits.
    pub fn trim(&self, cap: usize) -> rusqlite::Result<usize> {
        let cap = i64::try_from(cap).unwrap_or(i64::MAX);
        self.0.execute(
            "DELETE FROM pages WHERE id IN \
             (SELECT id FROM pages ORDER BY last_visit DESC, id DESC LIMIT -1 OFFSET ?1)",
            [cap],
        )
    }

    /// Every page in the file.
    pub fn pages(&self) -> rusqlite::Result<Vec<Page>> {
        let mut statement = self
            .0
            .prepare("SELECT url, title, visit_count, last_visit FROM pages")?;
        let rows = statement.query_map([], |row| {
            Ok(Page {
                url: row.get(0)?,
                title: row.get(1)?,
                visit_count: row.get(2)?,
                last_visit: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Writes `changes` in one transaction: all of them or none.
    pub fn write(&mut self, changes: &[Change]) -> rusqlite::Result<()> {
        let transaction = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for change in changes {
            match change {
                Change::Visit { url, title, at } => {
                    transaction.execute(
                        "INSERT INTO pages (url, title, visit_count, last_visit) VALUES (?1, ?2, 1, ?3) \
                         ON CONFLICT (url) DO UPDATE SET visit_count = visit_count + 1, \
                         last_visit = excluded.last_visit, \
                         title = CASE WHEN excluded.title <> '' THEN excluded.title ELSE title END",
                        params![url, title, at],
                    )?;
                    transaction.execute(
                        "INSERT INTO visits (page_id, at) SELECT id, ?2 FROM pages WHERE url = ?1",
                        params![url, at],
                    )?;
                }
                Change::Title { url, title } => {
                    transaction.execute(
                        "UPDATE pages SET title = ?2 WHERE url = ?1",
                        params![url, title],
                    )?;
                }
            }
        }
        transaction.commit()
    }

    /// Removes every page and visit. `secure_delete` overwrites the rows, and the
    /// write-ahead log, which still holds them, is emptied.
    pub fn clear(&mut self) -> rusqlite::Result<()> {
        let transaction = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(CLEAR_ALL)?;
        transaction.commit()?;
        self.0
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
    }
}
