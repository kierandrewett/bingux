//! Focus ownership: how a control stays focused while an overlay it opened has
//! the keyboard.
//!
//! A context menu, a dropdown list or a touch edit bar is opened *from* a
//! control, its **owner**. While the overlay is up, keyboard events go to the
//! overlay (its own focus node), but the owner must not look unfocused: a text
//! field keeps its caret, its selection and its ring, as every platform's
//! fields do while their menu is open. And when the overlay closes the owner
//! is the **focus return target**: it has real focus again, with the caret
//! where it was.
//!
//! There is one source of truth for "is this control focused" as it is drawn:
//! [`effective_focus`], fed by [`use_visual_focus`]. The ring, the caret, the
//! selection colour, the placeholder and the accessibility state of a control
//! all read that one value; no control keeps a second "menu is open" flag.
//!
//! The rules here are pure functions, so they are tested without a window.

use freya::prelude::*;

use crate::shell::overlays::Overlays;

/// Where an overlay stands relative to its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayPhase {
    /// The owner has no overlay.
    Closed,
    /// The overlay is up.
    Open,
    /// The overlay is animating out. Its focus is on the way back to the owner.
    Closing,
}

/// Whether the owner still counts as focused (the *focus-within owner*).
///
/// While the overlay is open it does. While it closes it does as long as the
/// overlay still holds the keyboard, because that focus is about to return to
/// the owner. Once focus has moved on (Tab, a click in another field, an item
/// that opened a dialog) the owner is simply not focused, with no flicker of
/// two focused controls during the exit animation.
pub fn keeps_focus(phase: OverlayPhase, overlay_holds_focus: bool) -> bool {
    match phase {
        OverlayPhase::Closed => false,
        OverlayPhase::Open => true,
        OverlayPhase::Closing => overlay_holds_focus,
    }
}

/// How a control is drawn: its real focus, or as the owner of an open overlay.
///
/// `keyboard_mode` is the window's navigation mode, which decides whether an
/// owner shows the keyboard ring or the pointer state: a menu opened with a
/// right click leaves the mode alone, one opened with a key (or navigated with
/// keys) is keyboard navigation.
pub fn effective_focus(real: Focus, owner: bool, keyboard_mode: bool) -> Focus {
    match (real, owner) {
        (Focus::Not, true) if keyboard_mode => Focus::Keyboard,
        (Focus::Not, true) => Focus::Pointer,
        (real, _) => real,
    }
}

/// What closing an overlay does with keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restore {
    /// Focus goes back to the owner: the overlay still held it.
    ToOwner,
    /// Focus stays where it is: it moved elsewhere (an item opened a popup,
    /// Tab went on, another control was pressed), or there is no owner.
    Keep,
}

/// Where focus goes when an overlay with (or without) an owner finishes
/// closing.
pub fn restore_on_close(has_owner: bool, overlay_holds_focus: bool) -> Restore {
    if has_owner && overlay_holds_focus {
        Restore::ToOwner
    } else {
        Restore::Keep
    }
}

/// A control's focus as it is drawn, from [`use_visual_focus`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualFocus {
    /// The focus to draw and report: the real focus, or the owner's.
    pub focus: Focus,
    /// The control only looks focused because an overlay it opened holds the
    /// keyboard. Its caret is steady, and keys go to the overlay.
    pub within_owner: bool,
}

impl VisualFocus {
    /// Whether the control is focused as drawn, real or as the owner.
    pub fn is_focused(self) -> bool {
        self.focus.is_focused()
    }
}

/// The focus of the control `id` as it should be drawn: its real focus, or
/// the owner's while the menu it opened is up (or closing back to it). Call
/// from `render`; it subscribes to the focus, the overlay and the input mode.
pub fn use_visual_focus(id: AccessibilityId) -> VisualFocus {
    let real = use_focus(id);
    let overlays = try_consume_context::<Overlays>();
    let window = Platform::get().current_window();
    let keyboard_mode = *window.navigation_mode.read() == NavigationMode::Keyboard;
    let real = real();
    let owner = overlays.is_some_and(|overlays| overlays.keeps_focus(id));
    VisualFocus {
        focus: effective_focus(real, owner, keyboard_mode),
        within_owner: real == Focus::Not && owner,
    }
}

/// One modal focus scope: the dialog, popup or sheet that holds the keyboard,
/// and the control that opened it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope<Id> {
    /// The scope's container: the node focus falls back to when nothing inside
    /// it can take focus, so Escape and the arrow keys still reach the scope
    /// and never leak to the page behind.
    pub container: Id,
    /// The control that opened it, which gets focus back when the scope closes.
    pub opener: Option<Id>,
}

/// The stack of open modal scopes. Only the top scope is live; every scope
/// below it, and the page, is inert: it is not tabbable, takes no shortcuts
/// and draws no focus ring. A scope opened from inside another (a menu from a
/// popup) stacks on it, and closing it returns focus into the scope below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeStack<Id> {
    scopes: Vec<Scope<Id>>,
}

impl<Id> Default for ScopeStack<Id> {
    fn default() -> Self {
        Self { scopes: Vec::new() }
    }
}

impl<Id: Copy + PartialEq> ScopeStack<Id> {
    /// Opens a scope on top of the others.
    pub fn push(&mut self, container: Id, opener: Option<Id>) {
        self.scopes.push(Scope { container, opener });
    }

