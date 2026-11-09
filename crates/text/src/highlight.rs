//! Syntax highlighting: what kind of text each part of a line is.
//!
//! The grammars are the bat syntax set (via `two-face`) run by `syntect`
//! with pure-Rust regexes: about 200 languages, MIT-licensed, no C to
//! build. Chosen over tree-sitter for breadth (one dump instead of a C
//! grammar crate per language) and because its state is per line, which
//! suits lazy work: only the lines on screen and the lines above them since
//! the last checkpoint are ever parsed.
//!
//! The highlighter keeps the parser's state at the start of every
//! [`CHECKPOINT`]th line. Lines are parsed on demand from the nearest
//! checkpoint, within a budget of lines a call, so a jump to the end of a
//! long file never stalls a frame: the lines not reached yet are plain and
//! the caller asks again on the next frame. An edit invalidates from its
//! line down.
//!
//! The kinds are the editor's ([`TokenKind`]), not the grammar's scopes, so
//! the colours come from the app's tokens and follow light and dark.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use ropey::Rope;

use crate::language::Language;

/// What a run of text is, for its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Comment,
    String,
    Escape,
    Number,
    Constant,
    Keyword,
    Operator,
    Type,
    Function,
    Variable,
    Property,
    Tag,
    Attribute,
    Heading,
    Emphasis,
    Strong,
    Link,
    Code,
    Quote,
    ListMarker,
    Inserted,
    Deleted,
    Changed,
    Invalid,
}

impl TokenKind {
    pub const ALL: [TokenKind; 24] = [
        TokenKind::Comment,
        TokenKind::String,
        TokenKind::Escape,
        TokenKind::Number,
        TokenKind::Constant,
        TokenKind::Keyword,
        TokenKind::Operator,
        TokenKind::Type,
        TokenKind::Function,
        TokenKind::Variable,
        TokenKind::Property,
        TokenKind::Tag,
        TokenKind::Attribute,
        TokenKind::Heading,
        TokenKind::Emphasis,
        TokenKind::Strong,
        TokenKind::Link,
        TokenKind::Code,
        TokenKind::Quote,
        TokenKind::ListMarker,
        TokenKind::Inserted,
        TokenKind::Deleted,
        TokenKind::Changed,
        TokenKind::Invalid,
    ];
}

/// A run of a line, in bytes of the line, and its kind.
pub type Span = (Range<usize>, TokenKind);

/// Lines between two saved parser states.
pub const CHECKPOINT: usize = 64;

/// Lines longer than this are not parsed: they are drawn plain and the
/// parser's state passes over them unchanged.
pub const LONGEST_PARSED_LINE: usize = 20_000;

/// The kind of the scope names `scopes`, innermost last.
pub fn classify(scopes: &[&str]) -> Option<TokenKind> {
    if scopes
        .iter()
        .any(|scope| scope.starts_with("meta.mapping.key"))
    {
        if let Some(innermost) = scopes.last() {
            if innermost.starts_with("string")
                || innermost.starts_with("punctuation.definition.string")
            {
                return Some(TokenKind::Property);
            }
        }
    }
    scopes.iter().rev().find_map(|scope| kind_of_scope(scope))
}

