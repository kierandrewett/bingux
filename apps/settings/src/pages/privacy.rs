//! Privacy: the portal permission rules and the default level. The rules live
//! in `portal-permissions.toml` under the Bingux config directory. They are
//! read and written through `bingux_portal::store`, which validates the file
//! and writes it atomically. A broken file is shown as an error and is never
//! overwritten; the portal denies requests until the file is fixed.

use std::path::{Path, PathBuf};

use bingux_portal::policy::{Capability, Device, Level, Rule};
use bingux_portal::store::{self, Store};
use freya::prelude::*;

use crate::icons::Icon;
use crate::system::{CommandError, background, background_then};
use crate::ui::{
    Group, Overlays, PageHead, ResponseKind, Row, Status, choice_row, dialog, icon_button,
    page_column, primary_button, row, status, switch_row, text_row, use_overlays,
};
use bingux_ui::style::*;

/// Every capability, in the order the editor lists them.
const CAPABILITIES: [Capability; 5] = [
    Capability::ScreenCast,
    Capability::RemoteDesktop,
    Capability::InputCapture,
    Capability::Screenshot,
    Capability::Access,
];

const DEVICES: [Device; 3] = [Device::Keyboard, Device::Pointer, Device::Touchscreen];

/// Rule levels: the stored value, its dropdown key and its label.
const LEVELS: [(Level, &str, &str); 4] = [
    (Level::Inherit, "inherit", "Use default"),
    (Level::Ask, "ask", "Ask"),
    (Level::Allow, "allow", "Allow"),
    (Level::Deny, "deny", "Deny"),
];

#[derive(Debug, Clone, PartialEq)]
struct Model {
    store: Store,
    path: PathBuf,
    /// False when no file exists yet, so the default level applies.
    exists: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct Ctx {
    model: State<Option<Result<Model, CommandError>>>,
    error: State<Option<String>>,
    overlays: Overlays,
}

/// Reads the store at `path`. A missing file gives an empty store. A file
/// that cannot be read or validated is an error, and the caller must not
/// write over it.
fn read_model(path: &Path) -> Result<Model, CommandError> {
    let store = store::load(path).map_err(|error| CommandError(error.to_string()))?;
    Ok(Model {
        store,
        path: path.to_path_buf(),
        exists: path.exists(),
    })
}

fn load() -> Result<Model, CommandError> {
    read_model(&store::default_path())
}

/// One edit to the rules, applied to a copy of the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Up,
    Down,
}

fn missing_rule() -> String {
    "That rule no longer exists. Try again.".to_owned()
}

/// Checks the store after an edit, so every write passes the schema.
fn checked(store: Store) -> Result<Store, String> {
    store.validate().map_err(|error| error.to_string())?;
    Ok(store)
}

/// Sets the top-level default. The store rejects `allow` here.
fn with_default(store: &Store, level: Level) -> Result<Store, String> {
    let mut next = store.clone();
    next.default = level;
    checked(next)
}

fn add_rule(store: &Store, rule: Rule) -> Result<Store, String> {
    let mut next = store.clone();
    next.rules.push(rule);
    checked(next)
}

fn replace_rule(store: &Store, index: usize, rule: Rule) -> Result<Store, String> {
    let mut next = store.clone();
    *next.rules.get_mut(index).ok_or_else(missing_rule)? = rule;
    checked(next)
}

fn delete_rule(store: &Store, index: usize) -> Result<Store, String> {
    if index >= store.rules.len() {
        return Err(missing_rule());
    }
    let mut next = store.clone();
    next.rules.remove(index);
    checked(next)
}

/// Swaps the rule at `index` with its neighbour. Order matters: the last
/// matching rule wins, so a move can change which rule decides.
fn move_rule(store: &Store, index: usize, step: Step) -> Result<Store, String> {
    let len = store.rules.len();
    if index >= len {
        return Err(missing_rule());
    }
    let target = match step {
        Step::Up => index.checked_sub(1),
        Step::Down => Some(index + 1).filter(|target| *target < len),
    }
    .ok_or_else(|| "That rule cannot move that way.".to_owned())?;
    let mut next = store.clone();
    next.rules.swap(index, target);
    checked(next)
}

