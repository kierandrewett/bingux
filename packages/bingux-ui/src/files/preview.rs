//! Quick Look: press Space on a file in a file view to see it.
//!
//! A large card opens over a dimmed scrim, as macOS Quick Look and GNOME
//! Sushi do, showing the file at the cursor: the picture scaled to fit, the
//! head of a text file, the thumbnail of a document or film, a folder's item
//! count and size, or the type's icon and the facts. See `preview_stage.rs`
//! for what each kind shows and how it loads.
//!
//! # Keys
//!
//! | Key | Does |
//! | --- | ---- |
//! | Space (in the file view) | open the preview of the selected file |
//! | Space, Escape | close it |
//! | Left, Right | the previous and next file; the selection follows |
//! | Up, Down | the file above and below in a grid; the previous and next in a list |
//! | Enter | close it and open the file as a double click does |
//! | Alt+Enter, Ctrl+I | close it and show the file's Properties |
//!
//! Space only opens the preview when the file view itself has the keyboard:
//! never while a rename field, the address bar or a filter has it, and not
//! in the middle of a word typed to jump to a file. The rules are in
//! [`space_opens`] and [`route`], pure functions with tests.

use bingux_files::preview::{Step, neighbour};
use freya::prelude::*;

use super::actions::{self, Command};
use super::preview_stage::Subject;
use super::view::{Ctx, Shared};
use super::zoom::ViewKind;
use crate::component::ComponentName;
use crate::controls::{icon_button, path_label};
use crate::icons::Icon;
use crate::shell::{Overlays, framed_dialog};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink, SECTION};

/// The largest and the smallest card, in logical pixels. It is cut down to
/// fit a small window.
const CARD: (f32, f32) = (1040.0, 760.0);
const CARD_MIN: (f32, f32) = (420.0, 360.0);

/// The card for a file view whose scrolled area is `view` wide and tall: as
/// wide as the view and a little taller, so over the chooser dialog it stays
/// inside the dialog and over a full window it is large.
pub(super) fn card_size(view: (f32, f32)) -> (f32, f32) {
    if view.0 < 1.0 || view.1 < 1.0 {
        return CARD;
    }
    (
        view.0.clamp(CARD_MIN.0, CARD.0),
        (view.1 + 96.0).clamp(CARD_MIN.1, CARD.1),
    )
}

// ---------------------------------------------------------------------------
// Which keys do what
// ---------------------------------------------------------------------------

/// What the file view knows when Space is pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Gate {
    /// The keyboard focus is on the file view itself, not on a text field
    /// or a control inside or beside it.
    pub view_focused: bool,
    /// An inline rename is open.
    pub renaming: bool,
    /// A word is being typed to jump to a file.
    pub typing_ahead: bool,
    pub has_selection: bool,
}

/// Whether this key press opens the preview. Plain Space does, and only when
/// the file view has the keyboard, no rename is open, no word is being typed
/// and something is selected. Ctrl+Space toggles the cursor row's selection
/// and is not this.
pub(super) fn space_opens(key: &Key, modifiers: Modifiers, gate: Gate) -> bool {
    let plain = !modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);
    matches!(key, Key::Character(text) if text == " ")
        && plain
        && gate.view_focused
        && !gate.renaming
        && !gate.typing_ahead
        && gate.has_selection
}

/// What a key does while the preview is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Route {
    Close,
    /// Close and open the file.
    Open,
    /// Close and show the Properties.
    Properties,
    Step(Step),
    Ignore,
}

/// The meaning of `key` in the open preview. `grid`: the files are laid out
/// in rows of tiles, so Up and Down step a line and not one file.
pub(super) fn route(key: &Key, modifiers: Modifiers, grid: bool) -> Route {
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let alt = modifiers.contains(Modifiers::ALT);
    match key {
        Key::Character(text) if text == " " && !ctrl && !alt => Route::Close,
        Key::Named(NamedKey::Escape) => Route::Close,
        Key::Named(NamedKey::Enter) if alt && !ctrl => Route::Properties,
        Key::Character(text) if ctrl && !alt && text.eq_ignore_ascii_case("i") => {
            Route::Properties
        }
        Key::Named(NamedKey::Enter) if !ctrl && !alt => Route::Open,
        Key::Named(NamedKey::ArrowLeft) if !ctrl && !alt => Route::Step(Step::Previous),
        Key::Named(NamedKey::ArrowRight) if !ctrl && !alt => Route::Step(Step::Next),
        Key::Named(NamedKey::ArrowUp) if !ctrl && !alt => {
            Route::Step(if grid { Step::Up } else { Step::Previous })
        }
        Key::Named(NamedKey::ArrowDown) if !ctrl && !alt => {
            Route::Step(if grid { Step::Down } else { Step::Next })
        }
        _ => Route::Ignore,
    }
}

