//! The keyboard shortcuts sheet (libadwaita's `ShortcutsDialog`, GNOME's
//! Ctrl+?): every shortcut the window answers now, by group, each as key
//! caps, with a filter at the top.
//!
//! It lists the commands of the focused view and of the window, a shortcut
//! an inner view has taken shown only once, under the command that has it,
//! and the keys that controls handle themselves ([`Command::key_hint`]). It
//! is a [`framed_dialog`]: Escape and a press on the scrim close it, and the
//! focus returns to where it was. Typing filters it by title or by key
//! ("ctrl+z").

use freya::prelude::*;

use super::fuzzy::fuzzy;
use super::{CommandGroup, Commands, Resolved, Shortcut};
use crate::a11y::SemanticsExt;
use crate::component::ComponentName;
use crate::controls::{icon_button, search_field};
use crate::group::{Group, caption};
use crate::icons::Icon;
use crate::rows::row;
use crate::shell::{HeaderBar, Overlays, framed_dialog};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// Open the shortcuts sheet of `commands` in the window `overlays` belongs to.
pub fn open_shortcuts(overlays: Overlays, commands: Commands) {
    overlays.show_dialog(framed_dialog(
        crate::tr!("ui-shortcuts-title"),
        Element::from(ShortcutsSheet { commands, overlays }),
        Dim::PopupWidth.px(),
        Dim::PopupHeight.px(),
    ));
}

/// One part of a command's key column: a shortcut's key caps, or the word
/// "or" between two shortcuts that each run the command.
enum Alternative<'a> {
    Keys(&'a Shortcut),
    Or,
}

/// The key column of a command, with "or" between alternatives. Without it
/// `Ctrl+/` and `Ctrl+?` next to each other read as one key listed twice.
fn alternatives(shortcuts: &[Shortcut]) -> Vec<Alternative<'_>> {
    let mut parts = Vec::with_capacity(shortcuts.len() * 2);
    for (index, shortcut) in shortcuts.iter().enumerate() {
        if index > 0 {
            parts.push(Alternative::Or);
        }
        parts.push(Alternative::Keys(shortcut));
    }
    parts
}

/// A shortcut as key caps: one small key per modifier and the key, as the
/// sheet and the palette show them. See [`key_caps`].
#[derive(Clone, PartialEq)]
pub struct KeyCaps {
    caps: Vec<String>,
}

/// The key caps of `shortcut`: `Ctrl` `Shift` `P`.
pub fn key_caps(shortcut: &Shortcut) -> KeyCaps {
    KeyCaps {
        caps: shortcut.caps(),
    }
}

impl Component for KeyCaps {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .component("KeyCaps")
            .a11y_role(AccessibilityRole::Label)
            .a11y_alt(self.caps.join("+"))
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S1)])
            .children(self.caps.iter().map(|cap| {
                rect()
                    .interactive(false)
                    .tw(
                        &tokens,
                        &[
                            flex_row(),
                            center(),
                            h(Dim::Box5),
                            min_w(Dim::Box5),
                            px(S1),
                            rounded(Radius::Mark),
                            border(Line::Hairline),
                            bg(Paint::Control),
                        ],
                    )
                    .child(
                        CAPTION
                            .in_ink(Ink::Muted)
                            .label(&tokens, cap.clone())
                            .max_lines(1),
                    )
                    .into_element()
            }))
    }
}

/// One line of the sheet: a command and the keys that reach it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SheetLine {
    pub group: CommandGroup,
    pub title: String,
    pub shortcuts: Vec<Shortcut>,
    pub keys_text: Option<String>,
    /// The title's letters the filter matched.
    pub marks: Vec<std::ops::Range<usize>>,
}

/// The lines of the sheet for `resolved`, filtered by `query`, grouped in
/// menu-bar order and, inside a group, in the order the commands came.
pub(crate) fn lines(resolved: &[Resolved], query: &str) -> Vec<SheetLine> {
    let query = query.trim();
    let mut lines: Vec<SheetLine> = resolved
        .iter()
        .filter(|entry| !entry.reachable.is_empty() || entry.command.keys_text.is_some())
        .filter_map(|entry| {
            let command = &entry.command;
            let keys = entry
                .reachable
                .iter()
                .map(Shortcut::label)
                .chain(command.keys_text.clone())
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            let marks = if query.is_empty() {
                Vec::new()
            } else if let Some(found) = fuzzy(query, &command.title) {
                found.ranges
            } else if keys.contains(&query.to_lowercase()) {
                Vec::new()
            } else {
                return None;
            };
            Some(SheetLine {
                group: command.group,
                title: command.title.clone(),
                shortcuts: entry.reachable.clone(),
                keys_text: command.keys_text.clone(),
                marks,
            })
        })
        .collect();
    lines.sort_by_key(|line| line.group.rank());
    lines
}

/// The sheet's content: a header, the filter and the groups.
#[derive(PartialEq)]
pub struct ShortcutsSheet {
    pub commands: Commands,
    pub overlays: Overlays,
}

impl Component for ShortcutsSheet {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let query = use_state(String::new);
        let scroll = use_scroll_controller(ScrollConfig::default);
        crate::rows::provide_row_width(
            crate::shell::dialog::frame_width(Dim::PopupWidth.px()) - 2.0 * S4.px(),
        );