/// The kind of one scope name, or `None` to look at the scope around it.
pub fn kind_of_scope(scope: &str) -> Option<TokenKind> {
    const RULES: &[(&str, TokenKind)] = &[
        ("comment", TokenKind::Comment),
        ("constant.character.escape", TokenKind::Escape),
        ("string.regexp", TokenKind::String),
        ("string.other.link", TokenKind::Link),
        ("string", TokenKind::String),
        ("constant.numeric", TokenKind::Number),
        ("constant", TokenKind::Constant),
        ("support.constant", TokenKind::Constant),
        ("variable.other.constant", TokenKind::Constant),
        ("variable.language", TokenKind::Keyword),
        ("markup.heading", TokenKind::Heading),
        ("entity.name.section", TokenKind::Heading),
        ("markup.bold", TokenKind::Strong),
        ("markup.italic", TokenKind::Emphasis),
        ("markup.underline.link", TokenKind::Link),
        ("markup.link", TokenKind::Link),
        ("meta.link", TokenKind::Link),
        ("markup.raw", TokenKind::Code),
        ("markup.quote", TokenKind::Quote),
        ("markup.list", TokenKind::ListMarker),
        ("punctuation.definition.list", TokenKind::ListMarker),
        ("markup.inserted", TokenKind::Inserted),
        ("markup.deleted", TokenKind::Deleted),
        ("markup.changed", TokenKind::Changed),
        ("meta.diff.range", TokenKind::Changed),
        ("meta.diff.header", TokenKind::Changed),
        ("entity.name.tag", TokenKind::Tag),
        ("entity.other.attribute-name", TokenKind::Attribute),
        ("entity.name.function", TokenKind::Function),
        ("support.function", TokenKind::Function),
        ("variable.function", TokenKind::Function),
        ("entity.name.macro", TokenKind::Function),
        ("entity.name.type", TokenKind::Type),
        ("entity.name.class", TokenKind::Type),
        ("entity.name.struct", TokenKind::Type),
        ("entity.name.enum", TokenKind::Type),
        ("entity.name.trait", TokenKind::Type),
        ("entity.name.interface", TokenKind::Type),
        ("entity.name.union", TokenKind::Type),
        ("entity.other.inherited-class", TokenKind::Type),
        ("support.type.property-name", TokenKind::Property),
        ("support.type", TokenKind::Type),
        ("support.class", TokenKind::Type),
        ("storage.type.numeric", TokenKind::Type),
        ("storage.type.primitive", TokenKind::Type),
        ("storage", TokenKind::Keyword),
        ("keyword.operator.word", TokenKind::Keyword),
        ("keyword.operator", TokenKind::Operator),
        ("keyword", TokenKind::Keyword),
        ("variable.parameter", TokenKind::Variable),
        ("variable.other.member", TokenKind::Property),
        ("variable.other.property", TokenKind::Property),
        ("meta.property-name", TokenKind::Property),
        ("entity.name.label", TokenKind::Constant),
        ("invalid", TokenKind::Invalid),
    ];
    RULES
        .iter()
        .find(|(prefix, _)| {
            scope.starts_with(prefix)
                && scope
                    .as_bytes()
                    .get(prefix.len())
                    .is_none_or(|byte| *byte == b'.')
        })
        .map(|(_, kind)| *kind)
}

#[cfg(feature = "syntax")]
mod engine {
    use std::sync::LazyLock;

