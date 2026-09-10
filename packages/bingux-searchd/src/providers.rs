use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rusqlite::{Connection, OpenFlags, params, types::ValueRef};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::SearchConfig;
use crate::matching::SearchQuery;
use crate::protocol::{ProviderResult, ResultKind};

/// Upper bound for cached desktop entries, preventing an unbounded XDG scan from consuming memory.
const MAX_APPLICATION_INDEX_ENTRIES: usize = 4_096;
/// Upper bound for cached file entries, preventing an unbounded configured root from consuming memory.
const MAX_FILE_INDEX_ENTRIES: usize = 20_000;
/// Upper bound for all filesystem entries inspected during one application-index refresh.
const MAX_APPLICATION_WALK_ENTRIES: usize = 32_768;
/// Upper bound for all filesystem entries inspected during one file-index refresh.
const MAX_FILE_WALK_ENTRIES: usize = 131_072;
const APPLICATION_INDEX_REFRESH_INTERVAL: Duration = Duration::from_secs(900);
const FILE_INDEX_REFRESH_INTERVAL: Duration = Duration::from_secs(300);
const MAX_DESKTOP_FILE_BYTES: u64 = 64 * 1024;
const MAX_DISPLAY_BYTES: usize = 16 * 1024;
const MAX_RESULT_ID_BYTES: usize = 128;
const SQLITE_QUERY_TIMEOUT: Duration = Duration::from_millis(50);

fn excluded_search_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Some(name) = component.as_os_str().to_str() else { return false; };
        include_str!("../search-excluded-directories.txt").lines()
            .any(|excluded| name.eq_ignore_ascii_case(excluded))
    }) || path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension|
        matches!(extension.to_ascii_lowercase().as_str(), "o" | "obj" | "pyc" | "pyo" | "class" | "tsbuildinfo"))
}

/// Small, bounded preference within a relevance tier. Never infer authorship
/// from a path: deep paths and hash-like components are merely weaker signals.
fn simple_path_score(score: f64, path: &Path) -> f64 {
    let mut depth = 0usize;
    let mut generated = 0usize;
    for component in path.components() {
        let Some(name) = component.as_os_str().to_str() else { continue; };
        if name.is_empty() || name == "/" { continue; }
        depth += 1;
        let compact: String = name.chars().filter(|character| *character != '-').collect();
        if compact.len() >= 16 && compact.chars().all(|character| character.is_ascii_hexdigit()) {
            generated += 1;
        }
    }
    let penalty = (depth.saturating_sub(1) as f64 * 0.002
        + path.as_os_str().len() as f64 * 0.00001
        + generated as f64 * 0.004).min(0.035);
    score * (1.0 - penalty)
}

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
    Chat {
        prompt: String,
    },
    None,
}

pub struct LocalProviders {
    applications: Arc<RwLock<Vec<IndexedCandidate>>>,
    files: Arc<RwLock<Vec<IndexedCandidate>>>,
    sqlite_sources: Vec<SqliteSource>,
    ai_enabled: bool,
    disabled_providers: Vec<String>,
    engines: Vec<crate::search_engines::SearchEngine>,
    default_engine: String,
    web_opener: Vec<String>,
    os_index_roots: Vec<PathBuf>,
}

