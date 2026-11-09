//! The catalogue check: the code and its `.ftl` files agree.
//!
//! Run it from a test of the crate that owns the catalogue:
//!
//! ```ignore
//! #[test]
//! fn the_catalogue_matches_the_code() {
//!     bingux_ui::i18n::check::crate_catalogue(env!("CARGO_MANIFEST_DIR"), "files").assert_clean();
//! }
//! ```
//!
//! It parses `locale/<language>/<domain>.ftl` with Fluent's own parser and
//! scans `src/**/*.rs` for the IDs the code asks for, and fails on:
//!
//! | Problem              | Means                                                                 |
//! | -------------------- | --------------------------------------------------------------------- |
//! | `syntax`             | a file Fluent cannot parse (the entry would be dropped)               |
//! | `duplicate`          | an ID written twice in one file (the second wins, silently)           |
//! | `missing`            | code asks for an ID (or `.attribute`) the source file does not have   |
//! | `variables`          | a `tr!` passes variables the message does not use, or misses some     |
//! | `unused`             | a source message no code asks for                                     |
//! | `stale`              | a translation of an ID the source no longer has                       |
//! | `foreign variable`   | a translation uses a `$variable` the source message does not          |
//! | `unknown reference`  | a `{ -term }` or `{ message }` that is in no file of the catalogue    |
//!
//! What counts as asking for an ID: `tr!("id", ..)`, `i18n_id!("id")`,
//! `Message::new("id")`, `i18n::text("id")` and `i18n::has("id")`, outside
//! comments. An ID built at run time (`format!("weather-sky-{code}")`) is
//! declared with a comment next to it, `// ftl-dynamic: weather-sky-`, and
//! every message starting with that prefix counts as used. A file whose text
//! holds `ftl-check: skip-file` (tests with catalogues of their own) is not
//! scanned. A translation that lacks a message is not a problem: the source
//! language fills it in. `scripts/ftl-check.py` reports the same, plus each
//! language's coverage, without building.
//!
//! An app may use bingux-ui's own messages (`tr!("ui-dialog-cancel")`): an ID
//! starting `ui-` that the app's catalogue lacks is checked against
//! bingux-ui's source catalogue instead.

// ftl-check: skip-file (the tests below ask for IDs of their own catalogues)

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use fluent_syntax::ast;

/// What the check found. Empty when the code and catalogues agree.
#[derive(Debug, Default)]
pub struct Report {
    problems: Vec<String>,
}

impl Report {
    /// Each problem, one line: `missing: files-empty-title (src/view.rs:12)`.
    pub fn problems(&self) -> &[String] {
        &self.problems
    }

    /// Panics with every problem when there is one: for a test.
    #[track_caller]
    pub fn assert_clean(&self) {
        assert!(
            self.problems.is_empty(),
            "the catalogue does not match the code:\n{self}"
        );
    }

    fn push(&mut self, kind: &str, text: impl fmt::Display) {
        self.problems.push(format!("{kind}: {text}"));
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for problem in &self.problems {
            writeln!(f, "  {problem}")?;
        }
        Ok(())
    }
}

/// One message or term of a file: its variables, per attribute (`None` is
/// the value), and the references it makes.
#[derive(Debug, Default, Clone)]
struct Entry {
    /// Variables of the value and of each attribute.
    variables: BTreeMap<Option<String>, BTreeSet<String>>,
    /// Messages this one references (their variables are its too).
    messages: BTreeSet<String>,
    /// Terms it references, with a leading `-`.
    terms: BTreeSet<String>,
}

/// One parsed `.ftl` file.
#[derive(Debug, Default)]
struct File {
    entries: BTreeMap<String, Entry>,
}

fn parse(path: &Path, report: &mut Report) -> Option<File> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            report.push("syntax", format!("{}: {error}", path.display()));
            return None;
        }
    };
    Some(parse_text(path, &text, report))
}

/// `text`, read from `path`, as a [`File`].
fn parse_text(path: &Path, text: &str, report: &mut Report) -> File {
    let resource = match fluent_syntax::parser::parse(text) {
        Ok(resource) => resource,
        Err((resource, errors)) => {
            for error in errors {
                let line = text[..error.pos.start.min(text.len())]
                    .lines()
                    .count()
                    .max(1);
                report.push("syntax", format!("{}:{line}: {error}", path.display()));
            }
            resource
        }
    };
    let mut file = File::default();
    for item in resource.body {
        let (id, value, attributes) = match item {
            ast::Entry::Message(message) => (
                message.id.name.to_string(),
                message.value,
                message.attributes,
            ),
            ast::Entry::Term(term) => (
                format!("-{}", term.id.name),
                Some(term.value),
                term.attributes,
            ),
            ast::Entry::Junk { content } => {
                report.push(
                    "syntax",
                    format!("{}: cannot read `{}`", path.display(), content.trim()),
                );
                continue;
            }
            _ => continue,
        };
        let mut entry = Entry::default();
        if let Some(value) = &value {
            let mut variables = BTreeSet::new();
            walk_pattern(value, &mut variables, &mut entry);
            entry.variables.insert(None, variables);
        }
        for attribute in &attributes {
            let mut variables = BTreeSet::new();
            walk_pattern(&attribute.value, &mut variables, &mut entry);
            entry
                .variables
                .insert(Some(attribute.id.name.to_string()), variables);
        }
        if file.entries.insert(id.clone(), entry).is_some() {
            report.push("duplicate", format!("{id} in {}", path.display()));
        }
    }
    file
}