    use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};

    /// The grammars. Loading takes a few tens of milliseconds; call
    /// [`super::warm`] off the UI thread at start-up.
    static SET: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

    pub fn set() -> &'static SyntaxSet {
        &SET
    }

    pub fn find(name: &str) -> Option<&'static SyntaxReference> {
        let set = set();
        set.find_syntax_by_name(name)
            .or_else(|| {
                set.syntaxes()
                    .iter()
                    .find(|syntax| syntax.name.eq_ignore_ascii_case(name))
            })
            .or_else(|| set.find_syntax_by_token(&name.to_lowercase()))
    }

    #[derive(Clone)]
    pub struct State {
        pub parse: ParseState,
        pub scopes: ScopeStack,
    }

    impl State {
        pub fn new(syntax: &'static SyntaxReference) -> Self {
            Self {
                parse: ParseState::new(syntax),
                scopes: ScopeStack::new(),
            }
        }
    }

    /// What each scope means: its own kind, and whether it is the key of a
    /// mapping (JSON, YAML), which makes the string inside it a property.
    pub type Kinds = std::collections::HashMap<Scope, (Option<super::TokenKind>, bool)>;

    /// Parses one line (with its `\n`) and returns its runs.
    pub fn line(state: &mut State, text: &str, kinds: &mut Kinds) -> Vec<super::Span> {
        let Ok(ops) = state.parse.parse_line(text, set()) else {
            return Vec::new();
        };
        let mut spans: Vec<super::Span> = Vec::new();
        let mut at = 0usize;
        let end = text.trim_end_matches('\n').len();
        let mut current = kind_of(&state.scopes, kinds);
        for (offset, op) in ops {
            let offset = offset.min(end);
            if offset > at {
                push(&mut spans, at..offset, current);
            }
            at = at.max(offset);
            if state.scopes.apply(&op).is_err() {
                break;
            }
            current = kind_of(&state.scopes, kinds);
        }
        if end > at {
            push(&mut spans, at..end, current);
        }
        spans
    }

    fn push(
        spans: &mut Vec<super::Span>,
        range: std::ops::Range<usize>,
        kind: Option<super::TokenKind>,
    ) {
        let Some(kind) = kind else { return };
        if let Some(last) = spans.last_mut()
            && last.1 == kind
            && last.0.end == range.start
        {
            last.0.end = range.end;
            return;
        }
        spans.push((range, kind));
    }

    fn kind_of(stack: &ScopeStack, kinds: &mut Kinds) -> Option<super::TokenKind> {
        let scopes = stack.as_slice();
        if scopes.iter().any(|scope| scope_kind(*scope, kinds).1) {
            return Some(super::TokenKind::Property);
        }
        scopes
            .iter()
            .rev()
            .find_map(|scope| scope_kind(*scope, kinds).0)
    }

    fn scope_kind(scope: Scope, kinds: &mut Kinds) -> (Option<super::TokenKind>, bool) {
        *kinds.entry(scope).or_insert_with(|| {
            let name = scope.build_string();
            (
                super::kind_of_scope(&name),
                name.starts_with("meta.mapping.key"),
            )
        })
    }
}

/// Load the grammars now, so the first document does not wait for them.
pub fn warm() {
    #[cfg(feature = "syntax")]
    {
        let _ = engine::set();
    }
}

/// The grammar named `name`, by name or file extension, ignoring case.
pub fn grammar_named(name: &str) -> Option<String> {
    #[cfg(feature = "syntax")]
    {
        engine::find(name).map(|syntax| syntax.name.clone())
    }
    #[cfg(not(feature = "syntax"))]
    {
        let _ = name;
        None
    }
}

/// The grammar for a file extension.
pub fn grammar_for_extension(extension: &str) -> Option<String> {
    #[cfg(feature = "syntax")]
    {
        engine::set()
            .find_syntax_by_extension(extension)
            .map(|syntax| syntax.name.clone())
    }
    #[cfg(not(feature = "syntax"))]
    {
        let _ = extension;
        None
    }
}

/// The grammar a first line names (`<?php`, a shebang the table misses).
pub fn grammar_for_first_line(line: &str) -> Option<String> {
    #[cfg(feature = "syntax")]
    {
        engine::set()
            .find_syntax_by_first_line(line)
            .filter(|syntax| syntax.name != "Plain Text")
            .map(|syntax| syntax.name.clone())
    }
    #[cfg(not(feature = "syntax"))]
    {
        let _ = line;
        None
    }
}

/// Every grammar's name, for a language menu.
pub fn grammar_names() -> Vec<String> {
    #[cfg(feature = "syntax")]
    {
        let mut names: Vec<String> = engine::set()
            .syntaxes()
            .iter()
            .filter(|syntax| !syntax.hidden && syntax.name != "Plain Text")
            .map(|syntax| syntax.name.clone())
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        names
    }
    #[cfg(not(feature = "syntax"))]
    {
        Vec::new()
    }
}