#[derive(Clone)]
struct IndexedCandidate {
    candidate: Candidate,
    normalized_search: String,
    normalized_title: String,
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
        Self::new_with_index_workers(config, true)
    }

    /// Constructs a local provider set without application and file indexes.
    pub fn new_without_index_workers(config: &SearchConfig) -> Result<Self> {
        Self::new_with_index_workers(config, false)
    }

    fn new_with_index_workers(config: &SearchConfig, start_index_workers: bool) -> Result<Self> {
        let applications = Arc::new(RwLock::new(Vec::new()));
        let files = Arc::new(RwLock::new(Vec::new()));
        if start_index_workers {
            let application_directories = xdg_application_directories();
            let application_launcher = config.commands.application_launcher.clone();
            let file_roots = config.file_roots.clone();
            let file_opener = config.commands.file_opener.clone();
            let progressive_files = Arc::clone(&files);

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
                move || index_files_with_progress(&file_roots, &file_opener, |snapshot| {
                    if let Ok(mut current) = progressive_files.write() {
                        // Retain the previous complete index during refresh;
                        // on startup publish useful partial results promptly.
                        if snapshot.len() > current.len() { *current = snapshot; }
                    }
                }),
            )
            .context("could not start the file search index worker")?;
        }

        Ok(Self {
            applications,
            files,
            ai_enabled: config.ai.is_some(),
            disabled_providers: config.disabled_providers.clone(),
            engines: config.engines.clone(),
            default_engine: config.default_engine.clone(),
            web_opener: config.commands.file_opener.clone(),
            os_index_roots: if start_index_workers {
                config.file_roots.clone()
            } else { Vec::new() },
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

        if query.trim().starts_with(['!', '?']) {
            return if self.ai_enabled { quick_chat_candidate(query).into_iter().collect() } else { Vec::new() };
        }
        let normalized_query = SearchQuery::parse(query);
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
        if !self.os_index_roots.is_empty() {
            candidates.extend(query_os_index(&normalized_query, &self.os_index_roots, &self.web_opener));
            // The OS index and fresh-file cache may discover the same path.
            let mut paths = BTreeSet::new();
            candidates.retain(|candidate| candidate.provider_id != "files" || paths.insert(candidate.result.subtitle.clone()));
        }
        for source in &self.sqlite_sources {
            candidates.extend(query_sqlite_source(source, query, &normalized_query, limit));
        }
        if self.ai_enabled {
            if let Some(candidate) = quick_chat_candidate(query) {
                candidates.push(candidate);
            }
        }
        if let Some(candidate) = calculation_candidate(query) {
            candidates.push(candidate);
        }
        if let Some(candidate) = conversion_candidate(query) { candidates.push(candidate); }
        if let Some(candidate) = specialised_web_candidate(query, &self.web_opener) { candidates.push(candidate); }
        if !self.disabled_providers.iter().any(|id| id == "web-shortcuts") {
            for engine in self.engines.iter().filter(|engine| engine.enabled) {
                if let Some(terms) = query.trim().strip_prefix(&format!("{}:", engine.shortcut)) {
                    if let Some(mut candidate) = engine_candidate(terms.trim(), &self.web_opener, engine) {
                        candidate.provider_id = "web-shortcuts".into();
                        candidate.result.result_id = engine.id.clone();
                        candidate.result.score = 1.0;
                        candidates.push(candidate);
                    }
                }
            }
        }
        candidates.retain(|candidate| !self.disabled_providers.contains(&candidate.provider_id));
        let web = if self.disabled_providers.iter().any(|p| p == "web") { None } else { self.engines.iter().find(|engine| engine.enabled && engine.id == self.default_engine)
            .and_then(|engine| engine_candidate(query, &self.web_opener, engine)) };
        // Leave one visible slot for Web even when local indexes are full.
        // A one-result request retains its highest-ranked local answer.
        let web_slots = usize::from(web.is_some() && (limit > 1 || candidates.is_empty()));
        rank_and_limit(&mut candidates, limit - web_slots);
        if web_slots > 0 {
            candidates.extend(web);
        }
        candidates
    }
}

/// Reuse the host's permission-filtered locate database. The helper has a
/// strict shared time budget, a bounded output count, and no shell evaluation.
/// Missing tools/databases or timed-out lookups simply leave cached results.
fn query_os_index(query: &SearchQuery, roots: &[PathBuf], opener: &[String]) -> Vec<Candidate> {
    let deadline = Instant::now() + Duration::from_millis(80);
    // A user-owned snapshot works under NoNewPrivileges without granting the
    // daemon access to the host's protected, setgid-readable database.
    let private_database = env::var_os("XDG_CACHE_HOME").map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|cache| cache.join("bingux/locate.db")).filter(|path| path.is_file());
    let mut paths = BTreeSet::new();
    for seed in query.index_seeds() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(5) { break; }
        let mut command = Command::new("timeout");
        command.args(["--signal=KILL", &format!("{}s", remaining.as_secs_f64()), "plocate", "-i", "-b", "-0", "-l", "256"]);
        if let Some(database) = &private_database { command.arg("-d").arg(database); }
        let Ok(output) = command.args(["--", &seed]).stdin(Stdio::null()).stderr(Stdio::null()).output() else { break; };
        for raw in output.stdout.split(|byte| *byte == 0) {
            let Ok(path) = std::str::from_utf8(raw) else { continue; };
            if path.is_empty() || path.len() > MAX_DISPLAY_BYTES { continue; }
            let path = PathBuf::from(path);
            if !roots.iter().any(|root| path.starts_with(root)) { continue; }
            if excluded_search_path(&path) || path.components().any(|part| part.as_os_str().to_str().is_some_and(|name| name.starts_with('.'))) { continue; }
            paths.insert(path);
        }
    }
    let mut candidates = Vec::new();
    let mut ids = BTreeSet::new();
    for path in paths {
        let Some(title) = path.file_name().and_then(|name| name.to_str()) else { continue; };
        let path_text = path.to_string_lossy();
        let Some(score) = query.score_fields(&title.to_lowercase(), &format!("{title} {path_text}").to_lowercase()) else { continue; };
        let Ok(metadata) = fs::metadata(&path) else { continue; };
        let kind = if metadata.is_dir() { ResultKind::Folder } else if metadata.is_file() { ResultKind::File } else { continue; };
        let result = ProviderResult {
            result_id: path_result_id(kind, &path_text, &mut ids),
            kind, title: title.to_owned(), subtitle: path_text.to_string(),
            icon: if kind == ResultKind::Folder { "folder".to_owned() } else { file_icon(&path).to_owned() }, score: simple_path_score(score, &path),
        };
        if result.validate().is_ok() {
            candidates.push(Candidate { provider_id: "files".to_owned(), result, activation: append_activation(opener, &path_text) });
        }
    }
    candidates
}

