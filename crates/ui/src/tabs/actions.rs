//! What a person does with the tabs of a window, put on screen: open, select,
//! close, duplicate, reopen, reorder, tear off, and the keys and menu items
//! that say the same.
//!
//! [`Tabs`] is the handle every handler of a window takes: the window's
//! [`TabSet`] and what closing the last tab needs. It is `Copy`, so each
//! closure gets its own. The rules (where a new tab opens, who is selected
//! after a close) are the set's and are tested there; this file only decides
//! what the *window* does about them: closing the last tab closes the window,
//! and asks first when the application says closing would lose something.
//!
//! Files is the first user (`apps/files`); an editor with a tab per document is
//! the second. It brings its own [`TabContent`](super::TabContent) and reuses
//! everything here.

use freya::prelude::*;

use super::set::{Closed, Tab, TabContent, TabSet};
use super::tearoff;
use super::windows::{self, TabWindow};
use crate::controls::{Detach, TabItem, TabStrip, tab_strip};
use crate::shell::{CANCEL_ID, DialogIcon, MenuItem, Overlays, ResponseKind, dialog, menu_item};
use crate::window::{Window, WindowLike};

/// Why closing the window now would lose something: an application's answer
/// to [`Tabs::guard`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseWarning {
    /// The dialog's question, such as "Close Files?".
    pub title: String,
    /// What would be lost.
    pub message: String,
}

/// The tabs of a window. Every field is `Copy`, so every handler takes its
/// own.
pub struct Tabs<P: TabContent> {
    set: State<TabSet<P>>,
    window: Window,
    overlays: Overlays,
    guard: fn() -> Option<CloseWarning>,
}

impl<P: TabContent> Clone for Tabs<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P: TabContent> Copy for Tabs<P> {}

fn nothing_to_lose() -> Option<CloseWarning> {
    None
}

impl<P: TabContent> Tabs<P> {
    /// The tabs of `set` in `window`, which the dialogs of `overlays` ask
    /// through.
    pub fn new(set: State<TabSet<P>>, window: Window, overlays: Overlays) -> Self {
        Self {
            set,
            window,
            overlays,
            guard: nothing_to_lose,
        }
    }

    /// Ask `guard` before the window closes: a warning it returns is put to
    /// the person, with *Keep open* as the default answer.
    pub fn guard(mut self, guard: fn() -> Option<CloseWarning>) -> Self {
        self.guard = guard;
        self
    }

    /// The tabs.
    pub fn set(self) -> State<TabSet<P>> {
        self.set
    }

    /// Open `content` in a new tab beside the one on show and go to it.
    /// Returns the new tab's id.
    pub fn open(mut self, content: P) -> u64 {
        self.set.write().open(content)
    }

    pub fn select(mut self, id: u64) {
        let known = {
            let set = self.set.peek();
            set.active() != id && set.ids().contains(&id)
        };
        if known {
            self.set.write().select(id);
        }
    }

    /// Close a tab. The tab that takes the selection is the neighbour to its
    /// right, else the one to its left; the last tab closes the window.
    pub fn close(mut self, id: u64) {
        let closed = {
            let set = self.set.peek();
            set.ids().contains(&id).then_some(set.len() == 1)
        };
        match closed {
            Some(true) => self.close_window(),
            Some(false) => {
                if self.set.write().close(id) == Closed::Window {
                    self.close_window();
                }
            }
            None => {}
        }
    }

    /// Close the tab on show.
    pub fn close_active(self) {
        let active = self.set.peek().active();
        self.close(active);
    }

    /// Close the window, after asking if the application's guard says closing
    /// would lose something.
    pub fn close_window(self) {
        let window = self.window;
        match (self.guard)() {
            Some(warning) => self.overlays.show_dialog(
                dialog(warning.title, warning.message)
                    .cancel("Keep open")
                    .default_action(CANCEL_ID)
                    .icon(DialogIcon::Warning)
                    .response("Close window", ResponseKind::Destructive, move |_: ()| {
                        window.close()
                    }),
            ),
            None => window.close(),
        }
    }

    /// Close the tabs of a menu's *Close other tabs* or *Close tabs to the
    /// right*.
    pub fn close_many(mut self, ids: &[u64]) {
        self.set.write().close_many(ids);
    }

    /// Open a copy of a tab beside it.
    pub fn duplicate(mut self, id: u64) {
        self.set.write().duplicate(id);
    }

    /// Bring back the tab closed last.
    pub fn reopen(mut self) {
        if self.set.peek().can_reopen() {
            self.set.write().reopen();
        }
    }

    /// Go `by` tabs along, wrapping at either end.
    pub fn step(mut self, by: isize) {
        self.set.write().step(by);
    }

    /// Go to the tab at `index` from the start, or the last for `usize::MAX`.
    pub fn nth(mut self, index: usize) {
        self.set.write().nth(index);
    }

    /// A tab was dragged (or moved by its menu) to place `to` in the list.
    pub fn reorder(mut self, id: u64, to: usize) {
        self.set.write().reorder(id, to);
    }

    /// Change what tab `id` shows, when `differs` says it is not what it
    /// should be already: a tab that went to another folder, a document that
    /// was renamed. An unchanged tab is not written, so it does not redraw.
    pub fn update_if(
        mut self,
        id: u64,
        differs: impl FnOnce(&P) -> bool,
        edit: impl FnOnce(&mut P),
    ) {
        let changed = self.set.peek().content(id).is_some_and(differs);
        if changed {
            self.set.write().update(id, edit);
        }
    }

