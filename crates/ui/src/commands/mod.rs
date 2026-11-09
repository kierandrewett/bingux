//! Commands: one registry of what an application can do, and everything that
//! is built from it (SwiftUI `Commands` and `focusedValue`, the AppKit
//! responder chain, GTK actions).
//!
//! A [`Command`] is an id, a title, an optional icon, its keyboard
//! shortcuts, whether it is enabled or checked, the [`CommandGroup`] and
//! section it belongs to, and what it does. An application declares its
//! commands where their state lives and gets, with no more code:
//!
//! - **Keyboard shortcuts**, through one router per window. A shortcut runs
//!   its command; a disabled command takes its key and does nothing.
//! - **The main menu**: [`app_menu`](crate::about::app_menu) in the header
//!   lists the commands by group (File, Edit, View...), with their shortcuts,
//!   ticks and disabled states, then the application's own items and About.
//! - **Context menus**: [`Commands::context_menu`] builds one from command
//!   ids, leaving unavailable commands out and shortcuts off, as the HIG asks.
//! - **The keyboard shortcuts sheet** (Ctrl+? or Ctrl+/), listing every
//!   shortcut the window answers, by group, with a filter.
//! - **The command palette** (Ctrl+Shift+P): every enabled command, found by
//!   fuzzy search, the ones run last first, each with its shortcut.
//!
//! # Where commands come from
//!
//! Commands are contributed by **scopes**, like a responder chain:
//!
//! - [`use_window_commands`] adds commands for the whole window (New window,
//!   Find, Preferences). They are always available.
//! - [`command_scope`] wraps a view (a file list, an editor) and adds
//!   commands that apply while that view has the keyboard focus (Rename,
//!   Select all). Scopes nest. The innermost scope that holds the focus wins:
//!   its Ctrl+A shadows the window's, and a command id it shares with an
//!   outer scope replaces that command.
//!
//! The router learns which scopes hold the focus from the key itself: a key
//! press travels from the focused control up through every scope it is in.
//! A shortcut of a view therefore runs only while the focus is inside that
//! view. Menus and the palette list the commands of the view that last had
//! the focus (by keyboard or by a click inside it), so opening the header
//! menu with the pointer does not lose them.
//!
//! A control that handles a key itself (a text field typing, a list moving
//! its selection) stops the key, and no command sees it: a text field keeps
//! its own Ctrl+Z, Ctrl+C and Ctrl+A.
//!
//! While a dialog or a menu is open the router stands aside: they hold the
//! keyboard.
//!
//! In a debug build, two commands of one scope (or two window scopes) that
//! claim the same shortcut are reported on stderr, once, as is a window
//! command with a bare key (a letter with no Ctrl or Alt), which must belong
//! to a view (WCAG 2.1.4).

mod fuzzy;
mod palette;
mod sheet;
mod shortcut;
#[cfg(all(test, feature = "testing"))]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;

use freya::prelude::*;

pub use palette::{CommandPalette, open_palette};
pub use sheet::{KeyCaps, ShortcutsSheet, key_caps, open_shortcuts};
pub use shortcut::{IntoShortcut, ParseError, Shortcut};

use crate::component::ComponentName;
use crate::icons::Icon;
use crate::shell::menu::{MenuItem, menu_item, menu_separator, submenu};
use crate::shell::overlays::Overlays;

/// Where a command sits in the main menu and in the shortcuts sheet: the
/// menus of a menu bar, in the order a menu bar has them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandGroup {
    /// The application itself: New window, Preferences, Quit. Listed at the
    /// top level of the main menu, not in a submenu.
    App,
    File,
    Edit,
    View,
    /// Moving around: Back, Forward, Go to folder, Next tab.
    Go,
    Window,
    /// A menu of the application's own, after Window and before Help.
    Other(&'static str),
    Help,
}

impl CommandGroup {
    /// The group's name in the language shown, as a submenu and a heading
    /// in the shortcuts sheet. An `Other` group's name is the app's own text
    /// (pass it a [`tr!`](crate::tr) result leaked once, or a fixed name).
    pub fn title(self) -> String {
        match self {
            CommandGroup::App => crate::tr!("ui-command-group-app"),
            CommandGroup::File => crate::tr!("ui-command-group-file"),
            CommandGroup::Edit => crate::tr!("ui-command-group-edit"),
            CommandGroup::View => crate::tr!("ui-command-group-view"),
            CommandGroup::Go => crate::tr!("ui-command-group-go"),
            CommandGroup::Window => crate::tr!("ui-command-group-window"),
            CommandGroup::Other(name) => name.to_owned(),
            CommandGroup::Help => crate::tr!("ui-command-group-help"),
        }
    }