fn web_candidate(query: &str, opener: &[String]) -> Option<Candidate> {
    engine_candidate(query, opener, &crate::search_engines::defaults()[0])
}

fn engine_candidate(query: &str, opener: &[String], engine: &crate::search_engines::SearchEngine) -> Option<Candidate> {
    let query = query.trim();
    if query.is_empty() || opener.is_empty() { return None; }
    let candidate = Candidate {
        provider_id: "web".to_owned(),
        result: ProviderResult {
            result_id: "search".to_owned(), kind: ResultKind::Action,
            title: query.to_owned(), subtitle: engine.name.clone(),
            icon: if engine.id == "duckduckgo" { "duckduckgo" } else { "web-browser-symbolic" }.to_owned(),
            score: 0.1,
        },
        activation: append_activation(opener, &engine.search_url(query)),
    };
    candidate.result.validate().ok()?;
    Some(candidate)
}

fn quick_chat_candidate(query: &str) -> Option<Candidate> {
    let prompt = quick_chat_prompt(query)?;
    Some(Candidate {
        provider_id: "ai".to_owned(),
        result: ProviderResult {
            result_id: "quick-chat".to_owned(),
            kind: ResultKind::Chat,
            title: "Ask AI".to_owned(),
            subtitle: "Ask the configured assistant".to_owned(),
            icon: "dialog-question-symbolic".to_owned(),
            score: 1.0,
        },
        activation: Activation::Chat { prompt },
    })
}

