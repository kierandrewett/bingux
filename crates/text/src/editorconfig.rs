//! `.editorconfig`: the indentation, line endings and clean-up a project
//! asks for (<https://editorconfig.org>). The files are read from the
//! document's folder up to the root, or to a file that says `root = true`;
//! a nearer file wins.

use std::path::Path;

use crate::encoding::LineEnding;
use crate::indent::Indent;

/// What the `.editorconfig` files say about one file. `None` is "not said".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorConfig {
    pub indent: Option<Indent>,
    pub tab_width: Option<usize>,
    pub line_ending: Option<LineEnding>,
    /// `utf-8`, `utf-8-bom`, `latin1`, `utf-16le`, `utf-16be`.
    pub charset: Option<String>,
    pub trim_trailing_whitespace: Option<bool>,
    pub insert_final_newline: Option<bool>,
}

impl EditorConfig {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Reads the configuration for `path`. Reads files: call it off the UI
/// thread.
pub fn for_file(path: &Path) -> EditorConfig {
    let mut files = Vec::new();
    if path.file_name().is_none() {
        return EditorConfig::default();
    }
    let mut folder = path.parent();
    while let Some(dir) = folder {
        let candidate = dir.join(".editorconfig");
        if let Ok(text) = std::fs::read_to_string(&candidate) {
            let root = is_root(&text);
            files.push((dir.to_path_buf(), text));
            if root {
                break;
            }
        }
        folder = dir.parent();
    }
    let mut properties: Vec<(String, String)> = Vec::new();
    // Farthest first, so nearer files override.
    for (dir, text) in files.iter().rev() {
        let relative = path.strip_prefix(dir).unwrap_or(path);
        let relative = relative.to_string_lossy().replace('\\', "/");
        properties.extend(matching(&text_sections(text), &relative));
    }
    resolve(&properties)
}

fn is_root(text: &str) -> bool {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return false;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("root")
        {
            return value.trim().eq_ignore_ascii_case("true");
        }
    }
    false
}

/// The sections of a file: each glob and its `key = value` pairs.
fn text_sections(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(glob) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            sections.push((glob.to_owned(), Vec::new()));
            continue;
        }
        if let (Some((key, value)), Some(section)) = (line.split_once('='), sections.last_mut()) {
            section
                .1
                .push((key.trim().to_lowercase(), value.trim().to_lowercase()));
        }
    }
    sections
}

fn matching(sections: &[(String, Vec<(String, String)>)], relative: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (glob, pairs) in sections {
        if glob_matches(glob, relative) {
            out.extend(pairs.iter().cloned());
        }
    }
    out
}