fn walk_pattern(pattern: &ast::Pattern<&str>, variables: &mut BTreeSet<String>, entry: &mut Entry) {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            walk_expression(expression, variables, entry);
        }
    }
}

fn walk_expression(
    expression: &ast::Expression<&str>,
    variables: &mut BTreeSet<String>,
    entry: &mut Entry,
) {
    match expression {
        ast::Expression::Select { selector, variants } => {
            walk_inline(selector, variables, entry);
            for variant in variants {
                walk_pattern(&variant.value, variables, entry);
            }
        }
        ast::Expression::Inline(inline) => walk_inline(inline, variables, entry),
    }
}

fn walk_inline(
    inline: &ast::InlineExpression<&str>,
    variables: &mut BTreeSet<String>,
    entry: &mut Entry,
) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            variables.insert(id.name.to_string());
        }
        ast::InlineExpression::FunctionReference { arguments, .. } => {
            for argument in &arguments.positional {
                walk_inline(argument, variables, entry);
            }
            for argument in &arguments.named {
                walk_inline(&argument.value, variables, entry);
            }
        }
        ast::InlineExpression::MessageReference { id, .. } => {
            entry.messages.insert(id.name.to_string());
        }
        ast::InlineExpression::TermReference { id, .. } => {
            // A term's own variables are its arguments, not the message's.
            entry.terms.insert(format!("-{}", id.name));
        }
        ast::InlineExpression::Placeable { expression } => {
            walk_expression(expression, variables, entry)
        }
        ast::InlineExpression::StringLiteral { .. }
        | ast::InlineExpression::NumberLiteral { .. } => {}
    }
}

/// The variables `id` (with `attribute`) reads, its referenced messages' included.
fn variables_of(file: &File, id: &str, attribute: Option<&str>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut seen = BTreeSet::new();
    collect(file, id, attribute.map(str::to_string), &mut out, &mut seen);
    out
}

fn collect(
    file: &File,
    id: &str,
    attribute: Option<String>,
    out: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(format!("{id}.{attribute:?}")) {
        return;
    }
    let Some(entry) = file.entries.get(id) else {
        return;
    };
    if let Some(variables) = entry.variables.get(&attribute) {
        out.extend(variables.iter().cloned());
    }
    for message in &entry.messages {
        collect(file, message, None, out, seen);
    }
}

/// One place code asks for a message.
#[derive(Debug)]
struct Use {
    id: String,
    /// The variables a `tr!` passes; `None` where they cannot be seen.
    variables: Option<BTreeSet<String>>,
    at: String,
}

/// The marker of a file the scan skips.
const SKIP: &str = concat!("ftl-check: ", "skip-file");

const ASKS: [(&str, bool); 5] = [
    ("tr!(", true),
    ("i18n_id!(", false),
    ("Message::new(", false),
    ("i18n::text(", false),
    ("i18n::has(", false),
];

/// The uses of message IDs in `source`, and the `ftl-dynamic:` prefixes.
fn scan(path: &str, source: &str, uses: &mut Vec<Use>, dynamic: &mut Vec<String>) {
    // Comments are blanked (so a documented example is not a use), keeping
    // byte offsets, after their `ftl-dynamic:` declarations are read.
    let mut code = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        match line.find("//") {
            Some(start) if !in_string(&line[..start]) => {
                let comment = &line[start..];
                if let Some(prefix) = comment.split("ftl-dynamic:").nth(1) {
                    dynamic.push(prefix.trim().trim_end_matches('*').to_string());
                }
                code.push_str(&line[..start]);
                code.extend(comment.chars().map(|c| if c == '\n' { '\n' } else { ' ' }));
            }
            _ => code.push_str(line),
        }
    }
    for (ask, has_args) in ASKS {
        let mut from = 0;
        while let Some(found) = code[from..].find(ask) {
            let start = from + found + ask.len();
            from = start;
            // `tr!(` must not be the end of a longer name (`str!(`).
            let before = code[..start - ask.len()].chars().next_back();
            if ask == "tr!(" && before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = code[start..].trim_start();
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            let Some(end) = rest.find('"') else {
                continue;
            };
            let id = rest[..end].to_string();
            let line = code[..start].lines().count();
            let variables = has_args.then(|| arguments(&rest[end + 1..]));
            uses.push(Use {
                id,
                variables,
                at: format!("{path}:{line}"),
            });
        }
    }
}

