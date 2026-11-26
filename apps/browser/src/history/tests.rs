//! History is kept in `browser.sqlite` and found through the bingux-ui
//! searchable `history`.
//!
//! Failures these guard against:
//!   1. visits, or a title that arrives after the load, lost at the next start;
//!   2. Clear history that leaves the pages in the file (free pages, the
//!      write-ahead log) or in the index;
//!   3. Clear history that claims to reach the engine's link colours when the
//!      engine cannot, or leaves visited links coloured when it can;
//!   4. a history that grows without bound;
//!   5. the address field's ranking putting a title match above a host that
//!      begins with what was typed;
//!   6. a damaged or newer file written over, or Start over that deletes it;
//!   7. a full disk, a read-only folder or a lock held by another program
//!      that is silent, or whose state never clears.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bingux_ui::search::{Cancel, SearchService};
use bingux_ui::web_view::test_double::TestDoubleBackend;
use rusqlite::Connection;

use crate::browser::{Browser, Options, Profile};
use crate::history::{Change, Database, History, INDEX, Page};
use crate::profiles::{Folders, Places};
use crate::settings::Settings;
use crate::store::Files;
use crate::store::{Event, Store};
use crate::store::{Keeping, NotSaved};
use crate::tabs::Place;

fn folders(root: &Path) -> Folders {
    Places::named_folders(root)
}

fn open(root: &Path, search: &Arc<SearchService>) -> Store {
    crate::locale::register();
    Store::open(&folders(root), Keeping::Saved, search).unwrap()
}

/// The last history state the store reported.
fn history_state(store: &Store) -> Option<Keeping> {
    store
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            Event::History(keeping) => Some(keeping),
            Event::Settings(_) => None,
        })
        .last()
}

fn urls(search: &Arc<SearchService>, request: &bingux_ui::search::Search) -> Vec<String> {
    search
        .search(request, &Cancel::new())
        .unwrap()
        .hits
        .iter()
        .map(|hit| History::visited(hit).url)
        .collect()
}

fn page(search: &Arc<SearchService>, url: &str) -> Option<Page> {
    search
        .index(INDEX)?
        .get(url)
        .map(|document| Page::from_document(&document))
}

fn bytes_of(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_default()
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn aside(root: &Path, prefix: &str) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(prefix)
        })
        .collect()
}

