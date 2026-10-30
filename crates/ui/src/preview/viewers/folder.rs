//! A folder: how much is in it, counted while the viewer is open.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bingux_files::display;
use bingux_files::ops::{Counting, Totals};
use freya::prelude::*;

use super::super::subject::Resolved;
use crate::component::ComponentName;
use crate::files::{FileIconSize, file_icon_view};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink, TITLE};

/// How often a folder's running total is read.
const TICK: Duration = Duration::from_millis(150);
/// The folder's icon, as a multiple of the 48 px hero size.
const ICON_SCALE: f32 = 3.0;

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The two lines of a folder's viewer: how many items, and how big. While
/// the count runs they grow and say "so far…"; a folder with an unreadable
/// part says "at least". Before anything is counted, the first line says so
/// and the second is a space, so both lines have their height from the start.
pub(super) fn folder_lines(totals: Totals) -> (String, String) {
    if totals.items() == 0 && !totals.done {
        return ("Counting…".to_owned(), " ".to_owned());
    }
    let mut size = display::size(totals.bytes);
    if !totals.done {
        size.push_str(" so far…");
    } else if totals.unreadable > 0 {
        size.insert_str(0, "at least ");
    }
    (plural(totals.items(), "item", "items"), size)
}

/// Count a folder in the background and return its running total. The count
/// stops when the component goes.
fn use_folder(path: std::path::PathBuf) -> State<Totals> {
    let mut totals = use_state(Totals::default);
    use_hook(move || {
        let counting = Rc::new(Counting::start(vec![path]));
        let reader = counting.clone();
        spawn(async move {
            loop {
                let now = reader.totals();
                totals.set(now);
                if now.done {
                    break;
                }
                timer(TICK).await;
            }
        });
        counting
    });
    totals
}

#[derive(PartialEq)]
pub(super) struct FolderView {
    pub resolved: Arc<Resolved>,
}

impl Component for FolderView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let path = self.resolved.facts.path.clone().unwrap_or_default();
        let totals = use_folder(path);
        let (items, size) = folder_lines(totals());
        let size_px = FileIconSize::Px((IconSize::Hero.px() * ICON_SCALE).round() as u16);
        rect()
            .component("PreviewFolder")
            .tw(&tokens, &[w_full(), h_full(), flex_col(), center(), gap(S2)])
            .child(file_icon_view(&tokens, &super::icon(&self.resolved), size_px, false))
            .child(TITLE.label(&tokens, items))
            .child(BODY.in_ink(Ink::Muted).label(&tokens, size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_counts_up_then_settles() {
        assert_eq!(folder_lines(Totals::default()), ("Counting…".to_owned(), " ".to_owned()));
        let counting = Totals {
            bytes: 1_500_000,
            files: 3,
            folders: 1,
            ..Totals::default()
        };
        assert_eq!(folder_lines(counting), ("4 items".into(), "1.5 MB so far…".into()));
        let done = Totals {
            done: true,
            files: 1,
            folders: 0,
            ..counting
        };
        assert_eq!(folder_lines(done), ("1 item".into(), "1.5 MB".into()));
        let partial = Totals { unreadable: 2, ..done };
        assert_eq!(folder_lines(partial).1, "at least 1.5 MB");
        let empty = Totals {
            done: true,
            ..Totals::default()
        };
        assert_eq!(folder_lines(empty), ("0 items".into(), "0 bytes".into()));
    }
}
