//! The command palette (Ctrl+Shift+P): a search over every enabled command
//! of the window, as in VS Code, Sublime Text and GNOME Builder.
//!
//! - The field has the focus. Typing filters by fuzzy match ("tgsb" finds
//!   "Toggle sidebar") on the title, then on the command's keywords and its
//!   group; the matched letters are marked.
//! - With nothing typed, the commands run last come first, then the rest in
//!   menu order. A command run last also ranks higher among matches.
//! - Each row shows the command's icon, title, group and shortcut, so the
//!   palette teaches the keys.
//! - Up and Down move the highlight, Enter runs it, a click runs a row,
//!   Escape closes. The command runs once the palette has closed and the
//!   focus is back where it was, so a command that opens a dialog or starts
//!   a rename finds the window as it left it.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use freya::prelude::*;

use super::fuzzy::fuzzy;
use super::sheet::key_caps;
use super::{Command, Commands, Resolved};
use crate::component::ComponentName;
use crate::controls::search_field;
use crate::icons::{self, Icon};
use crate::shell::{Overlays, framed_dialog};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, CAPTION, Ink};

/// How tall the palette's card is: the field and about eight rows.
const PALETTE_HEIGHT: f32 = 420.0;

/// How much a command run lately is worth when ranking matches.
const RECENT_BONUS: i32 = 20;

/// What the palette hands back to run after it closes.
#[derive(Clone, Default)]
struct Chosen(Rc<RefCell<Option<Command>>>);

impl PartialEq for Chosen {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Open the command palette of `commands` in the window `overlays` belongs to.
pub fn open_palette(overlays: Overlays, commands: Commands) {
    let chosen = Chosen::default();
    let after = chosen.clone();
    overlays.show_dialog(
        framed_dialog(
            "Commands",
            Element::from(CommandPalette {
                commands,
                overlays,
                chosen,
            }),
            Dim::PopupWidth.px(),
            PALETTE_HEIGHT,
        )
        .on_closed(move |_: ()| {
            if let Some(command) = after.0.borrow_mut().take() {
                commands.invoke(&command);
            }
        }),
    );
}

/// One row of the palette.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pick {
    pub command: Command,
    pub shortcut: Option<super::Shortcut>,
    pub marks: Vec<Range<usize>>,
}

/// The rows for `query`: the enabled commands that match, best first; with
/// no query, the ones run last first and then the rest in menu order.
pub(crate) fn picks(resolved: &[Resolved], recent: &[String], query: &str) -> Vec<Pick> {
    let candidates = resolved
        .iter()
        .filter(|entry| entry.command.in_palette && entry.command.enabled && entry.command.runs());
    let recency = |id: &str| recent.iter().position(|other| other == id);
    let query = query.trim();
    if query.is_empty() {
        let mut rows: Vec<(usize, &Resolved)> = candidates.enumerate().collect();
        rows.sort_by_key(|(order, entry)| {
            (
                recency(&entry.command.id).unwrap_or(usize::MAX),
                entry.command.group.rank(),
                *order,
            )
        });
        return rows
            .into_iter()
            .map(|(_, entry)| pick(entry, Vec::new()))
            .collect();
    }
    let mut scored: Vec<(i32, usize, Pick)> = candidates
        .enumerate()
        .filter_map(|(order, entry)| {
            let command = &entry.command;
            let (score, marks) = match fuzzy(query, &command.title) {
                Some(found) => (found.score, found.ranges),
                None => {
                    // A keyword or the group name finds it too, ranked below
                    // any title match and with no letters to mark.
                    let best = command
                        .keywords
                        .iter()
                        .map(String::as_str)
                        .chain([command.group.title()])
                        .filter_map(|word| fuzzy(query, word))
                        .map(|found| found.score)
                        .max()?;
                    (best - 1000, Vec::new())
                }
            };
            let bonus = recency(&command.id).map_or(0, |at| RECENT_BONUS - at as i32);
            Some((score + bonus, order, pick(entry, marks)))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, pick)| pick).collect()
}