// ---------------------------------------------------------------------------
// Opening it
// ---------------------------------------------------------------------------

/// Whether `event` is the Space that opens the preview in the view `ctx`.
pub(super) fn wants_open(
    ctx: &Ctx,
    event: &Event<KeyboardEventData>,
    typing_ahead: bool,
) -> bool {
    let focused = *Platform::get()
        .current_window()
        .focused_accessibility_id
        .peek();
    space_opens(
        &event.key,
        event.modifiers,
        Gate {
            view_focused: focused == ctx.focus,
            renaming: ctx.env.ui.rename.peek().is_some(),
            typing_ahead,
            has_selection: subject_row(ctx).is_some(),
        },
    )
}

/// Open the preview over the view `shared` belongs to.
pub(super) fn show(shared: &Shared) {
    let ctx = shared.get();
    if subject_row(&ctx).is_none() {
        return;
    }
    let overlays = ctx.env.overlays;
    // The dialog took the keyboard when it opened; give it back when it goes.
    let focus = ctx.focus;
    let view = *ctx.viewport.peek();
    let (width, height) = card_size((view.width(), view.height()));
    overlays.show_dialog(
        framed_dialog(
            "Preview",
            Element::from(QuickLook {
                shared: shared.clone(),
                overlays,
            }),
            width,
            height,
        )
        .on_closed(move |()| focus.request_focus()),
    );
}

/// The row being previewed: the cursor row when it is selected, else the
/// first selected row that is on show.
fn subject_row(ctx: &Ctx) -> Option<usize> {
    let selection = ctx.env.selection.peek();
    selection
        .cursor()
        .filter(|cursor| selection.contains(cursor))
        .and_then(|cursor| ctx.rows.row_of(cursor))
        .or_else(|| {
            selection
                .paths()
                .iter()
                .find_map(|path| ctx.rows.row_of(path))
        })
}

/// Move the selection one step from the previewed row, over rows that cannot
/// be chosen (a folder chooser does not take files). Stays put at an edge.
fn step(shared: &Shared, step: Step) {
    let ctx = shared.get();
    let Some(from) = subject_row(&ctx) else {
        return;
    };
    let columns = if ctx.zoom.kind() == ViewKind::Grid {
        ctx.lattice.get().map_or(1, |lattice| lattice.columns)
    } else {
        1
    };
    let mut at = from;
    while let Some(to) = neighbour(at, ctx.rows.len(), step, columns) {
        if ctx.selectable(ctx.rows.entry(to)) {
            ctx.go_to(to, false);
            ctx.reveal(to);
            return;
        }
        at = to;
    }
}

// ---------------------------------------------------------------------------
// The card
// ---------------------------------------------------------------------------

/// The content of the preview dialog. It reads the view's selection, so the
/// file it shows follows the arrow keys.
#[derive(PartialEq)]
struct QuickLook {
    shared: Shared,
    overlays: Overlays,
}

impl Component for QuickLook {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.shared.get();
        // Reading the selection here is what makes the card follow it.
        let selected = ctx.env.selection.read().paths().len();
        let overlays = self.overlays;
        let shared = self.shared.clone();
        let on_key = move |event: Event<KeyboardEventData>| {
            let ctx = shared.get();
            let route = route(&event.key, event.modifiers, ctx.zoom.kind() == ViewKind::Grid);
            if route == Route::Ignore {
                return;
            }
            event.stop_propagation();
            match route {
                Route::Close => overlays.dismiss_dialog(),
                Route::Step(direction) => step(&shared, direction),
                Route::Open => {
                    overlays.dismiss_dialog();
                    if let Some(row) = subject_row(&ctx) {
                        ctx.env.activate.call(ctx.rows.order[row]);
                    }
                }
                Route::Properties => {
                    overlays.dismiss_dialog();
                    let selected = ctx.selected_owned();
                    actions::run(&ctx.env, &selected, Command::Properties);
                }
                Route::Ignore => {}
            }
        };

        let row = subject_row(&ctx);
        let entry = row.map(|row| ctx.rows.entry(row).clone());
        let position = row.map(|row| {
            let mut text = format!("{} of {}", row + 1, ctx.rows.len());
            if selected > 1 {
                text.push_str(&format!(" · {selected} selected"));
            }
            text
        });

