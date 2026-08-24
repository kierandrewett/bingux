use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rusqlite::{Connection, OpenFlags, params};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use crate::config::SearchConfig;
use crate::matching::score_normalized;
use crate::protocol::{ProviderResult, ResultKind};

/// Upper bound for cached desktop entries, preventing an unbounded XDG scan from consuming memory.
const MAX_APPLICATION_INDEX_ENTRIES: usize = 4_096;
/// Upper bound for cached file entries, preventing an unbounded configured root from consuming memory.
const MAX_FILE_INDEX_ENTRIES: usize = 20_000;
const APPLICATION_INDEX_REFRESH_INTERVAL: Duration = Duration::from_secs(900);
const FILE_INDEX_REFRESH_INTERVAL: Duration = Duration::from_secs(300);
const MAX_DISPLAY_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub provider_id: String,
    pub result: ProviderResult,
    pub activation: Activation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activation {
    Spawn {
        program: String,
        arguments: Vec<String>,
    },
    Copy {
        text: String,
    },
    External {
        provider_id: String,
        result_id: String,
    },
    None,
}

pub struct LocalProviders {
    applications: Arc<RwLock<Vec<IndexedCandidate>>>,
    files: Arc<RwLock<Vec<IndexedCandidate>>>,
    sqlite_sources: Vec<SqliteSource>,
}

#[derive(Clone)]
struct IndexedCandidate {
    candidate: Candidate,
    normalized_search: String,
}

#[derive(Clone)]
struct SqliteSource {
    provider_id: String,
    display_name: String,
    database_path: PathBuf,
    query: String,
    activation_command: Vec<String>,
}

struct DesktopEntry {
    name: String,
    comment: String,
    icon: String,
}

impl LocalProviders {
    pub fn new(config: &SearchConfig) -> Result<Self> {
        let applications = Arc::new(RwLock::new(Vec::new()));
        let files = Arc::new(RwLock::new(Vec::new()));
        let application_directories = xdg_application_directories();
        let application_launcher = config.commands.application_launcher.clone();
        let file_roots = config.file_roots.clone();
        let file_opener = config.commands.file_opener.clone();

        start_index_worker(
            "bingux-search-app-index",
            Arc::clone(&applications),
            APPLICATION_INDEX_REFRESH_INTERVAL,
            move || index_applications(&application_directories, &application_launcher),
        )
        .context("could not start the application search index worker")?;
        start_index_worker(
            "bingux-search-file-index",
            Arc::clone(&files),
            FILE_INDEX_REFRESH_INTERVAL,
            move || index_files(&file_roots, &file_opener),
        )
        .context("could not start the file search index worker")?;

        Ok(Self {
            applications,
            files,
            sqlite_sources: config
                .sqlite_sources
                .iter()
                .map(|source| SqliteSource {
                    provider_id: source.id.clone(),
                    display_name: source.display_name.clone(),
                    database_path: source.database_path.clone(),
                    query: source.query.clone(),
                    activation_command: source.activation_command.clone(),
                })
                .collect(),
        })
    }

    pub fn query(&self, query: &str, limit: usize) -> Vec<Candidate> {
        if limit == 0 {
            return Vec::new();
        }

        let normalized_query = query.to_ascii_lowercase();
        let mut candidates = self
            .applications
            .read()
            .ok()
            .map(|applications| scored_index_candidates(&applications, &normalized_query, limit))
            .unwrap_or_default();
        candidates.extend(
            self.files
                .read()
                .ok()
                .map(|files| scored_index_candidates(&files, &normalized_query, limit))
                .unwrap_or_default(),
        );
        for source in &self.sqlite_sources {
            candidates.extend(query_sqlite_source(source, query, &normalized_query, limit));
        }
        if let Some(candidate) = calculation_candidate(query) {
            candidates.push(candidate);
        }
        rank_and_limit(&mut candidates, limit);
        candidates
    }
}

fn start_index_worker(
    name: &str,
    index: Arc<RwLock<Vec<IndexedCandidate>>>,
    refresh_interval: Duration,
    indexer: impl Fn() -> Vec<IndexedCandidate> + Send + 'static,
) -> Result<()> {
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            loop {
                let indexed = indexer();
                if let Ok(mut current) = index.write() {
                    *current = indexed;
                }
                thread::sleep(refresh_interval);
            }
        })?;
    Ok(())
}

