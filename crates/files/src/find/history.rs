//! The searches a person made lately, for the dropdown of the search field.
//!
//! A short list, newest first, kept in one small text file (a search per
//! line) at `$XDG_STATE_HOME/bingux/files/recent-searches`. A write is a
//! temporary file renamed over the old one; a write that fails is dropped:
//! it is a convenience.

use std::io;
use std::path::{Path, PathBuf};

/// The most searches kept.
pub const MAX_RECENT: usize = 8;

/// The longest search kept, in characters.
pub const MAX_LENGTH: usize = 200;

/// The recent searches, newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recents {
    items: Vec<String>,
}

impl Recents {
    /// The list from `text`: a search per line.
    pub fn parse(text: &str) -> Recents {
        let mut recents = Recents::default();
        for line in text.lines().rev() {
            recents.push(line);
        }
        recents
    }

    /// The list as a file holds it.
    pub fn text(&self) -> String {
        let mut text = String::new();
        for item in &self.items {
            text.push_str(item);
            text.push('\n');
        }
        text
    }

    /// The searches, newest first.
    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Remember a search: it goes first, and an earlier copy goes away. A
    /// blank search or one over [`MAX_LENGTH`] is not kept. Returns whether
    /// the list changed.
    pub fn push(&mut self, search: &str) -> bool {
        let search = search.trim();
        if search.is_empty() || search.chars().count() > MAX_LENGTH || search.contains(['\n', '\r'])
        {
            return false;
        }
        if self.items.first().is_some_and(|first| first == search) {
            return false;
        }
        self.items.retain(|item| item != search);
        self.items.insert(0, search.to_string());
        self.items.truncate(MAX_RECENT);
        true
    }

    /// Forget one search.
    pub fn remove(&mut self, search: &str) {
        self.items.retain(|item| item != search);
    }

    /// Forget them all.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// The file the list lives in.
    pub fn file() -> PathBuf {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("bingux/files/recent-searches")
    }

    /// The list in the standard file; empty when there is none.
    pub fn load() -> Recents {
        Recents::load_from(&Recents::file())
    }

    pub fn load_from(path: &Path) -> Recents {
        std::fs::read_to_string(path)
            .map(|text| Recents::parse(&text))
            .unwrap_or_default()
    }

    /// Write the list to the standard file.
    pub fn save(&self) -> io::Result<()> {
        self.save_to(&Recents::file())
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, self.text())?;
        std::fs::rename(&temporary, path)
    }
}