    /// The order of the groups: a menu bar's.
    fn rank(self) -> u8 {
        match self {
            CommandGroup::App => 0,
            CommandGroup::File => 1,
            CommandGroup::Edit => 2,
            CommandGroup::View => 3,
            CommandGroup::Go => 4,
            CommandGroup::Window => 5,
            CommandGroup::Other(_) => 6,
            CommandGroup::Help => 7,
        }
    }
}

/// One thing the application can do. Build with [`Command::new`].
#[derive(Clone, PartialEq)]
pub struct Command {
    id: String,
    title: String,
    icon: Option<Icon>,
    shortcuts: Vec<Shortcut>,
    /// What the shortcuts sheet shows for a key a control handles itself.
    keys_text: Option<String>,
    enabled: bool,
    checked: Option<bool>,
    destructive: bool,
    group: CommandGroup,
    section: Option<String>,
    keywords: Vec<String>,
    in_menu: bool,
    in_palette: bool,
    /// The router runs it from its shortcut. Off for a view whose own key
    /// handler answers the keys.
    routed: bool,
    on_run: Option<EventHandler<()>>,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("shortcuts", &self.shortcuts)
            .field("enabled", &self.enabled)
            .field("group", &self.group)
            .finish_non_exhaustive()
    }
}

impl Command {
    /// A command `id` (stable, such as `"file.rename"`) titled `title` (as a
    /// menu shows it: "Rename…", "Show hidden files"), that runs `on_run`.
    /// It starts enabled, in [`CommandGroup::App`], in the menu and the
    /// palette, with no shortcut.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        on_run: impl FnMut(()) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon: None,
            shortcuts: Vec::new(),
            keys_text: None,
            enabled: true,
            checked: None,
            destructive: false,
            group: CommandGroup::App,
            section: None,
            keywords: Vec::new(),
            in_menu: true,
            in_palette: true,
            routed: true,
            on_run: Some(EventHandler::new(on_run)),
        }
    }

    /// A key that a control handles itself, listed in the shortcuts sheet
    /// only: `keys` is how it is written ("Alt+1 to 9", "Space"). It is never
    /// routed and has no menu or palette entry.
    pub fn key_hint(
        id: impl Into<String>,
        title: impl Into<String>,
        keys: impl Into<String>,
    ) -> Self {
        Self {
            keys_text: Some(keys.into()),
            in_menu: false,
            in_palette: false,
            on_run: None,
            ..Self::new(id, title, |_| {})
        }
    }

    /// Adds a keyboard shortcut, written as people read it (`"Ctrl+Shift+P"`,
    /// `"F2"`, `"Ctrl+?"`) or as a [`Shortcut`]. A command may have several;
    /// menus show the first. Text that is not a shortcut is a programming
    /// error: a debug build panics, a release build leaves the shortcut out.
    pub fn shortcut(mut self, shortcut: impl IntoShortcut) -> Self {
        match shortcut.into_shortcut() {
            Ok(shortcut) => self.shortcuts.push(shortcut),
            Err(error) => {
                debug_assert!(false, "command {}: {error}", self.id);
                eprintln!("[commands] {}: {error}", self.id);
            }
        }
        self
    }

    /// An icon, shown in the palette.
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Whether the command can run now. A disabled command is dimmed in
    /// menus, left out of context menus and the palette, and its shortcut
    /// does nothing.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// A state the command toggles ("Show hidden files"): a tick in menus.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// The command destroys something: its menu item is coloured as such.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// The menu the command belongs to.
    pub fn group(mut self, group: CommandGroup) -> Self {
        self.group = group;
        self
    }

    /// A section inside the group: commands of one section sit together,
    /// with a separator between sections, in the order sections first appear.
    pub fn section(mut self, section: impl Into<String>) -> Self {
        self.section = Some(section.into());
        self
    }

    /// Other words the palette finds the command by ("hidden" for "Show
    /// hidden files" finds it by "dotfiles" too).
    pub fn keywords<S: Into<String>>(mut self, words: impl IntoIterator<Item = S>) -> Self {
        self.keywords.extend(words.into_iter().map(Into::into));
        self
    }

    /// Leaves the command out of the main menu. It still answers its
    /// shortcut and is in the palette.
    pub fn hidden_from_menu(mut self) -> Self {
        self.in_menu = false;
        self
    }

    /// Leaves the command out of the palette.
    pub fn hidden_from_palette(mut self) -> Self {
        self.in_palette = false;
        self
    }

    /// The view this command belongs to answers its shortcut in its own key
    /// handler, which runs before the router: the router shows the shortcut
    /// (menus, sheet, palette) and takes the key, but never runs the
    /// command from it, so it never runs twice. For a view that had keys
    /// before it had commands (a file list, a text editor).
    pub fn keys_handled_by_view(mut self) -> Self {
        self.routed = false;
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn shortcuts(&self) -> &[Shortcut] {
        &self.shortcuts
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_checked(&self) -> Option<bool> {
        self.checked
    }

    pub fn group_of(&self) -> CommandGroup {
        self.group
    }

    fn runs(&self) -> bool {
        self.on_run.is_some()
    }
}

/// A command as the window sees it now: the shortcuts it can still be
/// reached by (an inner scope may have taken some) and the scope it is from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Resolved {
    pub command: Command,
    pub reachable: Vec<Shortcut>,
    pub from_view: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeKind {
    Window,
    View,
}

struct ScopeEntry {
    key: u64,
    kind: ScopeKind,
    name: String,
    commands: Vec<Command>,
}

/// Commands the window host adds at query time: About, the sheet, the
/// palette, Undo and Redo.
type Extras = Rc<dyn Fn() -> Vec<Command>>;

#[derive(Default)]
struct Registry {
    scopes: Vec<ScopeEntry>,
    next_key: u64,
    /// Scopes a key press passed through, innermost first, until the
    /// router reads them.
    pending_key: Vec<u64>,
    /// The same for a pointer press.
    pending_pointer: Vec<u64>,
    /// The scopes that held the focus last, innermost first.
    focus: Vec<u64>,
    /// Command ids, the one run last first.
    recent: Vec<String>,
    /// Conflicts already reported, so each is reported once.
    reported: Vec<String>,
    /// The overlay hosts of the window, outermost (or oldest) first. The
    /// first routes keys and owns the palette, the sheet and the extras.
    hosts: Vec<Host>,
}

struct Host {
    id: u64,
    overlays: Overlays,
    extras: Extras,
}

/// How many commands the palette remembers as run last.
const RECENT: usize = 8;

impl Registry {
    /// The scopes that apply to `chain` in precedence order: the chain's view
    /// scopes innermost first, then window scopes newest first.
    fn ordered(&self, chain: &[u64]) -> Vec<&ScopeEntry> {
        let mut ordered: Vec<&ScopeEntry> = chain
            .iter()
            .filter_map(|key| {
                self.scopes
                    .iter()
                    .find(|scope| scope.key == *key && scope.kind == ScopeKind::View)
            })
            .collect();
        ordered.extend(
            self.scopes
                .iter()
                .rev()
                .filter(|scope| scope.kind == ScopeKind::Window),
        );
        ordered
    }

    fn resolve(&self, chain: &[u64]) -> Vec<Resolved> {
        let extras = self
            .hosts
            .first()
            .map(|host| (host.extras)())
            .unwrap_or_default();
        let mut sources: Vec<(bool, &[Command])> = self
            .ordered(chain)
            .into_iter()
            .map(|scope| (scope.kind == ScopeKind::View, scope.commands.as_slice()))
            .collect();
        sources.push((false, extras.as_slice()));
        resolve(&sources)
    }

    /// The last focus chain, without scopes that are gone.
    fn focus_chain(&self) -> Vec<u64> {
        self.focus
            .iter()
            .copied()
            .filter(|key| self.scopes.iter().any(|scope| scope.key == *key))
            .collect()
    }

    fn note_run(&mut self, id: &str) {
        self.recent.retain(|other| other != id);
        self.recent.insert(0, id.to_owned());
        self.recent.truncate(RECENT);
    }

    #[cfg(debug_assertions)]
    fn report(&mut self, message: String) {
        if !self.reported.contains(&message) {
            eprintln!("[commands] {message}");
            self.reported.push(message);
        }
    }
}

/// Merge command lists in precedence order: the first command with an id
/// wins, and the first command with a shortcut takes it, so a later command
/// keeps only the shortcuts nobody before it claimed.
pub(crate) fn resolve(sources: &[(bool, &[Command])]) -> Vec<Resolved> {
    let mut out: Vec<Resolved> = Vec::new();
    let mut taken: Vec<Shortcut> = Vec::new();
    for (from_view, commands) in sources {
        for command in *commands {
            if out.iter().any(|resolved| resolved.command.id == command.id) {
                continue;
            }
            let reachable: Vec<Shortcut> = command
                .shortcuts
                .iter()
                .filter(|shortcut| command.runs() && !taken.contains(shortcut))
                .cloned()
                .collect();
            taken.extend(reachable.iter().cloned());
            out.push(Resolved {
                command: command.clone(),
                reachable,
                from_view: *from_view,
            });
        }
    }
    out
}

/// Two commands of one list that claim one shortcut: `(shortcut, first id,
/// second id)`. Shadowing across scopes is the design; inside one list it is
/// a mistake.
pub(crate) fn conflicts(commands: &[Command]) -> Vec<(Shortcut, String, String)> {
    let mut found = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        for shortcut in &command.shortcuts {
            if let Some(other) = commands[index + 1..]
                .iter()
                .find(|other| other.id != command.id && other.shortcuts.contains(shortcut))
            {
                found.push((shortcut.clone(), command.id.clone(), other.id.clone()));
            }
        }
    }
    found
}