fn scored_index_candidates(
    index: &[IndexedCandidate],
    normalized_query: &str,
    limit: usize,
) -> Vec<Candidate> {
    let mut selected = Vec::with_capacity(limit.min(index.len()));
    for indexed in index {
        let Some(score) = score_normalized(normalized_query, &indexed.normalized_search) else {
            continue;
        };
        if selected.len() < limit {
            selected.push((score, indexed));
            continue;
        }
        let worst_index = selected
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| {
                compare_scored_candidates(left.0, &left.1.candidate, right.0, &right.1.candidate)
            })
            .map(|(index, _)| index)
            .expect("non-empty selection when it has reached its limit");
        if compare_scored_candidates(
            score,
            &indexed.candidate,
            selected[worst_index].0,
            &selected[worst_index].1.candidate,
        )
        .is_lt()
        {
            selected[worst_index] = (score, indexed);
        }
    }
    selected.sort_by(|left, right| {
        compare_scored_candidates(left.0, &left.1.candidate, right.0, &right.1.candidate)
    });
    selected
        .into_iter()
        .filter_map(|(score, indexed)| {
            let mut candidate = indexed.candidate.clone();
            candidate.result.score = score;
            candidate.result.validate().ok()?;
            Some(candidate)
        })
        .collect()
}

fn compare_scored_candidates(
    left_score: f64,
    left: &Candidate,
    right_score: f64,
    right: &Candidate,
) -> std::cmp::Ordering {
    right_score
        .total_cmp(&left_score)
        .then_with(|| left.provider_id.cmp(&right.provider_id))
        .then_with(|| left.result.title.cmp(&right.result.title))
        .then_with(|| left.result.subtitle.cmp(&right.result.subtitle))
        .then_with(|| left.result.result_id.cmp(&right.result.result_id))
}

fn rank_and_limit(candidates: &mut Vec<Candidate>, limit: usize) {
    candidates.sort_by(|left, right| {
        compare_scored_candidates(left.result.score, left, right.result.score, right)
    });
    candidates.truncate(limit);
}

fn xdg_application_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        directories.push(PathBuf::from(data_home).join("applications"));
    } else if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        directories.push(PathBuf::from(home).join(".local/share/applications"));
    }

    let data_directories = env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .map(|value| {
            env::split_paths(&value)
                .map(|path| path.join("applications"))
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share/applications"),
                PathBuf::from("/usr/share/applications"),
            ]
        });
    for directory in data_directories {
        if !directories.contains(&directory) {
            directories.push(directory);
        }
    }
    directories
}

fn filtered_walk(root: &Path) -> ignore::Walk {
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .sort_by_file_name(|left, right| left.cmp(right));
    builder.build()
}

fn index_applications(
    directories: &[PathBuf],
    application_launcher: &[String],
) -> Vec<IndexedCandidate> {
    let mut applications = BTreeMap::new();

    'directories: for directory in directories {
        for entry in filtered_walk(directory).filter_map(std::result::Result::ok) {
            if applications.len() >= MAX_APPLICATION_INDEX_ENTRIES {
                break 'directories;
            }
            let Some(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") {
                continue;
            }
            let Some(relative_path) = path.strip_prefix(directory).ok() else {
                continue;
            };
            let Some(desktop_id) = desktop_id_from_relative_path(relative_path) else {
                continue;
            };
            if applications.contains_key(&desktop_id) {
                continue;
            }
            let Ok(contents) = fs::read_to_string(path) else {
                continue;
            };
            let Some(entry) = parse_desktop_entry(&contents) else {
                continue;
            };

            let candidate = Candidate {
                provider_id: "applications".to_owned(),
                result: ProviderResult {
                    result_id: desktop_id.clone(),
                    kind: ResultKind::Application,
                    title: entry.name,
                    subtitle: entry.comment,
                    icon: entry.icon,
                    score: 0.0,
                },
                activation: append_activation(application_launcher, &desktop_id),
            };
            if candidate.result.validate().is_ok() {
                let normalized_search = candidate.result.title.to_ascii_lowercase();
                applications.insert(
                    desktop_id,
                    IndexedCandidate {
                        candidate,
                        normalized_search,
                    },
                );
            }
        }
    }
    applications.into_values().collect()
}

