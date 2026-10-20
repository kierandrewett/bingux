//! `bingux-config`: read, change and check `~/.config/bingux/bingux.toml`.
//!
//! The human commands print plain text and explain problems on stderr. The
//! shell protocol commands (`dump`, `save`, `import-layout`, `wallpaper`)
//! print one JSON object and `{"error": ...}` with status 1 on failure.

use std::io::{IsTerminal, Read, Write};
use std::process::{Command, ExitCode, Stdio};

use bingux_config::{Error, Loaded};
use serde_json::{Value, json};

const USAGE: &str = "bingux-config: configure the Bingux desktop shell

Usage: bingux-config COMMAND [ARGS]

The configuration lives in ~/.config/bingux/bingux.toml. Edit it by hand or
with these commands; both go through the same validation.

Commands:
  show                 Print the effective configuration as TOML
  get KEY              Print one value, for example desktop.dock_size
  set KEY VALUE        Change one value and confirm the result
  unset KEY            Remove a key so its default applies again
  explain KEY          Describe a key: accepted values, default, current value
  check                Validate the file and point at the first problem
  edit                 Open the file in $VISUAL or $EDITOR, then validate it
  path                 Print the configuration file path
  defaults             Print a commented file with every key at its default

Shell protocol (JSON on stdout, used by the shell and the Settings app):
  dump                 Merged configuration in the shell's JSON shape
  save                 Apply a JSON patch read from stdin
  import-layout        Import the runtime layout snapshot read from stdin once
  wallpaper            Resolve the GNOME background to {\"wallpaper\": \"file://...\"}

Examples:
  bingux-config set desktop.dock_size 64
  bingux-config set search.disabled_providers '[\"web\"]'
  bingux-config explain desktop.dock_click
  bingux-config edit
";

const MAX_SAVE_BYTES: usize = 32768;
const MAX_IMPORT_BYTES: usize = 131072;

fn read_stdin(limit: usize, too_large: &str) -> Result<Value, Error> {
    let mut buffer = Vec::with_capacity(4096);
    std::io::stdin()
        .lock()
        .take(limit as u64 + 1)
        .read_to_end(&mut buffer)
        .map_err(|e| Error::Io {
            path: "stdin".into(),
            message: e.to_string(),
        })?;
    if buffer.len() > limit {
        return Err(Error::Invalid {
            path: String::new(),
            message: too_large.into(),
        });
    }
    serde_json::from_slice(&buffer).map_err(|e| Error::Parse {
        path: "stdin".into(),
        message: e.to_string(),
    })
}

fn print_json(value: &Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut stdout, value);
    let _ = stdout.write_all(b"\n");
}

fn harnesses() -> Value {
    json!(
        ["pi", "claude"]
            .iter()
            .map(|name| json!({"id": name, "path": bingux_config::validate::which(name).unwrap_or_default()}))
            .collect::<Vec<_>>()
    )
}

fn dump(loaded: &Loaded) -> Value {
    json!({
        "data": bingux_config::legacy_json(loaded),
        "desktopDefaults": bingux_config::legacy_desktop_defaults(),
        "harnesses": harnesses(),
    })
}

fn restart_search_service() -> bool {
    Command::new("systemctl")
        .args(["--user", "try-restart", "bingux-searchd.service"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn json_command(command: &str) -> Result<Value, Error> {
    match command {
        "dump" | "read" => Ok(dump(&bingux_config::load()?)),
        "save" => {
            let patch = read_stdin(MAX_SAVE_BYTES, "Settings are too large.")?;
            let before = bingux_config::load()?;
            let after = bingux_config::save_patch(&patch)?;
            let mut warning = "";
            if before.config.search != after.config.search && !restart_search_service() {
                warning = "Saved. Restart the search service to apply search settings.";
            }
            Ok(json!({"data": bingux_config::legacy_json(&after), "warning": warning}))
        }
        "import-layout" => {
            let snapshot = read_stdin(MAX_IMPORT_BYTES, "Layout snapshot is too large.")?;
            let loaded = bingux_config::import_layout(&snapshot)?;
            Ok(json!({"data": bingux_config::legacy_json(&loaded)}))
        }
        "wallpaper" => Ok(json!({"wallpaper": bingux_config::wallpaper_uri()?})),
        _ => unreachable!("json_command only receives JSON commands"),
    }
}

/// Parse a CLI value as TOML; anything that is not valid TOML is a string.
fn parse_value(text: &str) -> Value {
    let candidate = format!("value = {text}");
    match toml::from_str::<toml::Table>(&candidate) {
        Ok(table) => {
            serde_json::to_value(&table["value"]).unwrap_or_else(|_| Value::String(text.into()))
        }
        Err(_) => Value::String(text.into()),
    }
}

/// Render a value the way it would appear in the TOML file.
fn render_toml(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => format!("{s:?}"),
        other => toml::to_string(&json!({"v": other}))
            .ok()
            .and_then(|text| text.trim().strip_prefix("v = ").map(str::to_string))
            .unwrap_or_else(|| other.to_string()),
    }
}

fn print_value(value: &Value) {
    match value {
        Value::Null => println!(),
        Value::String(text) => println!("{text}"),
        Value::Object(_) => println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        ),
        Value::Array(items) if items.iter().any(Value::is_object) => {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            )
        }
        other => println!("{}", render_toml(other)),
    }
}

