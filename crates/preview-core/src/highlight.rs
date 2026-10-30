//! A small lexer that colours code, markup and data, one line at a time.
//!
//! It is not a parser. It knows comments, strings, numbers, keywords and
//! (for data formats) keys, and carries what spans lines (a block comment, a
//! triple-quoted string, a Markdown fence) from one line to the next. A
//! language it does not know is [`Language::Plain`]: no colour, never an
//! error. A full highlighter can replace it behind [`Lexer`].

use std::ops::Range;

/// What a piece of a line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Keyword,
    /// A type or a variable: capitalised names in code, `$NAME` in a shell.
    Type,
    String,
    Comment,
    Number,
    /// The key of a data file, or the name of an attribute.
    Key,
    Heading,
}

/// A coloured piece of a line, in byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub token: Token,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Plain,
    Rust,
    C,
    Python,
    JavaScript,
    Shell,
    Json,
    Toml,
    Yaml,
    Markdown,
    Css,
    Markup,
}

impl Language {
    /// The language of a file called `name` of type `mime`.
    pub fn of(name: &str, mime: &str) -> Language {
        let lower = name.to_ascii_lowercase();
        let extension = lower.rsplit_once('.').map(|(_, extension)| extension).unwrap_or("");
        match extension {
            "rs" => Language::Rust,
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "java" | "cs" | "go" | "kt" | "swift" | "qml" | "m" | "glsl" | "vert" | "frag" => Language::C,
            "py" | "pyw" => Language::Python,
            "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "json5" => Language::JavaScript,
            "sh" | "bash" | "zsh" | "fish" | "env" => Language::Shell,
            "json" | "jsonl" | "ndjson" | "webmanifest" => Language::Json,
            "toml" | "ini" | "conf" | "cfg" | "desktop" | "service" | "ovpn" => Language::Toml,
            "yaml" | "yml" => Language::Yaml,
            "md" | "markdown" => Language::Markdown,
            "css" | "scss" => Language::Css,
            "html" | "htm" | "xml" | "svg" | "xhtml" | "plist" | "ui" => Language::Markup,
            _ => match mime {
                "application/json" => Language::Json,
                "text/markdown" => Language::Markdown,
                "text/x-shellscript" | "application/x-shellscript" => Language::Shell,
                "text/html" | "application/xml" | "text/xml" | "image/svg+xml" => Language::Markup,
                "text/css" => Language::Css,
                _ if lower == "makefile" || lower == "dockerfile" || lower == "pkgbuild" => Language::Shell,
                _ => Language::Plain,
            },
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Language::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
                "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
            ],
            Language::C => &[
                "auto", "bool", "break", "case", "catch", "char", "class", "const", "continue", "default", "delete", "do", "double", "else", "enum", "extends", "false", "final", "float",
                "for", "func", "function", "if", "import", "in", "int", "interface", "long", "namespace", "new", "null", "nullptr", "package", "private", "property", "protected",
                "public", "return", "short", "signal", "static", "struct", "switch", "this", "throw", "true", "try", "typedef", "union", "unsigned", "using", "var", "virtual", "void",
                "while",
            ],
            Language::Python => &[
                "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
                "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while", "with", "yield",
            ],
            Language::JavaScript => &[
                "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "default", "delete", "do", "else", "enum", "export", "extends", "false", "finally", "for",
                "from", "function", "if", "implements", "import", "in", "instanceof", "interface", "let", "new", "null", "of", "private", "public", "return", "static", "super",
                "switch", "this", "throw", "true", "try", "type", "typeof", "undefined", "var", "void", "while", "yield",
            ],
            Language::Shell => &[
                "case", "do", "done", "elif", "else", "esac", "exit", "export", "fi", "for", "function", "if", "in", "local", "return", "select", "then", "until", "while", "set",
                "source", "unset", "readonly", "shift", "trap", "eval", "exec",
            ],
            Language::Json | Language::Toml | Language::Yaml => &["true", "false", "null", "yes", "no", "on", "off"],
            _ => &[],
        }
    }

    fn line_comment(self) -> &'static [&'static str] {
        match self {
            Language::Rust | Language::C | Language::JavaScript => &["//"],
            Language::Python | Language::Shell | Language::Toml | Language::Yaml => &["#"],
            _ => &[],
        }
    }

    fn block_comment(self) -> Option<(&'static str, &'static str)> {
        match self {
            Language::Rust | Language::C | Language::JavaScript | Language::Css => Some(("/*", "*/")),
            Language::Markup => Some(("<!--", "-->")),
            _ => None,
        }
    }

    fn quotes(self) -> &'static [char] {
        match self {
            Language::Plain | Language::Markdown => &[],
            Language::Json => &['"'],
            Language::Rust | Language::C | Language::Css | Language::Markup => &['"', '\''],
            Language::JavaScript => &['"', '\'', '`'],
            _ => &['"', '\''],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Normal,
    /// Inside a block comment that ends with `end`.
    Block(&'static str),
    /// Inside a string that continues over lines, closed by `quote`; `triple`
    /// for Python's `"""`.
    Text { quote: char, triple: bool },
    /// Inside a Markdown fenced block.
    Fence,
}

/// Lexes lines in order, carrying what spans lines.
#[derive(Clone, Debug)]
pub struct Lexer {
    language: Language,
    state: State,
}

impl Lexer {
    pub fn new(language: Language) -> Self {
        Lexer {
            language,
            state: State::Normal,
        }
    }

    /// The coloured spans of the next `line`, which has no line break.
    pub fn line(&mut self, line: &str) -> Vec<Span> {
        if self.language == Language::Plain {
            return Vec::new();
        }
        let mut spans = Vec::new();
        if self.language == Language::Markdown {
            self.markdown(line, &mut spans);
            return spans;
        }
        let mut at = 0;
        match self.state {
            State::Block(end) => at = self.block(line, 0, 0, end, &mut spans),
            State::Text { quote, triple } => {
                let end = self.text(line, 0, quote, triple);
                spans.push(Span { range: 0..end, token: Token::String });
                at = end;
            }
            _ => {}
        }
        let language = self.language;
        let bytes = line.as_bytes();
        let key_end = key_end(language, line);
        while at < line.len() {
            let rest = &line[at..];
            let character = rest.chars().next().unwrap_or(' ');
            if language.line_comment().iter().any(|marker| rest.starts_with(marker)) && comment_allowed(language, line, at) {
                spans.push(Span { range: at..line.len(), token: Token::Comment });
                break;
            }
            if let Some((open, close)) = language.block_comment()
                && rest.starts_with(open)
            {
                at = self.block(line, at, at + open.len(), close, &mut spans);
                continue;
            }
            if language.quotes().contains(&character) {
                let triple = language == Language::Python && rest.starts_with(&character.to_string().repeat(3));
                let start = at;
                let skip = if triple { 3 } else { 1 };
                let end = self.text(line, at + skip, character, triple);
                let is_key = matches!(language, Language::Json | Language::Yaml | Language::Toml) && line[end.min(line.len())..].trim_start().starts_with([':', '=']) && !(end == line.len() && self.state != State::Normal);
                spans.push(Span {
                    range: start..end,
                    token: if is_key { Token::Key } else { Token::String },
                });
                at = end;
                continue;
            }
            if language == Language::Markup && character == '<' {
                let (end, tag) = markup_tag(line, at);
                if let Some(range) = tag {
                    spans.push(Span { range, token: Token::Keyword });
                }
                at = end.max(at + 1);
                continue;
            }
            if language == Language::Shell && character == '$' {
                let end = rest[1..]
                    .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '{' || c == '}'))
                    .map_or(rest.len(), |offset| offset + 1);
                if end > 1 {
                    spans.push(Span { range: at..at + end, token: Token::Type });
                    at += end;
                    continue;
                }
            }
            if language == Language::Css && (character == '@' || character == '#') && rest.len() > 1 {
                let end = rest[1..].find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_')).map_or(rest.len(), |offset| offset + 1);
                if end > 1 {
                    spans.push(Span { range: at..at + end, token: if character == '@' { Token::Keyword } else { Token::Number } });
                    at += end;
                    continue;
                }
            }
            if character.is_ascii_digit() && !prev_is_word(bytes, at) {
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' && true))
                    .unwrap_or(rest.len());
                // A trailing dot is punctuation, not part of the number.
                let end = if rest[..end].ends_with('.') { end - 1 } else { end };
                spans.push(Span { range: at..at + end.max(1), token: Token::Number });
                at += end.max(1);
                continue;
            }
            if character.is_alphabetic() || character == '_' {
                let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' && matches!(language, Language::Css | Language::Yaml | Language::Toml | Language::Shell))).unwrap_or(rest.len());
                let word = &rest[..end];
                let token = if key_end.is_some_and(|key| at < key) {
                    Some(Token::Key)
                } else if language.keywords().contains(&word) {
                    Some(Token::Keyword)
                } else if matches!(language, Language::Rust | Language::C | Language::JavaScript | Language::Python) && character.is_uppercase() && word.len() > 1 && !word.chars().all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit()) {
                    Some(Token::Type)
                } else {
                    None
                };
                if let Some(token) = token {
                    spans.push(Span { range: at..at + end, token });
                }
                at += end;
                continue;
            }
            at += character.len_utf8();
        }
        spans.sort_by_key(|span| span.range.start);
        spans
    }

    /// A block comment that began at `start`, its text after the opening
    /// marker from `from`: colour it to its close, or to the line's end and
    /// carry the state. Returns where the line goes on.
    fn block(&mut self, line: &str, start: usize, from: usize, end: &'static str, spans: &mut Vec<Span>) -> usize {
        match line[from..].find(end) {
            Some(offset) => {
                let close = from + offset + end.len();
                spans.push(Span { range: start..close, token: Token::Comment });
                self.state = State::Normal;
                close
            }
            None => {
                spans.push(Span { range: start..line.len(), token: Token::Comment });
                self.state = State::Block(end);
                line.len()
            }
        }
    }

    /// A string from `from` to its closing quote (or the line's end), carrying
    /// the state when it may go on. Returns where the line goes on.
    fn text(&mut self, line: &str, from: usize, quote: char, triple: bool) -> usize {
        let mut at = from;
        let closing = if triple { quote.to_string().repeat(3) } else { quote.to_string() };
        while at < line.len() {
            let rest = &line[at..];
            if rest.starts_with('\\') {
                at += 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
                continue;
            }
            if rest.starts_with(&closing) {
                self.state = State::Normal;
                return at + closing.len();
            }
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
        // Unclosed: a triple quote and a template string go on; others end
        // with the line.
        self.state = if triple || quote == '`' { State::Text { quote, triple } } else { State::Normal };
        line.len()
    }

    fn markdown(&mut self, line: &str, spans: &mut Vec<Span>) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            self.state = if self.state == State::Fence { State::Normal } else { State::Fence };
            spans.push(Span { range: 0..line.len(), token: Token::Comment });
            return;
        }
        if self.state == State::Fence {
            spans.push(Span { range: 0..line.len(), token: Token::String });
            return;
        }
        if trimmed.starts_with('#') && trimmed.trim_start_matches('#').starts_with(' ') {
            spans.push(Span { range: 0..line.len(), token: Token::Heading });
        } else if trimmed.starts_with('>') {
            spans.push(Span { range: 0..line.len(), token: Token::Comment });
        } else {
            // `inline code`.
            let mut at = 0;
            while let Some(open) = line[at..].find('`') {
                let open = at + open;
                let Some(close) = line[open + 1..].find('`') else { break };
                spans.push(Span { range: open..open + close + 2, token: Token::String });
                at = open + close + 2;
            }
        }
    }
}

