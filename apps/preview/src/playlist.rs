//! The files the arrow keys reach: the picture or document that was opened and
//! its neighbours.
//!
//! Opening one file shows it among the files of its folder that are of its own
//! kind (the pictures beside a picture, the documents beside a document), in
//! the order a file manager lists them, so the arrows walk a folder the way
//! Files shows it. Opening several files, or a folder, makes those the list.
//! Nothing here touches the window: it is a list of paths and a place in it.

use std::path::{Path, PathBuf};

use bingux_files::mime::{self, Category};
use bingux_files::sort::natural_cmp;
use bingux_ui::preview::{Step, neighbour};

/// The most files one list holds: a folder of a million pictures is not read whole.
const LIMIT: usize = 20_000;

/// What a file is for the purpose of choosing its neighbours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Picture,
    Document,
    /// Everything else the preview can show: text, code, archives, songs and films.
    Other(Category),
}

/// The family of the file called `name`.
pub fn family_of(name: &str) -> Family {
    match mime::for_name(name).category {
        Category::Image => Family::Picture,
        Category::Document => Family::Document,
        other => Family::Other(other),
    }
}

/// A list of files and the place in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Playlist {
    files: Vec<PathBuf>,
    index: usize,
}

impl Playlist {
    /// Nothing open.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The list for a launch: one file is shown among its neighbours; a folder
    /// is its pictures and documents; several are themselves.
    pub fn of_launch(paths: &[PathBuf]) -> Self {
        match paths {
            [] => Self::empty(),
            [one] if one.is_dir() => {
                let files = readable_in(one);
                Playlist { files, index: 0 }
            }
            [one] => Self::beside(one),
            many => {
                let mut files = Vec::new();
                for path in many {
                    if path.is_dir() {
                        files.extend(readable_in(path));
                    } else if path.is_file() {
                        files.push(path.clone());
                    }
                }
                files.dedup();
                files.truncate(LIMIT);
                Playlist { files, index: 0 }
            }
        }
    }

    /// `file`, with the files of its folder that are of its kind around it.
    pub fn beside(file: &Path) -> Self {
        let family = file
            .file_name()
            .map(|name| family_of(&name.to_string_lossy()));
        let mut files = match (file.parent(), family) {
            (Some(folder), Some(family)) => {
                let mut found = list_folder(folder, |name| family_of(name) == family);
                // A file the type database does not place is still opened, alone.
                if !found.iter().any(|path| path == file) {
                    found.push(file.to_owned());
                    sort_like_files(&mut found);
                }
                found
            }
            _ => vec![file.to_owned()],
        };
        files.truncate(LIMIT);
        let index = files.iter().position(|path| path == file).unwrap_or(0);
        Playlist { files, index }
    }

    pub fn current(&self) -> Option<&Path> {
        self.files.get(self.index).map(PathBuf::as_path)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// "3 of 24", as numbers, when there is more than one file.
    pub fn position(&self) -> Option<(usize, usize)> {
        (self.files.len() > 1).then_some((self.index + 1, self.files.len()))
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Whether every file is a picture: a slideshow needs more than one.
    pub fn is_pictures(&self) -> bool {
        self.len() > 1
            && self.files.iter().all(|path| {
                path.file_name()
                    .is_some_and(|name| family_of(&name.to_string_lossy()) == Family::Picture)
            })
    }

    /// Move one step. `false` at an end, where the list stays put.
    pub fn step(&mut self, step: Step) -> bool {
        match neighbour(self.index, self.files.len(), step, 1) {
            Some(to) => {
                self.index = to;
                true
            }
            None => false,
        }
    }

    /// Move one step forward, from the last file to the first: what a
    /// slideshow does.
    pub fn advance_wrapping(&mut self) {
        if self.files.len() > 1 {
            self.index = (self.index + 1) % self.files.len();
        }
    }

    pub fn first(&mut self) {
        self.index = 0;
    }

    pub fn last(&mut self) {
        self.index = self.files.len().saturating_sub(1);
    }

    /// Take the current file out of the list (it went to the Trash). The next
    /// file takes its place, or the one before when it was the last. Returns
    /// the file and the place it had, for [`Playlist::put_back`].
    pub fn remove_current(&mut self) -> Option<(PathBuf, usize)> {
        if self.index >= self.files.len() {
            return None;
        }
        let at = self.index;
        let removed = self.files.remove(at);
        if self.index >= self.files.len() {
            self.index = self.files.len().saturating_sub(1);
        }
        Some((removed, at))
    }

    /// Put a removed file back where it was, and show it again.
    pub fn put_back(&mut self, file: PathBuf, at: usize) {
        let at = at.min(self.files.len());
        self.files.insert(at, file);
        self.index = at;
    }

    /// Show `file` if it is in the list; true when it was.
    pub fn show(&mut self, file: &Path) -> bool {
        match self.files.iter().position(|path| path == file) {
            Some(index) => {
                self.index = index;
                true
            }
            None => false,
        }
    }
}

/// The files of `folder` that `wanted` accepts by name, hidden files left out,
/// in the order a file manager lists them.
fn list_folder(folder: &Path, wanted: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.')
                && wanted(&name)
                && entry.file_type().is_ok_and(|kind| !kind.is_dir())
        })
        .map(|entry| entry.path())
        .take(LIMIT)
        .collect();
    sort_like_files(&mut found);
    found
}