fn parse_desktop_entry(contents: &str) -> Option<DesktopEntry> {
    let mut in_desktop_entry = false;
    let mut name = None;
    let mut comment = None;
    let mut icon = None;
    let mut exec = None;
    let mut hidden = false;
    let mut no_display = false;
    let mut entry_type = None;

    for line in contents.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with('[') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry || line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Name" => name = Some(value.trim().to_owned()),
            "Comment" => comment = Some(value.trim().to_owned()),
            "Icon" => icon = Some(value.trim().to_owned()),
            "Exec" => exec = Some(value.trim().to_owned()),
            "Hidden" => hidden = matches!(value.trim(), "true" | "TRUE"),
            "NoDisplay" => no_display = matches!(value.trim(), "true" | "TRUE"),
            "Type" => entry_type = Some(value.trim().to_owned()),
            _ => {}
        }
    }

    let name = name?;
    if hidden
        || no_display
        || entry_type.as_deref() != Some("Application")
        || name.is_empty()
        || exec?.is_empty()
        || !safe_display_text(&name)
    {
        return None;
    }
    Some(DesktopEntry {
        name,
        comment: comment
            .filter(|value| safe_display_text(value))
            .unwrap_or_default(),
        icon: icon
            .filter(|value| safe_display_text(value))
            .unwrap_or_default(),
    })
}

fn desktop_id_from_relative_path(path: &Path) -> Option<String> {
    let path = path.to_str()?;
    let stem = path.strip_suffix(".desktop")?;
    let id = stem.replace('/', "-") + ".desktop";
    let result = ProviderResult {
        result_id: id.clone(),
        kind: ResultKind::Application,
        title: String::new(),
        subtitle: String::new(),
        icon: String::new(),
        score: 0.0,
    };
    result.validate().ok()?;
    Some(id)
}

fn index_files(roots: &[PathBuf], file_opener: &[String]) -> Vec<IndexedCandidate> {
    let mut files = BTreeMap::new();
    let mut result_ids = BTreeSet::new();

    'roots: for root in roots {
        for entry in filtered_walk(root).filter_map(std::result::Result::ok) {
            if files.len() >= MAX_FILE_INDEX_ENTRIES {
                break 'roots;
            }
            let Some(file_type) = entry.file_type() else {
                continue;
            };
            if entry.depth() == 0 || file_type.is_symlink() {
                continue;
            }
            let kind = if file_type.is_dir() {
                ResultKind::Folder
            } else if file_type.is_file() {
                ResultKind::File
            } else {
                continue;
            };
            let path = entry.path();
            let Some(path_text) = path.to_str() else {
                continue;
            };
            let Some(title) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if title.is_empty() || !safe_display_text(title) || files.contains_key(path_text) {
                continue;
            }

            let result_id = path_result_id(kind, path_text, &mut result_ids);
            let candidate = Candidate {
                provider_id: "files".to_owned(),
                result: ProviderResult {
                    result_id,
                    kind,
                    title: title.to_owned(),
                    subtitle: path_text.to_owned(),
                    icon: match kind {
                        ResultKind::Folder => "folder".to_owned(),
                        ResultKind::File => "text-x-generic".to_owned(),
                        _ => unreachable!(),
                    },
                    score: 0.0,
                },
                activation: append_activation(file_opener, path_text),
            };
            if candidate.result.validate().is_ok() {
                let normalized_search = format!("{title} {path_text}").to_ascii_lowercase();
                files.insert(
                    path_text.to_owned(),
                    IndexedCandidate {
                        candidate,
                        normalized_search,
                    },
                );
            }
        }
    }
    files.into_values().collect()
}

fn path_result_id(kind: ResultKind, path: &str, used_ids: &mut BTreeSet<String>) -> String {
    let prefix = match kind {
        ResultKind::File => "file",
        ResultKind::Folder => "folder",
        _ => unreachable!(),
    };
    let base = format!("{prefix}:{:016x}", stable_hash(path.as_bytes()));
    let mut result_id = base.clone();
    let mut collision = 2usize;
    while used_ids.contains(&result_id) {
        result_id = format!("{base}:{collision}");
        collision += 1;
    }
    used_ids.insert(result_id.clone());
    result_id
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn query_sqlite_source(
    source: &SqliteSource,
    query: &str,
    normalized_query: &str,
    limit: usize,
) -> Vec<Candidate> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(connection) = Connection::open_with_flags(&source.database_path, flags) else {
        return Vec::new();
    };
    let Ok(mut statement) = connection.prepare(&source.query) else {
        return Vec::new();
    };
    let column_count = statement.column_count();
    if column_count < 2 {
        return Vec::new();
    }
    let sqlite_limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let Ok(mut rows) = statement.query(params![query, sqlite_limit]) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    while candidates.len() < limit {
        let Ok(Some(row)) = rows.next() else {
            break;
        };
        let (Ok(result_id), Ok(title)) = (row.get::<_, String>(0), row.get::<_, String>(1)) else {
            continue;
        };
        let subtitle = if column_count >= 3 {
            row.get::<_, Option<String>>(2)
                .ok()
                .flatten()
                .unwrap_or_default()
        } else {
            String::new()
        };
        let source_label = if safe_display_text(&source.display_name) {
            source.display_name.as_str()
        } else {
            source.provider_id.as_str()
        };
        let subtitle = if subtitle.is_empty() {
            source_label.to_owned()
        } else {
            format!("{source_label}: {subtitle}")
        };
        if result_id.is_empty()
            || title.is_empty()
            || !safe_display_text(&title)
            || !safe_display_text(&subtitle)
        {
            continue;
        }
        let normalized_candidate = format!("{title} {subtitle}").to_ascii_lowercase();
        let score = score_normalized(normalized_query, &normalized_candidate).unwrap_or(0.45);
        let candidate = Candidate {
            provider_id: source.provider_id.clone(),
            result: ProviderResult {
                result_id: result_id.clone(),
                kind: ResultKind::Database,
                title,
                subtitle,
                icon: "database".to_owned(),
                score,
            },
            activation: sqlite_activation(&source.activation_command, &result_id),
        };
        if candidate.result.validate().is_ok() {
            candidates.push(candidate);
        }
    }
    rank_and_limit(&mut candidates, limit);
    candidates
}

