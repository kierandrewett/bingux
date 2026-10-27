//! Validation of the snake_case configuration tree.
//!
//! This is a direct port of the rules in the former `settings-backend.py`,
//! including its error messages, so the shell and tests keep their contract.
//! The rules run on a loose JSON tree so that a wrong shape produces the
//! documented message instead of a serde error.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use serde_json::{Map, Value};

use crate::Error;

pub const PROVIDERS: &[&str] = &[
    "applications",
    "files",
    "calculation",
    "conversions",
    "web",
    "web-shortcuts",
    "external",
];

pub const EXTERNAL_CONTROL_IDS: &[&str] = &[
    "search",
    "clock",
    "capture",
    "tray",
    "privacy",
    "metrics",
    "keyboard",
    "overflow",
    "controls",
    "notifications",
    "terminal",
    "notes",
    "monitor",
    "calendar",
    "media",
    "tasks",
];

const PORTABLE_CONTROLS: &[&str] = &[
    "control-account",
    "control-settings",
    "control-session",
    "control-lock",
    "control-volume",
    "control-microphone",
    "control-battery",
    "control-media",
    "controls-header",
    "controls-audio",
    "controls-tiles",
    "control-divider",
    "control-header-space",
    "control-customise",
];

/// Native Control Centre groups, in their default order.
pub const CONTROL_GROUPS: &[(&str, &[&str])] = &[
    (
        "control-centre",
        &[
            "controls-header",
            "controls-audio",
            "control-divider",
            "controls-tiles",
            "control-media",
            "control-customise",
        ],
    ),
    (
        "controls-header",
        &[
            "control-account",
            "control-header-space",
            "control-battery",
            "control-settings",
            "control-session",
            "control-lock",
        ],
    ),
    ("controls-audio", &["control-volume", "control-microphone"]),
];

/// Control identifiers as stored in `control_order` and layout items. These are
/// values, not keys, so they keep the shell's camelCase spelling.
const CONTROL_NAMES: &[&str] = &[
    "network",
    "bluetooth",
    "vpn",
    "dnd",
    "nightLight",
    "power",
    "awake",
];
const FOLDER_COLOURS: &[&str] = &[
    "blue", "teal", "green", "yellow", "orange", "red", "pink", "purple", "slate",
];
const DISPLAY_MODES: &[&str] = &["native", "icons", "text", "both"];
const CONTAINER_KEYS: &[&str] = &[
    "top-left",
    "top-center",
    "top-right",
    "dock",
    "sidebar",
    "control-centre",
    "controls-header",
    "controls-audio",
    "controls-tiles",
];

fn invalid(path: &str, message: &str) -> Error {
    Error::Invalid {
        path: path.to_string(),
        message: message.to_string(),
    }
}

fn is_identifier_segment(text: &str, first: usize) -> bool {
    text.len() <= first + 127
        && !text.is_empty()
        && text.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && b"._-".contains(&byte))
        })
}

/// `extension:<extension-id>/<widget-id>` with the documented character set.
pub fn is_extension_widget(item: &str) -> bool {
    let Some(rest) = item.strip_prefix("extension:") else {
        return false;
    };
    let Some((extension, widget)) = rest.split_once('/') else {
        return false;
    };
    is_identifier_segment(extension, 1) && is_identifier_segment(widget, 1) && !widget.contains('/')
}

fn is_instance(item: &str, kinds: &[&str]) -> bool {
    let Some((kind, number)) = item.split_once(':') else {
        return false;
    };
    kinds.contains(&kind) && is_small_number(number)
}

/// `[1-9][0-9]{0,3}`
fn is_small_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 4
        && bytes[0].is_ascii_digit()
        && bytes[0] != b'0'
        && bytes.iter().all(u8::is_ascii_digit)
}

pub fn is_external_control(item: &str) -> bool {
    is_extension_widget(item)
        || EXTERNAL_CONTROL_IDS.contains(&item)
        || is_instance(item, &["label", "icon"])
}