/// Whether a `//` after `prefix` sits inside a string literal.
fn in_string(prefix: &str) -> bool {
    let mut inside = false;
    let mut escaped = false;
    for c in prefix.chars() {
        match c {
            '\\' if inside => escaped = !escaped,
            '"' if !escaped => inside = !inside,
            _ => escaped = false,
        }
    }
    inside
}

/// The `name = value` names of a macro call's arguments, from just after the
/// ID to its closing parenthesis.
fn arguments(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut depth = 0i32;
    let mut part = String::new();
    let mut inside = false;
    let mut escaped = false;
    let mut finish = |part: &mut String, names: &mut BTreeSet<String>| {
        if let Some((name, _)) = part.split_once('=') {
            let name = name.trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                names.insert(name.to_string());
            }
        }
        part.clear();
    };
    for c in text.chars() {
        if inside {
            match c {
                '\\' => escaped = !escaped,
                '"' if !escaped => inside = false,
                _ => escaped = false,
            }
            continue;
        }
        match c {
            '"' => inside = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                finish(&mut part, &mut names);
                return names;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                finish(&mut part, &mut names);
                continue;
            }
            _ => {}
        }
        if depth == 0 {
            part.push(c);
        }
    }
    names
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// Checks the catalogue `domain` of the crate at `crate_dir` against the
/// crate's `src/`. See the module documentation.
pub fn crate_catalogue(crate_dir: impl AsRef<Path>, domain: &str) -> Report {
    let crate_dir = crate_dir.as_ref();
    let mut sources = Vec::new();
    rust_files(&crate_dir.join("src"), &mut sources);
    sources.sort();
    let sources: Vec<(String, String)> = sources
        .iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(path).ok()?;
            if text.contains(SKIP) {
                return None;
            }
            let name = path
                .strip_prefix(crate_dir)
                .unwrap_or(path)
                .display()
                .to_string();
            Some((name, text))
        })
        .collect();
    check(&crate_dir.join("locale"), domain, &sources)
}

/// [`crate_catalogue`] over `sources` (path, text) and the files under
/// `locale_dir`.
pub fn check(locale_dir: &Path, domain: &str, sources: &[(String, String)]) -> Report {
    let mut report = Report::default();
    let file_name = format!("{domain}.ftl");
    let source_path = locale_dir.join("en-GB").join(&file_name);
    let Some(source) = parse(&source_path, &mut report) else {
        return report;
    };

    // An app may ask for bingux-ui's own messages (`ui-`): they are checked
    // against bingux-ui's source catalogue, which every app has compiled in.
    let shared = (domain != "bingux-ui").then(|| {
        let builtin = super::catalogue::builtin();
        parse_text(Path::new("bingux-ui.ftl"), builtin.source(), &mut report)
    });

    let mut uses = Vec::new();
    let mut dynamic = Vec::new();
    for (path, text) in sources {
        scan(path, text, &mut uses, &mut dynamic);
    }

    let mut used: BTreeSet<String> = BTreeSet::new();
    for asked in &uses {
        let (id, attribute) = match asked.id.split_once('.') {
            Some((id, attribute)) => (id, Some(attribute)),
            None => (asked.id.as_str(), None),
        };
        let file = match &shared {
            Some(shared) if id.starts_with("ui-") && !source.entries.contains_key(id) => shared,
            _ => {
                used.insert(id.to_string());
                &source
            }
        };
        let present = file
            .entries
            .get(id)
            .is_some_and(|entry| entry.variables.contains_key(&attribute.map(str::to_string)));
        if !present {
            report.push("missing", format!("{} ({})", asked.id, asked.at));
            continue;
        }
        if let Some(passed) = &asked.variables {
            let wanted = variables_of(file, id, attribute);
            if &wanted != passed {
                report.push(
                    "variables",
                    format!(
                        "{} wants {:?}, {} passes {:?}",
                        asked.id, wanted, asked.at, passed
                    ),
                );
            }
        }
    }
    // A message that only other messages reference is used through them.
    let mut referenced = BTreeSet::new();
    for entry in source.entries.values() {
        referenced.extend(entry.messages.iter().cloned());
    }
    for id in source.entries.keys() {
        if id.starts_with('-') || used.contains(id) || referenced.contains(id) {
            continue;
        }
        if dynamic
            .iter()
            .any(|prefix| !prefix.is_empty() && id.starts_with(prefix.as_str()))
        {
            continue;
        }
        report.push("unused", format!("{id} in {}", source_path.display()));
    }
    references(&source, &source, &source_path, &mut report);

    let Ok(languages) = std::fs::read_dir(locale_dir) else {
        return report;
    };
    let mut languages: Vec<PathBuf> = languages
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.file_name().is_some_and(|name| name != "en-GB"))
        .map(|dir| dir.join(&file_name))
        .filter(|path| path.is_file())
        .collect();
    languages.sort();
    for path in languages {
        let Some(translation) = parse(&path, &mut report) else {
            continue;
        };
        for (id, entry) in &translation.entries {
            let Some(original) = source.entries.get(id) else {
                if !id.starts_with('-') {
                    report.push("stale", format!("{id} in {}", path.display()));
                }
                continue;
            };
            for attribute in entry.variables.keys() {
                if !original.variables.contains_key(attribute) {
                    report.push(
                        "stale",
                        format!(
                            "{id}.{} in {}",
                            attribute.as_deref().unwrap_or(""),
                            path.display()
                        ),
                    );
                    continue;
                }
                let theirs = variables_of(&translation, id, attribute.as_deref());
                let ours = variables_of(&source, id, attribute.as_deref());
                let foreign: Vec<&String> = theirs.difference(&ours).collect();
                if !foreign.is_empty() {
                    report.push(
                        "foreign variable",
                        format!("{id} uses {foreign:?} in {}", path.display()),
                    );
                }
            }
        }
        references(&translation, &source, &path, &mut report);
    }
    report
}

