//! The Navigation section of the Properties pane: the inspected window's
//! stack as its router holds it (`Router::snapshot`), so a wrong Back or a
//! missing history row can be read off instead of guessed.
//!
//! The router of a window registers a reader with [`super::register_navigation`];
//! this reads it when the pane renders, which is on every new snapshot of the
//! target, so the section follows the stack.

use freya::prelude::*;
use freya::winit::window::WindowId;

use crate::controls::property_row;
use crate::group::Group;
use crate::nav::{EntryInfo, Mode, ModalKind, Snapshot};
use crate::style::*;
use crate::theme::Tokens;
use crate::typography::CODE;

/// The section for `window`, or `None` when the window has no router (a
/// window that is not an `Application` window, or an inspector's own).
pub(super) fn section(tokens: &Tokens, window: WindowId) -> Option<Element> {
    let snapshot = super::navigation(window)?;
    Some(Group::titled("Navigation").rows(rows(tokens, &snapshot)).into())
}

fn value(tokens: &Tokens, text: String) -> Element {
    CODE.label(tokens, text)
        .max_lines(1)
        .text_overflow(TextOverflow::Ellipsis)
        .into()
}

fn row(tokens: &Tokens, name: impl Into<String>, text: String) -> Element {
    property_row(name).control(value(tokens, text)).into()
}

/// `title  uri`: what a stack entry is, and the place it names.
fn entry_text(entry: &EntryInfo) -> String {
    format!("{}  {}", entry.title, entry.uri)
}

fn rows(tokens: &Tokens, snapshot: &Snapshot) -> Vec<Element> {
    let mut rows = vec![
        row(tokens, "Location", snapshot.current.uri.clone()),
        row(tokens, "Route", snapshot.current.route.clone()),
        row(tokens, "Title", snapshot.current.title.clone()),
        row(
            tokens,
            "Mode",
            match snapshot.mode {
                Mode::Hierarchical => "hierarchical".to_string(),
                Mode::Linear => "linear".to_string(),
            },
        ),
        row(tokens, "Entry", snapshot.current.id.to_string()),
    ];
    for (name, bound) in &snapshot.current.params {
        rows.push(row(tokens, format!("Param {name}"), bound.clone()));
    }
    // The stack as Back sees it: nearest first, with the entry id a history
    // menu choice is addressed by.
    for (steps, entry) in snapshot.back.iter().rev().enumerate() {
        rows.push(row(
            tokens,
            format!("Back {}", steps + 1),
            format!("#{} {}", entry.id, entry_text(entry)),
        ));
    }
    for (steps, entry) in snapshot.forward.iter().enumerate() {
        rows.push(row(
            tokens,
            format!("Forward {}", steps + 1),
            format!("#{} {}", entry.id, entry_text(entry)),
        ));
    }
    // Top first: the one Back dismisses.
    for modal in snapshot.modals.iter().rev() {
        let kind = match modal.kind {
            ModalKind::Menu => "Menu",
            ModalKind::Choice => "Choice",
            ModalKind::Dialog => "Dialog",
            ModalKind::Sheet => "Sheet",
        };
        rows.push(row(tokens, format!("Modal {kind}"), modal.title.clone()));
    }
    rows
}