fn has_control_char(text: &str) -> bool {
    text.chars().any(|c| (c as u32) < 32)
}

fn is_int(value: &Value) -> bool {
    value.is_i64() || value.is_u64()
}

fn string_list(value: &Value) -> Option<Vec<&str>> {
    value.as_array()?.iter().map(Value::as_str).collect()
}

fn unique<'a>(items: &[&'a str]) -> bool {
    let set: HashSet<&&str> = items.iter().collect();
    set.len() == items.len()
}

fn control_layout(layout: &Value) -> Result<(), Error> {
    if layout.is_null() {
        return Ok(());
    }
    let path = "desktop.control_layout";
    let Some(map) = layout.as_object() else {
        return Err(invalid(
            path,
            "This control-centre layout version is not supported.",
        ));
    };
    let keys: BTreeSet<&str> = map.keys().map(String::as_str).collect();
    if keys != BTreeSet::from(["version", "groups"])
        || !is_int(&layout["version"])
        || layout["version"].as_i64() != Some(1)
    {
        return Err(invalid(
            path,
            "This control-centre layout version is not supported.",
        ));
    }
    let Some(groups) = layout["groups"].as_object() else {
        return Err(invalid(path, "Invalid control-centre groups."));
    };
    let expected: BTreeSet<&str> = CONTROL_GROUPS.iter().map(|(name, _)| *name).collect();
    let actual: BTreeSet<&str> = groups.keys().map(String::as_str).collect();
    if actual != expected {
        return Err(invalid(path, "Invalid control-centre groups."));
    }
    for (group, members) in CONTROL_GROUPS {
        let items = groups
            .get(*group)
            .and_then(string_list)
            .filter(|items| items.len() <= 256);
        let valid = items.as_ref().is_some_and(|items| {
            unique(items)
                && items.iter().all(|item| {
                    members.contains(item)
                        || (*group == "control-centre" && is_external_control(item))
                })
        });
        if !valid {
            return Err(invalid(
                path,
                "A control-centre widget can only appear once in its group.",
            ));
        }
    }
    Ok(())
}