/// The editable fields of one rule. Text is trimmed when the rule is built.
#[derive(Debug, Clone, PartialEq)]
struct Draft {
    name: String,
    app_id: String,
    exe: String,
    pattern: String,
    monitors: String,
    capabilities: Vec<Capability>,
    level: Level,
    devices: Vec<Device>,
    clipboard: bool,
}

impl Draft {
    /// A new rule. It asks by default and has no capabilities yet.
    fn new() -> Self {
        Draft {
            name: String::new(),
            app_id: String::new(),
            exe: String::new(),
            pattern: String::new(),
            monitors: String::new(),
            capabilities: Vec::new(),
            level: Level::Ask,
            devices: Vec::new(),
            clipboard: false,
        }
    }

    fn from_rule(rule: &Rule) -> Self {
        Draft {
            name: rule.name.clone(),
            app_id: rule.app_id.clone().unwrap_or_default(),
            exe: rule.exe.clone().unwrap_or_default(),
            pattern: rule.pattern.clone().unwrap_or_default(),
            monitors: rule.monitors.join(", "),
            capabilities: rule.capabilities.clone(),
            level: rule.level,
            devices: rule.devices.clone(),
            clipboard: rule.clipboard,
        }
    }

    /// Checks the draft and builds the rule. Monitors, devices and the
    /// clipboard flag apply only to some capabilities, so they are dropped
    /// when the chosen capabilities do not use them.
    fn to_rule(&self) -> Result<Rule, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Enter a name for the rule.".to_owned());
        }
        let capabilities: Vec<Capability> = CAPABILITIES
            .iter()
            .copied()
            .filter(|capability| self.capabilities.contains(capability))
            .collect();
        if capabilities.is_empty() {
            return Err("Choose at least one capability.".to_owned());
        }
        let app_id = non_empty(&self.app_id);
        let exe = non_empty(&self.exe);
        let pattern = non_empty(&self.pattern);
        if app_id.is_none() && exe.is_none() && pattern.is_none() {
            return Err("Enter an app ID, a program path or a pattern.".to_owned());
        }
        if exe
            .as_deref()
            .is_some_and(|path| !Path::new(path).is_absolute())
        {
            return Err("The program path must start with /.".to_owned());
        }
        let monitors = if capabilities.iter().any(|c| c.supports_monitors()) {
            split_names(&self.monitors)
        } else {
            Vec::new()
        };
        let remote = capabilities.contains(&Capability::RemoteDesktop);
        let devices = if remote {
            DEVICES
                .iter()
                .copied()
                .filter(|device| self.devices.contains(device))
                .collect()
        } else {
            Vec::new()
        };
        Ok(Rule {
            name: name.to_owned(),
            app_id,
            exe,
            pattern,
            capabilities,
            level: self.level,
            monitors,
            devices,
            clipboard: remote && self.clipboard,
        })
    }
}

fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Splits a comma-separated list, dropping blank entries.
fn split_names(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Returns `items` with `item` added or removed, keeping the other items in
/// their order.
fn toggled<T: PartialEq + Clone>(items: &[T], item: T, on: bool) -> Vec<T> {
    let mut next: Vec<T> = items
        .iter()
        .filter(|existing| **existing != item)
        .cloned()
        .collect();
    if on {
        next.push(item);
    }
    next
}

fn capability_label(capability: Capability) -> &'static str {
    match capability {
        Capability::ScreenCast => "Screen cast",
        Capability::RemoteDesktop => "Remote desktop",
        Capability::InputCapture => "Input capture",
        Capability::Screenshot => "Screenshot",
        Capability::Access => "Access",
    }
}

fn device_label(device: Device) -> &'static str {
    match device {
        Device::Keyboard => "Keyboard",
        Device::Pointer => "Pointer",
        Device::Touchscreen => "Touchscreen",
    }
}

