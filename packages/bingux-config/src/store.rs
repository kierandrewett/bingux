//! Reading and writing `bingux.toml`.
//!
//! Reads parse the file into a loose tree, merge defaults and validate.
//! Writes edit the existing document with `toml_edit`, so comments, key
//! order and formatting that the user added by hand survive graphical edits.
//! Every write is atomic (temporary file, fsync, rename) under an exclusive
//! `config.lock` so the shell and the Settings app never interleave.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item};

use crate::schema::Config;
use crate::{Error, legacy, validate};

/// Directory that holds `bingux.toml`, honouring `XDG_CONFIG_HOME`.
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("bingux")
}

/// Path of the configuration file.
pub fn config_path() -> PathBuf {
    config_dir().join("bingux.toml")
}

fn legacy_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Default tree in snake_case.
pub fn defaults_tree() -> Value {
    serde_json::to_value(Config::default()).expect("defaults serialise")
}

/// Every declared key, including optional ones, for shape checks.
fn schema_tree() -> Value {
    serde_json::to_value(Config::with_all_keys()).expect("schema serialises")
}

/// Exclusive advisory lock on `config.lock` for the lifetime of the guard.
pub struct Lock {
    _file: File,
}

impl Lock {
    pub fn acquire() -> Result<Self, Error> {
        let dir = config_dir();
        fs::create_dir_all(&dir).map_err(|source| Error::io(&dir, source))?;
        let path = dir.join("config.lock");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&path)
            .map_err(|source| Error::io(&path, source))?;
        // SAFETY: flock on an open descriptor we own; it has no memory effects.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(Error::io(&path, std::io::Error::last_os_error()));
        }
        Ok(Self { _file: file })
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // SAFETY: releasing a lock on a descriptor we still own.
        unsafe {
            libc::flock(self._file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

fn toml_to_json(value: toml::Value) -> Value {
    serde_json::to_value(value).expect("toml values map to json")
}

fn parse_document(text: &str, path: &Path) -> Result<DocumentMut, Error> {
    text.parse::<DocumentMut>().map_err(|error| Error::Parse {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

fn tree_from_text(text: &str, path: &Path) -> Result<Value, Error> {
    let table: toml::Table = toml::from_str(text).map_err(|error| Error::Parse {
        path: path.display().to_string(),
        message: error.message().to_string(),
    })?;
    Ok(toml_to_json(toml::Value::Table(table)))
}

/// Overlay `tree` on the defaults. Only top-level sections and their direct
/// keys merge; nested tables replace the default as a whole, like the old
/// helper's `DEFAULTS[key] | data[key]`.
fn merge_defaults(tree: &Value) -> Value {
    let mut merged = defaults_tree();
    if let (Value::Object(target), Value::Object(source)) = (&mut merged, tree) {
        for (section, values) in source {
            if let (Some(Value::Object(dest)), Value::Object(values)) =
                (target.get_mut(section), values)
            {
                for (key, value) in values {
                    dest.insert(key.clone(), value.clone());
                }
            }
        }
    }
    merged
}

/// Convert a legacy `settings.json` into the snake_case tree, dropping
/// explicit nulls so they mean "absent" in TOML.
fn tree_from_legacy_json(value: &Value) -> Value {
    fn strip_nulls(value: &Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .filter(|(_, v)| !v.is_null())
                    .map(|(k, v)| (k.clone(), strip_nulls(v)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(strip_nulls).collect()),
            other => other.clone(),
        }
    }
    strip_nulls(&legacy::from_legacy(value))
}

/// Validated configuration plus the tree it came from.
pub struct Loaded {
    pub config: Config,
    pub tree: Value,
}

fn read_tree(path: &Path) -> Result<Value, Error> {
    match fs::read_to_string(path) {
        Ok(text) => tree_from_text(&text, path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(source) => Err(Error::io(path, source)),
    }
}

fn finish(tree: Value) -> Result<Loaded, Error> {
    validate::check_shape(&tree, &schema_tree())?;
    let merged = merge_defaults(&tree);
    validate::validate(&merged, None)?;
    let config = serde_json::from_value(merged.clone()).map_err(|error| Error::Invalid {
        path: String::new(),
        message: error.to_string(),
    })?;
    Ok(Loaded {
        config,
        tree: merged,
    })
}

/// Load the configuration, migrating `settings.json` on first use.
pub fn load() -> Result<Loaded, Error> {
    let path = config_path();
    if !path.exists() && legacy_path().is_file() {
        let _lock = Lock::acquire()?;
        if !path.exists() {
            migrate_legacy(&path)?;
        }
    }
    finish(read_tree(&path)?)
}

fn migrate_legacy(path: &Path) -> Result<(), Error> {
    let old = legacy_path();
    let text = fs::read_to_string(&old).map_err(|source| Error::io(&old, source))?;
    let value: Value = serde_json::from_str(&text).map_err(|error| Error::Parse {
        path: old.display().to_string(),
        message: error.to_string(),
    })?;
    let tree = tree_from_legacy_json(&value);
    // Validate before writing so a broken legacy file is reported, not copied.
    finish(tree.clone())?;
    let mut document = DocumentMut::new();
    document.decor_mut().set_prefix("# Bingux configuration. Migrated from settings.json.\n# See docs/configuration.md or run `bingux-config defaults` for every key.\n\n");
    apply_tree(&mut document, &tree);
    write_atomic(path, &document.to_string())?;
    let migrated = old.with_extension("json.migrated");
    fs::rename(&old, &migrated).map_err(|source| Error::io(&old, source))
}

/// Write `document` to `path` atomically with mode 0600.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), Error> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|source| Error::io(dir, source))?;
    let temporary = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|source| Error::io(path, source))
}

fn json_to_item(value: &Value) -> Item {
    match value {
        Value::Null => Item::None,
        Value::Bool(b) => toml_edit::value(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml_edit::value(i)
            } else {
                toml_edit::value(n.as_f64().unwrap_or_default())
            }
        }
        Value::String(s) => toml_edit::value(s.as_str()),
        Value::Array(items) => {
            if !items.is_empty() && items.iter().all(Value::is_object) {
                let mut tables = toml_edit::ArrayOfTables::new();
                for item in items {
                    if let Item::Table(table) = json_to_item(item) {
                        tables.push(table);
                    }
                }
                Item::ArrayOfTables(tables)
            } else {
                let mut array = toml_edit::Array::new();
                for item in items {
                    if let Some(value) = json_to_item(item).into_value().ok() {
                        array.push(value);
                    }
                }
                Item::Value(toml_edit::Value::Array(array))
            }
        }
        Value::Object(map) => {
            let mut table = toml_edit::Table::new();
            table.set_implicit(false);
            for (key, value) in map {
                let item = json_to_item(value);
                if !item.is_none() {
                    table[key] = item;
                }
            }
            Item::Table(table)
        }
    }
}

/// Replace `section.key` in the document, keeping the comment above the key.
fn set_key(document: &mut DocumentMut, section: &str, key: &str, value: &Value) {
    if !document.contains_table(section) {
        document[section] = Item::Table(toml_edit::Table::new());
    }
    let Some(table) = document[section].as_table_mut() else {
        return;
    };
    let prefix = table.key(key).and_then(|k| {
        k.leaf_decor()
            .prefix()
            .and_then(|p| p.as_str())
            .map(str::to_string)
    });
    let suffix = table.get(key).and_then(Item::as_value).and_then(|v| {
        v.decor()
            .suffix()
            .and_then(|s| s.as_str())
            .map(str::to_string)
    });
    if value.is_null() {
        table.remove(key);
        return;
    }
    let mut item = json_to_item(value);
    if let (Some(suffix), Some(new_value)) = (suffix, item.as_value_mut()) {
        new_value.decor_mut().set_suffix(suffix);
    }
    table[key] = item;
    if let (Some(prefix), Some(mut k)) = (prefix, table.key_mut(key)) {
        k.leaf_decor_mut().set_prefix(prefix);
    }
}

/// Write every key of `tree` into the document.
fn apply_tree(document: &mut DocumentMut, tree: &Value) {
    let Value::Object(sections) = tree else {
        return;
    };
    for (section, values) in sections {
        let Value::Object(values) = values else {
            continue;
        };
        for (key, value) in values {
            set_key(document, section, key, value);
        }
    }
}

fn document_at(path: &Path) -> Result<DocumentMut, Error> {
    match fs::read_to_string(path) {
        Ok(text) => parse_document(&text, path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut document = DocumentMut::new();
            document.decor_mut().set_prefix(
                "# Bingux configuration.\n# See docs/configuration.md or run `bingux-config defaults` for every key.\n\n",
            );
            Ok(document)
        }
        Err(source) => Err(Error::io(path, source)),
    }
}

/// Snake_case patch (sections to key maps). `null` removes a key.
fn write_patch(patch: &Value) -> Result<Loaded, Error> {
    let _lock = Lock::acquire()?;
    let path = config_path();
    if !path.exists() && legacy_path().is_file() {
        migrate_legacy(&path)?;
    }
    let current = finish(read_tree(&path)?)?;
    validate::check_shape(patch, &schema_tree())?;
    let mut candidate = current.tree.clone();
    if let (Value::Object(target), Value::Object(source)) = (&mut candidate, patch) {
        for (section, values) in source {
            if let (Some(Value::Object(dest)), Value::Object(values)) =
                (target.get_mut(section), values)
            {
                for (key, value) in values {
                    if value.is_null() {
                        dest.insert(key.clone(), defaults_tree()[section][key].clone());
                    } else {
                        dest.insert(key.clone(), value.clone());
                    }
                }
            }
        }
    }
    let executable = validate::validate(&candidate, Some(&current.tree))?;
    let mut patch = patch.clone();
    if let Some(executable) = executable
        && let Some(ai) = patch
            .pointer_mut("/search/ai")
            .and_then(Value::as_object_mut)
    {
        ai.insert("executable".into(), Value::String(executable));
    }
    let mut document = document_at(&path)?;
    apply_tree(&mut document, &patch);
    write_atomic(&path, &document.to_string())?;
    finish(read_tree(&path)?)
}

/// Apply a partial update in the shell's camelCase JSON shape.
pub fn save_patch(patch: &Value) -> Result<Loaded, Error> {
    write_patch(&legacy::from_legacy(patch))
}

/// Apply a partial update in the snake_case shape.
pub fn save_tree_patch(patch: &Value) -> Result<Loaded, Error> {
    write_patch(patch)
}

/// Set one dotted key such as `desktop.dock_size`.
pub fn set_value(key: &str, value: Value) -> Result<Loaded, Error> {
    let Some((section, name)) = key.split_once('.') else {
        return Err(Error::Invalid {
            path: key.to_string(),
            message: "Use SECTION.KEY, for example desktop.dock_size.".into(),
        });
    };
    let mut patch = Map::new();
    let mut values = Map::new();
    values.insert(name.to_string(), value);
    patch.insert(section.to_string(), Value::Object(values));
    write_patch(&Value::Object(patch))
}

/// Read one dotted key from the merged tree.
pub fn get_value(key: &str) -> Result<Value, Error> {
    let loaded = load()?;
    let lookup = |tree: &Value| {
        let mut current = tree.clone();
        for part in key.split('.') {
            current = current.get(part).cloned()?;
        }
        Some(current)
    };
    if let Some(value) = lookup(&loaded.tree) {
        return Ok(value);
    }
    // Declared optional keys read as null when the file does not set them.
    if lookup(&schema_tree()).is_some() {
        return Ok(Value::Null);
    }
    Err(Error::Invalid {
        path: key.to_string(),
        message: "Unknown setting.".into(),
    })
}

/// Import the resolved runtime layout once (camelCase snapshot from the shell).
pub fn import_layout(snapshot: &Value) -> Result<Loaded, Error> {
    let _lock = Lock::acquire()?;
    let path = config_path();
    if !path.exists() && legacy_path().is_file() {
        migrate_legacy(&path)?;
    }
    let current = finish(read_tree(&path)?)?;
    let desktop = current.tree["desktop"]
        .as_object()
        .expect("desktop section");
    let already_imported = desktop["layout_version"].as_i64() == Some(1);
    if already_imported && !desktop["control_layout"].is_null() {
        return Ok(current);
    }
    let ready = snapshot.get("version").and_then(Value::as_i64) == Some(1)
        && snapshot.get("controlCentreReady").and_then(Value::as_bool) == Some(true);
    if !ready {
        return Err(Error::Invalid {
            path: "desktop.layout".into(),
            message: "Wait for the current desktop layout to finish loading.".into(),
        });
    }
    let snapshot = legacy::from_legacy(snapshot);
    let mut patch = Map::new();
    if !already_imported {
        patch.insert("layout_version".into(), Value::from(1));
        patch.insert(
            "layout".into(),
            if desktop["layout"].is_null() {
                snapshot["layout"].clone()
            } else {
                desktop["layout"].clone()
            },
        );
        let edge = desktop["sidebar_edge"]
            .as_str()
            .map(Value::from)
            .unwrap_or_else(|| snapshot["sidebar"]["edge"].clone());
        if !edge.is_null() {
            patch.insert("sidebar_edge".into(), edge);
        }
        let mut dock_apps = Map::new();
        for key in ["pinned_apps", "order"] {
            dock_apps.insert(key.into(), snapshot["dock"][key].clone());
        }
        patch.insert("dock_apps".into(), Value::Object(dock_apps));
        patch.insert("control_centre".into(), snapshot["control_centre"].clone());
    }
    let layout = snapshot
        .get("control_layout")
        .cloned()
        .unwrap_or_else(native_control_layout);
    patch.insert("control_layout".into(), layout);
    let backup = config_dir().join(if already_imported {
        "control-layout-before-import.json"
    } else {
        "layout-before-import.json"
    });
    if !backup.exists() {
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "settings": legacy::to_legacy(&current.tree),
            "runtime": legacy::to_legacy(&snapshot),
        }))
        .expect("backup serialises");
        write_atomic(&backup, &(text + "\n"))?;
    }
    drop(_lock);
    let mut full = Map::new();
    full.insert("desktop".into(), Value::Object(patch));
    write_patch(&Value::Object(full))
}