/// The window's command registry. A `Copy` handle: read it with
/// [`use_commands`], keep it in closures.
#[derive(Clone, Copy, PartialEq)]
pub struct Commands {
    registry: State<Rc<RefCell<Registry>>>,
}

impl Commands {
    fn detached() -> Self {
        Self {
            registry: State::create(Rc::new(RefCell::new(Registry::default()))),
        }
    }

    fn with<R>(self, f: impl FnOnce(&mut Registry) -> R) -> Option<R> {
        let registry = self.registry.try_peek()?.clone();
        let mut registry = registry.borrow_mut();
        Some(f(&mut registry))
    }

    fn add_scope(self, kind: ScopeKind) -> u64 {
        self.with(|registry| {
            registry.next_key += 1;
            let key = registry.next_key;
            registry.scopes.push(ScopeEntry {
                key,
                kind,
                name: String::new(),
                commands: Vec::new(),
            });
            key
        })
        .unwrap_or_default()
    }

    fn remove_scope(self, key: u64) {
        self.with(|registry| {
            registry.scopes.retain(|scope| scope.key != key);
            registry.focus.retain(|other| *other != key);
        });
    }

    fn set_scope(self, key: u64, name: String, commands: Vec<Command>) {
        self.with(|registry| {
            #[cfg(debug_assertions)]
            {
                for (shortcut, first, second) in conflicts(&commands) {
                    registry.report(format!("{shortcut} is the shortcut of both {first} and {second}"));
                }
                let window = registry
                    .scopes
                    .iter()
                    .any(|scope| scope.key == key && scope.kind == ScopeKind::Window);
                if window {
                    for command in &commands {
                        if let Some(shortcut) = command.shortcuts.iter().find(|shortcut| shortcut.is_bare()) {
                            registry.report(format!(
                                "{} has the bare key {shortcut} as a window shortcut; give it to a view scope (WCAG 2.1.4)",
                                command.id
                            ));
                        }
                    }
                }
            }
            if let Some(scope) = registry.scopes.iter_mut().find(|scope| scope.key == key) {
                scope.name = name;
                scope.commands = commands;
            }
        });
    }