fn pick(entry: &Resolved, marks: Vec<Range<usize>>) -> Pick {
    Pick {
        command: entry.command.clone(),
        shortcut: entry.reachable.first().cloned(),
        marks,
    }
}

/// The scroll offset (negative, as the controller keeps it) that shows row
/// `index` of rows `row` tall in a viewport `viewport` tall, moving as little
/// as it can from `current`.
fn offset_for(index: usize, current: i32, row: f32, viewport: f32) -> i32 {
    let top = index as f32 * row;
    let shown_top = -(current as f32);
    let want = if top < shown_top {
        top
    } else if top + row > shown_top + viewport {
        top + row - viewport
    } else {
        shown_top
    };
    -(want.max(0.0) as i32)
}

/// The palette's content: the field and the rows.
#[derive(PartialEq)]
pub struct CommandPalette {
    commands: Commands,
    overlays: Overlays,
    chosen: Chosen,
}

impl Component for CommandPalette {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = self.overlays;
        let commands = self.commands;
        let query = use_state(String::new);
        let mut highlight = use_state(|| 0usize);
        let mut viewport = use_state(|| 0.0f32);
        let mut scroll = use_scroll_controller(ScrollConfig::default);

        let rows = picks(&commands.resolved(), &commands.recent(), &query.read());
        let count = rows.len();
        let at = (*highlight.read()).min(count.saturating_sub(1));
        let row_height = Dim::MenuItem.px();

        let chosen = self.chosen.clone();
        let choose = {
            let rows = rows.clone();
            move |index: usize| {
                if let Some(row) = rows.get(index) {
                    *chosen.0.borrow_mut() = Some(row.command.clone());
                    overlays.dismiss_dialog();
                }
            }
        };
        let submit = {
            let choose = choose.clone();
            move |_: String| choose(*highlight.peek())
        };
        let step = move |delta: f64| {
            if count == 0 {
                return;
            }
            // Up is +1 from the field; the list runs downwards.
            let next = if delta > 0.0 {
                highlight.peek().checked_sub(1).unwrap_or(count - 1)
            } else {
                (*highlight.peek() + 1) % count
            };
            highlight.set(next);
            let current = <(i32, i32)>::from(scroll).1;
            scroll.scroll_to_y(offset_for(next, current, row_height, *viewport.peek()));
        };

        let field = search_field(query, move |_: String| {
            highlight.set(0);
            scroll.scroll_to_y(0);
        })
        .placeholder("Search commands")
        .fill_width()
        .autofocus()
        .on_submit(submit)
        .on_arrow(step)
        .on_escape(move |_| overlays.dismiss_dialog());

        let list: Element = if rows.is_empty() {
            rect()
                .tw(&tokens, &[w_full(), h_full(), center(), p(S6)])
                .child(BODY.in_ink(Ink::Muted).label(
                    &tokens,
                    format!("No commands match “{}”.", query.read().trim()),
                ))
                .into_element()
        } else {
            ScrollView::new_controlled(scroll)
                .child(rect().tw(&tokens, &[flex_col(), w_full(), p(S2)]).children(
                    rows.iter().enumerate().map(|(index, row)| {
                        let choose = choose.clone();
                        PaletteRow {
                            pick: row.clone(),
                            highlighted: index == at,
                            on_hover: EventHandler::new(move |_: ()| highlight.set(index)),
                            on_press: EventHandler::new(move |_: ()| choose(index)),
                        }
                        .into_element()
                    }),
                ))
                .into_element()
        };

        rect()
            .component("CommandPalette")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .child(rect().tw(&tokens, &[w_full(), p(S3)]).child(field))
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .min_height(Size::px(0.0))
                    .on_sized(move |event: Event<SizedEventData>| {
                        viewport.set_if_modified(event.area.height() - 2.0 * S2.px());
                    })
                    .child(list),
            )
    }
}

/// One command in the palette.
#[derive(PartialEq)]
struct PaletteRow {
    pick: Pick,
    highlighted: bool,
    on_hover: EventHandler<()>,
    on_press: EventHandler<()>,
}