/// The built-in Control Centre grouping.
pub fn native_control_layout() -> Value {
    let groups: Map<String, Value> = validate::CONTROL_GROUPS
        .iter()
        .map(|(name, members)| ((*name).to_string(), serde_json::json!(members)))
        .collect();
    serde_json::json!({"version": 1, "groups": groups})
}

/// Fully commented TOML with every key at its default.
pub fn defaults_document() -> String {
    let defaults = defaults_tree();
    let mut document = DocumentMut::new();
    document.decor_mut().set_prefix(
        "# Bingux configuration: every key at its default value.\n# Copy this file to ~/.config/bingux/bingux.toml and edit the keys you need.\n# Nested tables such as [desktop.dock_apps] are written by the shell.\n\n",
    );
    for doc in crate::schema::KEY_DOCS {
        let value = defaults[doc.section]
            .get(doc.key)
            .cloned()
            .unwrap_or(Value::Null);
        if value.is_null() {
            // Optional key: show it commented out with an example value.
            let example = match doc.key {
                "sidebar_edge" => "\"left\"",
                "file_roots" => "[\"/home/user/Documents\"]",
                _ => "\"\"",
            };
            if !document.contains_table(doc.section) {
                document[doc.section] = Item::Table(toml_edit::Table::new());
            }
            let table = document[doc.section].as_table_mut().expect("table");
            let marker = format!("# {}\n# {} = {}\n", doc.accepted, doc.key, example);
            let existing = table
                .decor()
                .suffix()
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string();
            table.decor_mut().set_suffix(existing + "\n" + &marker);
            continue;
        }
        set_key(&mut document, doc.section, doc.key, &value);
        if let Some(mut key) = document[doc.section]
            .as_table_mut()
            .and_then(|t| t.key_mut(doc.key))
        {
            key.leaf_decor_mut()
                .set_prefix(format!("# {}\n", doc.accepted));
        }
    }
    if let Some(engines) = defaults["search"].get("engines") {
        set_key(&mut document, "search", "engines", engines);
    }
    document.to_string()
}
