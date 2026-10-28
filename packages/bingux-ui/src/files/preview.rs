//! The file view's side of previewing: what opens a preview, which file it
//! shows, and what its buttons do.
//!
//! The preview itself (viewers, frames, keys) is `bingux_ui::preview`; it
//! knows no file list. This module is the adapter: it turns the view's
//! selection into a [`PreviewSource`], steps the selection on the arrow keys,
//! and answers Open and Properties.
//!
//! # Keys in the file view
//!
//! | Key | Does |
//! | --- | ---- |
//! | Space | preview the selected file in the last frame used (popup or pane); with the pane open, hide it |
//! | Alt+P, F3 | show or hide the docked pane |
//!
//! Space only opens the preview when the file view itself has the keyboard:
//! never while a rename field, the address bar or a filter has it, and not in
//! the middle of a word typed to jump to a file. The rules are in
//! [`space_opens`], a pure function with tests. The keys inside the preview
//! are in `bingux_ui::preview`.

use freya::prelude::*;

use super::actions::{self, Command};
use super::view::{Ctx, Shared};
use super::view_state::{load as remembered, save as remember};
use super::zoom::ViewKind;
use crate::preview::{Mode, MIN_WINDOW, PreviewDock, PreviewHost, PreviewNav, PreviewSource, Step, card_size, neighbour, show as show_frame};
use crate::style::is_compact;

/// What the view remembers of the frame the person last chose.
const MODE_KEY: &str = "preview.mode";

// ---------------------------------------------------------------------------
// Which keys open it
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

/// Whether this key press shows or hides the pane: Alt+P or F3, whatever the
/// selection is, when the file view has the keyboard and no word is being
/// typed.
pub(super) fn pane_toggles(key: &Key, modifiers: Modifiers, gate: Gate) -> bool {
    let pressed = match key {
        Key::Named(NamedKey::F3) => !modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META),
        Key::Character(text) => {
            text.eq_ignore_ascii_case("p") && modifiers.contains(Modifiers::ALT) && !modifiers.intersects(Modifiers::CONTROL | Modifiers::META)
        }
        _ => false,
    };
    pressed && gate.view_focused && !gate.renaming && !gate.typing_ahead
}

/// The gate of the view `ctx` for the key press `typing_ahead` says is part of
/// a word.
fn gate(ctx: &Ctx, typing_ahead: bool) -> Gate {
    let focused = *Platform::get().current_window().focused_accessibility_id.peek();
    Gate {
        view_focused: focused == ctx.focus,
        renaming: ctx.env.ui.rename.peek().is_some(),
        typing_ahead,
        has_selection: subject_row(ctx).is_some(),
    }
}

/// Whether `event` is the Space that opens the preview in the view `ctx`.
pub(super) fn wants_open(ctx: &Ctx, event: &Event<KeyboardEventData>, typing_ahead: bool) -> bool {
    space_opens(&event.key, event.modifiers, gate(ctx, typing_ahead))
}

/// Whether `event` is the key that shows or hides the pane.
pub(super) fn wants_pane(ctx: &Ctx, event: &Event<KeyboardEventData>, typing_ahead: bool) -> bool {
    pane_toggles(&event.key, event.modifiers, gate(ctx, typing_ahead))
}

// ---------------------------------------------------------------------------
// Opening it
// ---------------------------------------------------------------------------

fn last_mode() -> Mode {
    remembered::<String>(MODE_KEY)
        .and_then(|name| Mode::from_name(&name))
        .unwrap_or(Mode::Popup)
}

fn remember_mode(mode: Mode) {
    if matches!(mode, Mode::Popup | Mode::Pane) {
        remember(MODE_KEY, &mode.name().to_owned());
    }
}

/// Space, the menu's Preview: show the preview in the frame the person used
/// last, or hide the pane when it is open.
pub(super) fn toggle(shared: &Shared) {
    let ctx = shared.get();
    if *ctx.pane.peek() {
        close_pane(&ctx);
        return;
    }
    open_in(shared, last_mode());
}

/// Alt+P, F3, the toolbar button: show or hide the pane.
pub(super) fn toggle_pane(shared: &Shared) {
    let ctx = shared.get();
    if *ctx.pane.peek() {
        close_pane(&ctx);
    } else {
        open_in(shared, Mode::Pane);
    }
}

fn close_pane(ctx: &Ctx) {
    let mut pane = ctx.pane;
    pane.set(false);
    ctx.focus.request_focus();
}

fn open_in(shared: &Shared, mode: Mode) {
    let ctx = shared.get();
    // A pane needs a window wide enough for a list beside it.
    let room = ctx.pane_row.peek().width() >= MIN_WINDOW;
    let mode = if mode == Mode::Pane && !room { Mode::Popup } else { mode.available(is_compact()) };
    match mode {
        Mode::Pane => {
            remember_mode(Mode::Pane);
            let mut pane = ctx.pane;
            pane.set(true);
        }
        Mode::Popup | Mode::Fullscreen => {
            if subject_row(&ctx).is_none() {
                return;
            }
            remember_mode(Mode::Popup);
            show_popup(shared, mode);
        }
    }
}