fn quick_chat_prompt(query: &str) -> Option<String> {
    let prompt = query.trim().strip_prefix('!').or_else(|| query.trim().strip_prefix('?'))?.trim();
    (!prompt.is_empty()).then(|| prompt.to_owned())
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
    normalized_query: &SearchQuery,
    limit: usize,
) -> Vec<Candidate> {
    let mut selected = Vec::with_capacity(limit.min(index.len()));
    for indexed in index {
        let Some(score) = normalized_query.score_fields(&indexed.normalized_title, &indexed.normalized_search) else {
            continue;
        };
        let score = if matches!(indexed.candidate.result.kind, ResultKind::File | ResultKind::Folder) {
            simple_path_score(score, Path::new(&indexed.candidate.result.subtitle))
        } else { score };
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
    // Keep several sources visible instead of filling the launcher with one index.
    let mut counts = BTreeMap::new();
    candidates.retain(|candidate| {
        let count = counts.entry(candidate.provider_id.clone()).or_insert(0);
        *count += 1;
        *count <= 5
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
    let mut walked_entries = 0;

    'directories: for directory in directories {
        for entry in filtered_walk(directory) {
            walked_entries += 1;
            if walked_entries > MAX_APPLICATION_WALK_ENTRIES {
                break 'directories;
            }
            let Ok(entry) = entry else {
                continue;
            };
            if applications.len() >= MAX_APPLICATION_INDEX_ENTRIES {
                break 'directories;
            }
            let Some(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            let is_file = file_type.is_file() || (file_type.is_symlink() && path.is_file());
            if !is_file {
                continue;
            }
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
            let Some(contents) = read_bounded_text(path, MAX_DESKTOP_FILE_BYTES) else {
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
                let normalized_search = candidate.result.title.to_lowercase();
                applications.insert(
                    desktop_id,
                    IndexedCandidate {
                        candidate,
                        normalized_title: normalized_search.clone(),
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

fn index_files_with_progress(roots: &[PathBuf], file_opener: &[String], mut publish: impl FnMut(Vec<IndexedCandidate>)) -> Vec<IndexedCandidate> {
    let mut files = BTreeMap::new();
    let mut result_ids = BTreeSet::new();

    let mut walked_entries = 0;
    let mut file_entries = 0;
    let mut last_publish = Instant::now();
    // Seed every configured root before descending, then visit directories
    // breadth-first so one deep download tree cannot starve later roots.
    let mut pending: VecDeque<_> = roots.iter().cloned().map(|path| (path, true)).collect();
    'roots: while let Some((root, include_root)) = pending.pop_front() {
        if excluded_search_path(&root) { continue; }
        let mut builder = WalkBuilder::new(&root);
        builder.follow_links(false)
            .max_depth(Some(if include_root { 0 } else { 1 }))
            .sort_by_file_name(|left, right| left.cmp(right))
            .filter_entry(|entry| !excluded_search_path(entry.path()));
        for entry in builder.build() {
            walked_entries += 1;
            if walked_entries > MAX_FILE_WALK_ENTRIES {
                break 'roots;
            }
            let Ok(entry) = entry else {
                continue;
            };
            if files.len() >= MAX_FILE_INDEX_ENTRIES {
                break 'roots;
            }
            if entry.depth() == 0 && !include_root {
                continue;
            }
            let Some(mut file_type) = entry.file_type() else {
                continue;
            };
            let symlink = file_type.is_symlink();
            if symlink {
                let Ok(metadata) = fs::metadata(entry.path()) else { continue; };
                file_type = metadata.file_type();
            }
            let kind = if file_type.is_dir() {
                // Aliases are results, not recursive traversal edges. Explicit
                // configured symlink roots are the one permitted exception.
                if !symlink || include_root {
                    pending.push_back((entry.path().to_owned(), false));
                }
                ResultKind::Folder
            } else if file_type.is_file() {
                // Keep a quarter of the bounded index available for folders.
                if file_entries >= MAX_FILE_INDEX_ENTRIES * 3 / 4 { continue; }
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
                        ResultKind::File => file_icon(path).to_owned(),
                        _ => unreachable!(),
                    },
                    score: 0.0,
                },
                activation: append_activation(file_opener, path_text),
            };
            if candidate.result.validate().is_ok() {
                if kind == ResultKind::File { file_entries += 1; }
                let normalized_search = format!("{title} {path_text}").to_lowercase();
                files.insert(
                    path_text.to_owned(),
                    IndexedCandidate {
                        candidate,
                        normalized_title: title.to_lowercase(),
                        normalized_search,
                    },
                );
            }
        }
        if include_root || last_publish.elapsed() >= Duration::from_millis(250) {
            publish(files.values().cloned().collect());
            last_publish = Instant::now();
        }
    }
    files.into_values().collect()
}

fn file_icon(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "pdf" => "application-pdf",
        "doc" | "docx" | "odt" | "rtf" | "pages" => "x-office-document",
        "xls" | "xlsx" | "ods" | "csv" | "tsv" | "numbers" => "x-office-spreadsheet",
        "ppt" | "pptx" | "odp" | "key" => "x-office-presentation",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" | "heic" | "bmp" | "tif" | "tiff" | "ico" => "image-x-generic",
        "mp3" | "flac" | "wav" | "ogg" | "opus" | "m4a" | "aac" | "aiff" | "mid" | "midi" => "audio-x-generic",
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" | "mpeg" | "mpg" | "ogv" => "video-x-generic",
        "zip" | "tar" | "gz" | "bz2" | "xz" | "zst" | "7z" | "rar" | "tgz" | "deb" | "rpm" => "package-x-generic",
        "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "h" | "cpp" | "hpp" | "go" | "rb" | "java" | "kt" | "swift" | "sh" | "bash" | "zsh" | "qml" | "nix" | "css" | "scss" | "json" | "toml" | "yaml" | "yml" | "xml" | "sql" => "text-x-script",
        "html" | "htm" | "url" => "text-html",
        "ttf" | "otf" | "woff" | "woff2" => "font-x-generic",
        "iso" | "img" | "qcow2" | "vdi" | "vmdk" => "drive-harddisk",
        "desktop" | "appimage" | "exe" => "application-x-executable",
        _ => "text-x-generic",
    }
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
    normalized_query: &SearchQuery,
    limit: usize,
) -> Vec<Candidate> {
    let Ok(metadata) = fs::metadata(&source.database_path) else {
        return Vec::new();
    };
    if !metadata.is_file() {
        return Vec::new();
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(connection) = Connection::open_with_flags(&source.database_path, flags) else {
        return Vec::new();
    };
    let deadline = Instant::now() + SQLITE_QUERY_TIMEOUT;
    if connection
        .progress_handler(1_000, Some(move || Instant::now() >= deadline))
        .is_err()
    {
        return Vec::new();
    }
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
        let Some(result_id) = row
            .get_ref(0)
            .ok()
            .and_then(|value| bounded_sqlite_text(value, MAX_RESULT_ID_BYTES))
        else {
            continue;
        };
        let Some(title) = row
            .get_ref(1)
            .ok()
            .and_then(|value| bounded_sqlite_text(value, MAX_DISPLAY_BYTES))
        else {
            continue;
        };
        let subtitle = if column_count >= 3 {
            let Ok(value) = row.get_ref(2) else {
                continue;
            };
            let Some(subtitle) = bounded_sqlite_optional_text(value, MAX_DISPLAY_BYTES) else {
                continue;
            };
            subtitle
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
        let normalized_candidate = format!("{title} {subtitle}").to_lowercase();
        let Some(score) = normalized_query.score_fields(&title.to_lowercase(), &normalized_candidate) else {
            continue;
        };
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
fn read_bounded_text(path: &Path, max_bytes: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut contents = String::new();
    file.by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_string(&mut contents)
        .ok()?;
    (contents.len() as u64 <= max_bytes).then_some(contents)
}

fn bounded_sqlite_text(value: ValueRef<'_>, max_bytes: usize) -> Option<String> {
    let ValueRef::Text(bytes) = value else {
        return None;
    };
    if bytes.len() > max_bytes {
        return None;
    }
    String::from_utf8(bytes.to_vec()).ok()
}

fn bounded_sqlite_optional_text(value: ValueRef<'_>, max_bytes: usize) -> Option<String> {
    if matches!(value, ValueRef::Null) {
        Some(String::new())
    } else {
        bounded_sqlite_text(value, max_bytes)
    }
}

fn safe_display_text(value: &str) -> bool {
    value.len() <= MAX_DISPLAY_BYTES && !value.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::{
        Activation, Candidate, append_activation, bounded_sqlite_optional_text,
        bounded_sqlite_text, calculation_candidate, desktop_id_from_relative_path,
        evaluate_calculation, index_applications, parse_desktop_entry, quick_chat_candidate,
        rank_and_limit, sqlite_activation, web_candidate, file_icon,
    };
    use crate::protocol::{ProviderResult, ResultKind};
    use rusqlite::types::ValueRef;
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
    fn simpler_paths_rank_first_without_overriding_match_quality() {
        let simple = Path::new("/home/me/Documents/report.pdf");
        let deep = Path::new("/home/me/Downloads/archive/project/exports/0123456789abcdef/report.pdf");
        assert!(super::simple_path_score(1.0, simple) > super::simple_path_score(1.0, deep));
        assert!(super::simple_path_score(1.0, deep) > super::simple_path_score(0.94, simple));
        let index: Vec<_> = [deep, simple].into_iter().enumerate().map(|(i, path)| {
            let mut entry = candidate("files", &i.to_string(), "report.pdf", 0.0);
            entry.result.kind = ResultKind::File;
            entry.result.subtitle = path.to_str().unwrap().to_owned();
            super::IndexedCandidate { candidate: entry, normalized_title: "report.pdf".into(), normalized_search: format!("report.pdf {}", path.display()) }
        }).collect();
        let matches = super::scored_index_candidates(&index, &super::SearchQuery::parse("report.pdf"), 1);
        assert_eq!(matches[0].result.subtitle, simple.to_str().unwrap());
    }

    #[test]
    fn exact_project_folders_beat_deep_copies_and_partial_names_generically() {
        for name in ["garden", "Sketchbook", "sample-project"] {
            let simple = format!("/home/person/dev/{name}");
            let paths = [format!("/home/person/archive/by-path/home/person/dev/{name}"), format!("/home/person/{name}-backup"), simple.clone()];
            let index: Vec<_> = paths.iter().enumerate().map(|(i, path)| {
                let title = Path::new(path).file_name().unwrap().to_str().unwrap();
                let mut entry = candidate("files", &i.to_string(), title, 0.0);
                entry.result.kind = ResultKind::Folder;
                entry.result.subtitle = path.clone();
                super::IndexedCandidate { candidate: entry, normalized_title: title.to_lowercase(), normalized_search: format!("{title} {path}").to_lowercase() }
            }).collect();
            let matches = super::scored_index_candidates(&index, &super::SearchQuery::parse(&name.to_uppercase()), 3);
            assert_eq!(matches[0].result.subtitle, simple);
            assert!(matches[0].result.score > matches[1].result.score);
        }
    }

    #[test]
    fn excludes_build_trees_and_objects_but_keeps_source_and_personal_paths() {
        for directory in include_str!("../search-excluded-directories.txt").lines() {
            assert!(super::excluded_search_path(&Path::new("/home/me/project").join(directory.to_uppercase()).join("report.txt")));
        }
        for path in ["/home/me/project/main.o", "/home/me/project/Module.CLASS", "/home/me/build"] {
            assert!(super::excluded_search_path(Path::new(path)));
        }
        for path in ["/home/me/building plans/report.pdf", "/home/me/project/build.rs", "/home/me/project/src/main.rs", "/home/me/project/lib/report.txt", "/home/me/Documents/2026/report.pdf"] {
            assert!(!super::excluded_search_path(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn indexed_search_applies_boolean_phrases_exclusions_and_extensions_before_limit() {
        let index: Vec<_> = ["annual report.pdf", "annual report draft.pdf", "meeting notes.txt", "report annual.pdf"]
            .into_iter().enumerate().map(|(i, title)| super::IndexedCandidate {
                candidate: candidate("files", &i.to_string(), title, 0.0),
                normalized_title: title.to_owned(),
                normalized_search: format!("{title} /documents/{title}"),
            }).collect();
        let query = super::SearchQuery::parse("\"annual report\" -draft ext:pdf OR \"meeting notes\"");
        let matches = super::scored_index_candidates(&index, &query, 10);
        let mut titles: Vec<_> = matches.iter().map(|c| c.result.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, vec!["annual report.pdf", "meeting notes.txt"]);
        assert_eq!(super::scored_index_candidates(&index, &query, 1).len(), 1);
    }

    #[test]
    fn file_index_includes_root_and_symlink_folders_case_insensitively() {
        let root = std::env::temp_dir().join(format!("bingux-folder-regression-{}", std::process::id()));
        std::fs::create_dir_all(root.join("MiXeD Folder")).unwrap();
        std::fs::create_dir_all(root.join("build/generated")).unwrap();
        std::fs::write(root.join("MiXeD Folder/compiled.o"), "object").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("MiXeD Folder"), root.join("Folder Alias")).unwrap();
        let mut snapshots = Vec::new();
        let index = super::index_files_with_progress(std::slice::from_ref(&root), &["xdg-open".into()], |snapshot| snapshots.push(snapshot));
        let indexed_root = index.iter().any(|entry| entry.candidate.result.subtitle == root.to_str().unwrap());
        let query = super::SearchQuery::parse("\"mixed FOLDER\"");
        let matched = super::scored_index_candidates(&index, &query, 10);
        let found = matched.iter().any(|entry| entry.result.title == "MiXeD Folder");
        #[cfg(unix)]
        let alias = index.iter().any(|entry| entry.candidate.result.title == "Folder Alias" && entry.candidate.result.kind == ResultKind::Folder);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(indexed_root, "configured root folders must themselves be searchable");
        assert!(!index.iter().any(|entry| entry.candidate.result.subtitle.contains("/build") || entry.candidate.result.title == "compiled.o"));
        assert!(snapshots.first().is_some_and(|snapshot| snapshot.len() == 1 && snapshot[0].candidate.result.subtitle == root.to_str().unwrap()), "publish the root before scanning its contents");
        assert!(found, "folder matching must ignore case");
        #[cfg(unix)]
        assert!(alias, "directory aliases must be searchable without traversing them");
    }

    #[cfg(unix)]
    #[test]
    fn indexes_desktop_files_exported_through_file_symlinks() {
        use std::fs::{create_dir_all, remove_dir_all, write};
        use std::os::unix::fs::symlink;
        use std::time::{SystemTime, UNIX_EPOCH};

        let root = std::env::temp_dir().join(format!(
            "bingux-searchd-symlink-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before Unix epoch")
                .as_nanos(),
        ));
        let application_directory = root.join("applications");
        create_dir_all(&application_directory).expect("create application directory");
        let source = root.join("source.desktop");
        write(
            &source,
            "[Desktop Entry]\nName=Exported\nComment=Symlinked app\nIcon=exported\nExec=exported\nType=Application\n",
        )
        .expect("write desktop entry");
        symlink(&source, application_directory.join("exported.desktop"))
            .expect("create desktop entry symlink");

        let indexed = index_applications(
            std::slice::from_ref(&application_directory),
            &["/usr/bin/gtk-launch".to_owned()],
        );

        remove_dir_all(&root).expect("remove test directory");
        assert_eq!(indexed.len(), 1);
        assert_eq!(indexed[0].candidate.result.title, "Exported");
    }

    #[test]
    fn offers_chat_only_for_nonempty_question_prompts() {
        let candidate = quick_chat_candidate("  ? explain Rust ownership  ").expect("quick chat");

        assert_eq!(candidate.provider_id, "ai");
        assert_eq!(candidate.result.kind, ResultKind::Chat);
        assert_eq!(candidate.result.score, 1.0);
        assert_eq!(
            candidate.activation,
            Activation::Chat {
                prompt: "explain Rust ownership".to_owned(),
            }
        );
        assert!(quick_chat_candidate("explain Rust ownership").is_none());
        assert!(quick_chat_candidate(" ?   ").is_none());
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
    fn web_search_encodes_query_as_one_url_argument() {
        let candidate = web_candidate("café & cats? #1", &["/usr/bin/xdg-open".into()])
            .expect("web search action");
        assert_eq!(candidate.provider_id, "web");
        assert_eq!(candidate.result.title, "café & cats? #1");
        assert_eq!(candidate.result.subtitle, "DuckDuckGo");
        assert_eq!(candidate.result.icon, "duckduckgo");
        assert_eq!(candidate.activation, Activation::Spawn {
            program: "/usr/bin/xdg-open".into(),
            arguments: vec!["https://duckduckgo.com/?q=caf%C3%A9%20%26%20cats%3F%20%231".into()],
        });
        assert!(web_candidate("   ", &["/usr/bin/xdg-open".into()]).is_none());
        assert!(web_candidate("hello", &[]).is_none());
    }

    #[test]
    fn file_types_use_distinct_theme_icons_and_unknown_types_fall_back() {
        for (name, icon) in [
            ("report.PDF", "application-pdf"),
            ("budget.xlsx", "x-office-spreadsheet"),
            ("photo.heic", "image-x-generic"),
            ("recording.flac", "audio-x-generic"),
            ("movie.mkv", "video-x-generic"),
            ("backup.tar.gz", "package-x-generic"),
            ("main.rs", "text-x-script"),
            ("README", "text-x-generic"),
            ("thing.unknown", "text-x-generic"),
        ] {
            assert_eq!(file_icon(Path::new(name)), icon);
        }
    }

    #[test]
    fn crowded_source_leaves_room_for_other_groups() {
        let mut candidates: Vec<_> = (0..20)
            .map(|index| candidate("applications", &format!("app-{index}"), "App", 0.9))
            .collect();
        candidates.push(candidate("files", "document", "Document", 0.5));
        rank_and_limit(&mut candidates, 20);
        assert_eq!(candidates.len(), 6);
        assert_eq!(candidates.last().unwrap().provider_id, "files");
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
                    "/usr/libexec/bingux/open-record".to_owned(),
                    "--id".to_owned(),
                    "{id}".to_owned(),
                ],
                "record_7",
            ),
            Activation::Spawn {
                program: "/usr/libexec/bingux/open-record".to_owned(),
                arguments: vec!["--id".to_owned(), "record_7".to_owned()],
            }
        );
    }

    #[test]
    fn appends_launch_targets_to_an_absolute_command_vector() {
        assert_eq!(
            append_activation(
                &[
                    "/usr/bin/gtk-launch".to_owned(),
                    "--verbose".to_owned()
                ],
                "org.example.Editor.desktop",
            ),
            Activation::Spawn {
                program: "/usr/bin/gtk-launch".to_owned(),
                arguments: vec![
                    "--verbose".to_owned(),
                    "org.example.Editor.desktop".to_owned()
                ],
            }
        );
    }
    #[test]
    fn bounds_sqlite_text_before_allocating_owned_strings() {
        assert_eq!(
            bounded_sqlite_text(ValueRef::Text(b"record-7"), 16),
            Some("record-7".to_owned())
        );
        assert!(bounded_sqlite_text(ValueRef::Text(b"too long"), 3).is_none());
        assert!(bounded_sqlite_text(ValueRef::Blob(b"record-7"), 16).is_none());
        assert_eq!(
            bounded_sqlite_optional_text(ValueRef::Null, 16),
            Some(String::new())
        );
    }
}


fn specialised_web_candidate(query: &str, opener: &[String]) -> Option<Candidate> {
    let (prefix, search) = query.trim().split_once(':')?;
    let (name, base) = match prefix.to_ascii_lowercase().as_str() {
        "wiki" => ("Wikipedia", "https://en.wikipedia.org/w/index.php?search="),
        "gh" => ("GitHub", "https://github.com/search?q="),
        "maps" => ("Maps", "https://www.openstreetmap.org/search?query="),
        "yt" => ("YouTube", "https://www.youtube.com/results?search_query="),
        _ => return None,
    };
    let mut candidate = web_candidate(search, opener)?;
    let encoded = search.trim().bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { char::from(b).to_string() } else { format!("%{b:02X}") }).collect::<String>();
    candidate.provider_id = "web-shortcuts".into();
    candidate.result.subtitle = name.into();
    candidate.result.icon = "web-browser".into();
    candidate.result.score = 1.0;
    candidate.activation = append_activation(opener, &format!("{base}{encoded}"));
    Some(candidate)
}

fn conversion_candidate(query: &str) -> Option<Candidate> {
    let parts = query.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 4 || !matches!(parts[2], "to" | "in") { return None; }
    let value = parts[0].parse::<f64>().ok()?;
    fn unit(name: &str) -> Option<(&'static str, f64, f64)> {
        Some(match name.to_ascii_lowercase().as_str() {
            "mm" => ("length", 0.001, 0.0), "cm" => ("length", 0.01, 0.0), "m" => ("length", 1.0, 0.0), "km" => ("length", 1000.0, 0.0),
            "in" | "inch" => ("length", 0.0254, 0.0), "ft" => ("length", 0.3048, 0.0), "mi" => ("length", 1609.344, 0.0),
            "g" => ("mass", 0.001, 0.0), "kg" => ("mass", 1.0, 0.0), "lb" | "lbs" => ("mass", 0.45359237, 0.0), "oz" => ("mass", 0.028349523125, 0.0),
            "c" | "°c" => ("temperature", 1.0, 273.15), "f" | "°f" => ("temperature", 5.0/9.0, 255.3722222222222), "k" => ("temperature", 1.0, 0.0),
            "b" => ("data", 1.0, 0.0), "kb" => ("data", 1000.0, 0.0), "mb" => ("data", 1e6, 0.0), "gb" => ("data", 1e9, 0.0),
            "kib" => ("data", 1024.0, 0.0), "mib" => ("data", 1048576.0, 0.0), "gib" => ("data", 1073741824.0, 0.0),
            "s" => ("time", 1.0, 0.0), "min" => ("time", 60.0, 0.0), "h" => ("time", 3600.0, 0.0), "day" => ("time", 86400.0, 0.0),
            _ => return None,
        })
    }
    let (dimension, factor, offset) = unit(parts[1])?;
    let (target, divisor, shift) = unit(parts[3])?;
    if dimension != target { return None; }
    let answer = (value * factor + offset - shift) / divisor;
    if !answer.is_finite() { return None; }
    let number = format!("{answer:.6}").trim_end_matches('0').trim_end_matches('.').to_owned();
    let text = format!("{number} {}", parts[3]);
    Some(Candidate { provider_id: "conversions".into(), result: ProviderResult {
        result_id: "convert".into(), kind: ResultKind::Calculation, title: text.clone(), subtitle: query.trim().into(), icon: "accessories-calculator".into(), score: 1.0,
    }, activation: Activation::Copy { text } })
}