fn control_action_placement(desktop: &Value) -> Result<(), Error> {
    let layout = &desktop["layout"];
    if layout.is_null() {
        return Ok(());
    }
    let Some(zones) = layout
        .as_object()
        .filter(|zones| zones.values().all(Value::is_array))
    else {
        return Err(invalid("desktop.layout", "Invalid desktop layout."));
    };
    let placed: Vec<&str> = zones
        .values()
        .flat_map(|items| items.as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let actions: HashSet<&str> = placed
        .iter()
        .copied()
        .filter(|item| PORTABLE_CONTROLS.contains(item))
        .collect();
    let external: HashSet<&str> = placed
        .iter()
        .copied()
        .filter(|item| is_external_control(item))
        .collect();
    let grouped = &desktop["control_layout"];
    if let Some(groups) = grouped.get("groups").and_then(Value::as_object) {
        let centre: HashSet<&str> = groups
            .get("control-centre")
            .and_then(string_list)
            .unwrap_or_default()
            .into_iter()
            .collect();
        if !external.is_disjoint(&centre) {
            return Err(invalid(
                "desktop.layout",
                "A status or decoration widget can only be placed in one container.",
            ));
        }
        let all: HashSet<&str> = groups
            .values()
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !actions.is_empty() && !actions.is_disjoint(&all) {
            return Err(invalid(
                "desktop.layout",
                "A control widget can only be placed in one container.",
            ));
        }
    } else if !actions.is_empty() {
        return Err(invalid(
            "desktop.layout",
            "A control widget can only be placed in one container.",
        ));
    }
    Ok(())
}

fn validate_desktop(desktop: &Value, previous: Option<&Value>) -> Result<(), Error> {
    let version = &desktop["layout_version"];
    if let Some(previous) = previous
        && previous["layout_version"].as_i64() == Some(1)
        && version.as_i64() != Some(1)
    {
        return Err(invalid(
            "desktop.layout_version",
            "The desktop layout has changed. Reload preferences before saving.",
        ));
    }
    if !is_int(version) || !matches!(version.as_i64(), Some(0 | 1)) {
        return Err(invalid(
            "desktop.layout_version",
            "This desktop layout version is not supported.",
        ));
    }
    let applications = &desktop["dock_apps"];
    if !applications.is_null() {
        let keys: Option<BTreeSet<&str>> = applications
            .as_object()
            .map(|map| map.keys().map(String::as_str).collect());
        if keys != Some(BTreeSet::from(["pinned_apps", "order"])) {
            return Err(invalid(
                "desktop.dock_apps",
                "Invalid dock application layout.",
            ));
        }
        for items in applications.as_object().into_iter().flat_map(Map::values) {
            let valid = string_list(items).is_some_and(|items| {
                items.len() <= 256
                    && unique(&items)
                    && items.iter().all(|item| {
                        !item.is_empty() && item.len() <= 256 && !has_control_char(item)
                    })
            });
            if !valid {
                return Err(invalid(
                    "desktop.dock_apps",
                    "Invalid dock application identifiers.",
                ));
            }
        }
    }
    let Some(folders) = desktop["dock_folders"]
        .as_array()
        .filter(|folders| folders.len() <= 64)
    else {
        return Err(invalid("desktop.dock_folders", "Invalid dock folders."));
    };
    let mut folder_ids: HashSet<&str> = HashSet::new();
    let mut member_ids: HashSet<&str> = HashSet::new();
    for folder in folders {
        let path = "desktop.dock_folders";
        let Some(fields) = folder.as_object() else {
            return Err(invalid(path, "Invalid dock folder."));
        };
        let keys: BTreeSet<&str> = fields.keys().map(String::as_str).collect();
        if keys != BTreeSet::from(["id", "name", "color", "apps"]) {
            return Err(invalid(path, "Invalid dock folder."));
        }
        let id = folder["id"].as_str().filter(|id| {
            id.strip_prefix("folder:").is_some_and(|rest| {
                !rest.is_empty()
                    && rest
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            })
        });
        let Some(id) = id.filter(|id| !folder_ids.contains(id)) else {
            return Err(invalid(path, "Invalid dock folder identifier."));
        };
        let name_ok = folder["name"].as_str().is_some_and(|name| {
            (1..=64).contains(&name.chars().count()) && !has_control_char(name)
        });
        if !name_ok {
            return Err(invalid(path, "Invalid dock folder name."));
        }
        if !folder["color"]
            .as_str()
            .is_some_and(|colour| FOLDER_COLOURS.contains(&colour))
        {
            return Err(invalid(path, "Invalid dock folder colour."));
        }
        let members = string_list(&folder["apps"]).filter(|members| {
            members.len() <= 256
                && unique(members)
                && members
                    .iter()
                    .all(|item| !item.is_empty() && item.len() <= 256 && !has_control_char(item))
                && members.iter().all(|item| !member_ids.contains(item))
        });
        let Some(members) = members else {
            return Err(invalid(path, "Invalid dock folder members."));
        };
        folder_ids.insert(id);
        member_ids.extend(members);
    }
    control_layout(&desktop["control_layout"])?;
    if let Some(previous) = previous
        && !previous["control_layout"].is_null()
        && desktop["control_layout"].is_null()
    {
        return Err(invalid(
            "desktop.control_layout",
            "The control-centre layout has changed. Reload preferences before saving.",
        ));
    }
    let control_order = &desktop["control_order"];
    if !control_order.is_null() {
        let valid = string_list(control_order).is_some_and(|items| {
            unique(&items) && items.iter().all(|item| CONTROL_NAMES.contains(item))
        });
        if !valid {
            return Err(invalid(
                "desktop.control_order",
                "Invalid control-centre widget order.",
            ));
        }
    }
    let controls = &desktop["control_centre"];
    if !controls.is_null() {
        let valid = controls.as_object().is_some_and(|map| {
            map.keys().map(String::as_str).collect::<BTreeSet<_>>()
                == BTreeSet::from(["vpn", "dnd", "night_light", "power", "awake"])
                && map.values().all(Value::is_boolean)
        });
        if !valid {
            return Err(invalid(
                "desktop.control_centre",
                "Invalid control-centre preferences.",
            ));
        }
    }
    let Some(containers) = desktop["containers"].as_object() else {
        return Err(invalid(
            "desktop.containers",
            "Invalid container display options.",
        ));
    };
    for (key, options) in containers {
        let valid = CONTAINER_KEYS.contains(&key.as_str())
            && options.as_object().is_some_and(|map| {
                map.len() == 1
                    && map
                        .get("display")
                        .and_then(Value::as_str)
                        .is_some_and(|m| DISPLAY_MODES.contains(&m))
            });
        if !valid {
            return Err(invalid(
                "desktop.containers",
                "Invalid container display options.",
            ));
        }
    }
    let Some(widget_options) = desktop["widget_options"]
        .as_object()
        .filter(|map| map.len() <= 256)
    else {
        return Err(invalid(
            "desktop.widget_options",
            "Invalid widget display options.",
        ));
    };
    for (key, options) in widget_options {
        let path = "desktop.widget_options";
        let Some(options) = options.as_object().filter(|map| {
            map.keys()
                .all(|k| ["display", "label", "icon", "width"].contains(&k.as_str()))
        }) else {
            return Err(invalid(path, "Invalid widget display options."));
        };
        if let Some(width) = options.get("width") {
            let fixed = key == "control-header-space" || is_instance(key, &["spacer"]);
            if !fixed || !is_int(width) || !(8..=160).contains(&width.as_i64().unwrap_or(0)) {
                return Err(invalid(
                    path,
                    "Fixed spaces must be between 8 and 160 pixels.",
                ));
            }
        }
        let display = options
            .get("display")
            .map_or(Some("inherit"), Value::as_str);
        if !display.is_some_and(|mode| mode == "inherit" || DISPLAY_MODES.contains(&mode)) {
            return Err(invalid(path, "Invalid widget display mode."));
        }
        for field in ["label", "icon"] {
            let text = options.get(field).map_or(Some(""), Value::as_str);
            if !text.is_some_and(|text| text.chars().count() <= 160 && !has_control_char(text)) {
                return Err(invalid(path, "Invalid widget label or icon."));
            }
        }
    }
    let choices: [(&str, &[&str]); 5] = [
        ("dock_alignment", &["left", "center", "right"]),
        ("dock_click", &["toggle", "focus", "launch"]),
        ("dock_middle_click", &["launch", "close", "none"]),
        ("dock_scroll", &["cycle", "none"]),
        ("dock_scroll_direction", &["natural", "reverse"]),
    ];
    for (key, allowed) in choices {
        if !desktop[key]
            .as_str()
            .is_some_and(|value| allowed.contains(&value))
        {
            return Err(invalid(
                &format!("desktop.{key}"),
                "Invalid desktop behaviour.",
            ));
        }
    }
    let edge = &desktop["sidebar_edge"];
    if !edge.is_null()
        && !edge
            .as_str()
            .is_some_and(|value| ["left", "right", "top"].contains(&value))
    {
        return Err(invalid(
            "desktop.sidebar_edge",
            "Invalid desktop behaviour.",
        ));
    }
    let size = &desktop["dock_size"];
    if !is_int(size) || !(32..=80).contains(&size.as_i64().unwrap_or(0)) {
        return Err(invalid(
            "desktop.dock_size",
            "Dock icons must be between 32 and 80 pixels.",
        ));
    }
    let layout = &desktop["layout"];
    if !layout.is_null() {
        let path = "desktop.layout";
        let zones = ["top-left", "top-center", "top-right", "dock", "sidebar"];
        let Some(map) = layout.as_object() else {
            return Err(invalid(path, "Invalid desktop layout."));
        };
        if map.keys().map(String::as_str).collect::<BTreeSet<_>>() != zones.into_iter().collect() {
            return Err(invalid(path, "Invalid desktop layout."));
        }
        let controls: Vec<String> = CONTROL_NAMES
            .iter()
            .map(|name| format!("control-{name}"))
            .collect();
        let mut seen: HashSet<&str> = HashSet::new();
        for (zone, items) in map {
            let Some(items) = items.as_array().filter(|items| items.len() <= 256) else {
                return Err(invalid(path, "Invalid container widget list."));
            };
            let kinds: &[&str] = if zone.starts_with("top-") {
                &["spacer", "spring", "label", "icon"]
            } else {
                &["label", "icon"]
            };
            let mut zone_items = Vec::with_capacity(items.len());
            for item in items {
                let Some(item) = item.as_str() else {
                    return Err(invalid(path, "Invalid widget identifier."));
                };
                let allowed = EXTERNAL_CONTROL_IDS.contains(&item)
                    || controls.iter().any(|c| c == item)
                    || PORTABLE_CONTROLS.contains(&item)
                    || is_instance(item, kinds)
                    || is_extension_widget(item);
                if !allowed || seen.contains(item) {
                    return Err(invalid(
                        path,
                        "A widget can only be placed once in a compatible area.",
                    ));
                }
                zone_items.push(item);
            }
            if !unique(&zone_items) {
                return Err(invalid(path, "A widget can only be placed once."));
            }
            seen.extend(zone_items);
        }
        control_action_placement(desktop)?;
        let placed_controls = controls.iter().any(|c| seen.contains(c.as_str()));
        let order_conflict = match string_list(control_order) {
            None => true,
            Some(order) => order
                .iter()
                .any(|name| seen.contains(format!("control-{name}").as_str())),
        };
        if placed_controls && order_conflict {
            return Err(invalid(
                path,
                "A control can only be placed in one container.",
            ));
        }
    }
    for key in ["dock", "sidebar", "metrics", "clock_seconds"] {
        if !desktop[key].is_boolean() {
            return Err(invalid(&format!("desktop.{key}"), "Invalid toggle value."));
        }
    }
    let wallpaper_ok = desktop["wallpaper"].as_str().is_some_and(|wallpaper| {
        wallpaper.len() <= 4096
            && !has_control_char(wallpaper)
            && (wallpaper.is_empty() || Path::new(wallpaper).is_absolute())
    });
    if !wallpaper_ok {
        return Err(invalid(
            "desktop.wallpaper",
            "Wallpaper must be an absolute file path no longer than 4096 characters.",
        ));
    }
    Ok(())
}

fn validate_search(search: &Value) -> Result<(), Error> {
    let disabled = string_list(&search["disabled_providers"]);
    if !disabled.is_some_and(|items| items.iter().all(|p| PROVIDERS.contains(p))) {
        return Err(invalid(
            "search.disabled_providers",
            "Unknown search provider.",
        ));
    }
    let roots = &search["file_roots"];
    if !roots.is_null() {
        let valid = string_list(roots).is_some_and(|roots| {
            roots.len() <= 32
                && roots
                    .iter()
                    .all(|p| Path::new(p).is_absolute() && !p.contains('\0'))
        });
        if !valid {
            return Err(invalid(
                "search.file_roots",
                "Use absolute paths for search locations, with at most 32 locations.",
            ));
        }
    }
    let Some(engines) = search["engines"]
        .as_array()
        .filter(|e| (1..=32).contains(&e.len()))
    else {
        return Err(invalid(
            "search.engines",
            "Add between 1 and 32 search engines.",
        ));
    };
    let mut ids: HashSet<&str> = HashSet::new();
    let mut shortcuts: HashSet<&str> = HashSet::new();
    for engine in engines {
        let path = "search.engines";
        let Some(fields) = engine.as_object() else {
            return Err(invalid(path, "Invalid search engine."));
        };
        let keys: BTreeSet<&str> = fields.keys().map(String::as_str).collect();
        if keys != BTreeSet::from(["id", "name", "shortcut", "url", "enabled"]) {
            return Err(invalid(path, "Invalid search engine."));
        }
        for field in ["id", "name", "shortcut", "url"] {
            if !engine[field]
                .as_str()
                .is_some_and(|text| !has_control_char(text))
            {
                return Err(invalid(path, "Invalid search engine text."));
            }
        }
        let id = engine["id"].as_str().unwrap_or_default();
        if !is_slug(id, 64) || ids.contains(id) {
            return Err(invalid(path, "Search engine identifiers must be unique."));
        }
        let shortcut = engine["shortcut"].as_str().unwrap_or_default();
        if !is_slug(shortcut, 24)
            || shortcuts.contains(shortcut)
            || ["wiki", "gh", "maps", "yt"].contains(&shortcut)
        {
            return Err(invalid(
                path,
                "Choose a unique shortcut with letters, numbers or hyphens. wiki, gh, maps and yt are reserved.",
            ));
        }
        let name = engine["name"].as_str().unwrap_or_default();
        if name.trim().is_empty() || name.chars().count() > 80 {
            return Err(invalid(path, "Enter a name of up to 80 characters."));
        }
        let url = engine["url"].as_str().unwrap_or_default();
        if !engine_url_ok(url) {
            return Err(invalid(
                path,
                "Use an HTTP or HTTPS URL with {query} in its path or query.",
            ));
        }
        if !engine["enabled"].is_boolean() {
            return Err(invalid(path, "Invalid engine toggle."));
        }
        ids.insert(id);
        shortcuts.insert(shortcut);
    }
    let default = search["default_engine"].as_str().unwrap_or_default();
    let default_enabled = engines
        .iter()
        .any(|e| e["id"].as_str() == Some(default) && e["enabled"].as_bool() == Some(true));
    if !default_enabled {
        return Err(invalid(
            "search.default_engine",
            "The default search engine must be enabled.",
        ));
    }
    Ok(())
}

/// `[a-z0-9][a-z0-9-]{0,max-1}`
fn is_slug(text: &str, max: usize) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= max
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn engine_url_ok(text: &str) -> bool {
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    let netloc_has_query = url.host_str().is_some_and(|host| host.contains("{query}"));
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && text.len() <= 2048
        && text.matches("{query}").count() == 1
        && !netloc_has_query
}

/// Validate the AI section and return the resolved executable when one is
/// configured, so the caller can persist it like the Python helper did.
///
/// Whether the harness is installed is a property of this machine, not of
/// the file, so it is enforced only when `require_installed` is set (on a
/// write). A saved or migrated config whose harness is now missing still
/// loads; the search service reports the missing program when it is used.
fn validate_ai(search: &Value, require_installed: bool) -> Result<Option<String>, Error> {
    let ai = &search["ai"];
    if ai.is_null() {
        return Ok(None);
    }
    let path = "search.ai";
    let Some(map) = ai.as_object().filter(|map| {
        map.keys()
            .all(|k| ["harness", "executable", "model"].contains(&k.as_str()))
    }) else {
        return Err(invalid(path, "Invalid AI settings."));
    };
    let harness = map.get("harness").and_then(Value::as_str);
    let Some(harness) = harness.filter(|h| ["pi", "claude"].contains(h)) else {
        return Err(invalid(path, "Choose Pi or Claude Code."));
    };
    let model = map.get("model").map_or(Some(""), Value::as_str);
    if !model.is_some_and(|m| m.chars().count() <= 256 && !has_control_char(m)) {
        return Err(invalid(path, "Invalid model."));
    }
    let executable = map
        .get("executable")
        .and_then(Value::as_str)
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .or_else(|| which(harness));
    let usable = executable
        .as_deref()
        .is_some_and(|e| Path::new(e).is_absolute() && is_executable(Path::new(e)));
    if usable {
        return Ok(executable);
    }
    if require_installed {
        return Err(invalid(
            path,
            "The selected CLI is not installed or executable.",
        ));
    }
    Ok(None)
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

/// Resolve a command name on `PATH`.
pub fn which(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
        .map(|p| p.to_string_lossy().into_owned())
}

fn validate_previews(previews: &Value) -> Result<(), Error> {
    for key in ["enabled", "prewarm"] {
        if !previews[key].is_boolean() {
            return Err(invalid(&format!("previews.{key}"), "Invalid toggle value."));
        }
    }
    let limit = &previews["max_megabytes"];
    if !is_int(limit) || !(1..=20).contains(&limit.as_i64().unwrap_or(0)) {
        return Err(invalid(
            "previews.max_megabytes",
            "Preview limit must be between 1 and 20 MB.",
        ));
    }
    Ok(())
}

fn validate_privacy(privacy: &Value) -> Result<(), Error> {
    if !privacy["public_ip_lookup"].is_boolean() {
        return Err(invalid("privacy.public_ip_lookup", "Invalid toggle value."));
    }
    if !privacy["show_personal_images"].is_boolean() {
        return Err(invalid("privacy.show_personal_images", "Invalid toggle value."));
    }
    Ok(())
}

/// Reject sections and keys that the schema does not declare.
pub fn check_shape(tree: &Value, defaults: &Value) -> Result<(), Error> {
    let Some(sections) = tree.as_object() else {
        return Err(invalid("", "Unknown settings section."));
    };
    let known = defaults.as_object().expect("defaults are an object");
    for (section, values) in sections {
        if !known.contains_key(section) {
            return Err(invalid(section, "Unknown settings section."));
        }
        let Some(values) = values.as_object() else {
            return Err(invalid(section, "Unknown setting."));
        };
        let allowed = known[section]
            .as_object()
            .expect("section defaults are an object");
        for key in values.keys() {
            if !allowed.contains_key(key) {
                return Err(invalid(&format!("{section}.{key}"), "Unknown setting."));
            }
        }
    }
    Ok(())
}

/// Validate a complete tree (defaults already merged). `previous` is the
/// tree currently on disk when validating a proposed write, and
/// `require_ai_installed` is set when that write changes `search.ai`, so a
/// harness that was removed after being chosen blocks only new AI settings,
/// not loads or unrelated writes. Returns the resolved AI executable so the
/// caller can store it.
pub fn validate(
    tree: &Value,
    previous: Option<&Value>,
    require_ai_installed: bool,
) -> Result<Option<String>, Error> {
    validate_search(&tree["search"])?;
    validate_desktop(&tree["desktop"], previous.map(|p| &p["desktop"]))?;
    let executable = validate_ai(&tree["search"], require_ai_installed)?;
    validate_previews(&tree["previews"])?;
    validate_privacy(&tree["privacy"])?;
    Ok(executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert!(is_extension_widget("extension:org.bingux.example/clock"));
        assert!(!is_extension_widget("extension:Org/clock"));
        assert!(!is_extension_widget("extension:org"));
        assert!(is_instance("spacer:12", &["spacer"]));
        assert!(!is_instance("spacer:0", &["spacer"]));
        assert!(!is_instance("spacer:12345", &["spacer"]));
        assert!(is_slug("duckduckgo", 64));
        assert!(!is_slug("-x", 64));
        assert!(engine_url_ok("https://duckduckgo.com/?q={query}"));
        assert!(!engine_url_ok("https://{query}.example.com/"));
        assert!(!engine_url_ok("ftp://example.com/{query}"));
    }
}