        // The standard header bar: its icon buttons take the header size and
        // its insets, so Close sits where it does in every other header.
        let title: Element = rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S3), w_full()])
            .child(
                rect().tw(&tokens, &[w_flex()]).child(
                    path_label(
                        entry
                            .as_ref()
                            .map_or("Nothing to preview", |entry| entry.name.as_str()),
                    )
                    .name()
                    .style(SECTION),
                ),
            )
            .maybe_child(position.map(|text| CAPTION.in_ink(Ink::Muted).label(&tokens, text).max_lines(1)))
            .into();
        let header = crate::shell::HeaderBar::new("")
            .draggable(false)
            .title_slot(title)
            .trailing(
                icon_button(Icon::WindowClose, move |_| overlays.dismiss_dialog())
                    .tooltip("Close (Space)"),
            );

        rect()
            .component("QuickLook")
            .tw(&tokens, &[flex_col(), grow_children(), w_full(), h_full()])
            .on_global_key_down(on_key)
            .child(header)
            .child(rect().tw(&tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)]))
            .child(match entry {
                Some(entry) => Element::from(Subject { entry }),
                None => rect().tw(&tokens, &[w_full(), h_flex()]).into(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(text: &str) -> Key {
        Key::Character(text.to_owned())
    }

    const READY: Gate = Gate {
        view_focused: true,
        renaming: false,
        typing_ahead: false,
        has_selection: true,
    };

    #[test]
    fn space_opens_only_when_the_view_has_the_keyboard() {
        let none = Modifiers::empty();
        assert!(space_opens(&character(" "), none, READY));
        // A rename field, the address bar or a filter has the keyboard.
        let elsewhere = Gate { view_focused: false, ..READY };
        assert!(!space_opens(&character(" "), none, elsewhere));
        let renaming = Gate { renaming: true, ..READY };
        assert!(!space_opens(&character(" "), none, renaming));
    }

    #[test]
    fn space_is_left_to_a_word_being_typed() {
        let typing = Gate { typing_ahead: true, ..READY };
        assert!(!space_opens(&character(" "), Modifiers::empty(), typing));
    }

    #[test]
    fn space_needs_something_selected_and_no_command_modifier() {
        let nothing = Gate { has_selection: false, ..READY };
        assert!(!space_opens(&character(" "), Modifiers::empty(), nothing));
        // Ctrl+Space toggles the row; Alt and Meta are other shortcuts.
        for held in [Modifiers::CONTROL, Modifiers::ALT, Modifiers::META] {
            assert!(!space_opens(&character(" "), held, READY));
        }
        assert!(space_opens(&character(" "), Modifiers::SHIFT, READY));
        assert!(!space_opens(&character("a"), Modifiers::empty(), READY));
        assert!(!space_opens(&Key::Named(NamedKey::Enter), Modifiers::empty(), READY));
    }

    #[test]
    fn the_open_preview_answers_its_keys() {
        let none = Modifiers::empty();
        assert_eq!(route(&character(" "), none, false), Route::Close);
        assert_eq!(route(&Key::Named(NamedKey::Escape), none, false), Route::Close);
        assert_eq!(route(&Key::Named(NamedKey::Enter), none, false), Route::Open);
        assert_eq!(
            route(&Key::Named(NamedKey::Enter), Modifiers::ALT, false),
            Route::Properties
        );
        assert_eq!(route(&character("i"), Modifiers::CONTROL, false), Route::Properties);
        assert_eq!(route(&character("a"), none, false), Route::Ignore);
        // Ctrl+Space is not Close: it is the view's toggle, which is not open.
        assert_eq!(route(&character(" "), Modifiers::CONTROL, false), Route::Ignore);
    }

    #[test]
    fn the_card_follows_the_view_it_opens_over() {
        // Before the view is laid out: the full size.
        assert_eq!(card_size((0.0, 0.0)), CARD);
        // The chooser's list: the card stays inside the dialog.
        assert_eq!(card_size((650.0, 440.0)), (650.0, 536.0));
        // A big window: capped. A tiny one: floored.
        assert_eq!(card_size((2000.0, 1400.0)), CARD);
        assert_eq!(card_size((200.0, 100.0)), CARD_MIN);
    }

    #[test]
    fn arrows_step_by_one_in_a_list_and_by_a_line_in_a_grid() {
        let none = Modifiers::empty();
        let arrow = |named: NamedKey, grid: bool| route(&Key::Named(named), none, grid);
        assert_eq!(arrow(NamedKey::ArrowLeft, false), Route::Step(Step::Previous));
        assert_eq!(arrow(NamedKey::ArrowRight, true), Route::Step(Step::Next));
        assert_eq!(arrow(NamedKey::ArrowDown, false), Route::Step(Step::Next));
        assert_eq!(arrow(NamedKey::ArrowUp, false), Route::Step(Step::Previous));
        assert_eq!(arrow(NamedKey::ArrowDown, true), Route::Step(Step::Down));
        assert_eq!(arrow(NamedKey::ArrowUp, true), Route::Step(Step::Up));
    }
}