fn prev_is_word(bytes: &[u8], at: usize) -> bool {
    at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_')
}

/// A line comment marker counts only where a comment can start: `#` in a
/// shell, TOML or YAML starts one at the line's start or after a space.
fn comment_allowed(language: Language, line: &str, at: usize) -> bool {
    match language {
        Language::Shell | Language::Yaml | Language::Toml | Language::Python => at == 0 || line[..at].ends_with([' ', '\t', ';']),
        _ => true,
    }
}

/// Where the key of a `key: value` or `key = value` line ends, for data
/// formats, so its word is coloured as a key.
fn key_end(language: Language, line: &str) -> Option<usize> {
    if !matches!(language, Language::Yaml | Language::Toml) {
        return None;
    }
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return None;
    }
    let indent = line.len() - trimmed.len();
    let body = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    let lead = indent + (trimmed.len() - body.len());
    let separator = if language == Language::Yaml { ':' } else { '=' };
    let end = body.find(separator)?;
    let key = &body[..end];
    (!key.is_empty() && !key.contains(' ') || language == Language::Toml && !key.is_empty()).then_some(lead + end)
}

/// The tag name of a `<tag` or `</tag` at `at`, as a range including the
/// angle bracket, and where scanning resumes.
fn markup_tag(line: &str, at: usize) -> (usize, Option<Range<usize>>) {
    let rest = &line[at + 1..];
    let name_start = rest.strip_prefix('/').or_else(|| rest.strip_prefix('?')).or_else(|| rest.strip_prefix('!')).map_or(0, |after| rest.len() - after.len());
    let name_len = rest[name_start..].find(|c: char| !(c.is_alphanumeric() || c == '-' || c == ':' || c == '_' || c == '.')).unwrap_or(rest.len() - name_start);
    if name_len == 0 {
        return (at + 1, None);
    }
    let end = at + 1 + name_start + name_len;
    (end, Some(at..end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(language: Language, text: &str) -> Vec<Vec<(&str, Token)>> {
        let mut lexer = Lexer::new(language);
        text.lines()
            .map(|line| lexer.line(line).into_iter().map(|span| (&line[span.range], span.token)).collect())
            .collect()
    }

    #[test]
    fn rust_keywords_strings_comments_and_numbers() {
        let got = tokens(Language::Rust, "let x = \"a//b\"; // note\nfn main() { 42 }");
        assert_eq!(
            got[0],
            [("let", Token::Keyword), ("\"a//b\"", Token::String), ("// note", Token::Comment)]
        );
        assert_eq!(got[1], [("fn", Token::Keyword), ("42", Token::Number)]);
    }

    #[test]
    fn block_comments_carry_over_lines() {
        let got = tokens(Language::C, "int a; /* one\ntwo */ int b;\nlast");
        assert_eq!(got[0], [("int", Token::Keyword), ("/* one", Token::Comment)]);
        assert_eq!(got[1], [("two */", Token::Comment), ("int", Token::Keyword)]);
        assert_eq!(got[2], []);
    }

    #[test]
    fn python_triple_quotes_carry_over_lines() {
        let got = tokens(Language::Python, "x = \"\"\"a\nb\"\"\"\ny = 1");
        assert_eq!(got[0], [("\"\"\"a", Token::String)]);
        assert_eq!(got[1], [("b\"\"\"", Token::String)]);
        assert_eq!(got[2], [("1", Token::Number)]);
    }

    #[test]
    fn json_keys_differ_from_string_values() {
        let got = tokens(Language::Json, "{\"name\": \"bingux\", \"n\": 3, \"ok\": true}");
        assert_eq!(
            got[0],
            [("\"name\"", Token::Key), ("\"bingux\"", Token::String), ("\"n\"", Token::Key), ("3", Token::Number), ("\"ok\"", Token::Key), ("true", Token::Keyword)]
        );
    }

    #[test]
    fn markdown_headings_fences_and_code() {
        let got = tokens(Language::Markdown, "# Title\ntext `code` here\n```rust\nlet x;\n```\nafter");
        assert_eq!(got[0], [("# Title", Token::Heading)]);
        assert_eq!(got[1], [("`code`", Token::String)]);
        assert_eq!(got[3], [("let x;", Token::String)]);
        assert_eq!(got[5], []);
    }

    #[test]
    fn toml_sections_comments_and_keys() {
        let got = tokens(Language::Toml, "name = \"x\" # trailing\nport = 8080");
        assert_eq!(got[0], [("name", Token::Key), ("\"x\"", Token::String), ("# trailing", Token::Comment)]);
        assert_eq!(got[1], [("port", Token::Key), ("8080", Token::Number)]);
    }

    #[test]
    fn a_hash_inside_a_word_is_not_a_comment() {
        let got = tokens(Language::Shell, "echo a#b # real\nx=$HOME");
        assert_eq!(got[0], [("# real", Token::Comment)]);
        assert_eq!(got[1], [("$HOME", Token::Type)]);
    }

    #[test]
    fn markup_tags_and_attributes_strings() {
        let got = tokens(Language::Markup, "<a href=\"x\">hi</a><!-- c -->");
        assert_eq!(got[0], [("<a", Token::Keyword), ("\"x\"", Token::String), ("</a", Token::Keyword), ("<!-- c -->", Token::Comment)]);
    }

    #[test]
    fn languages_are_found_by_name_then_type() {
        assert_eq!(Language::of("main.RS", ""), Language::Rust);
        assert_eq!(Language::of("Makefile", ""), Language::Shell);
        assert_eq!(Language::of("data", "application/json"), Language::Json);
        assert_eq!(Language::of("README", "application/octet-stream"), Language::Plain);
    }

    #[test]
    fn unknown_text_is_not_coloured_and_nothing_panics_on_odd_input() {
        assert!(tokens(Language::Plain, "anything 1 \"x\"")[0].is_empty());
        for language in [Language::Rust, Language::Json, Language::Yaml, Language::Markup, Language::Css, Language::Python, Language::Shell, Language::Toml, Language::Markdown, Language::C, Language::JavaScript] {
            let mut lexer = Lexer::new(language);
            for line in ["", "\"", "'''", "/*", "<", "<!--", "é\"é", "a: ", "= 1", "- - -", "1.", "0x", "`", "\\", "$", "@", "#"] {
                let spans = lexer.line(line);
                for span in spans {
                    assert!(span.range.end <= line.len() && line.is_char_boundary(span.range.start) && line.is_char_boundary(span.range.end), "{language:?} {line:?} {span:?}");
                }
            }
        }
    }
}