#[test]
fn visits_and_a_late_title_survive_a_new_store() {
    let root = Files::temp();
    let search = SearchService::new();
    let store = open(&root, &search);
    store.record_visit("https://example.com/", "");
    store.set_title("https://example.com/", "Example Domain");
    store.record_visit("https://example.com/", "");
    store.record_visit("about:blank", "");
    store.record_visit("bingux:settings", "Settings");
    store.flush();
    assert_eq!(history_state(&store), Some(Keeping::Saved));
    drop(store);
    let mode = fs::metadata(folders(&root).store())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    let again = SearchService::new();
    let store = open(&root, &again);
    store.flush();
    assert_eq!(
        store.history_len(),
        1,
        "the browser's own pages are not history"
    );
    let found = page(&again, "https://example.com/").unwrap();
    assert_eq!(found.title, "Example Domain");
    assert_eq!(found.visit_count, 2);
    let visits: i64 = Connection::open(folders(&root).store())
        .unwrap()
        .query_row("SELECT count(*) FROM visits", [], |row| row.get(0))
        .unwrap();
    assert_eq!(visits, 2, "each visit keeps its time");
    drop(store);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn a_page_load_in_the_window_is_recorded_and_a_private_window_records_none() {
    let root = Files::temp();
    let search = SearchService::new();
    let mut browser = Browser::with_profile(
        TestDoubleBackend::new(),
        Options::default(),
        Profile {
            id: None,
            name: None,
            settings: Settings::default(),
            settings_keeping: Keeping::Saved,
            store: Some(open(&root, &search)),
        },
    );
    browser.open_tab("https://example.com/", Place::End, true);
    browser.poll();
    browser.flush_store();
    assert_eq!(browser.history_keeping, Keeping::Saved);
    assert_eq!(browser.history_len(), 1);
    assert!(browser.has_history());

    let private = Browser::new(TestDoubleBackend::new(), Options { private: true });
    assert!(!private.has_history());
    assert_eq!(private.history_clear_scope().history, false);
    drop(browser);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn clear_history_empties_the_file_and_the_index_and_asks_the_engine_to_forget_visits() {
    let root = Files::temp();
    let search = SearchService::new();
    let double = TestDoubleBackend::new();
    let mut browser = Browser::with_profile(
        double.clone(),
        Options::default(),
        Profile {
            id: None,
            name: None,
            settings: Settings::default(),
            settings_keeping: Keeping::Saved,
            store: Some(open(&root, &search)),
        },
    );
    browser.open_tab("https://secret.example/private-page", Place::End, true);
    browser.poll();
    browser.flush_store();
    browser.take_notices();
    double.clear_calls();
    let path = folders(&root).store();
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    let before = [bytes_of(&path), bytes_of(&wal)].concat();
    assert!(contains(&before, "secret.example"), "the visit was written");

    browser.clear_history();
    browser.flush_store();
    assert_eq!(browser.history_len(), 0);
    let after = [bytes_of(&path), bytes_of(&wal)].concat();
    assert!(
        !contains(&after, "secret.example"),
        "no trace in the file or its log"
    );
    let told = browser.take_notices();
    assert_eq!(told[0].text, "History cleared");
    assert!(
        double
            .calls()
            .contains(&bingux_ui::web_view::test_double::Call::ClearHistory),
        "the engine forgets its visits, so visited links stop looking visited"
    );
    assert!(browser.history_clear_scope().link_colours);
    drop(browser);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn an_engine_that_cannot_clear_visits_is_said_so_and_not_asked() {
    crate::locale::register();
    let double = TestDoubleBackend::with_capabilities(bingux_ui::web_view::Capabilities {
        clear_history: false,
        ..bingux_ui::web_view::Capabilities::all()
    });
    let root = Files::temp();
    let search = SearchService::new();
    let mut browser = Browser::with_profile(
        double.clone(),
        Options::default(),
        Profile {
            id: None,
            name: None,
            settings: Settings::default(),
            settings_keeping: Keeping::Saved,
            store: Some(open(&root, &search)),
        },
    );
    browser.poll();
    double.clear_calls();
    browser.clear_history();
    let told = browser.take_notices();
    assert!(told[0].text.contains("Link colours"), "{}", told[0].text);
    assert!(double.calls().is_empty());
    drop(browser);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn the_cap_keeps_the_pages_visited_last() {
    let root = Files::temp();
    let search = SearchService::new();
    let store = open(&root, &search);
    for name in ["one", "two", "three"] {
        store.record_visit(&format!("https://{name}.example/"), name);
        store.flush();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    drop(store);
    let again = SearchService::new();
    let store = Store::open_with_cap(&folders(&root), Keeping::Saved, &again, 2).unwrap();
    store.flush();
    assert_eq!(
        urls(&again, &History::recent(10)),
        ["https://three.example/", "https://two.example/"]
    );
    drop(store);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn the_searchable_ranks_a_host_that_begins_with_the_text_first() {
    let root = Files::temp();
    let search = SearchService::new();
    let store = open(&root, &search);
    store.record_visit("https://example.com/", "Home");
    store.flush();
    std::thread::sleep(std::time::Duration::from_millis(2));
    for _ in 0..3 {
        store.record_visit(
            "https://news.other.org/an-example-story",
            "An example story",
        );
    }
    store.flush();
    assert_eq!(
        urls(&search, &History::suggestions("exa")),
        [
            "https://example.com/",
            "https://news.other.org/an-example-story"
        ],
        "the host prefix first, though visited less and earlier"
    );
    drop(store);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn a_damaged_file_is_left_as_it_is_until_start_over_moves_it_aside() {
    let root = Files::temp();
    let path = folders(&root).store();
    let garbage = vec![0x5a_u8; 8192];
    fs::write(&path, &garbage).unwrap();
    let search = SearchService::new();
    let mut browser = Browser::with_profile(
        TestDoubleBackend::new(),
        Options::default(),
        Profile {
            id: None,
            name: None,
            settings: Settings::default(),
            settings_keeping: Keeping::Saved,
            store: Some(open(&root, &search)),
        },
    );
    browser.open_tab("https://example.com/", Place::End, true);
    browser.poll();
    browser.flush_store();
    assert!(browser.history_keeping.can_start_over());
    assert!(
        browser
            .take_notices()
            .iter()
            .any(|notice| notice.text.contains("History could not be read"))
    );
    assert_eq!(
        browser.history_len(),
        1,
        "this run's visits are still found"
    );
    assert_eq!(fs::read(&path).unwrap(), garbage);

    browser.start_over_history();
    browser.flush_store();
    assert_eq!(browser.history_keeping, Keeping::Saved);
    let kept = aside(&root, "browser.sqlite.damaged-");
    assert_eq!(kept.len(), 1);
    assert_eq!(fs::read(&kept[0]).unwrap(), garbage);
    let written: i64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT count(*) FROM pages", [], |row| row.get(0))
        .unwrap();
    assert_eq!(written, 1, "the visit of this run is in the new file");
    drop(browser);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn a_newer_browsers_file_is_neither_read_nor_written() {
    let root = Files::temp();
    let path = folders(&root).store();
    {
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE future (x); PRAGMA user_version = 99;")
            .unwrap();
    }
    let before = fs::read(&path).unwrap();
    let search = SearchService::new();
    let store = open(&root, &search);
    store.record_visit("https://example.com/", "");
    store.clear_history();
    store.record_visit("https://example.com/", "");
    store.flush();
    assert_eq!(
        history_state(&store),
        Some(Keeping::NotSaved(NotSaved::TooNew {
            found: 99,
            supported: Database::VERSION
        }))
    );
    drop(store);
    assert_eq!(fs::read(&path).unwrap(), before);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn a_full_database_is_disk_full() {
    // A real SQLITE_FULL: the file may not grow past its pages.
    let root = Files::temp();
    let path = folders(&root).store();
    let mut database = Database::open(&path).unwrap();
    let pages: i64 = database
        .connection()
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .unwrap();
    database
        .connection()
        .execute_batch(&format!("PRAGMA max_page_count = {pages}"))
        .unwrap();
    let many: Vec<Change> = (0..2000)
        .map(|at| Change::Visit {
            url: format!("https://site{at}.example/{}", "x".repeat(200)),
            title: String::new(),
            at,
        })
        .collect();
    let error = database.write(&many).unwrap_err();
    assert_eq!(Database::not_saved(&error), NotSaved::DiskFull);
    drop(database);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn a_read_only_folder_says_so_and_a_later_write_keeps_everything() {
    let root = Files::temp();
    let search = SearchService::new();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
    let store = open(&root, &search);
    store.record_visit("https://example.com/", "");
    store.flush();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        history_state(&store),
        Some(Keeping::NotSaved(NotSaved::ReadOnly))
    );
    store.record_visit("https://example.org/", "");
    store.flush();
    assert_eq!(history_state(&store), Some(Keeping::Saved));
    drop(store);
    let again = SearchService::new();
    let store = open(&root, &again);
    store.flush();
    assert_eq!(store.history_len(), 2, "what waited was written");
    drop(store);
    Files::remove_tree(&root).unwrap();
}

#[test]
fn a_file_another_program_holds_is_busy_until_it_lets_go() {
    let root = Files::temp();
    let search = SearchService::new();
    let store = open(&root, &search);
    store.flush();
    let holder = Connection::open(folders(&root).store()).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    store.record_visit("https://example.com/", "");
    store.flush();
    assert_eq!(
        history_state(&store),
        Some(Keeping::NotSaved(NotSaved::Busy))
    );
    holder.execute_batch("COMMIT").unwrap();
    store.record_visit("https://example.org/", "");
    store.flush();
    assert_eq!(history_state(&store), Some(Keeping::Saved));
    drop(store);
    drop(holder);
    Files::remove_tree(&root).unwrap();
}