    /// Every command the window has now, in precedence order: the focused
    /// view's first, then the window's, then the standard ones.
    pub fn all(self) -> Vec<Command> {
        self.resolved()
            .into_iter()
            .map(|resolved| resolved.command)
            .collect()
    }

    pub(crate) fn resolved(self) -> Vec<Resolved> {
        self.with(|registry| {
            let chain = registry.focus_chain();
            registry.resolve(&chain)
        })
        .unwrap_or_default()
    }

    /// The command `id`, as the window has it now.
    pub fn get(self, id: &str) -> Option<Command> {
        self.resolved()
            .into_iter()
            .find(|resolved| resolved.command.id == id)
            .map(|resolved| resolved.command)
    }

    /// Run the command `id` if the window has it and it is enabled. Returns
    /// whether it ran.
    pub fn run(self, id: &str) -> bool {
        match self.get(id) {
            Some(command) => self.invoke(&command),
            None => false,
        }
    }

    /// Run `command` if it is enabled, and remember it as run last.
    pub(crate) fn invoke(self, command: &Command) -> bool {
        let Some(handler) = command.on_run.clone().filter(|_| command.enabled) else {
            return false;
        };
        self.with(|registry| registry.note_run(&command.id));
        handler.call(());
        true
    }

    /// The ids of the commands run last, the newest first.
    pub fn recent(self) -> Vec<String> {
        self.with(|registry| registry.recent.clone())
            .unwrap_or_default()
    }