    /// A tab was torn off the strip (see [`super::tearoff`]). Let go beyond
    /// the strip, it opens a window of its own and this window keeps the
    /// rest. Still held, this window is what follows the pointer, so it keeps
    /// only that tab and the others go to a window of their own.
    pub fn tear(mut self, detach: Detach) {
        let Some(entry) = windows::find(Platform::window_id()) else {
            return;
        };
        let size = entry.size.get();
        if detach.released {
            let moved = self.set.write().detach(detach.id);
            if let Some(moved) = moved {
                windows::open(vec![moved], 0, size);
            }
            return;
        }
        let others = self.set.write().detach_others(detach.id);
        if let Some((others, on_show)) = others {
            windows::open(others, on_show, size);
        }
        tearoff::begin(&entry, detach.pointer, detach.press);
    }

    /// The tab strip of the window, wired to every action above. `items`
    /// are the tabs as the strip draws them, `window` is what
    /// [`use_tab_window`](super::use_tab_window) returned and `new_tab` opens a
    /// tab from the strip's New tab button. The caller may add what is its own
    /// (`on_hover` and `drop_target` for something dragged onto a tab).
    pub fn strip(
        self,
        items: Vec<TabItem>,
        window: &TabWindow,
        new_tab: impl Into<EventHandler<()>>,
    ) -> TabStrip {
        let (current, can_reopen) = {
            let set = self.set.read();
            (set.active(), set.can_reopen())
        };
        tab_strip(items, current)
            .on_select(move |id: u64| self.select(id))
            .on_detach(move |detach: Detach| self.tear(detach))
            .probe(window.probe())
            .insertion(window.insertion())
            .on_close(move |id: u64| self.close(id))
            .on_close_many(move |ids: Vec<u64>| self.close_many(&ids))
            .on_duplicate(move |id: u64| self.duplicate(id))
            .on_reopen(can_reopen, move |_: ()| self.reopen())
            .on_new(new_tab)
            .on_reorder(move |(id, to): (u64, usize)| self.reorder(id, to))
    }

    /// The window's tab keys. `new_tab` opens a tab for Ctrl+T.
    ///
    /// | Key | Does |
    /// | --- | ---- |
    /// | Ctrl+T | a new tab |
    /// | Ctrl+Shift+T | reopen the tab closed last |
    /// | Ctrl+W | close the tab |
    /// | Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+Page Down, Ctrl+Page Up | the next or previous tab |
    /// | Alt+1 to Alt+9 | the tab at that place (9 is the last) |
    ///
    /// The handler takes the key it uses (`stop_propagation`) and leaves the
    /// rest to the view.
    pub fn shortcuts(
        self,
        new_tab: impl Into<EventHandler<()>>,
    ) -> impl Fn(Event<KeyboardEventData>) {
        let new_tab = new_tab.into();
        move |event: Event<KeyboardEventData>| {
            let ctrl = event.modifiers.contains(Modifiers::CONTROL);
            let alt = event.modifiers.contains(Modifiers::ALT);
            let shift = event.modifiers.contains(Modifiers::SHIFT);
            match &event.key {
                Key::Character(text)
                    if ctrl && !alt && !shift && text.eq_ignore_ascii_case("t") =>
                {
                    event.stop_propagation();
                    new_tab.call(());
                }
                Key::Character(text) if ctrl && !alt && shift && text.eq_ignore_ascii_case("t") => {
                    event.stop_propagation();
                    self.reopen();
                }
                Key::Character(text)
                    if ctrl && !alt && !shift && text.eq_ignore_ascii_case("w") =>
                {
                    event.stop_propagation();
                    self.close_active();
                }
                Key::Named(NamedKey::Tab) if ctrl && !alt => {
                    event.stop_propagation();
                    self.step(if shift { -1 } else { 1 });
                }
                Key::Named(NamedKey::PageDown) if ctrl && !alt => {
                    event.stop_propagation();
                    self.step(1);
                }
                Key::Named(NamedKey::PageUp) if ctrl && !alt => {
                    event.stop_propagation();
                    self.step(-1);
                }
                Key::Character(text) if alt && !ctrl && !shift => {
                    if let Some(digit) = text.chars().next().and_then(|c| c.to_digit(10))
                        && digit >= 1
                    {
                        event.stop_propagation();
                        self.nth(if digit == 9 {
                            usize::MAX
                        } else {
                            digit as usize - 1
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// The tab entries of the window's main menu: New tab, Close tab and
    /// Reopen closed tab (greyed when there is none), each with its key. The
    /// application adds its own entries around them.
    pub fn menu_items(self, new_tab: impl Into<EventHandler<()>>) -> Vec<MenuItem> {
        let new_tab = new_tab.into();
        let can_reopen = self.set.read().can_reopen();
        let reopen =
            menu_item("Reopen closed tab", move |_: ()| self.reopen()).shortcut("Ctrl+Shift+T");
        vec![
            menu_item("New tab", move |_: ()| new_tab.call(())).shortcut("Ctrl+T"),
            menu_item("Close tab", move |_: ()| self.close_active()).shortcut("Ctrl+W"),
            if can_reopen {
                reopen
            } else {
                reopen.disabled()
            },
        ]
    }
}

/// The tab strip's items for `tabs`: one per tab, with the title the host
/// made for it (`titles`, in the same order) and the icon `icon` says.
pub fn tab_items<P: TabContent>(
    tabs: &[Tab<P>],
    titles: Vec<String>,
    icon: impl Fn(&P) -> crate::icons::Icon,
) -> Vec<TabItem> {
    tabs.iter()
        .zip(titles)
        .map(|(tab, title)| TabItem::new(tab.id, title).icon(icon(&tab.content)))
        .collect()
}