    /// Closes the scope with this `container`, wherever it is in the stack
    /// (a scope can go away out of order: its owner unmounted). Returns where
    /// focus goes: the closed scope's opener when it was the live one, and
    /// `None` when a scope below it is still live and keeps its own focus.
    pub fn remove(&mut self, container: Id) -> Option<Id> {
        let at = self.scopes.iter().position(|scope| scope.container == container)?;
        let was_top = at + 1 == self.scopes.len();
        let scope = self.scopes.remove(at);
        if was_top { scope.opener } else { None }
    }

    /// The live scope, if any scope is open.
    pub fn top(&self) -> Option<Scope<Id>> {
        self.scopes.last().copied()
    }

    /// How many scopes are open.
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Whether any scope is open, so the page behind is inert.
    pub fn is_modal(&self) -> bool {
        !self.scopes.is_empty()
    }

    /// Whether `container` is the live scope.
    pub fn is_live(&self, container: Id) -> bool {
        self.top().is_some_and(|scope| scope.container == container)
    }

    /// Where focus goes after the focused node went away: the live scope's
    /// container, never a node of the page behind. `None` with no scope open.
    pub fn fallback(&self) -> Option<Id> {
        self.top().map(|scope| scope.container)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_owner_is_focused_while_its_overlay_is_open() {
        assert!(keeps_focus(OverlayPhase::Open, true));
        // Even before the overlay has taken the keyboard.
        assert!(keeps_focus(OverlayPhase::Open, false));
    }

    #[test]
    fn a_control_with_no_overlay_is_not_an_owner() {
        assert!(!keeps_focus(OverlayPhase::Closed, true));
        assert!(!keeps_focus(OverlayPhase::Closed, false));
    }

    #[test]
    fn a_closing_overlay_keeps_the_owner_focused_only_while_it_holds_focus() {
        // Escape or a click outside: focus is about to return, no flicker.
        assert!(keeps_focus(OverlayPhase::Closing, true));
        // Tab or a click in another field moved focus on: the owner lets go
        // at once, so two controls never look focused together.
        assert!(!keeps_focus(OverlayPhase::Closing, false));
    }

    #[test]
    fn real_focus_is_never_changed_by_ownership() {
        for real in [Focus::Pointer, Focus::Keyboard] {
            for owner in [false, true] {
                for keyboard in [false, true] {
                    assert_eq!(effective_focus(real, owner, keyboard), real);
                }
            }
        }
    }

    #[test]
    fn an_owner_draws_as_focused_in_the_window_mode() {
        assert_eq!(effective_focus(Focus::Not, true, false), Focus::Pointer);
        assert_eq!(effective_focus(Focus::Not, true, true), Focus::Keyboard);
    }

    #[test]
    fn a_control_that_is_neither_focused_nor_an_owner_is_not_focused() {
        assert_eq!(effective_focus(Focus::Not, false, false), Focus::Not);
        assert_eq!(effective_focus(Focus::Not, false, true), Focus::Not);
    }

    #[test]
    fn closing_returns_focus_to_the_owner_only_if_the_overlay_held_it() {
        assert_eq!(restore_on_close(true, true), Restore::ToOwner);
        // An item opened a popup, or Tab moved on: keep the new focus.
        assert_eq!(restore_on_close(true, false), Restore::Keep);
        // A menu with no owner (a window-level menu) has nowhere to return.
        assert_eq!(restore_on_close(false, true), Restore::Keep);
        assert_eq!(restore_on_close(false, false), Restore::Keep);
    }

    #[test]
    fn visual_focus_reports_focus_as_drawn() {
        let owner = VisualFocus { focus: Focus::Pointer, within_owner: true };
        assert!(owner.is_focused());
        let plain = VisualFocus { focus: Focus::Not, within_owner: false };
        assert!(!plain.is_focused());
    }

    #[test]
    fn a_pushed_scope_is_live_and_makes_the_page_inert() {
        let mut stack = ScopeStack::<u32>::default();
        assert!(!stack.is_modal());
        assert_eq!(stack.fallback(), None);
        stack.push(10, Some(1));
        assert!(stack.is_modal());
        assert!(stack.is_live(10));
        assert_eq!(stack.fallback(), Some(10));
    }

    #[test]
    fn closing_the_live_scope_returns_focus_to_its_opener() {
        let mut stack = ScopeStack::<u32>::default();
        stack.push(10, Some(1));
        assert_eq!(stack.remove(10), Some(1));
        assert!(!stack.is_modal());
    }

    #[test]
    fn a_nested_scope_returns_into_the_scope_below() {
        let mut stack = ScopeStack::<u32>::default();
        stack.push(10, Some(1)); // a popup opened from the page
        stack.push(20, Some(11)); // a menu opened from a control inside it
        assert_eq!(stack.depth(), 2);
        assert!(stack.is_live(20));
        assert!(!stack.is_live(10), "the popup is inert under its menu");
        assert_eq!(stack.fallback(), Some(20));
        // Closing the menu returns into the popup, not to the page.
        assert_eq!(stack.remove(20), Some(11));
        assert!(stack.is_live(10));
        assert_eq!(stack.remove(10), Some(1));
    }

    #[test]
    fn a_scope_removed_out_of_order_leaves_the_live_scope_alone() {
        let mut stack = ScopeStack::<u32>::default();
        stack.push(10, Some(1));
        stack.push(20, Some(11));
        // The popup's owner unmounts while its menu is still up: the menu
        // keeps the keyboard, and nothing steals focus back to the page.
        assert_eq!(stack.remove(10), None);
        assert!(stack.is_live(20));
        assert_eq!(stack.depth(), 1);
        // An unknown scope is ignored.
        assert_eq!(stack.remove(99), None);
    }
}