fn resolve(properties: &[(String, String)]) -> EditorConfig {
    let mut config = EditorConfig::default();
    let mut style = None;
    let mut size = None;
    for (key, value) in properties {
        match key.as_str() {
            "indent_style" => style = Some(value.clone()),
            "indent_size" => size = Some(value.clone()),
            "tab_width" => {
                config.tab_width = value.parse().ok().filter(|width| (1..=16).contains(width))
            }
            "end_of_line" => {
                config.line_ending = match value.as_str() {
                    "lf" => Some(LineEnding::Lf),
                    "crlf" => Some(LineEnding::CrLf),
                    "cr" => Some(LineEnding::Cr),
                    _ => None,
                }
            }
            "charset" => config.charset = Some(value.clone()),
            "trim_trailing_whitespace" => config.trim_trailing_whitespace = parse_bool(value),
            "insert_final_newline" => config.insert_final_newline = parse_bool(value),
            _ => {}
        }
    }
    let width = size
        .as_deref()
        .and_then(|size| size.parse::<u8>().ok())
        .filter(|width| (1..=16).contains(width));
    config.indent = match style.as_deref() {
        Some("tab") => Some(Indent::Tabs),
        Some("space") => Some(Indent::Spaces(
            width
                .or(config.tab_width.map(|width| width as u8))
                .unwrap_or(4),
        )),
        _ => width.map(Indent::Spaces),
    };
    if config.tab_width.is_none() && size.as_deref() != Some("tab") {
        config.tab_width = width.map(usize::from);
    }
    config
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Whether an EditorConfig glob matches `path` (relative, `/`-separated).
/// A glob with no `/` matches the file name in any folder.
pub fn glob_matches(glob: &str, path: &str) -> bool {
    let (glob, path) = if glob.contains('/') {
        (glob.trim_start_matches('/').to_owned(), path.to_owned())
    } else {
        (format!("**/{glob}"), path.to_owned())
    };
    let pattern = glob_to_regex(&glob);
    regex::Regex::new(&pattern).is_ok_and(|regex| regex.is_match(&path))
}

fn glob_to_regex(glob: &str) -> String {
    let mut out = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut index = 0;
    let mut braces = 0;
    while index < chars.len() {
        let ch = chars[index];
        match ch {
            '*' if chars.get(index + 1) == Some(&'*') => {
                // `**/` matches no folder too.
                if chars.get(index + 2) == Some(&'/') {
                    out.push_str("(?:.*/)?");
                    index += 3;
                    continue;
                }
                out.push_str(".*");
                index += 2;
                continue;
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            '[' => {
                let close = chars[index..]
                    .iter()
                    .position(|c| *c == ']')
                    .map(|offset| index + offset);
                match close {
                    Some(close) => {
                        let inner: String = chars[index + 1..close].iter().collect();
                        let inner = inner
                            .strip_prefix('!')
                            .map_or(inner.clone(), |rest| format!("^{rest}"));
                        out.push('[');
                        out.push_str(&inner.replace('\\', "\\\\"));
                        out.push(']');
                        index = close + 1;
                        continue;
                    }
                    None => out.push_str("\\["),
                }
            }
            '{' => {
                braces += 1;
                out.push_str("(?:");
            }
            '}' if braces > 0 => {
                braces -= 1;
                out.push(')');
            }
            ',' if braces > 0 => out.push('|'),
            ch => out.push_str(&regex::escape(&ch.to_string())),
        }
        index += 1;
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_matches("*", "src/main.rs"));
        assert!(glob_matches("*.rs", "src/main.rs"));
        assert!(glob_matches("*.{js,ts}", "web/app.ts"));
        assert!(glob_matches("Makefile", "Makefile"));
        assert!(glob_matches("src/**.rs", "src/a/b.rs"));
        assert!(!glob_matches("src/*.rs", "src/a/b.rs"));
        assert!(glob_matches("[Mm]akefile", "makefile"));
    }

    #[test]
    fn sections_resolve_with_later_winning() {
        let text = "root = true\n[*]\nindent_style = space\nindent_size = 2\nend_of_line = lf\n[Makefile]\nindent_style = tab\n";
        let sections = text_sections(text);
        let general = resolve(&matching(&sections, "src/a.rs"));
        assert_eq!(general.indent, Some(Indent::Spaces(2)));
        assert_eq!(general.line_ending, Some(LineEnding::Lf));
        let make = resolve(&matching(&sections, "Makefile"));
        assert_eq!(make.indent, Some(Indent::Tabs));
        assert!(is_root(text));
    }

    #[test]
    fn reads_files_up_the_tree() {
        let dir = std::env::temp_dir().join(format!("bingux-text-ec-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            dir.join(".editorconfig"),
            "root = true\n[*]\nindent_size = 8\ntrim_trailing_whitespace = true\n",
        )
        .unwrap();
        std::fs::write(sub.join(".editorconfig"), "[*.py]\nindent_size = 4\n").unwrap();
        let config = for_file(&sub.join("a.py"));
        assert_eq!(config.indent, Some(Indent::Spaces(4)));
        assert_eq!(config.trim_trailing_whitespace, Some(true));
        let other = for_file(&sub.join("a.txt"));
        assert_eq!(other.indent, Some(Indent::Spaces(8)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
