//! A profile's history: `browser.sqlite`, and the bingux-ui searchable
//! `history` that the address field and the new tab page ask.
//!
//! Only the store's thread opens the database. The index is created by the
//! store with the search service's `create_index` and filled from the file
//! when the store opens it; after that the store keeps the two in step, so
//! nothing reads the file twice and no second connection exists.
//!
//! History is private data: errors name the file and the SQLite code, never
//! an address.

#[cfg(test)]
mod tests;

use std::fs::OpenOptions;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use bingux_ui::search::{Document, Hit, Schema, Search};
use rusqlite::{Connection, ErrorCode, OpenFlags, TransactionBehavior, params};

use crate::address::Address;
use crate::address::{Suggestion, Visited};
use crate::store::files::Problem;
use crate::store::keeping::NotSaved;

/// The name of the searchable: one per process, and a process has one profile.
pub const INDEX: &str = "history";

/// The most pages kept: older ones are trimmed when the store opens.
pub const CAP: usize = 50_000;

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

/// The schema version this build reads and writes.
pub const VERSION: u32 = STEPS.len() as u32;

/// One page of history: what the index and the address field show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub visit_count: u32,
    /// Unix milliseconds.
    pub last_visit: i64,
}

/// What changed, waiting to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Visit { url: String, title: String, at: i64 },
    Title { url: String, title: String },
}

/// Whether a load of `url` is history: a page on the web or a file, not the
/// browser's own pages and not a blank tab.
pub fn is_recorded(url: &str) -> bool {
    !(url.is_empty() || url.starts_with("about:") || url.starts_with("bingux:"))
}

/// The searchable's schema.
pub fn schema() -> Schema {
    Schema::new()
        .searchable("host", 10.0)
        .searchable("title", 8.0)
        .searchable("path", 2.0)
        .sortable(["visit_count", "last_visit"])
        .filterable(["visit_count", "last_visit"])
        .displayed(["title", "uri", "visit_count", "last_visit"])
        .external(false)
}

/// The index document of `page`: its id is the address.
pub fn document(page: &Page) -> Document {
    let host = Address::new(&page.url).host();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    let path = page
        .url
        .split_once("://")
        .map_or(page.url.as_str(), |(_, rest)| rest)
        .split_once('/')
        .map_or("", |(_, path)| path)
        .to_string();
    Document::new(page.url.clone())
        .title(&page.title)
        .uri(&page.url)
        .field("host", host)
        .field("path", path)
        .field("visit_count", page.visit_count)
        .field("last_visit", page.last_visit)
}

/// The page an index document holds.
pub fn page_of(document: &Document) -> Page {
    let text = |name: &str| {
        document
            .fields
            .get(name)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let number = |name: &str| {
        document
            .fields
            .get(name)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    Page {
        url: document.id.clone(),
        title: text("title"),
        visit_count: u32::try_from(number("visit_count")).unwrap_or(u32::MAX),
        last_visit: number("last_visit"),
    }
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
pub fn open(path: &Path) -> Result<Connection, NotSaved> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| io_not_saved(&error))?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|error| io_not_saved(&error))?;
    drop(file);
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| not_saved(&error))?;
    let setup = |connection: &mut Connection| -> Result<(), NotSaved> {
        let fail = |error: rusqlite::Error| not_saved(&error);
        connection.busy_timeout(BUSY_TIMEOUT).map_err(fail)?;
        // Reading the version reads the header: a file that is not a
        // database fails here, and a newer one is refused before anything
        // (the WAL switch included) writes to it.
        let found = user_version(connection).map_err(fail)?;
        if found > VERSION {
            return Err(NotSaved::TooNew {
                found,
                supported: VERSION,
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
        if found < VERSION {
            migrate(connection).map_err(fail)?;
        }
        Ok(())
    };
    setup(&mut connection)?;
    Ok(connection)
}

/// Runs the steps the file has not seen, in one immediate transaction.
fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let found = user_version(&transaction)? as usize;
    for step in STEPS.iter().skip(found) {
        transaction.execute_batch(step)?;
    }
    transaction.execute_batch(&format!("PRAGMA user_version = {VERSION}"))?;
    transaction.commit()
}

/// Removes all but the `cap` pages visited last, and their visits.
pub fn trim(connection: &Connection, cap: usize) -> rusqlite::Result<usize> {
    let cap = i64::try_from(cap).unwrap_or(i64::MAX);
    connection.execute(
        "DELETE FROM pages WHERE id IN \
         (SELECT id FROM pages ORDER BY last_visit DESC, id DESC LIMIT -1 OFFSET ?1)",
        [cap],
    )
}

/// Every page in the file.
pub fn pages(connection: &Connection) -> rusqlite::Result<Vec<Page>> {
    let mut statement =
        connection.prepare("SELECT url, title, visit_count, last_visit FROM pages")?;
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
pub fn write(connection: &mut Connection, changes: &[Change]) -> rusqlite::Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
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

/// The statements that empty the history.
const CLEAR_ALL: &str = include_str!("clear.sql");

/// Removes every page and visit. `secure_delete` overwrites the rows, and the
/// write-ahead log, which still holds them, is emptied.
pub fn clear(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(CLEAR_ALL)?;
    transaction.commit()?;
    connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
}

/// What the address field asks the searchable for `typed`: relevance first
/// (a host that begins with it before a title or path that has it), then the
/// latest visit, then the most visits.
pub fn suggestions(typed: &str) -> Search {
    Search::new(typed)
        .index(INDEX)
        .limit(Suggestion::LIMIT)
        .sort("_score:desc")
        .sort("last_visit:desc")
        .sort("visit_count:desc")
}

/// The `limit` pages visited last, the latest first.
pub fn recent(limit: usize) -> Search {
    Search::match_all()
        .index(INDEX)
        .limit(limit)
        .sort("last_visit:desc")
}

/// The row a hit is.
pub fn visited(hit: &Hit) -> Visited {
    Visited {
        url: hit.uri().unwrap_or(hit.id.as_str()).to_string(),
        title: hit.title().unwrap_or_default().to_string(),
    }
}

/// Every page: the hit count is how many pages the history holds.
pub fn everything() -> Search {
    Search::match_all().index(INDEX).limit(1)
}