    /// Open the command palette.
    pub fn show_palette(self) {
        if let Some(overlays) = self.overlays() {
            open_palette(overlays, self);
        }
    }

    /// Open the keyboard shortcuts sheet.
    pub fn show_shortcuts(self) {
        if let Some(overlays) = self.overlays() {
            open_shortcuts(overlays, self);
        }
    }

    /// A menu item for `command`: its title, shortcut, tick and state.
    fn item(self, resolved: &Resolved, shortcut: bool) -> MenuItem {
        let command = resolved.command.clone();
        let mut item = menu_item(command.title.clone(), move |_: ()| {
            self.invoke(&command);
        });
        let command = &resolved.command;
        if let Some(checked) = command.checked {
            item = item.checked(checked);
        }
        if !command.enabled {
            item = item.disabled();
        }
        if command.destructive {
            item = item.destructive();
        }
        if shortcut && let Some(first) = resolved.reachable.first() {
            item = item.shortcut(first.label());
        }
        item
    }

    /// The commands of `group` as menu rows, in sections.
    fn group_items(self, resolved: &[&Resolved]) -> Vec<MenuItem> {
        let mut sections: Vec<Option<&str>> = Vec::new();
        for entry in resolved {
            let section = entry.command.section.as_deref();
            if !sections.contains(&section) {
                sections.push(section);
            }
        }
        let mut items = Vec::new();
        for section in sections {
            if !items.is_empty() {
                items.push(menu_separator());
            }
            items.extend(
                resolved
                    .iter()
                    .filter(|entry| entry.command.section.as_deref() == section)
                    .map(|entry| self.item(entry, true)),
            );
        }
        items
    }

    /// The main menu built from the commands: one submenu per group (File,
    /// Edit, View...) in menu-bar order, or the group's items alone when
    /// there is one group, then the [`CommandGroup::App`] commands at the top
    /// level. [`app_menu`](crate::about::app_menu) appends About.
    pub fn main_menu_items(self) -> Vec<MenuItem> {
        let resolved = self.resolved();
        let in_menu: Vec<&Resolved> = resolved
            .iter()
            .filter(|entry| entry.command.in_menu && entry.command.runs())
            .collect();
        let mut groups: Vec<CommandGroup> = Vec::new();
        for entry in &in_menu {
            let group = entry.command.group;
            if group != CommandGroup::App && !groups.contains(&group) {
                groups.push(group);
            }
        }
        // Stable: two `Other` groups keep the order they first appeared in.
        groups.sort_by_key(|group| group.rank());
        let mut items = Vec::new();
        let of = |group: CommandGroup| -> Vec<&Resolved> {
            in_menu
                .iter()
                .copied()
                .filter(|entry| entry.command.group == group)
                .collect()
        };
        if groups.len() == 1 {
            items.extend(self.group_items(&of(groups[0])));
        } else {
            for group in &groups {
                items.push(submenu(group.title(), self.group_items(&of(*group))));
            }
        }
        let app = self.group_items(&of(CommandGroup::App));
        if !app.is_empty() {
            if !items.is_empty() {
                items.push(menu_separator());
            }
            items.extend(app);
        }
        items
    }

    /// A context menu of commands: `sections` of command ids, separated by
    /// lines. A command the window does not have, or that is disabled, is
    /// left out, and a section left empty takes no separator. Context menus
    /// show no shortcuts.
    pub fn context_menu(self, sections: &[&[&str]]) -> Vec<MenuItem> {
        let resolved = self.resolved();
        let mut items = Vec::new();
        for section in sections {
            let rows: Vec<MenuItem> = section
                .iter()
                .filter_map(|id| resolved.iter().find(|entry| entry.command.id == *id))
                .filter(|entry| entry.command.enabled && entry.command.runs())
                .map(|entry| self.item(entry, false))
                .collect();
            if rows.is_empty() {
                continue;
            }
            if !items.is_empty() {
                items.push(menu_separator());
            }
            items.extend(rows);
        }
        items
    }

