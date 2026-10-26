//! Test helpers shared by the unit tests.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
};

/// A uniquely named temp directory removed on drop.
pub struct TempDir(PathBuf);

static NEXT: AtomicU32 = AtomicU32::new(0);

impl TempDir {
    pub fn new(label: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!(
            "bingux-data-usage-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn db(&self) -> PathBuf {
        self.0.join("data-usage.sqlite3")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