fn append_activation(command: &[String], argument: &str) -> Activation {
    let Some((program, configured_arguments)) = command.split_first() else {
        return Activation::None;
    };
    if !Path::new(program).is_absolute()
        || program.contains('\0')
        || configured_arguments
            .iter()
            .any(|entry| entry.is_empty() || entry.contains('\0'))
        || argument.is_empty()
        || argument.contains('\0')
    {
        return Activation::None;
    }
    let mut arguments = configured_arguments.to_vec();
    arguments.push(argument.to_owned());
    Activation::Spawn {
        program: program.clone(),
        arguments,
    }
}

fn sqlite_activation(command: &[String], result_id: &str) -> Activation {
    let Some((program, configured_arguments)) = command.split_first() else {
        return Activation::None;
    };
    if !Path::new(program).is_absolute()
        || program.contains('\0')
        || configured_arguments
            .iter()
            .any(|argument| argument.is_empty() || argument.contains('\0'))
    {
        return Activation::None;
    }
    Activation::Spawn {
        program: program.clone(),
        arguments: configured_arguments
            .iter()
            .map(|argument| {
                if argument == "{id}" {
                    result_id.to_owned()
                } else {
                    argument.clone()
                }
            })
            .collect(),
    }
}

fn calculation_candidate(query: &str) -> Option<Candidate> {
    let value = evaluate_calculation(query)?;
    let candidate = Candidate {
        provider_id: "calculation".to_owned(),
        result: ProviderResult {
            result_id: "calculation".to_owned(),
            kind: ResultKind::Calculation,
            title: value.clone(),
            subtitle: query.trim().to_owned(),
            icon: "accessories-calculator".to_owned(),
            score: 1.0,
        },
        activation: Activation::Copy { text: value },
    };
    candidate.result.validate().ok()?;
    Some(candidate)
}

fn evaluate_calculation(query: &str) -> Option<String> {
    let mut parser = ArithmeticParser::new(query);
    let value = parser.parse_expression()?;
    parser.skip_whitespace();
    if parser.position != parser.input.len() || !parser.explicit || !value.is_finite() {
        return None;
    }
    Some(if value == 0.0 {
        "0".to_owned()
    } else {
        value.to_string()
    })
}

struct ArithmeticParser<'a> {
    input: &'a [u8],
    position: usize,
    explicit: bool,
}

impl<'a> ArithmeticParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            position: 0,
            explicit: false,
        }
    }

    fn parse_expression(&mut self) -> Option<f64> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'+') => {
                    self.position += 1;
                    self.explicit = true;
                    value += self.parse_term()?;
                }
                Some(b'-') => {
                    self.position += 1;
                    self.explicit = true;
                    value -= self.parse_term()?;
                }
                _ => return Some(value),
            }
        }
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut value = self.parse_unary()?;
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b'*') => {
                    self.position += 1;
                    self.explicit = true;
                    value *= self.parse_unary()?;
                }
                Some(b'/') => {
                    self.position += 1;
                    self.explicit = true;
                    let divisor = self.parse_unary()?;
                    if divisor == 0.0 {
                        return None;
                    }
                    value /= divisor;
                }
                _ => return Some(value),
            }
        }
    }

    fn parse_unary(&mut self) -> Option<f64> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'+') => {
                self.position += 1;
                self.explicit = true;
                Some(self.parse_unary()?)
            }
            Some(b'-') => {
                self.position += 1;
                self.explicit = true;
                Some(-self.parse_unary()?)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        self.skip_whitespace();
        if self.peek() == Some(b'(') {
            self.position += 1;
            self.explicit = true;
            let value = self.parse_expression()?;
            self.skip_whitespace();
            if self.peek()? != b')' {
                return None;
            }
            self.position += 1;
            return Some(value);
        }
        self.parse_number()
    }

    fn parse_number(&mut self) -> Option<f64> {
        self.skip_whitespace();
        let start = self.position;
        let mut digits = 0usize;
        while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
            self.position += 1;
            digits += 1;
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.position += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return None;
        }
        std::str::from_utf8(&self.input[start..self.position])
            .ok()?
            .parse()
            .ok()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(byte) if byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }
}