fn level_key(level: Level) -> &'static str {
    LEVELS
        .iter()
        .find(|(item, _, _)| *item == level)
        .map_or("inherit", |(_, key, _)| key)
}

fn level_label(level: Level) -> &'static str {
    LEVELS
        .iter()
        .find(|(item, _, _)| *item == level)
        .map_or("Use default", |(_, _, label)| label)
}

fn level_from_key(key: &str) -> Level {
    LEVELS
        .iter()
        .find(|(_, item, _)| *item == key)
        .map_or(Level::Inherit, |(level, _, _)| *level)
}

/// One line for a rule row: its selectors, capabilities and level.
fn rule_summary(rule: &Rule) -> String {
    let mut selectors = Vec::new();
    if let Some(app_id) = &rule.app_id {
        selectors.push(format!("App {app_id}"));
    }
    if let Some(exe) = &rule.exe {
        selectors.push(format!("Program {exe}"));
    }
    if let Some(pattern) = &rule.pattern {
        selectors.push(format!("Pattern {pattern}"));
    }
    let capabilities: Vec<&str> = rule
        .capabilities
        .iter()
        .map(|capability| capability_label(*capability))
        .collect();
    format!(
        "{} · {} · {}",
        selectors.join(" and "),
        capabilities.join(", "),
        level_label(rule.level)
    )
}

/// Writes the store in the background and calls `done` on the UI thread.
fn save_then(path: PathBuf, next: Store, done: impl FnOnce(Result<(), String>) + 'static) {
    background_then(
        move || store::save(&path, &next).map_err(|error| error.to_string()),
        done,
    );
}

/// Saves an edited store, then reloads. A failed save shows in the error line.
fn commit(ctx: Ctx, path: PathBuf, next: Store) {
    save_then(path, next, move |result| match result {
        Ok(()) => {
            let mut error = ctx.error;
            error.set(None);
            reload(ctx);
        }
        Err(message) => {
            let mut error = ctx.error;
            error.set(Some(message));
        }
    });
}

/// Applies one edit to the loaded store and saves it. An edit that fails
/// its checks sets the error line and writes nothing.
fn apply(ctx: Ctx, model: &Model, edit: impl FnOnce(&Store) -> Result<Store, String>) {
    match edit(&model.store) {
        Ok(next) => commit(ctx, model.path.clone(), next),
        Err(message) => {
            let mut error = ctx.error;
            error.set(Some(message));
        }
    }
}

fn reload(ctx: Ctx) {
    background(ctx.model, load);
}

fn open_editor(ctx: Ctx, model: &Model, index: Option<usize>, draft: Draft) {
    let title = if index.is_some() {
        "Edit rule"
    } else {
        "Add rule"
    };
    ctx.overlays.show_dialog(
        dialog(title, "")
            .content(RuleEditor {
                store: model.store.clone(),
                path: model.path.clone(),
                index,
                initial: draft,
                ctx,
            })
            .cancel("Cancel"),
    );
}

fn confirm_delete(ctx: Ctx, model: &Model, index: usize) {
    let Some(rule) = model.store.rules.get(index) else {
        return;
    };
    let name = rule.name.clone();
    let model = model.clone();
    ctx.overlays.show_dialog(
        dialog(
            format!("Delete {name}?"),
            "Apps that match this rule fall back to the other rules and the default level.",
        )
        .cancel("Cancel")
        .response("Delete", ResponseKind::Destructive, move |_| {
            apply(ctx, &model, |store| delete_rule(store, index));
        }),
    );
}

#[derive(PartialEq)]
pub struct PrivacyPage;

