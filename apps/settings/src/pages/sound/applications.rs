//! The Applications subpage of Sound: one row for each application that plays
//! or records sound, with its volume, mute, output and a live level meter.
//!
//! The rows are laid out so that nothing above a changing row moves:
//!
//! - An application keeps its place for as long as the page is open. A new
//!   application is added at the end, whatever its name, so rows never
//!   reorder.
//! - An application that stops keeps its row for a few reads, dimmed and
//!   labelled "Stopped", so a stream that pauses between two tracks does not
//!   make the list jump.
//! - With no application at all the page shows one row, and the first
//!   application replaces it.
//!
//! The level meters run only while this page is open in a focused window.

use freya::prelude::*;

use bingux_ui::prelude::PageHead;
use bingux_ui::style::*;

use super::{Stream, app_row, application_key, by_application, reload, use_ctx};
use crate::pages::loading::veiled;
use crate::ui::{Group, Icon, Status, page_column, row, status};

/// How many reads an application may be missing from before its row goes.
/// Reads are five seconds apart.
const GHOST_READS: u8 = 3;

/// One application's row: the streams it had when last seen.
#[derive(Clone, PartialEq)]
struct Held {
    key: String,
    streams: Vec<Stream>,
    /// Reads in a row that did not list the application.
    missed: u8,
}

/// Bring `held` up to date with the applications `live` that are playing or
/// recording now. The order of `held` never changes: a new application goes
/// last, and one that is missing counts a miss until it is dropped.
fn merge(held: &mut Vec<Held>, live: Vec<Vec<Stream>>) {
    let mut live: Vec<(String, Vec<Stream>)> = live
        .into_iter()
        .map(|streams| (application_key(&streams[0]), streams))
        .collect();
    for entry in held.iter_mut() {
        match live.iter().position(|(key, _)| *key == entry.key) {
            Some(at) => {
                entry.streams = live.remove(at).1;
                entry.missed = 0;
            }
            None => entry.missed = entry.missed.saturating_add(1),
        }
    }
    held.retain(|entry| entry.missed < GHOST_READS);
    for (key, streams) in live {
        held.push(Held {
            key,
            streams,
            missed: 0,
        });
    }
}

/// The volumes of everything that plays or records sound.
#[derive(PartialEq)]
pub struct ApplicationsPage;

impl Component for ApplicationsPage {
    fn render(&self) -> impl IntoElement {
        let ctx = use_ctx(true);
        let mut held = use_state(|| {
            let mut held = Vec::new();
            if let Some(Ok(data)) = ctx.model.peek().as_ref() {
                merge(&mut held, by_application(&data.streams));
            }
            held
        });
        // Fold each new read into the list. The effect only writes `held`,
        // never reads it, so it cannot loop.
        use_side_effect(move || {
            let guard = ctx.model.read();
            if let Some(Ok(data)) = guard.as_ref() {
                merge(&mut held.write(), by_application(&data.streams));
            }
        });

        let (above, loading) = match ctx.model.read().as_ref() {
            None => (false, true),
            Some(Ok(data)) => (data.above_100, false),
            Some(Err(e)) => {
                let message = e.0.clone();
                return veiled(
                    page_column().child(PageHead::new()).child(
                        status(Status::Error(message)).action("Try again", move |_| reload(ctx.model)),
                    ),
                    false,
                );
            }
        };

        let list = held.read().clone();
        let group = if list.is_empty() {
            Group::new(None::<String>).row(row("Nothing is playing or recording.").leading(Icon::Sound))
        } else {
            Group::new(None::<String>).rows(
                list.iter()
                    .map(|entry| app_row(&entry.streams, above, ctx, entry.missed == 0)),
            )
        };
        veiled(page_column().child(PageHead::new()).child(group), loading)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(id: u32, name: &str) -> Stream {
        Stream {
            id,
            name: name.to_string(),
            playing_to: vec!["Speakers".to_string()],
            recording_from: Vec::new(),
            paused: false,
            level: Some((1.0, false)),
            icon: String::new(),
            app: None,
        }
    }

    fn keys(held: &[Held]) -> Vec<&str> {
        held.iter().map(|entry| entry.key.as_str()).collect()
    }

    #[test]
    fn new_applications_go_last_and_order_never_changes() {
        let mut held = Vec::new();
        merge(&mut held, vec![vec![stream(2, "Zed")]]);
        merge(
            &mut held,
            vec![vec![stream(1, "Alpha")], vec![stream(2, "Zed")]],
        );
        assert_eq!(keys(&held), ["Zed", "Alpha"]);
    }

    #[test]
    fn a_stopped_application_keeps_its_row_for_a_few_reads() {
        let mut held = Vec::new();
        merge(
            &mut held,
            vec![vec![stream(1, "Alpha")], vec![stream(2, "Beta")]],
        );
        for read in 1..GHOST_READS {
            merge(&mut held, vec![vec![stream(2, "Beta")]]);
            assert_eq!(keys(&held), ["Alpha", "Beta"], "after {read} reads");
            assert_eq!(held[0].missed, read);
        }
        merge(&mut held, vec![vec![stream(2, "Beta")]]);
        assert_eq!(keys(&held), ["Beta"]);
    }

    #[test]
    fn an_application_that_returns_takes_its_place_back() {
        let mut held = Vec::new();
        merge(
            &mut held,
            vec![vec![stream(1, "Alpha")], vec![stream(2, "Beta")]],
        );
        merge(&mut held, vec![vec![stream(2, "Beta")]]);
        merge(
            &mut held,
            vec![vec![stream(2, "Beta")], vec![stream(9, "Alpha")]],
        );
        assert_eq!(keys(&held), ["Alpha", "Beta"]);
        assert_eq!(held[0].missed, 0);
        assert_eq!(held[0].streams[0].id, 9);
    }
}