fn explain(key: &str) -> Result<ExitCode, Error> {
    let Some((section, name)) = key.split_once('.') else {
        return Err(Error::Invalid {
            path: key.to_string(),
            message: "Use SECTION.KEY, for example desktop.dock_size.".into(),
        });
    };
    let current = bingux_config::get_value(key)?;
    let default = bingux_config::defaults_tree()[section]
        .get(name)
        .cloned()
        .unwrap_or(Value::Null);
    let doc = bingux_config::schema::KEY_DOCS
        .iter()
        .find(|doc| doc.section == section && doc.key == name);
    println!("{key}");
    match doc {
        Some(doc) => println!("  accepted: {}", doc.accepted),
        None => println!("  accepted: a table written by the shell; see docs/configuration.md"),
    }
    println!(
        "  default:  {}",
        if default.is_null() {
            "(not set)".to_string()
        } else {
            render_toml(&default)
        }
    );
    println!(
        "  current:  {}",
        if current.is_null() {
            "(not set)".to_string()
        } else {
            render_toml(&current)
        }
    );
    Ok(ExitCode::SUCCESS)
}

fn show() -> Result<ExitCode, Error> {
    let loaded = bingux_config::load()?;
    let text = toml::to_string_pretty(&loaded.config).map_err(|e| Error::Invalid {
        path: String::new(),
        message: e.to_string(),
    })?;
    println!(
        "# Effective configuration ({})",
        bingux_config::config_path().display()
    );
    print!("{text}");
    Ok(ExitCode::SUCCESS)
}

fn edit() -> Result<ExitCode, Error> {
    let path = bingux_config::config_path();
    if !path.exists() {
        // Start from the commented defaults so the editor shows every key.
        bingux_config::store::write_atomic(&path, &bingux_config::defaults_document())?;
    }
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|e| !e.is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|e| !e.is_empty()))
        .unwrap_or_else(|| "vi".into());
    let before = std::fs::read(&path).map_err(|e| Error::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("bingux-config-edit")
        .arg(&path)
        .status()
        .map_err(|e| Error::Command {
            program: editor.clone(),
            message: e.to_string(),
        })?;
    if !status.success() {
        return Err(Error::Command {
            program: editor,
            message: "exited with an error; the file was not checked".into(),
        });
    }
    match bingux_config::load() {
        Ok(_) => {
            println!("{}: ok", path.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(error) => {
            // Keep the broken edit next to the last good file so nothing is lost.
            let broken = path.with_extension("toml.rejected");
            let _ = std::fs::rename(&path, &broken);
            bingux_config::store::write_atomic(&path, &String::from_utf8_lossy(&before))?;
            eprintln!(
                "bingux-config: {error}\nThe edit was saved to {} and the previous file was restored.",
                broken.display()
            );
            Ok(ExitCode::FAILURE)
        }
    }
}

fn human_command(command: &str, args: &[String]) -> Result<ExitCode, Error> {
    match (command, args) {
        ("show", []) => show(),
        ("get", [key]) => {
            print_value(&bingux_config::get_value(key)?);
            Ok(ExitCode::SUCCESS)
        }
        ("set", [key, value]) => {
            let loaded = bingux_config::set_value(key, parse_value(value))?;
            let (section, name) = key.split_once('.').unwrap_or((key, ""));
            let stored = loaded.tree[section]
                .get(name)
                .cloned()
                .unwrap_or(Value::Null);
            println!("{key} = {}", render_toml(&stored));
            Ok(ExitCode::SUCCESS)
        }
        ("unset", [key]) => {
            bingux_config::set_value(key, Value::Null)?;
            let (section, name) = key.split_once('.').unwrap_or((key, ""));
            let default = bingux_config::defaults_tree()[section]
                .get(name)
                .cloned()
                .unwrap_or(Value::Null);
            if default.is_null() {
                println!("{key} removed");
            } else {
                println!("{key} = {} (default)", render_toml(&default));
            }
            Ok(ExitCode::SUCCESS)
        }
        ("explain", [key]) => explain(key),
        ("check", []) => match bingux_config::load() {
            Ok(_) => {
                println!("{}: ok", bingux_config::config_path().display());
                Ok(ExitCode::SUCCESS)
            }
            Err(error) => {
                eprintln!("{}: {error}", bingux_config::config_path().display());
                Ok(ExitCode::FAILURE)
            }
        },
        ("edit", []) => edit(),
        ("path", []) => {
            println!("{}", bingux_config::config_path().display());
            Ok(ExitCode::SUCCESS)
        }
        ("defaults", []) => {
            print!("{}", bingux_config::defaults_document());
            Ok(ExitCode::SUCCESS)
        }
        _ => {
            eprintln!("bingux-config: wrong arguments for `{command}`\n");
            eprint!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        if std::io::stdout().is_terminal() {
            print!("{USAGE}");
        } else {
            eprint!("{USAGE}");
        }
        return ExitCode::from(2);
    };
    match command {
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        "--version" | "-V" | "version" => {
            println!("bingux-config {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "dump" | "read" | "save" | "import-layout" | "wallpaper" if args.len() == 1 => {
            match json_command(command) {
                Ok(value) => {
                    print_json(&value);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    print_json(&json!({"error": error.message()}));
                    ExitCode::FAILURE
                }
            }
        }
        "show" | "get" | "set" | "unset" | "explain" | "check" | "edit" | "path" | "defaults" => {
            match human_command(command, &args[1..]) {
                Ok(code) => code,
                Err(error) => {
                    eprintln!("bingux-config: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("bingux-config: unknown command `{command}`\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
