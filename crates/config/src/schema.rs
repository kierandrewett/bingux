//! Declared configuration schema.
//!
//! These types are the single source of truth for key names, defaults and
//! documentation. The TOML file uses the snake_case names below. The shell
//! still consumes the older camelCase JSON shape, which `legacy` derives from
//! this schema so the two never drift.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Complete configuration with every default applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub search: Search,
    pub previews: Previews,
    pub desktop: Desktop,
    pub privacy: Privacy,
    pub appearance: Appearance,
}

impl Config {
    /// A configuration with every optional table present, used to learn
    /// the complete set of declared keys for shape checks.
    pub fn with_all_keys() -> Self {
        let mut config = Self::default();
        config.search.file_roots = Some(Vec::new());
        config.search.ai = Some(Ai::default());
        config.desktop.sidebar_edge = Some(String::new());
        config.desktop.dock_apps = Some(DockApps::default());
        config.desktop.layout = Some(BTreeMap::new());
        config.desktop.control_centre = Some(ControlCentre::default());
        config.desktop.control_order = Some(Vec::new());
        config.desktop.control_layout = Some(ControlLayout::default());
        config
    }
}

/// Search providers, website engines and the optional AI harness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Search {
    /// Provider identifiers that search skips.
    pub disabled_providers: Vec<String>,
    /// Identifier of the engine used when no shortcut is typed.
    pub default_engine: String,
    /// Absolute directories that file search indexes. Absent means the
    /// built-in home directory roots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_roots: Option<Vec<String>>,
    /// Website search engines, in display order.
    pub engines: Vec<Engine>,
    /// AI assistant harness. Absent disables the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai: Option<Ai>,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            disabled_providers: Vec::new(),
            default_engine: "duckduckgo".into(),
            file_roots: None,
            engines: vec![Engine {
                id: "duckduckgo".into(),
                name: "DuckDuckGo".into(),
                shortcut: "ddg".into(),
                url: "https://duckduckgo.com/?q={query}".into(),
                enabled: true,
            }],
            ai: None,
        }
    }
}

/// One website search engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Engine {
    pub id: String,
    pub name: String,
    pub shortcut: String,
    pub url: String,
    pub enabled: bool,
}

/// AI harness selection for the search assistant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Ai {
    /// `pi` or `claude`.
    pub harness: String,
    /// Absolute path of the harness executable. Filled from `PATH` when empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    /// Model name passed to the harness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// File preview behaviour in search results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Previews {
    pub enabled: bool,
    pub prewarm: bool,
    /// Largest file that is previewed, in megabytes (1 to 20).
    pub max_megabytes: i64,
}

impl Default for Previews {
    fn default() -> Self {
        Self {
            enabled: true,
            prewarm: true,
            max_megabytes: 20,
        }
    }
}

/// Desktop layout, dock behaviour and saved widget placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Desktop {
    pub dock: bool,
    pub sidebar: bool,
    pub metrics: bool,
    pub clock_seconds: bool,
    /// Absolute image path, or empty for no wallpaper.
    pub wallpaper: String,
    /// Screen edge for the sidebar. Absent lets the shell choose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar_edge: Option<String>,
    pub layout_version: i64,
    pub dock_size: i64,
    pub dock_alignment: String,
    pub dock_click: String,
    pub dock_middle_click: String,
    pub dock_scroll: String,
    pub dock_scroll_direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dock_apps: Option<DockApps>,
    pub dock_folders: Vec<DockFolder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<BTreeMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_centre: Option<ControlCentre>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_order: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_layout: Option<ControlLayout>,
    pub containers: BTreeMap<String, Container>,
    pub widget_options: BTreeMap<String, WidgetOptions>,
}

impl Default for Desktop {
    fn default() -> Self {
        Self {
            dock: true,
            sidebar: true,
            metrics: true,
            clock_seconds: false,
            wallpaper: String::new(),
            sidebar_edge: None,
            layout_version: 0,
            dock_size: 56,
            dock_alignment: "center".into(),
            dock_click: "toggle".into(),
            dock_middle_click: "launch".into(),
            dock_scroll: "cycle".into(),
            dock_scroll_direction: "natural".into(),
            dock_apps: None,
            dock_folders: Vec::new(),
            layout: None,
            control_centre: None,
            control_order: None,
            control_layout: None,
            containers: BTreeMap::new(),
            widget_options: BTreeMap::new(),
        }
    }
}

/// Pinned dock applications and their order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DockApps {
    pub pinned_apps: Vec<String>,
    pub order: Vec<String>,
}

/// A dock folder grouping several applications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockFolder {
    pub id: String,
    pub name: String,
    pub color: String,
    pub apps: Vec<String>,
}

/// Optional Control Centre tiles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ControlCentre {
    pub vpn: bool,
    pub dnd: bool,
    pub night_light: bool,
    pub power: bool,
    pub awake: bool,
}