/// The highlighting of one document. See the [module](self).
pub struct Highlighter {
    #[cfg(feature = "syntax")]
    syntax: Option<&'static syntect::parsing::SyntaxReference>,
    #[cfg(feature = "syntax")]
    checkpoints: Vec<engine::State>,
    #[cfg(feature = "syntax")]
    kinds: engine::Kinds,
    lines: HashMap<usize, Rc<[Span]>>,
}

/// What [`Highlighter::spans`] could do within its budget.
pub struct Lines {
    /// The runs of each line asked for; `None` for a line not reached yet.
    pub lines: Vec<Option<Rc<[Span]>>>,
    /// Every line asked for has its runs.
    pub done: bool,
}

impl Highlighter {
    /// A highlighter for `language`; one with no grammar leaves every line
    /// plain.
    pub fn new(language: &Language) -> Self {
        #[cfg(feature = "syntax")]
        {
            let syntax = if language.is_plain() {
                None
            } else {
                engine::find(language.syntax_name())
            };
            Self {
                syntax,
                checkpoints: syntax
                    .map(|syntax| vec![engine::State::new(syntax)])
                    .unwrap_or_default(),
                kinds: HashMap::new(),
                lines: HashMap::new(),
            }
        }
        #[cfg(not(feature = "syntax"))]
        {
            let _ = language;
            Self {
                lines: HashMap::new(),
            }
        }
    }

    /// True when the language has a grammar.
    pub fn is_active(&self) -> bool {
        #[cfg(feature = "syntax")]
        {
            self.syntax.is_some()
        }
        #[cfg(not(feature = "syntax"))]
        {
            false
        }
    }

    /// Line `line` changed, and everything after it may parse differently.
    pub fn invalidate_from(&mut self, line: usize) {
        #[cfg(feature = "syntax")]
        self.checkpoints.truncate(line / CHECKPOINT + 1);
        self.lines.retain(|key, _| *key < line);
    }

    /// The runs of lines `first..last` of `rope`, parsing at most `budget`
    /// lines that are not known yet.
    pub fn spans(&mut self, rope: &Rope, first: usize, last: usize, budget: usize) -> Lines {
        let last = last.min(rope.len_lines());
        let first = first.min(last);
        if !self.is_active() {
            return Lines {
                lines: vec![None; last - first],
                done: true,
            };
        }
        if (first..last).all(|line| self.lines.contains_key(&line)) {
            return Lines {
                lines: (first..last)
                    .map(|line| self.lines.get(&line).cloned())
                    .collect(),
                done: true,
            };
        }
        #[cfg(feature = "syntax")]
        {
            self.parse_to(rope, first, last, budget);
        }
        #[cfg(not(feature = "syntax"))]
        let _ = budget;
        // Keep the cache to a few screens around what is shown.
        if self.lines.len() > 4096 {
            self.lines
                .retain(|line, _| *line + 1024 >= first && *line < last + 1024);
        }
        let lines: Vec<Option<Rc<[Span]>>> = (first..last)
            .map(|line| self.lines.get(&line).cloned())
            .collect();
        let done = lines.iter().all(Option::is_some);
        Lines { lines, done }
    }