    /// The key press `event` reached the window, after passing through the
    /// scopes the router collected. Runs the command it is the shortcut of.
    /// Returns whether a command took the key.
    fn route_key(self, event: &KeyboardEventData) -> bool {
        let Some(found) = self.with(|registry| {
            let chain = std::mem::take(&mut registry.pending_key);
            if !chain.is_empty() {
                registry.focus = chain.clone();
            }
            registry.resolve(&chain).into_iter().find(|entry| {
                entry
                    .reachable
                    .iter()
                    .any(|shortcut| shortcut.matches(event))
            })
        }) else {
            return false;
        };
        let Some(entry) = found else {
            return false;
        };
        // A command its view answers itself takes the key and does nothing.
        if entry.command.routed {
            self.invoke(&entry.command);
        }
        true
    }
}

/// The window's commands. Below an [`OverlayHost`](crate::shell::OverlayHost)
/// (and anywhere in an [`Application`](crate::app::Application) window) this
/// is the window's registry; elsewhere a registry of its own, so a component
/// that declares commands still renders in a gallery or a test.
pub fn use_commands() -> Commands {
    use_hook(|| try_consume_context::<Commands>().unwrap_or_else(Commands::detached))
}

/// Provides the window's registry to the tree below, or returns the one an
/// outer host already provides.
pub(crate) fn provide_commands() -> Commands {
    use_hook(|| {
        try_consume_context::<Commands>().unwrap_or_else(|| {
            let commands = Commands::detached();
            provide_context(commands);
            commands
        })
    })
}

/// Commands for the whole window, available wherever the focus is. Call it
/// on every render with the commands as they are now; they go when the
/// component does.
///
/// ```ignore
/// use_window_commands(vec![
///     Command::new("file.new-window", "New window", move |_| open_window())
///         .shortcut("Ctrl+N")
///         .group(CommandGroup::File),
///     Command::new("view.hidden", "Show hidden files", move |_| hidden.toggle())
///         .shortcut("Ctrl+H")
///         .checked(hidden())
///         .group(CommandGroup::View),
/// ]);
/// ```
pub fn use_window_commands(commands: Vec<Command>) {
    let registry = use_commands();
    let key = use_hook(move || registry.add_scope(ScopeKind::Window));
    use_drop(move || registry.remove_scope(key));
    registry.set_scope(key, String::new(), commands);
}

/// A view that contributes commands while it has the keyboard focus. See
/// [`command_scope`].
#[derive(PartialEq)]
pub struct CommandScope {
    name: String,
    commands: Vec<Command>,
    child: Element,
    fill: bool,
}

/// Wrap `child`, a view such as a file list or an editor, so its `commands`
/// apply while the focus is inside it, ahead of the window's and of any
/// scope around it. `name` says what the view is ("File list"), for the
/// inspector.
pub fn command_scope(
    name: impl Into<String>,
    commands: Vec<Command>,
    child: impl Into<Element>,
) -> CommandScope {
    CommandScope {
        name: name.into(),
        commands,
        child: child.into(),
        fill: false,
    }
}

impl CommandScope {
    /// The scope's box fills its parent, as a view that fills the window
    /// does. Without it the box is the size of its child.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }
}

impl Component for CommandScope {
    fn render(&self) -> impl IntoElement {
        let registry = use_commands();
        let key = use_hook(move || registry.add_scope(ScopeKind::View));
        use_drop(move || registry.remove_scope(key));
        registry.set_scope(key, self.name.clone(), self.commands.clone());
        rect()
            .component("CommandScope")
            .maybe(self.fill, |scope| scope.expanded())
            .on_key_down(move |_: Event<KeyboardEventData>| {
                registry.with(|registry| {
                    if !registry.pending_key.contains(&key) {
                        registry.pending_key.push(key);
                    }
                });
            })
            .on_pointer_down(move |_: Event<PointerEventData>| {
                registry.with(|registry| {
                    if !registry.pending_pointer.contains(&key) {
                        registry.pending_pointer.push(key);
                    }
                });
            })
            .child(self.child.clone())
    }
}