fn safe_display_text(value: &str) -> bool {
    value.len() <= MAX_DISPLAY_BYTES && !value.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::{
        Activation, Candidate, append_activation, calculation_candidate,
        desktop_id_from_relative_path, evaluate_calculation, parse_desktop_entry, rank_and_limit,
        sqlite_activation,
    };
    use crate::protocol::{ProviderResult, ResultKind};
    use std::path::Path;

    fn candidate(provider_id: &str, result_id: &str, title: &str, score: f64) -> Candidate {
        Candidate {
            provider_id: provider_id.to_owned(),
            result: ProviderResult {
                result_id: result_id.to_owned(),
                kind: ResultKind::Action,
                title: title.to_owned(),
                subtitle: String::new(),
                icon: String::new(),
                score,
            },
            activation: Activation::None,
        }
    }

    #[test]
    fn calculates_parenthesized_decimal_expression() {
        let candidate = calculation_candidate("  -(1.5 + .5) * 2 ").expect("calculation");
        assert_eq!(candidate.result.title, "-4");
        assert_eq!(
            candidate.activation,
            Activation::Copy {
                text: "-4".to_owned()
            }
        );
        assert!(candidate.result.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_and_unsafe_calculations() {
        assert_eq!(evaluate_calculation("launch firefox"), None);
        assert_eq!(evaluate_calculation("1 / 0"), None);
        assert_eq!(evaluate_calculation("42"), None);
        assert_eq!(evaluate_calculation("(1 + 2"), None);
    }

    #[test]
    fn ranks_candidates_by_score_then_stable_fields() {
        let mut candidates = vec![
            candidate("files", "b", "Beta", 0.7),
            candidate("applications", "a", "Alpha", 0.7),
            candidate("files", "c", "Gamma", 0.9),
        ];
        rank_and_limit(&mut candidates, 3);
        let ids: Vec<_> = candidates
            .iter()
            .map(|candidate| candidate.result.result_id.as_str())
            .collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    #[test]
    fn parses_visible_desktop_entry_and_safe_activation_vector() {
        let entry = parse_desktop_entry(
            "[Desktop Entry]\nName=Editor\nComment=Write code\nIcon=editor\nExec=editor %F\nType=Application\n",
        )
        .expect("visible application");
        assert_eq!(entry.name, "Editor");
        assert!(
            parse_desktop_entry(
                "[Desktop Entry]\nName=Hidden\nExec=hidden\nType=Application\nHidden=true\n"
            )
            .is_none()
        );
        assert_eq!(
            desktop_id_from_relative_path(Path::new("tools/editor.desktop")),
            Some("tools-editor.desktop".to_owned())
        );
        assert_eq!(
            sqlite_activation(
                &[
                    "open-record".to_owned(),
                    "--id".to_owned(),
                    "{id}".to_owned()
                ],
                "record_7"
            ),
            Activation::None
        );
        assert_eq!(
            sqlite_activation(
                &[
                    "/nix/store/example/bin/open-record".to_owned(),
                    "--id".to_owned(),
                    "{id}".to_owned(),
                ],
                "record_7",
            ),
            Activation::Spawn {
                program: "/nix/store/example/bin/open-record".to_owned(),
                arguments: vec!["--id".to_owned(), "record_7".to_owned()],
            }
        );
    }

    #[test]
    fn appends_launch_targets_to_an_absolute_command_vector() {
        assert_eq!(
            append_activation(
                &[
                    "/nix/store/example/bin/gtk-launch".to_owned(),
                    "--verbose".to_owned()
                ],
                "org.example.Editor.desktop",
            ),
            Activation::Spawn {
                program: "/nix/store/example/bin/gtk-launch".to_owned(),
                arguments: vec![
                    "--verbose".to_owned(),
                    "org.example.Editor.desktop".to_owned()
                ],
            }
        );
    }
}