/// Open the popup (or the full-screen sheet) over the view `shared` belongs to.
fn show_popup(shared: &Shared, mode: Mode) {
    let ctx = shared.get();
    let overlays = ctx.env.overlays;
    // The dialog took the keyboard when it opened; give it back when it goes.
    let focus = ctx.focus;
    let view = *ctx.viewport.peek();
    show_frame(
        overlays,
        mode,
        card_size((view.width(), view.height())),
        is_compact(),
        QuickLook {
            shared: shared.clone(),
            mode,
        },
        move |()| focus.request_focus(),
    );
}

/// The row being previewed: the cursor row when it is selected, else the
/// first selected row that is on show.
pub(super) fn subject_row(ctx: &Ctx) -> Option<usize> {
    let selection = ctx.env.selection.peek();
    selection
        .cursor()
        .filter(|cursor| selection.contains(cursor))
        .and_then(|cursor| ctx.rows.row_of(cursor))
        .or_else(|| selection.paths().iter().find_map(|path| ctx.rows.row_of(path)))
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
// The popup and the pane
// ---------------------------------------------------------------------------

/// What the buttons of a preview of the view `shared` do, in `mode`.
fn nav(shared: &Shared, mode: Mode) -> PreviewNav {
    let overlays = shared.get().env.overlays;
    let nav = PreviewNav::new({
        let shared = shared.clone();
        move |_| match mode {
            Mode::Pane => close_pane(&shared.get()),
            _ => overlays.dismiss_dialog(),
        }
    })
    .dockable(true)
    .on_open({
        let shared = shared.clone();
        move |_| {
            let ctx = shared.get();
            if let Some(row) = subject_row(&ctx) {
                ctx.env.activate.call(ctx.rows.order[row]);
            }
        }
    })
    .on_properties({
        let shared = shared.clone();
        move |_| {
            let ctx = shared.get();
            let selected = ctx.selected_owned();
            actions::run(&ctx.env, &selected, Command::Properties);
        }
    })
    .on_mode({
        let shared = shared.clone();
        move |next: Mode| match next {
            Mode::Pane => {
                shared.get().env.overlays.dismiss_dialog();
                open_in(&shared, Mode::Pane);
            }
            other => {
                // A pane becomes a popup; a popup becomes full screen and back.
                let ctx = shared.get();
                if *ctx.pane.peek() {
                    close_pane(&ctx);
                }
                remember_mode(Mode::Popup);
                show_popup(&shared, other);
            }
        }
    });
    if mode == Mode::Pane {
        nav
    } else {
        let ctx = shared.get();
        let selected = ctx.env.selection.read().paths().len();
        let _ = selected;
        nav.grid(ctx.zoom.kind() == ViewKind::Grid).on_step({
            let shared = shared.clone();
            move |direction: Step| step(&shared, direction)
        })
    }
}

/// The content of the popup. It reads the view's selection, so the file it
/// shows follows the arrow keys.
#[derive(PartialEq)]
struct QuickLook {
    shared: Shared,
    mode: Mode,
}

impl Component for QuickLook {
    fn render(&self) -> impl IntoElement {
        let ctx = self.shared.get();
        // Reading the selection here is what makes the preview follow it.
        let _ = ctx.env.selection.read();
        let row = subject_row(&ctx);
        let source = row.map(|row| PreviewSource::path(ctx.rows.entry(row).path.clone()));
        let mut nav = nav(&self.shared, self.mode);
        if let Some(row) = row {
            nav = nav.position(row + 1, ctx.rows.len());
        }
        PreviewHost::new(source, self.mode, nav)
    }
}

/// The docked pane for the view `shared` belongs to, last in the row that
/// holds the view. `row` is that row's area.
pub(super) fn dock(shared: &Shared, row: Area) -> Element {
    let ctx = shared.get();
    // Reading the selection here is what makes the pane follow it.
    let _ = ctx.env.selection.read();
    let open = *ctx.pane.read();
    let source = subject_row(&ctx).map(|row| PreviewSource::path(ctx.rows.entry(row).path.clone()));
    PreviewDock::new(open, row, source, nav(shared, Mode::Pane)).into()
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
    fn the_pane_has_its_own_keys_and_needs_no_selection() {
        let none = Modifiers::empty();
        let nothing = Gate { has_selection: false, ..READY };
        assert!(pane_toggles(&Key::Named(NamedKey::F3), none, nothing));
        assert!(pane_toggles(&character("p"), Modifiers::ALT, nothing));
        assert!(pane_toggles(&character("P"), Modifiers::ALT | Modifiers::SHIFT, READY));
        // A plain P is a letter typed to jump to a file; Ctrl+P is print.
        assert!(!pane_toggles(&character("p"), none, READY));
        assert!(!pane_toggles(&character("p"), Modifiers::CONTROL, READY));
        let elsewhere = Gate { view_focused: false, ..READY };
        assert!(!pane_toggles(&Key::Named(NamedKey::F3), none, elsewhere));
        let typing = Gate { typing_ahead: true, ..READY };
        assert!(!pane_toggles(&Key::Named(NamedKey::F3), none, typing));
    }
}
