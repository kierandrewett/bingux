//! Adapter between the TOML schema (snake_case) and the JSON shape the shell
//! still reads (camelCase, `null` for absent options).
//!
//! Only declared keys are renamed. Maps whose keys are user identifiers
//! (`containers`, `widget_options`, `layout`, `groups`) keep their keys.

use serde_json::{Map, Value};

const RENAMES: &[(&str, &str)] = &[
    ("clock_seconds", "clockSeconds"),
    ("sidebar_edge", "sidebarEdge"),
    ("layout_version", "layoutVersion"),
    ("dock_size", "dockSize"),
    ("dock_alignment", "dockAlignment"),
    ("dock_click", "dockClick"),
    ("dock_middle_click", "dockMiddleClick"),
    ("dock_scroll", "dockScroll"),
    ("dock_scroll_direction", "dockScrollDirection"),
    ("dock_apps", "dockApps"),
    ("dock_folders", "dockFolders"),
    ("pinned_apps", "pinnedApps"),
    ("control_centre", "controlCentre"),
    ("control_order", "controlOrder"),
    ("control_layout", "controlLayout"),
    ("widget_options", "widgetOptions"),
    ("night_light", "nightLight"),
    ("disabled_providers", "disabledProviders"),
    ("default_engine", "defaultEngine"),
    ("file_roots", "fileRoots"),
    ("max_megabytes", "maxMegabytes"),
    ("public_ip_lookup", "publicIpLookup"),
    ("font_family", "fontFamily"),
    ("show_personal_images", "showPersonalImages"),
];

/// Keys whose immediate children are identifiers, not schema keys.
const FREE_FORM: &[&str] = &[
    "containers",
    "widget_options",
    "widgetOptions",
    "layout",
    "groups",
];

/// Optional keys that the JSON shape always carries, as `null` when absent.
const NULLABLE: &[(&str, &str)] = &[
    ("search", "ai"),
    ("search", "fileRoots"),
    ("desktop", "layout"),
    ("desktop", "sidebarEdge"),
    ("desktop", "dockApps"),
    ("desktop", "controlCentre"),
    ("desktop", "controlOrder"),
    ("desktop", "controlLayout"),
];

fn rename(key: &str, to_camel: bool) -> String {
    for (snake, camel) in RENAMES {
        if to_camel && key == *snake {
            return (*camel).to_string();
        }
        if !to_camel && key == *camel {
            return (*snake).to_string();
        }
    }
    key.to_string()
}

fn convert(value: &Value, to_camel: bool, free_form: bool) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            for (key, child) in map {
                let name = if free_form {
                    key.clone()
                } else {
                    rename(key, to_camel)
                };
                let child_free = !free_form && FREE_FORM.contains(&key.as_str());
                out.insert(name, convert(child, to_camel, child_free));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| convert(item, to_camel, false))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Snake_case tree to the camelCase JSON shape, adding `null` for absent
/// optional keys so the shell sees the same structure as before.
pub fn to_legacy(tree: &Value) -> Value {
    let mut out = convert(tree, true, false);
    if let Value::Object(sections) = &mut out {
        for (section, key) in NULLABLE {
            if let Some(Value::Object(map)) = sections.get_mut(*section) {
                map.entry(*key).or_insert(Value::Null);
            }
        }
    }
    out
}

/// CamelCase JSON (from the shell or an old `settings.json`) to the
/// snake_case tree. `null` values stay `null`; callers decide what they mean.
pub fn from_legacy(value: &Value) -> Value {
    convert(value, false, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renames_declared_keys_only() {
        let snake = json!({
            "desktop": {
                "dock_size": 60,
                "widget_options": {"spacer:1": {"width": 12}, "extension:org.x/my_widget": {"label": "a"}},
                "control_layout": {"version": 1, "groups": {"controls-header": ["control_x"]}},
                "control_centre": {"night_light": true}
            },
            "privacy": {"public_ip_lookup": false, "show_personal_images": true}
        });
        let camel = to_legacy(&snake);
        assert_eq!(camel["desktop"]["dockSize"], 60);
        assert_eq!(
            camel["desktop"]["widgetOptions"]["extension:org.x/my_widget"]["label"],
            "a"
        );
        assert_eq!(
            camel["desktop"]["controlLayout"]["groups"]["controls-header"][0],
            "control_x"
        );
        assert_eq!(camel["desktop"]["controlCentre"]["nightLight"], true);
        assert_eq!(camel["desktop"]["layout"], Value::Null);
        assert_eq!(camel["privacy"]["publicIpLookup"], false);
        assert_eq!(camel["privacy"]["showPersonalImages"], true);
        let back = from_legacy(&camel);
        assert_eq!(back["desktop"]["dock_size"], 60);
        assert_eq!(back["desktop"]["control_centre"]["night_light"], true);
        assert_eq!(back["desktop"]["layout"], Value::Null);
    }
}
