//! Which language a document is in: from a modeline, the file name, the
//! extension, a shebang or the first line, in that order. A language knows
//! its comment markers and whether it is indented with tabs; its colours
//! come from the highlighter ([`crate::highlight`]), which reads the same
//! name.

use std::path::Path;

/// What the editor knows about a language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Language {
    /// A stable lower-case key: `rust`, `python`, `plain`.
    pub id: String,
    /// The name a person reads: `Rust`, `Python`, `Plain Text`.
    pub name: String,
    pub line_comment: Option<&'static str>,
    pub block_comment: Option<(&'static str, &'static str)>,
    /// Indented with tabs unless the file says otherwise (Make, Go).
    pub tabs: bool,
    /// Prose: no auto-closed quotes, wrap on by default, spell checking.
    pub prose: bool,
    /// The highlighter's name for its grammar, when it is not `name`.
    pub syntax: Option<String>,
}

impl Language {
    pub fn plain() -> Self {
        Self {
            id: "plain".into(),
            name: "Plain Text".into(),
            line_comment: None,
            block_comment: None,
            tabs: false,
            prose: true,
            syntax: None,
        }
    }

    pub fn is_plain(&self) -> bool {
        self.id == "plain"
    }

    pub fn is_markdown(&self) -> bool {
        self.id == "markdown"
    }

    /// The grammar name to highlight with.
    pub fn syntax_name(&self) -> &str {
        self.syntax.as_deref().unwrap_or(&self.name)
    }
}

struct Spec {
    id: &'static str,
    name: &'static str,
    syntax: Option<&'static str>,
    extensions: &'static [&'static str],
    files: &'static [&'static str],
    interpreters: &'static [&'static str],
    line: Option<&'static str>,
    block: Option<(&'static str, &'static str)>,
    tabs: bool,
    prose: bool,
}

const fn spec(id: &'static str, name: &'static str) -> Spec {
    Spec {
        id,
        name,
        syntax: None,
        extensions: &[],
        files: &[],
        interpreters: &[],
        line: None,
        block: None,
        tabs: false,
        prose: false,
    }
}

const C_BLOCK: Option<(&str, &str)> = Some(("/*", "*/"));
const XML_BLOCK: Option<(&str, &str)> = Some(("<!--", "-->"));