/// Names in natural order, ignoring case: `img2` before `img10`.
fn sort_like_files(files: &mut [PathBuf]) {
    files.sort_by(|a, b| {
        let (a, b) = (
            a.file_name().unwrap_or_default().to_string_lossy(),
            b.file_name().unwrap_or_default().to_string_lossy(),
        );
        natural_cmp(&a, &b)
    });
}

/// The pictures and documents in a folder.
fn readable_in(folder: &Path) -> Vec<PathBuf> {
    list_folder(folder, |name| {
        matches!(family_of(name), Family::Picture | Family::Document)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(files: &[&str]) -> PathBuf {
        static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "bingux-preview-playlist-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for name in files {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        dir
    }

    fn names(playlist: &Playlist) -> Vec<String> {
        playlist
            .files()
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_picture_is_shown_among_the_pictures_of_its_folder_in_natural_order() {
        let dir = folder(&[
            "img10.png",
            "img2.png",
            "img1.jpg",
            "notes.txt",
            ".hidden.png",
            "Report.pdf",
        ]);
        let list = Playlist::beside(&dir.join("img2.png"));
        assert_eq!(names(&list), ["img1.jpg", "img2.png", "img10.png"]);
        assert_eq!(list.position(), Some((2, 3)));
        assert!(list.is_pictures());
    }

    #[test]
    fn a_document_is_shown_among_the_documents_beside_it() {
        let dir = folder(&["a.pdf", "b.pdf", "c.png"]);
        let list = Playlist::beside(&dir.join("b.pdf"));
        assert_eq!(names(&list), ["a.pdf", "b.pdf"]);
        assert!(!list.is_pictures());
    }

    #[test]
    fn a_file_the_database_does_not_place_is_still_shown() {
        let dir = folder(&["mystery.zzzz", "a.png"]);
        let list = Playlist::beside(&dir.join("mystery.zzzz"));
        assert_eq!(list.current(), Some(dir.join("mystery.zzzz").as_path()));
        assert_eq!(list.position(), None, "alone: no counter");
    }

    #[test]
    fn the_arrows_stop_at_the_ends_and_a_slideshow_wraps() {
        let dir = folder(&["a.png", "b.png", "c.png"]);
        let mut list = Playlist::beside(&dir.join("a.png"));
        assert!(!list.step(Step::Previous));
        assert!(list.step(Step::Next) && list.step(Step::Next));
        assert!(!list.step(Step::Next));
        assert_eq!(list.position(), Some((3, 3)));
        list.advance_wrapping();
        assert_eq!(list.position(), Some((1, 3)));
        list.last();
        assert_eq!(list.position(), Some((3, 3)));
    }

    #[test]
    fn removing_the_current_file_shows_the_next_and_putting_it_back_shows_it_again() {
        let dir = folder(&["a.png", "b.png", "c.png"]);
        let mut list = Playlist::beside(&dir.join("b.png"));
        let (removed, at) = list.remove_current().unwrap();
        assert_eq!(names(&list), ["a.png", "c.png"]);
        assert_eq!(list.current(), Some(dir.join("c.png").as_path()));
        list.put_back(removed.clone(), at);
        assert_eq!(names(&list), ["a.png", "b.png", "c.png"]);
        assert_eq!(list.current(), Some(removed.as_path()));
        // The last file removed leaves the one before it.
        list.last();
        list.remove_current();
        assert_eq!(list.current(), Some(dir.join("b.png").as_path()));
        // Everything removed leaves nothing.
        let mut one = Playlist::beside(&dir.join("a.png"));
        for _ in 0..3 {
            one.remove_current();
        }
        assert!(one.is_empty() && one.current().is_none());
    }

    #[test]
    fn a_launch_with_a_folder_or_several_files_is_those_files() {
        let dir = folder(&["a.png", "b.pdf", "c.txt"]);
        assert_eq!(
            names(&Playlist::of_launch(std::slice::from_ref(&dir))),
            ["a.png", "b.pdf"]
        );
        let several = Playlist::of_launch(&[
            dir.join("c.txt"),
            dir.join("a.png"),
            dir.join("nothing.png"),
        ]);
        assert_eq!(
            names(&several),
            ["c.txt", "a.png"],
            "in the order given; a missing file is left out"
        );
        assert!(Playlist::of_launch(&[]).is_empty());
    }
}
