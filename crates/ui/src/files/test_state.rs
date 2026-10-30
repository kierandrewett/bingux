//! Files that a test thread keeps for itself.
//!
//! A test must never meet the person's files, nor a file that every test of the process shares:
//! what one test saved would be on the screen of the next one, whatever the order they run in.
//! Every test runs on a thread of its own, so the folder here is the thread's, and it is removed
//! when the thread ends.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

thread_local! {
    static FOLDER: Folder = {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Folder(std::env::temp_dir().join(format!(
            "bingux-ui-test-state-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    };
}

/// The path of the file called `name` in this thread's folder. Nothing is created: the writer
/// makes the folder.
pub(super) fn private_file(name: &str) -> PathBuf {
    FOLDER.with(|folder| folder.0.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_threads_never_share_a_file_and_the_folder_goes_with_its_thread() {
        let here = private_file("recent-searches");
        assert_eq!(here, private_file("recent-searches"), "one thread, one folder");
        let there = std::thread::spawn(|| {
            let file = private_file("recent-searches");
            std::fs::create_dir_all(file.parent().expect("a folder")).expect("a folder");
            std::fs::write(&file, "kept").expect("a file");
            file
        })
        .join()
        .expect("the thread ended");
        assert_ne!(here, there, "another thread has a file of its own");
        assert!(!there.exists(), "the folder of a thread that ended is gone");
    }
}