/// The languages the editor names itself, with their comment markers.
/// Anything else the highlighter knows is found by its own extension list.
const SPECS: &[Spec] = &[
    Spec {
        extensions: &["rs"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("rust", "Rust")
    },
    Spec {
        extensions: &["py", "pyw", "pyi"],
        files: &["SConstruct", "SConscript"],
        interpreters: &["python", "python2", "python3"],
        line: Some("#"),
        ..spec("python", "Python")
    },
    Spec {
        extensions: &["js", "mjs", "cjs", "jsx"],
        interpreters: &["node", "nodejs", "deno", "bun"],
        line: Some("//"),
        block: C_BLOCK,
        syntax: Some("JavaScript"),
        ..spec("javascript", "JavaScript")
    },
    Spec {
        extensions: &["ts", "mts", "cts"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("typescript", "TypeScript")
    },
    Spec {
        extensions: &["tsx"],
        line: Some("//"),
        block: C_BLOCK,
        syntax: Some("TypeScriptReact"),
        ..spec("tsx", "TSX")
    },
    Spec {
        extensions: &["c", "h"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("c", "C")
    },
    Spec {
        extensions: &[
            "cc", "cpp", "cxx", "c++", "hh", "hpp", "hxx", "h++", "ipp", "inl",
        ],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("cpp", "C++")
    },
    Spec {
        extensions: &["cs", "csx"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("csharp", "C#")
    },
    Spec {
        extensions: &["go"],
        line: Some("//"),
        block: C_BLOCK,
        tabs: true,
        ..spec("go", "Go")
    },
    Spec {
        extensions: &["java"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("java", "Java")
    },
    Spec {
        extensions: &["kt", "kts"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("kotlin", "Kotlin")
    },
    Spec {
        extensions: &["swift"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("swift", "Swift")
    },
    Spec {
        extensions: &["scala", "sc"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("scala", "Scala")
    },
    Spec {
        extensions: &["dart"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("dart", "Dart")
    },
    Spec {
        extensions: &["zig"],
        line: Some("//"),
        ..spec("zig", "Zig")
    },
    Spec {
        extensions: &["php", "phtml"],
        interpreters: &["php"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("php", "PHP")
    },
    Spec {
        extensions: &["rb", "rake", "gemspec"],
        files: &["Gemfile", "Rakefile", "Vagrantfile"],
        interpreters: &["ruby"],
        line: Some("#"),
        ..spec("ruby", "Ruby")
    },
    Spec {
        extensions: &["pl", "pm"],
        interpreters: &["perl"],
        line: Some("#"),
        ..spec("perl", "Perl")
    },
    Spec {
        extensions: &["lua"],
        interpreters: &["lua", "luajit"],
        line: Some("--"),
        ..spec("lua", "Lua")
    },
    Spec {
        extensions: &["sh", "bash", "zsh", "ksh", "ebuild", "eclass"],
        files: &[
            ".bashrc",
            ".bash_profile",
            ".bash_logout",
            ".profile",
            ".zshrc",
            ".zprofile",
            "PKGBUILD",
            "APKBUILD",
        ],
        interpreters: &["sh", "bash", "zsh", "ksh", "dash", "ash"],
        line: Some("#"),
        syntax: Some("Bourne Again Shell (bash)"),
        ..spec("shell", "Shell")
    },
    Spec {
        extensions: &["fish"],
        interpreters: &["fish"],
        line: Some("#"),
        ..spec("fish", "Fish")
    },
    Spec {
        extensions: &["ps1", "psm1", "psd1"],
        line: Some("#"),
        ..spec("powershell", "PowerShell")
    },
    Spec {
        extensions: &["mk", "mak"],
        files: &["Makefile", "makefile", "GNUmakefile"],
        interpreters: &["make"],
        line: Some("#"),
        tabs: true,
        ..spec("make", "Makefile")
    },
    Spec {
        extensions: &["cmake"],
        files: &["CMakeLists.txt"],
        line: Some("#"),
        ..spec("cmake", "CMake")
    },
    Spec {
        extensions: &["dockerfile", "containerfile"],
        files: &["Dockerfile", "Containerfile"],
        line: Some("#"),
        ..spec("dockerfile", "Dockerfile")
    },
    Spec {
        extensions: &["nix"],
        line: Some("#"),
        block: C_BLOCK,
        ..spec("nix", "Nix")
    },
    Spec {
        extensions: &["md", "markdown", "mdown", "mkd", "mdx"],
        block: XML_BLOCK,
        prose: true,
        ..spec("markdown", "Markdown")
    },
    Spec {
        extensions: &["rst"],
        prose: true,
        ..spec("restructuredtext", "reStructuredText")
    },
    Spec {
        extensions: &["tex", "sty", "cls", "ltx"],
        line: Some("%"),
        prose: true,
        ..spec("latex", "LaTeX")
    },
    Spec {
        extensions: &["html", "htm", "xhtml", "shtml"],
        block: XML_BLOCK,
        ..spec("html", "HTML")
    },
    Spec {
        extensions: &[
            "xml", "svg", "xsd", "xsl", "xslt", "plist", "ui", "glade", "metainfo", "appdata",
            "csproj", "rss", "atom",
        ],
        block: XML_BLOCK,
        ..spec("xml", "XML")
    },
    Spec {
        extensions: &["css"],
        block: C_BLOCK,
        ..spec("css", "CSS")
    },
    Spec {
        extensions: &["scss"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("scss", "SCSS")
    },
    Spec {
        extensions: &["sass"],
        line: Some("//"),
        ..spec("sass", "Sass")
    },
    Spec {
        extensions: &["less"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("less", "Less")
    },
    Spec {
        extensions: &["json", "jsonl", "ndjson", "geojson", "webmanifest"],
        files: &[".babelrc", ".eslintrc", "composer.lock", "flake.lock"],
        ..spec("json", "JSON")
    },
    Spec {
        extensions: &["jsonc", "json5"],
        line: Some("//"),
        block: C_BLOCK,
        syntax: Some("JSON"),
        ..spec("jsonc", "JSON with Comments")
    },
    Spec {
        extensions: &["toml"],
        files: &["Cargo.lock", "Pipfile", "uv.lock", "poetry.lock"],
        line: Some("#"),
        ..spec("toml", "TOML")
    },
    Spec {
        extensions: &["yaml", "yml"],
        files: &[".clang-format"],
        line: Some("#"),
        ..spec("yaml", "YAML")
    },
    Spec {
        extensions: &[
            "ini", "cfg", "conf", "desktop", "service", "socket", "timer", "mount", "target",
            "path", "slice", "network", "netdev", "link",
        ],
        files: &[".gitconfig", ".editorconfig", "mimeapps.list"],
        line: Some("#"),
        syntax: Some("INI"),
        ..spec("ini", "INI")
    },
    Spec {
        extensions: &["sql"],
        line: Some("--"),
        block: C_BLOCK,
        ..spec("sql", "SQL")
    },
    Spec {
        extensions: &["diff", "patch", "rej"],
        ..spec("diff", "Diff")
    },
    Spec {
        extensions: &["hs", "lhs"],
        line: Some("--"),
        block: Some(("{-", "-}")),
        ..spec("haskell", "Haskell")
    },
    Spec {
        extensions: &["ml", "mli"],
        block: Some(("(*", "*)")),
        ..spec("ocaml", "OCaml")
    },
    Spec {
        extensions: &["ex", "exs"],
        line: Some("#"),
        ..spec("elixir", "Elixir")
    },
    Spec {
        extensions: &["erl", "hrl"],
        line: Some("%"),
        ..spec("erlang", "Erlang")
    },
    Spec {
        extensions: &["clj", "cljs", "cljc", "edn"],
        line: Some(";"),
        ..spec("clojure", "Clojure")
    },
    Spec {
        extensions: &["lisp", "lsp", "cl", "el", "scm", "ss", "rkt"],
        line: Some(";"),
        ..spec("lisp", "Lisp")
    },
    Spec {
        extensions: &["r"],
        line: Some("#"),
        ..spec("r", "R")
    },
    Spec {
        extensions: &["jl"],
        interpreters: &["julia"],
        line: Some("#"),
        ..spec("julia", "Julia")
    },
    Spec {
        extensions: &["vim"],
        files: &[".vimrc", "vimrc"],
        line: Some("\""),
        ..spec("vim", "VimL")
    },
    Spec {
        extensions: &["qml"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("qml", "QML")
    },
    Spec {
        extensions: &["proto"],
        line: Some("//"),
        block: C_BLOCK,
        ..spec("protobuf", "Protocol Buffer")
    },
    Spec {
        extensions: &["graphql", "gql"],
        line: Some("#"),
        ..spec("graphql", "GraphQL")
    },
    Spec {
        extensions: &["tf", "tfvars", "hcl"],
        line: Some("#"),
        block: C_BLOCK,
        ..spec("terraform", "Terraform")
    },
    Spec {
        extensions: &["star", "bzl", "bazel"],
        files: &["BUILD", "WORKSPACE"],
        line: Some("#"),
        syntax: Some("Python"),
        ..spec("starlark", "Starlark")
    },
    Spec {
        extensions: &["gitignore", "dockerignore", "ignore"],
        files: &[".gitignore", ".dockerignore", ".gitattributes"],
        line: Some("#"),
        syntax: Some("Git Ignore"),
        ..spec("ignore", "Ignore List")
    },
    Spec {
        extensions: &["log"],
        prose: true,
        syntax: Some("log"),
        ..spec("log", "Log")
    },
    Spec {
        extensions: &["csv", "tsv"],
        syntax: Some("Advanced CSV"),
        ..spec("csv", "CSV")
    },
    Spec {
        extensions: &["txt", "text"],
        prose: true,
        ..spec("plain", "Plain Text")
    },
];

fn from_spec(spec: &Spec) -> Language {
    Language {
        id: spec.id.into(),
        name: spec.name.into(),
        line_comment: spec.line,
        block_comment: spec.block,
        tabs: spec.tabs,
        prose: spec.prose,
        syntax: spec.syntax.map(str::to_owned),
    }
}

/// The language with this id or name (either case), from the editor's table
/// or, failing that, a grammar the highlighter has.
pub fn by_name(name: &str) -> Option<Language> {
    if name.eq_ignore_ascii_case("plain")
        || name.eq_ignore_ascii_case("text")
        || name.eq_ignore_ascii_case("plain text")
    {
        return Some(Language::plain());
    }
    if let Some(spec) = SPECS
        .iter()
        .find(|spec| spec.id.eq_ignore_ascii_case(name) || spec.name.eq_ignore_ascii_case(name))
    {
        return Some(from_spec(spec));
    }
    crate::highlight::grammar_named(name).map(|grammar| grammar_language(&grammar))
}

/// The languages a person can pick from, by name.
pub fn all() -> Vec<Language> {
    let mut list: Vec<Language> = vec![Language::plain()];
    let mut seen = std::collections::HashSet::new();
    seen.insert("plain".to_owned());
    for spec in SPECS {
        if seen.insert(spec.id.to_owned()) {
            list.push(from_spec(spec));
        }
    }
    for grammar in crate::highlight::grammar_names() {
        let id = grammar.to_lowercase();
        if list.iter().any(|language| {
            language.name.eq_ignore_ascii_case(&grammar)
                || language.syntax_name().eq_ignore_ascii_case(&grammar)
        }) {
            continue;
        }
        if seen.insert(id) {
            list.push(grammar_language(&grammar));
        }
    }
    list[1..].sort_by_key(|language| language.name.to_lowercase());
    list
}

fn grammar_language(grammar: &str) -> Language {
    Language {
        id: grammar.to_lowercase(),
        name: grammar.to_owned(),
        line_comment: None,
        block_comment: None,
        tabs: false,
        prose: false,
        syntax: None,
    }
}

/// The language of a document named `path` (a file name or a path; may be
/// empty for an untitled document) whose text starts with `head`.
pub fn detect(path: Option<&Path>, head: &str) -> Language {
    if let Some(name) = modeline(head) {
        if let Some(language) = by_name(&name) {
            return language;
        }
    }
    if let Some(path) = path {
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if let Some(spec) = SPECS.iter().find(|spec| spec.files.contains(&file)) {
            return from_spec(spec);
        }
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase);
        // `Dockerfile.dev`, `Makefile.am`: the name says it.
        let stem = file.split('.').next().unwrap_or_default();
        if let Some(spec) = SPECS
            .iter()
            .find(|spec| spec.files.contains(&stem) && !stem.is_empty())
        {
            if extension
                .as_deref()
                .is_none_or(|ext| !SPECS.iter().any(|other| other.extensions.contains(&ext)))
            {
                return from_spec(spec);
            }
        }
        if let Some(extension) = extension.as_deref() {
            if let Some(spec) = SPECS
                .iter()
                .find(|spec| spec.extensions.contains(&extension))
            {
                return from_spec(spec);
            }
            if let Some(grammar) = crate::highlight::grammar_for_extension(extension) {
                return by_name(&grammar).unwrap_or_else(|| grammar_language(&grammar));
            }
        }
        if file.starts_with('.') && file.ends_with("rc") && head.trim_start().starts_with('{') {
            return by_name("json").unwrap_or_else(Language::plain);
        }
    }
    let first = head.lines().next().unwrap_or_default();
    if let Some(interpreter) = shebang(first) {
        if let Some(spec) = SPECS
            .iter()
            .find(|spec| spec.interpreters.contains(&interpreter.as_str()))
        {
            return from_spec(spec);
        }
    }
    if first.starts_with("<?xml") {
        return by_name("xml").unwrap_or_else(Language::plain);
    }
    if first
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("<!doctype html")
    {
        return by_name("html").unwrap_or_else(Language::plain);
    }
    if first.starts_with("diff --git")
        || first.starts_with("--- ")
            && head
                .lines()
                .nth(1)
                .is_some_and(|line| line.starts_with("+++ "))
    {
        return by_name("diff").unwrap_or_else(Language::plain);
    }
    if let Some(grammar) = crate::highlight::grammar_for_first_line(first) {
        return by_name(&grammar).unwrap_or_else(|| grammar_language(&grammar));
    }
    Language::plain()
}

/// The program a `#!` line runs: `/usr/bin/env python3 -u` is `python3`.
pub fn shebang(first: &str) -> Option<String> {
    let rest = first.strip_prefix("#!")?.trim();
    let mut words = rest.split_whitespace();
    let mut program = words.next()?.rsplit('/').next()?.to_owned();
    if program == "env" {
        program = words.find(|word| !word.starts_with('-'))?.to_owned();
    }
    // `python3.12` is python3.
    let base = program.trim_end_matches(|ch: char| ch.is_ascii_digit() || ch == '.');
    if base != program && !base.is_empty() {
        let major = program[base.len()..].split('.').next().unwrap_or_default();
        return Some(format!("{base}{major}"));
    }
    Some(program)
}

/// The language a Vim or Emacs modeline names, in the first or last five
/// lines: `vim: set ft=rust:`, `vi: filetype=python`, `-*- mode: ruby -*-`.
pub fn modeline(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().take(5).collect();
    for line in lines {
        if let Some(name) = vim_modeline(line).or_else(|| emacs_modeline(line)) {
            return Some(name);
        }
    }
    None
}

fn vim_modeline(line: &str) -> Option<String> {
    let at = ["vim:", "vi:", "ex:"]
        .iter()
        .find_map(|tag| line.find(tag).map(|index| index + tag.len()))?;
    let rest = &line[at..];
    for word in rest.split(|ch: char| ch.is_whitespace() || ch == ':') {
        if let Some(value) = word
            .strip_prefix("ft=")
            .or_else(|| word.strip_prefix("filetype="))
            .or_else(|| word.strip_prefix("syntax="))
        {
            return Some(vim_name(value).to_owned());
        }
    }
    None
}

/// Vim's file type names that differ from the editor's ids.
fn vim_name(name: &str) -> &str {
    match name {
        "sh" | "bash" | "zsh" => "shell",
        "javascriptreact" => "javascript",
        "typescriptreact" => "tsx",
        "cs" => "csharp",
        "tex" => "latex",
        "text" => "plain",
        "dosini" => "ini",
        other => other,
    }
}

fn emacs_modeline(line: &str) -> Option<String> {
    let start = line.find("-*-")? + 3;
    let end = line[start..].find("-*-")? + start;
    let inner = line[start..end].trim();
    let mode = if inner.contains(':') {
        inner.split(';').find_map(|part| {
            let (key, value) = part.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("mode")
                .then(|| value.trim().to_owned())
        })?
    } else {
        inner.to_owned()
    };
    let mode = mode.to_lowercase();
    Some(match mode.as_str() {
        "c++" => "cpp".into(),
        "sh" | "shell-script" => "shell".into(),
        "js" => "javascript".into(),
        "text" => "plain".into(),
        other => other.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(path: &str, head: &str) -> String {
        detect(Some(Path::new(path)), head).id
    }

    #[test]
    fn extensions_and_names() {
        assert_eq!(id("main.rs", ""), "rust");
        assert_eq!(id("Makefile", ""), "make");
        assert_eq!(id("README.MD", ""), "markdown");
        assert_eq!(id("Dockerfile.dev", ""), "dockerfile");
        assert_eq!(id("notes.txt", ""), "plain");
    }

    #[test]
    fn shebangs() {
        assert_eq!(id("tool", "#!/usr/bin/env python3\nprint(1)"), "python");
        assert_eq!(id("run", "#!/bin/bash\n"), "shell");
        assert_eq!(
            shebang("#!/usr/bin/env -S python3.12 -u").as_deref(),
            Some("python3")
        );
    }

    #[test]
    fn modelines_win() {
        assert_eq!(id("notes.txt", "# vim: set ft=python :\n"), "python");
        assert_eq!(id("build", "# -*- mode: ruby -*-\n"), "ruby");
        assert_eq!(id("x", "// -*- c++ -*-\n"), "cpp");
    }

    #[test]
    fn makefiles_use_tabs() {
        assert!(detect(Some(Path::new("Makefile")), "").tabs);
        assert!(!detect(Some(Path::new("a.py")), "").tabs);
    }

    #[test]
    fn untitled_documents_are_plain() {
        assert!(detect(None, "hello").is_plain());
    }
}