/// Grouped Control Centre widget placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ControlLayout {
    pub version: i64,
    pub groups: BTreeMap<String, Vec<String>>,
}

/// Presentation of one widget container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Container {
    pub display: String,
}

/// Presentation overrides for one widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WidgetOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
}

/// How Bingux windows look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    /// Font family for the shell and Bingux applications. Empty means
    /// Adwaita Sans, or the fontconfig default sans-serif when that family
    /// is not installed.
    pub font_family: String,
}

/// Privacy choices: outbound network requests and what the login screen shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Privacy {
    /// Allow the Control Centre to look up the public address through a
    /// third-party web service. Off by default.
    pub public_ip_lookup: bool,
    /// Show this account's avatar and wallpaper on the login screen before
    /// anyone has signed in. The login screen is readable by every local
    /// account. On by default.
    pub show_personal_images: bool,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            public_ip_lookup: false,
            show_personal_images: true,
        }
    }
}

/// Documentation for one key, used by `bingux-config defaults` and the docs.
pub struct KeyDoc {
    pub section: &'static str,
    pub key: &'static str,
    pub accepted: &'static str,
}

/// Human descriptions of every top-level key. Nested tables such as
/// `desktop.dock_apps` are written by the shell and are described in the
/// configuration guide rather than in the generated file.
pub const KEY_DOCS: &[KeyDoc] = &[
    KeyDoc {
        section: "desktop",
        key: "dock",
        accepted: "true or false. Show the dock.",
    },
    KeyDoc {
        section: "desktop",
        key: "sidebar",
        accepted: "true or false. Show the edge sidebar.",
    },
    KeyDoc {
        section: "desktop",
        key: "metrics",
        accepted: "true or false. Show system monitors in the top bar.",
    },
    KeyDoc {
        section: "desktop",
        key: "clock_seconds",
        accepted: "true or false. Show seconds in the clock.",
    },
    KeyDoc {
        section: "desktop",
        key: "wallpaper",
        accepted: "Absolute image path, or \"\" for none.",
    },
    KeyDoc {
        section: "desktop",
        key: "sidebar_edge",
        accepted: "\"left\", \"right\" or \"top\". Remove the key to let the shell choose.",
    },
    KeyDoc {
        section: "desktop",
        key: "layout_version",
        accepted: "0 or 1. Managed by the shell.",
    },
    KeyDoc {
        section: "desktop",
        key: "dock_size",
        accepted: "Icon size in pixels, 32 to 80.",
    },
    KeyDoc {
        section: "desktop",
        key: "dock_alignment",
        accepted: "\"left\", \"center\" or \"right\".",
    },
    KeyDoc {
        section: "desktop",
        key: "dock_click",
        accepted: "\"toggle\", \"focus\" or \"launch\". Click on a running app.",
    },
    KeyDoc {
        section: "desktop",
        key: "dock_middle_click",
        accepted: "\"launch\", \"close\" or \"none\".",
    },
    KeyDoc {
        section: "desktop",
        key: "dock_scroll",
        accepted: "\"cycle\" or \"none\". Scroll over the dock.",
    },
    KeyDoc {
        section: "desktop",
        key: "dock_scroll_direction",
        accepted: "\"natural\" or \"reverse\".",
    },
    KeyDoc {
        section: "search",
        key: "disabled_providers",
        accepted: "Array of \"applications\", \"files\", \"calculation\", \"conversions\", \"web\", \"web-shortcuts\", \"external\".",
    },
    KeyDoc {
        section: "search",
        key: "default_engine",
        accepted: "Identifier of an enabled engine in [[search.engines]].",
    },
    KeyDoc {
        section: "search",
        key: "file_roots",
        accepted: "Array of up to 32 absolute directories. Remove the key for the defaults.",
    },
    KeyDoc {
        section: "search",
        key: "ai",
        accepted: "Table with harness (\"pi\" or \"claude\") and optional model and executable. Remove it to disable the assistant.",
    },
    KeyDoc {
        section: "previews",
        key: "enabled",
        accepted: "true or false. Preview files in search results.",
    },
    KeyDoc {
        section: "previews",
        key: "prewarm",
        accepted: "true or false. Prepare previews for nearby results.",
    },
    KeyDoc {
        section: "previews",
        key: "max_megabytes",
        accepted: "Largest previewed file, 1 to 20.",
    },
    KeyDoc {
        section: "appearance",
        key: "font_family",
        accepted: "Installed font family name, for example \"Cantarell\". \"\" uses Adwaita Sans, or the system sans-serif when that is missing.",
    },
    KeyDoc {
        section: "privacy",
        key: "public_ip_lookup",
        accepted: "true or false. Ask ipapi.co for the public address in the Control Centre.",
    },
    KeyDoc {
        section: "privacy",
        key: "show_personal_images",
        accepted: "true or false. Show your avatar and wallpaper on the login screen. Turn off to show a neutral screen instead.",
    },
];