impl Component for PrivacyPage {
    fn render(&self) -> impl IntoElement {
        let model = use_state::<Option<Result<Model, CommandError>>>(|| None);
        let error = use_state::<Option<String>>(|| None);
        let overlays = use_overlays();
        let ctx = Ctx {
            model,
            error,
            overlays,
        };

        use_hook(move || reload(ctx));

        let Some(loaded) = model.read().clone() else {
            return page_column()
                .child(PageHead::new())
                .child(status(Status::Loading(
                    "Reading the permission rules.".into(),
                )))
                .into_element();
        };

        let data = match loaded {
            Ok(data) => data,
            Err(e) => {
                let path = store::default_path();
                return page_column()
                    .child(PageHead::new())
                    .child(status(Status::Error(e.0)).action("Try again", move |_| reload(ctx)))
                    .child(status(Status::Info(format!(
                        "Fix or remove {}, then try again. This page does not change the file while it cannot be read, and portal requests stay denied until then.",
                        path.display()
                    ))))
                    .into_element();
            }
        };

        let mut page = page_column().child(PageHead::new());

        if let Some(message) = error.read().clone() {
            page = page.child(status(Status::Error(message)));
        }

        page.child(defaults_group(&data, ctx))
            .child(rules_group(&data, ctx))
            .into_element()
    }
}

fn defaults_group(data: &Model, ctx: Ctx) -> Group {
    let model = data.clone();
    let selected = if data.store.default == Level::Deny {
        "deny"
    } else {
        "ask"
    };
    let options = vec![
        ("ask".to_owned(), "Ask".to_owned()),
        ("deny".to_owned(), "Deny".to_owned()),
    ];
    let mut group = Group::titled("Default").row(
        choice_row("Default level", options, selected, move |key: String| {
            let level = if key == "deny" {
                Level::Deny
            } else {
                Level::Ask
            };
            apply(ctx, &model, move |store| with_default(store, level));
        })
        .subtitle("Applies when no rule decides. Deny refuses every request that no rule allows."),
    );
    if !data.exists {
        group = group.row(
            row("No permission file yet")
                .subtitle("Requests use the default level. Saving a rule creates the file."),
        );
    }
    group
}

fn rules_group(data: &Model, ctx: Ctx) -> Group {
    let mut group = Group::titled("Rules");
    if data.store.rules.is_empty() {
        group = group.row(row("No rules").subtitle(
            "Requests use the default level. Add a rule to allow or deny a specific app.",
        ));
    }
    for index in 0..data.store.rules.len() {
        group = group.row(rule_row(data, index, ctx));
    }
    let model = data.clone();
    group.row(
        row("Add a rule")
            .subtitle("A matching deny always wins. Otherwise the rule listed last wins.")
            .trailing(primary_button("Add rule", move |_| {
                open_editor(ctx, &model, None, Draft::new());
            })),
    )
}

fn rule_row(data: &Model, index: usize, ctx: Ctx) -> Row {
    let rule = data.store.rules[index].clone();
    let last = data.store.rules.len() - 1;
    let mut actions = rect().tw_layout(&[flex_row(), items(Align::Center), gap(S2)]);
    if index > 0 {
        let model = data.clone();
        actions = actions.child(
            icon_button(Icon::PanUp, move |_| {
                apply(ctx, &model, |store| move_rule(store, index, Step::Up));
            })
            .tooltip("Move rule up"),
        );
    }
    if index < last {
        let model = data.clone();
        actions = actions.child(
            icon_button(Icon::PanDown, move |_| {
                apply(ctx, &model, |store| move_rule(store, index, Step::Down));
            })
            .tooltip("Move rule down"),
        );
    }
    let model = data.clone();
    let edit_rule = rule.clone();
    actions = actions.child(
        icon_button(Icon::Pencil, move |_| {
            open_editor(ctx, &model, Some(index), Draft::from_rule(&edit_rule));
        })
        .tooltip("Edit rule"),
    );
    let model = data.clone();
    actions = actions.child(
        icon_button(Icon::Delete, move |_| confirm_delete(ctx, &model, index))
            .tooltip("Delete rule"),
    );
    row(rule.name.clone())
        .subtitle(rule_summary(&rule))
        .trailing(actions)
}

/// The rule form, shown in a dialog. It owns its field state and saves the
/// rule itself, because the dialog's own buttons cannot read the form.
#[derive(PartialEq)]
struct RuleEditor {
    store: Store,
    path: PathBuf,
    index: Option<usize>,
    initial: Draft,
    ctx: Ctx,
}

