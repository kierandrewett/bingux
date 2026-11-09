//! The tabs of the last window, kept between runs in the same state file as
//! the window size and the views of each folder (`bingux_ui::files::remember`).
//!
//! A window writes its tabs (the place each one shows, in order, and which is
//! on show) whenever they change, so what is kept is the window as it was when
//! it was closed, whichever way it was closed. Files started with no argument
//! opens them again; started on a folder or a file, it opens that instead. A
//! folder that has gone since is left out, and with none left the window opens
//! on the home folder.

use std::path::PathBuf;

use bingux_ui::files::{self, Where};

const KEY: &str = "files-session";

/// A window's tabs as the state file keeps them: each tab's folder (`None`
/// for the recent files) and the index of the tab on show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    tabs: Vec<Option<PathBuf>>,
    on_show: usize,
}

impl Session {
    /// The tabs at `places`, `on_show` (an index) being the one on show.
    pub fn of(places: &[Where], on_show: usize) -> Self {
        Session {
            tabs: places
                .iter()
                .map(|at| match at {
                    Where::Folder(path) => Some(path.clone()),
                    Where::Recent => None,
                })
                .collect(),
            on_show,
        }
    }

    /// The places to open again and which is on show: the tabs whose folder
    /// `exists`, `None` when none is left.
    pub fn places(&self, exists: impl Fn(&std::path::Path) -> bool) -> Option<(Vec<Where>, usize)> {
        let mut on_show = 0;
        let mut places = Vec::with_capacity(self.tabs.len());
        for (index, tab) in self.tabs.iter().enumerate() {
            let at = match tab {
                Some(path) if exists(path) => Where::Folder(path.clone()),
                Some(_) => continue,
                None => Where::Recent,
            };
            if index <= self.on_show {
                on_show = places.len();
            }
            places.push(at);
        }
        (!places.is_empty()).then_some((places, on_show))
    }
}

/// The tabs of the window that was closed last, if any are still there.
pub fn remembered() -> Option<(Vec<Where>, usize)> {
    let (tabs, on_show) = files::remembered::<(Vec<Option<PathBuf>>, usize)>(KEY)?;
    Session { tabs, on_show }.places(|path| path.is_dir())
}

/// Keep `session` as the one to open next time. A window that has given its
/// tabs to another (a tab dropped on another window's strip) has none and
/// keeps nothing.
pub fn remember(session: &Session) {
    if !session.tabs.is_empty() {
        files::remember(KEY, &(&session.tabs, session.on_show));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn folder(path: &str) -> Where {
        Where::Folder(PathBuf::from(path))
    }

    #[test]
    fn a_session_gives_back_its_tabs() {
        let session = Session::of(&[folder("/a"), Where::Recent, folder("/b")], 2);
        assert_eq!(
            session.places(|_| true),
            Some((vec![folder("/a"), Where::Recent, folder("/b")], 2))
        );
    }

    #[test]
    fn a_folder_that_has_gone_is_left_out_and_the_tab_on_show_follows() {
        let session = Session::of(&[folder("/gone"), folder("/a"), folder("/b")], 2);
        let exists = |path: &Path| path != Path::new("/gone");
        assert_eq!(
            session.places(exists),
            Some((vec![folder("/a"), folder("/b")], 1))
        );
        // The tab on show itself has gone: the one before it is shown.
        let session = Session::of(&[folder("/a"), folder("/gone"), folder("/b")], 1);
        assert_eq!(
            session.places(exists),
            Some((vec![folder("/a"), folder("/b")], 0))
        );
        // Nothing is left.
        let session = Session::of(&[folder("/gone")], 0);
        assert_eq!(session.places(exists), None);
    }
}