/// The search words of a catalogue attribute: a comma-separated list.
fn keywords(list: String) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The standard commands every window has: the palette, the shortcuts
/// sheet and About.
fn standard(commands: Commands) -> Vec<Command> {
    let mut list = vec![
        Command::new(
            "app.command-palette",
            crate::tr!("ui-command-palette"),
            move |_| commands.show_palette(),
        )
        .shortcut("Ctrl+Shift+P")
        .icon(Icon::Search)
        .keywords(keywords(crate::tr!("ui-command-palette.keywords")))
        .section("help")
        .hidden_from_palette(),
        Command::new(
            "app.shortcuts",
            crate::tr!("ui-command-shortcuts"),
            move |_| commands.show_shortcuts(),
        )
        .shortcut("Ctrl+?")
        .shortcut("Ctrl+/")
        .icon(Icon::Keyboard)
        .keywords(keywords(crate::tr!("ui-command-shortcuts.keywords")))
        .section("help"),
    ];
    if crate::about::registered() {
        let name = crate::about::info().name;
        list.push(
            Command::new(
                "app.about",
                crate::tr!("ui-about-title", app = name),
                move |_| {
                    if let Some(overlays) = commands.overlays() {
                        crate::about::open(overlays);
                    }
                },
            )
            .icon(Icon::About)
            .keywords(keywords(crate::tr!("ui-command-about.keywords")))
            // The main menu ends with About from `about::app_menu_items`.
            .hidden_from_menu(),
        );
    }
    list
}

/// Registers an overlay host with the window's registry: its overlays, the
/// standard commands and Undo and Redo. Returns the host's id. Only the
/// first host of the window routes keys (see [`attach_router`]): a host
/// nested in another (a gallery inside a window) leaves routing to the
/// outer one, so a key never runs a command twice, and a host mounted again
/// after an error takes over when the old one goes.
pub(crate) fn use_router(
    commands: Commands,
    overlays: Overlays,
    undo: crate::undo::UndoStack,
    announcer: crate::a11y::Announcer,
) -> u64 {
    let id = use_hook(move || {
        commands
            .with(|registry| {
                registry.next_key += 1;
                let id = registry.next_key;
                registry.hosts.push(Host {
                    id,
                    overlays,
                    extras: Rc::new(move || {
                        let mut extras = undo.commands(announcer);
                        extras.extend(standard(commands));
                        extras
                    }),
                });
                id
            })
            .unwrap_or_default()
    });
    use_drop(move || {
        commands.with(|registry| registry.hosts.retain(|host| host.id != id));
    });
    id
}

impl Commands {
    fn overlays(self) -> Option<Overlays> {
        self.with(|registry| registry.hosts.first().map(|host| host.overlays))
            .flatten()
    }

    fn routes(self, host: u64) -> bool {
        self.with(|registry| registry.hosts.first().is_some_and(|first| first.id == host))
            .unwrap_or(false)
    }
}

/// The key and pointer handlers of an overlay host. Every host has them;
/// only the window's first host acts on them.
pub(crate) fn attach_router(host: Rect, commands: Commands, overlays: Overlays, id: u64) -> Rect {
    host.on_global_key_down(move |event: Event<KeyboardEventData>| {
        if !commands.routes(id) {
            return;
        }
        // A dialog or a menu holds the keyboard; the palette and the sheet
        // are dialogs too.
        if overlays.dialog_open() || overlays.menu_open() {
            commands.with(|registry| registry.pending_key.clear());
            return;
        }
        if commands.route_key(&event) {
            event.stop_propagation();
            event.prevent_default();
        }
    })
    .on_global_key_up(move |_: Event<KeyboardEventData>| {
        // A key a control stopped after a scope saw it leaves its scopes
        // behind: forget them.
        if commands.routes(id) {
            commands.with(|registry| registry.pending_key.clear());
        }
    })
    .on_global_pointer_down(move |_: Event<PointerEventData>| {
        if !commands.routes(id) {
            return;
        }
        commands.with(|registry| {
            let chain = std::mem::take(&mut registry.pending_pointer);
            if !chain.is_empty() {
                registry.focus = chain;
            }
        });
    })
    .on_global_pointer_up(move |_: Event<PointerEventData>| {
        if commands.routes(id) {
            commands.with(|registry| registry.pending_pointer.clear());
        }
    })
}