impl Component for RuleEditor {
    fn render(&self) -> impl IntoElement {
        let name = use_state(|| self.initial.name.clone());
        let app_id = use_state(|| self.initial.app_id.clone());
        let exe = use_state(|| self.initial.exe.clone());
        let pattern = use_state(|| self.initial.pattern.clone());
        let monitors = use_state(|| self.initial.monitors.clone());
        let capabilities = use_state(|| self.initial.capabilities.clone());
        let level = use_state(|| self.initial.level);
        let devices = use_state(|| self.initial.devices.clone());
        let clipboard = use_state(|| self.initial.clipboard);
        let error = use_state::<Option<String>>(|| None);

        let read = move || Draft {
            name: name.read().clone(),
            app_id: app_id.read().clone(),
            exe: exe.read().clone(),
            pattern: pattern.read().clone(),
            monitors: monitors.read().clone(),
            capabilities: capabilities.read().clone(),
            level: *level.read(),
            devices: devices.read().clone(),
            clipboard: *clipboard.read(),
        };
        let current = read();
        let monitors_apply = current.capabilities.iter().any(|c| c.supports_monitors());
        let remote = current.capabilities.contains(&Capability::RemoteDesktop);

        let store = self.store.clone();
        let path = self.path.clone();
        let index = self.index;
        let ctx = self.ctx;
        let save = move |_| {
            let mut error = error;
            let result = read().to_rule().and_then(|rule| match index {
                Some(index) => replace_rule(&store, index, rule),
                None => add_rule(&store, rule),
            });
            match result {
                Err(message) => error.set(Some(message)),
                Ok(next) => {
                    error.set(None);
                    save_then(path.clone(), next, move |result| match result {
                        Ok(()) => {
                            ctx.overlays.dismiss_dialog();
                            let mut page_error = ctx.error;
                            page_error.set(None);
                            reload(ctx);
                        }
                        Err(message) => {
                            let mut error = error;
                            error.set(Some(message));
                        }
                    });
                }
            }
        };

        let mut fields = Group::new(None::<String>)
            .row(text_row("Name", name, |_: String| {}))
            .row(
                text_row("App ID", app_id, |_: String| {})
                    .subtitle("The app's ID, such as org.example.Viewer."),
            )
            .row(
                text_row("Program", exe, |_: String| {})
                    .subtitle("An absolute path, such as /usr/bin/viewer."),
            )
            .row(
                text_row("Pattern", pattern, |_: String| {})
                    .subtitle("A glob over the requester. * matches any run, ? one character."),
            );

        for capability in CAPABILITIES {
            let on = current.capabilities.contains(&capability);
            fields = fields.row(switch_row(
                capability_label(capability),
                on,
                move |next: bool| {
                    let mut capabilities = capabilities;
                    let items = capabilities.read().clone();
                    capabilities.set(toggled(&items, capability, next));
                },
            ));
        }

        if monitors_apply {
            fields = fields.row(
                text_row("Monitors", monitors, |_: String| {})
                    .subtitle("Output names, separated by commas, such as DP-1."),
            );
        }

        if remote {
            for device in DEVICES {
                let on = current.devices.contains(&device);
                fields = fields.row(switch_row(device_label(device), on, move |next: bool| {
                    let mut devices = devices;
                    let items = devices.read().clone();
                    devices.set(toggled(&items, device, next));
                }));
            }
            fields = fields.row(
                switch_row("Clipboard", current.clipboard, move |next: bool| {
                    let mut clipboard = clipboard;
                    clipboard.set(next);
                })
                .subtitle("Remote desktop sessions may use the clipboard."),
            );
        }

        let options: Vec<(String, String)> = LEVELS
            .iter()
            .map(|(_, key, label)| (key.to_string(), label.to_string()))
            .collect();
        fields = fields.row(choice_row(
            "Level",
            options,
            level_key(current.level),
            move |key: String| {
                let mut level = level;
                level.set(level_from_key(&key));
            },
        ));

        let mut form = rect().width(Size::fill()).child(fields);
        if let Some(message) = error.read().clone() {
            form = form.child(status(Status::Error(message)));
        }
        form.child(
            Group::new(None::<String>)
                .row(row("Save the rule").trailing(primary_button("Save", save))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(name: &str) -> Rule {
        Rule {
            name: name.to_owned(),
            app_id: Some(format!("org.example.{name}")),
            exe: None,
            pattern: None,
            capabilities: vec![Capability::Screenshot],
            level: Level::Allow,
            monitors: Vec::new(),
            devices: Vec::new(),
            clipboard: false,
        }
    }

    fn store_with(names: &[&str]) -> Store {
        Store {
            version: store::STORE_VERSION,
            default: Level::Inherit,
            rules: names.iter().map(|name| rule(name)).collect(),
        }
    }

    fn names(store: &Store) -> Vec<&str> {
        store.rules.iter().map(|rule| rule.name.as_str()).collect()
    }

    fn draft(name: &str, caps: &[Capability]) -> Draft {
        Draft {
            name: name.to_owned(),
            app_id: "org.example.App".to_owned(),
            capabilities: caps.to_vec(),
            ..Draft::new()
        }
    }

    fn temp_file(name: &str, text: Option<&str>) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bingux-privacy-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(store::STORE_FILE_NAME);
        if let Some(text) = text {
            std::fs::write(&path, text).unwrap();
        }
        path
    }

    #[test]
    fn default_level_accepts_ask_and_deny_but_not_allow() {
        let store = store_with(&[]);
        assert_eq!(
            with_default(&store, Level::Deny).unwrap().default,
            Level::Deny
        );
        assert_eq!(
            with_default(&store, Level::Ask).unwrap().default,
            Level::Ask
        );
        assert!(with_default(&store, Level::Allow).is_err());
    }

    #[test]
    fn add_rule_appends_and_rejects_a_duplicate_name() {
        let store = store_with(&["one"]);
        let next = add_rule(&store, rule("two")).unwrap();
        assert_eq!(names(&next), ["one", "two"]);
        assert!(add_rule(&store, rule("one")).is_err());
    }

    #[test]
    fn replace_rule_keeps_the_position_and_rejects_a_missing_index() {
        let store = store_with(&["one", "two"]);
        let next = replace_rule(&store, 1, rule("renamed")).unwrap();
        assert_eq!(names(&next), ["one", "renamed"]);
        assert!(replace_rule(&store, 5, rule("x")).is_err());
    }

    #[test]
    fn delete_rule_removes_one_rule_and_rejects_a_missing_index() {
        let store = store_with(&["one", "two", "three"]);
        let next = delete_rule(&store, 1).unwrap();
        assert_eq!(names(&next), ["one", "three"]);
        assert!(delete_rule(&store, 3).is_err());
    }

    #[test]
    fn move_rule_swaps_neighbours_and_stops_at_the_ends() {
        let store = store_with(&["one", "two", "three"]);
        let down = move_rule(&store, 0, Step::Down).unwrap();
        assert_eq!(names(&down), ["two", "one", "three"]);
        let up = move_rule(&store, 2, Step::Up).unwrap();
        assert_eq!(names(&up), ["one", "three", "two"]);
        assert!(move_rule(&store, 0, Step::Up).is_err());
        assert!(move_rule(&store, 2, Step::Down).is_err());
    }

    #[test]
    fn draft_needs_a_name_a_capability_and_a_selector() {
        let mut no_name = draft("  ", &[Capability::Screenshot]);
        assert!(no_name.to_rule().is_err());

        no_name = draft("ok", &[]);
        assert_eq!(
            no_name.to_rule().unwrap_err(),
            "Choose at least one capability."
        );

        let mut no_selector = draft("ok", &[Capability::Screenshot]);
        no_selector.app_id = "  ".to_owned();
        assert_eq!(
            no_selector.to_rule().unwrap_err(),
            "Enter an app ID, a program path or a pattern."
        );
    }

    #[test]
    fn draft_requires_an_absolute_program_path() {
        let mut relative = draft("ok", &[Capability::Access]);
        relative.exe = "bin/viewer".to_owned();
        assert_eq!(
            relative.to_rule().unwrap_err(),
            "The program path must start with /."
        );
        relative.exe = "/usr/bin/viewer".to_owned();
        assert!(relative.to_rule().is_ok());
    }

    #[test]
    fn draft_keeps_monitors_only_for_capabilities_that_use_them() {
        let mut screen = draft("ok", &[Capability::ScreenCast]);
        screen.monitors = " DP-1, , HDMI-A-1 ".to_owned();
        assert_eq!(screen.to_rule().unwrap().monitors, ["DP-1", "HDMI-A-1"]);

        let mut shot = draft("ok", &[Capability::Screenshot]);
        shot.monitors = "DP-1".to_owned();
        assert!(shot.to_rule().unwrap().monitors.is_empty());
    }

    #[test]
    fn draft_keeps_devices_and_clipboard_only_for_remote_desktop() {
        let mut cast = draft("ok", &[Capability::ScreenCast]);
        cast.devices = vec![Device::Keyboard];
        cast.clipboard = true;
        let rule = cast.to_rule().unwrap();
        assert!(rule.devices.is_empty());
        assert!(!rule.clipboard);

        let mut remote = draft("ok", &[Capability::RemoteDesktop]);
        remote.devices = vec![Device::Pointer, Device::Keyboard];
        remote.clipboard = true;
        let rule = remote.to_rule().unwrap();
        assert_eq!(rule.devices, [Device::Keyboard, Device::Pointer]);
        assert!(rule.clipboard);
    }

    #[test]
    fn draft_lists_capabilities_in_schema_order() {
        let built = draft("ok", &[Capability::Access, Capability::ScreenCast])
            .to_rule()
            .unwrap();
        assert_eq!(
            built.capabilities,
            [Capability::ScreenCast, Capability::Access]
        );
    }

    #[test]
    fn a_built_rule_passes_the_store_schema() {
        let store = store_with(&[]);
        let built = draft("ok", &[Capability::Screenshot]).to_rule().unwrap();
        let next = add_rule(&store, built).unwrap();
        assert_eq!(next.rules.len(), 1);
    }

    #[test]
    fn a_missing_file_gives_an_empty_store_with_the_default_applying() {
        let path = temp_file("missing", None);
        let model = read_model(&path).unwrap();
        assert!(!model.exists);
        assert_eq!(model.store, Store::empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_broken_file_is_an_error_and_is_left_unchanged() {
        let text = "version = 1\n[[rule]]\nname = \"broken\"\nlevel = \"allow\"\n";
        let path = temp_file("broken", Some(text));
        assert!(read_model(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_valid_file_loads_its_rules() {
        let text = "version = 1\ndefault = \"deny\"\n\n[[rule]]\nname = \"viewer\"\napp_id = \"org.example.Viewer\"\ncapabilities = [\"screenshot\"]\nlevel = \"allow\"\n";
        let path = temp_file("valid", Some(text));
        let model = read_model(&path).unwrap();
        assert!(model.exists);
        assert_eq!(model.store.default, Level::Deny);
        assert_eq!(names(&model.store), ["viewer"]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn level_keys_round_trip() {
        for (level, key, _) in LEVELS {
            assert_eq!(level_key(level), key);
            assert_eq!(level_from_key(key), level);
        }
    }

    #[test]
    fn rule_summary_names_selectors_capabilities_and_level() {
        let mut built = rule("viewer");
        built.exe = Some("/usr/bin/viewer".to_owned());
        built.capabilities = vec![Capability::ScreenCast, Capability::Screenshot];
        assert_eq!(
            rule_summary(&built),
            "App org.example.viewer and Program /usr/bin/viewer · Screen cast, Screenshot · Allow"
        );
    }
}