        let all = lines(&self.commands.resolved(), &query.read());
        let mut groups: Vec<CommandGroup> = Vec::new();
        for line in &all {
            if !groups.contains(&line.group) {
                groups.push(line.group);
            }
        }
        let content: Vec<Element> = if all.is_empty() {
            vec![caption(if query.read().trim().is_empty() {
                crate::tr!("ui-shortcuts-none-empty")
            } else {
                crate::tr!("ui-shortcuts-no-match-empty", query = query.read().trim())
            })]
        } else {
            groups
                .iter()
                .map(|group| {
                    Group::new(Some(group.title()))
                        .rows(all.iter().filter(|line| line.group == *group).map(|line| {
                            let keys: Element = match (line.shortcuts.first(), &line.keys_text) {
                                (Some(_), _) => rect()
                                    .tw(&tokens, &[flex_row(), items(Align::Center), gap(S3)])
                                    .children(alternatives(&line.shortcuts).into_iter().map(
                                        |part| {
                                            match part {
                                                Alternative::Keys(shortcut) => {
                                                    key_caps(shortcut).into_element()
                                                }
                                                Alternative::Or => CAPTION
                                                    .in_ink(Ink::Muted)
                                                    .label(
                                                        &tokens,
                                                        crate::tr!("ui-shortcut-or").to_string(),
                                                    )
                                                    .into_element(),
                                            }
                                        },
                                    ))
                                    .into_element(),
                                (None, Some(text)) => CAPTION
                                    .in_ink(Ink::Muted)
                                    .label(&tokens, text.clone())
                                    .into_element(),
                                (None, None) => rect().into_element(),
                            };
                            // A line is one list item, named by the command's
                            // title, its keys its keyboard shortcut and its
                            // description ("Command palette", then
                            // "Ctrl+Shift+P"), never part of the name. The key
                            // caps are hidden, as the item says them.
                            let spoken: Vec<String> = line
                                .shortcuts
                                .iter()
                                .map(Shortcut::label)
                                .chain(line.keys_text.clone())
                                .collect();
                            let mut semantics = crate::a11y::Semantics::new()
                                .role(AccessibilityRole::ListItem)
                                .hint(crate::format::List::or(spoken).format());
                            if let Some(first) = line.shortcuts.first() {
                                semantics = semantics.shortcut(first.label());
                            }
                            rect()
                                .width(Size::fill())
                                .semantics(semantics)
                                .child(
                                    row(line.title.clone())
                                        .title_marks(line.marks.clone())
                                        .trailing(rect().a11y_hidden().child(keys))
                                        .trailing_fit(),
                                )
                                .into_element()
                        }))
                        .into_element()
                })
                .collect()
        };

        let header = HeaderBar::new(crate::tr!("ui-shortcuts-title"))
            .draggable(false)
            .trailing(
                icon_button(Icon::WindowClose, move |_| overlays.dismiss_dialog())
                    .tooltip(&crate::tr!("ui-dialog-close")),
            );
        rect()
            .component("ShortcutsSheet")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .child(header)
            .child(
                rect().tw(&tokens, &[w_full(), px(S4), pb(S3)]).child(
                    search_field(query, |_: String| {})
                        .placeholder(crate::tr!("ui-shortcuts-search.placeholder"))
                        .fill_width()
                        .autofocus()
                        .on_escape(move |_| overlays.dismiss_dialog()),
                ),
            )
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .child(
                        ScrollView::new_controlled(scroll).child(
                            rect()
                                .tw(&tokens, &[flex_col(), w_full(), gap(S4), p(S4)])
                                .children(content),
                        ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, resolve};

    // Failure modes: a shortcut an inner view took listed twice; commands with
    // no keys cluttering the sheet; groups out of menu-bar order; a filter by
    // key ("ctrl+z") finding nothing.

    fn commands() -> Vec<Resolved> {
        let view = vec![
            Command::new("view.select-all", "Select all files", |_| {})
                .shortcut("Ctrl+A")
                .group(CommandGroup::Edit),
        ];
        let window = vec![
            Command::new("win.select-all", "Select all", |_| {})
                .shortcut("Ctrl+A")
                .group(CommandGroup::Edit),
            Command::new("win.find", "Find", |_| {})
                .shortcut("Ctrl+F")
                .group(CommandGroup::Edit),
            Command::new("win.new", "New window", |_| {})
                .shortcut("Ctrl+N")
                .group(CommandGroup::File),
            Command::new("win.prefs", "Preferences", |_| {}),
            Command::key_hint("win.tabs", "Go to a tab", "Alt+1 to 9").group(CommandGroup::Go),
        ];
        resolve(&[(true, &view), (false, &window)])
    }

    #[test]
    fn lists_each_reachable_key_once_in_menu_order() {
        let lines = lines(&commands(), "");
        let titles: Vec<&str> = lines.iter().map(|line| line.title.as_str()).collect();
        assert_eq!(
            titles,
            ["New window", "Select all files", "Find", "Go to a tab"]
        );
        assert_eq!(lines[3].keys_text.as_deref(), Some("Alt+1 to 9"));
    }

    #[test]
    fn filters_by_title_or_by_key() {
        let by_title = lines(&commands(), "nw");
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].title, "New window");
        let by_key = lines(&commands(), "ctrl+f");
        assert_eq!(by_key.len(), 1);
        assert_eq!(by_key[0].title, "Find");
        assert!(lines(&commands(), "zzz").is_empty());
    }

    #[test]
    fn alternative_shortcuts_are_separated_by_or() {
        let shortcuts = [
            Shortcut::parse("Ctrl+/").unwrap(),
            Shortcut::parse("Ctrl+?").unwrap(),
        ];
        let shown: Vec<String> = alternatives(&shortcuts)
            .iter()
            .map(|part| match part {
                Alternative::Keys(shortcut) => shortcut.to_string(),
                Alternative::Or => "or".to_string(),
            })
            .collect();
        assert_eq!(shown, ["Ctrl+/", "or", "Ctrl+?"]);
        assert_eq!(alternatives(&shortcuts[..1]).len(), 1);
        assert!(alternatives(&[]).is_empty());
    }
}