    #[cfg(feature = "syntax")]
    fn parse_to(&mut self, rope: &Rope, first: usize, last: usize, budget: usize) {
        let mut budget = budget;
        // The nearest checkpoint at or before `first`, extending the
        // checkpoints forward from the last one when needed.
        let known = (self.checkpoints.len() - 1) * CHECKPOINT;
        let start_block = first / CHECKPOINT;
        let (mut state, mut line) = if start_block < self.checkpoints.len() {
            (
                self.checkpoints[start_block].clone(),
                start_block * CHECKPOINT,
            )
        } else {
            (self.checkpoints[self.checkpoints.len() - 1].clone(), known)
        };
        let mut text = String::new();
        while line < last && budget > 0 {
            if line % CHECKPOINT == 0 && line / CHECKPOINT == self.checkpoints.len() {
                self.checkpoints.push(state.clone());
            }
            let slice = rope.line(line);
            let spans: Vec<Span> = if slice.len_chars() > LONGEST_PARSED_LINE {
                Vec::new()
            } else {
                text.clear();
                for chunk in slice.chunks() {
                    text.push_str(chunk);
                }
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                engine::line(&mut state, &text, &mut self.kinds)
            };
            if line >= first || !self.lines.contains_key(&line) {
                self.lines.entry(line).or_insert_with(|| spans.into());
            }
            line += 1;
            budget -= 1;
        }
        if line % CHECKPOINT == 0
            && line / CHECKPOINT == self.checkpoints.len()
            && line < rope.len_lines()
        {
            self.checkpoints.push(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_map_to_kinds() {
        assert_eq!(
            kind_of_scope("comment.line.double-slash.rust"),
            Some(TokenKind::Comment)
        );
        assert_eq!(
            kind_of_scope("keyword.operator.arithmetic"),
            Some(TokenKind::Operator)
        );
        assert_eq!(
            kind_of_scope("keyword.control.rust"),
            Some(TokenKind::Keyword)
        );
        assert_eq!(kind_of_scope("stringy"), None);
        assert_eq!(kind_of_scope("meta.block.rust"), None);
        assert_eq!(
            classify(&[
                "source.json",
                "meta.mapping.key.json",
                "string.quoted.double.json"
            ]),
            Some(TokenKind::Property)
        );
        assert_eq!(
            classify(&[
                "source.rust",
                "comment.line",
                "punctuation.definition.comment"
            ]),
            Some(TokenKind::Comment)
        );
    }

    #[cfg(feature = "syntax")]
    #[test]
    fn rust_is_highlighted_lazily() {
        let language = crate::language::detect(Some(std::path::Path::new("a.rs")), "");
        let mut text = String::new();
        for index in 0..500 {
            text.push_str(&format!("fn f{index}() -> u32 {{ 42 }} // note\n"));
        }
        let rope = Rope::from_str(&text);
        let mut highlighter = Highlighter::new(&language);
        assert!(highlighter.is_active());
        // A budget too small to reach line 400 from the top.
        let partial = highlighter.spans(&rope, 400, 410, 100);
        assert!(!partial.done);
        let mut done = false;
        for _ in 0..10 {
            done = highlighter.spans(&rope, 400, 410, 100).done;
            if done {
                break;
            }
        }
        assert!(done);
        let lines = highlighter.spans(&rope, 400, 401, 100);
        let spans = lines.lines[0].clone().unwrap();
        let line = rope.line(400).to_string();
        let kinds: Vec<(&str, TokenKind)> = spans
            .iter()
            .map(|(range, kind)| (&line[range.clone()], *kind))
            .collect();
        assert!(kinds.contains(&("fn", TokenKind::Keyword)), "{kinds:?}");
        assert!(
            kinds
                .iter()
                .any(|(text, kind)| text.contains("note") && *kind == TokenKind::Comment),
            "{kinds:?}"
        );
        assert!(kinds.contains(&("42", TokenKind::Number)), "{kinds:?}");
        // An edit throws away what follows it.
        highlighter.invalidate_from(200);
        assert!(!highlighter.spans(&rope, 400, 401, 50).done);
    }

    #[cfg(feature = "syntax")]
    #[test]
    fn markdown_headings() {
        let language = crate::language::detect(Some(std::path::Path::new("a.md")), "");
        let rope = Rope::from_str("# Title\n\nSome *text* and `code`.\n");
        let mut highlighter = Highlighter::new(&language);
        let lines = highlighter.spans(&rope, 0, 3, 100);
        assert!(lines.done);
        let first = lines.lines[0].clone().unwrap();
        assert!(
            first.iter().any(|(_, kind)| *kind == TokenKind::Heading),
            "{first:?}"
        );
    }
}