/// Every message or term `file` references exists in it or in the source.
fn references(file: &File, source: &File, path: &Path, report: &mut Report) {
    for (id, entry) in &file.entries {
        for target in entry.messages.iter().chain(entry.terms.iter()) {
            if !file.entries.contains_key(target) && !source.entries.contains_key(target) {
                report.push(
                    "unknown reference",
                    format!("{id} refers to {target} in {}", path.display()),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, language: &str, text: &str) {
        std::fs::create_dir_all(dir.join(language)).unwrap();
        std::fs::write(dir.join(language).join("demo.ftl"), text).unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bingux-i18n-check-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_matching_catalogue_is_clean() {
        let dir = temp("clean");
        write(
            &dir,
            "en-GB",
            "-brand = Bingux\ndemo-title = { -brand } files\n    .tooltip = Open { $name }\ndemo-items = { $count ->\n    [one] One item\n   *[other] { $count } items\n}\ndemo-dynamic-a = A\n",
        );
        write(&dir, "de", "demo-title = { -brand } Dateien\n");
        let code = r#"
            // ftl-dynamic: demo-dynamic-
            let a = tr!("demo-title");
            let b = tr!("demo-title.tooltip", name = file.name());
            let c = tr!("demo-items", count = items.len(),);
            // tr!("demo-gone") in a comment is not a use
        "#;
        let report = check(&dir, "demo", &[("src/a.rs".into(), code.into())]);
        report.assert_clean();
    }

    #[test]
    fn every_kind_of_problem_is_found() {
        let dir = temp("problems");
        write(
            &dir,
            "en-GB",
            "demo-a = A { $name }\ndemo-a = Again\ndemo-unused = U\ndemo-ref = { -nothing }\n",
        );
        write(
            &dir,
            "de",
            "demo-a = A { $other }\ndemo-old = Alt\ndemo-broken = {\n",
        );
        let code = r#"let x = tr!("demo-a", count = 2); let y = tr!("demo-missing"); let z = i18n_id!("demo-ref");"#;
        let report = check(&dir, "demo", &[("src/a.rs".into(), code.into())]);
        let kinds: BTreeSet<&str> = report
            .problems()
            .iter()
            .map(|problem| problem.split(':').next().unwrap())
            .collect();
        for kind in [
            "duplicate",
            "missing",
            "variables",
            "unused",
            "stale",
            "foreign variable",
            "unknown reference",
            "syntax",
        ] {
            assert!(kinds.contains(kind), "{kind} not found in\n{report}");
        }
    }

    #[test]
    fn arguments_are_read_through_nested_calls() {
        let names = arguments(
            r#", name = file.name(), count = items.iter().filter(|x| x == "a,b").count())"#,
        );
        assert_eq!(
            names,
            BTreeSet::from(["count".to_string(), "name".to_string()])
        );
    }

    #[test]
    fn bingux_ui_catalogue_matches_its_code() {
        crate_catalogue(env!("CARGO_MANIFEST_DIR"), "bingux-ui").assert_clean();
    }
}