impl Component for PaletteRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let command = &self.pick.command;
        let on_hover = self.on_hover.clone();
        let on_press = self.on_press.clone();
        let text = Ink::Text.color(&tokens);
        let mut parts: Vec<Element> = Vec::with_capacity(4);
        parts.push(
            rect()
                .tw(
                    &tokens,
                    &[w(Dim::IconControl), h(Dim::IconControl), center()],
                )
                .maybe_child(
                    command
                        .icon
                        .map(|icon: Icon| icons::icon(icon, IconSize::Control, text)),
                )
                .into_element(),
        );
        parts.push(
            crate::search_highlight::highlighted_label(
                BODY.in_ink(Ink::Text),
                &tokens,
                &command.title,
                &self.pick.marks,
            )
            .width(Size::flex(1.0))
            .into_element(),
        );
        if command.group != super::CommandGroup::App {
            parts.push(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, command.group.title())
                    .max_lines(1)
                    .into_element(),
            );
        }
        if let Some(shortcut) = &self.pick.shortcut {
            parts.push(key_caps(shortcut).into_element());
        }
        rect()
            .component("PaletteRow")
            .a11y_role(AccessibilityRole::ListBoxOption)
            .a11y_alt(command.title.clone())
            .a11y_builder({
                let selected = self.highlighted;
                move |node| node.set_selected(selected)
            })
            .tw(&tokens, crate::style::MENU_ITEM)
            .tw(
                &tokens,
                &[surface(Paint::Transparent, self.highlighted, false, false)],
            )
            .on_pointer_enter(move |_| on_hover.call(()))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_press.call(());
            })
            .children(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{CommandGroup, resolve};

    // Failure modes: a disabled or palette-hidden command offered; recent
    // commands not first with an empty query; a keyword match outranking a
    // title match; the highlight scrolled out of view.

    fn resolved() -> Vec<Resolved> {
        let window = vec![
            Command::new("tab.new", "New tab", |_| {})
                .shortcut("Ctrl+T")
                .group(CommandGroup::File),
            Command::new("view.hidden", "Show hidden files", |_| {})
                .group(CommandGroup::View)
                .keywords(["dotfiles"]),
            Command::new("edit.paste", "Paste", |_| {})
                .enabled(false)
                .group(CommandGroup::Edit),
            Command::new("app.palette", "Command palette", |_| {}).hidden_from_palette(),
            Command::new("app.prefs", "Preferences", |_| {}),
            Command::key_hint("tab.go", "Go to a tab", "Alt+1 to 9"),
        ];
        resolve(&[(false, &window)])
    }

    fn ids(picks: &[Pick]) -> Vec<&str> {
        picks.iter().map(|pick| pick.command.id.as_str()).collect()
    }

    #[test]
    fn offers_only_enabled_commands_recent_first() {
        let rows = picks(&resolved(), &["view.hidden".to_owned()], "");
        assert_eq!(ids(&rows), ["view.hidden", "app.prefs", "tab.new"]);
        assert_eq!(
            rows[2].shortcut.as_ref().map(|s| s.label()).as_deref(),
            Some("Ctrl+T")
        );
    }

    #[test]
    fn titles_rank_above_keywords_and_recent_breaks_ties() {
        let rows = picks(&resolved(), &[], "dot");
        assert_eq!(ids(&rows), ["view.hidden"], "a keyword finds it");
        assert!(rows[0].marks.is_empty());
        let rows = picks(&resolved(), &[], "pre");
        assert_eq!(ids(&rows)[0], "app.prefs");
        assert!(!rows[0].marks.is_empty());
        let rows = picks(&resolved(), &[], "view");
        assert_eq!(ids(&rows), ["view.hidden"], "the group name finds it");
    }

    #[test]
    fn the_highlight_stays_in_view() {
        assert_eq!(offset_for(0, 0, 36.0, 180.0), 0);
        assert_eq!(offset_for(4, 0, 36.0, 180.0), 0);
        assert_eq!(offset_for(5, 0, 36.0, 180.0), -36);
        assert_eq!(offset_for(1, -72, 36.0, 180.0), -36);
    }
}
