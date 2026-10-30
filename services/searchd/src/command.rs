//! Explicit command mode for Super search.
//!
//! A query whose first non-blank character is `>` asks to run a command line.
//! The text is split into an argument vector with a small quote-aware tokenizer
//! and executed directly. No shell, globbing, variable expansion or command
//! substitution ever runs, so quoting only groups words.

use crate::protocol::{ProviderResult, ResultKind};
use crate::providers::{Activation, Candidate};

pub const COMMAND_PROVIDER_ID: &str = "command";
const COMMAND_PREFIX: char = '>';

/// True when the query addresses command mode, whether or not it parses.
pub fn is_command_query(query: &str) -> bool {
    query.trim_start().starts_with(COMMAND_PREFIX)
}

/// Splits the text after `>` into `[program, arguments...]`.
///
/// Returns `None` for an empty command, an empty program name, an unclosed
/// quote, a dangling backslash, or a NUL byte, since none of these can be
/// executed as written.
pub fn parse_command(query: &str) -> Option<Vec<String>> {
    let rest = query.trim_start().strip_prefix(COMMAND_PREFIX)?;
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote = Quote::None;
    let mut characters = rest.chars();

    while let Some(character) = characters.next() {
        if character == '\0' {
            return None;
        }
        match quote {
            Quote::None => match character {
                whitespace if whitespace.is_whitespace() => {
                    if in_word {
                        words.push(std::mem::take(&mut word));
                        in_word = false;
                    }
                }
                '\'' => {
                    quote = Quote::Single;
                    in_word = true;
                }
                '"' => {
                    quote = Quote::Double;
                    in_word = true;
                }
                '\\' => {
                    word.push(characters.next()?);
                    in_word = true;
                }
                other => {
                    word.push(other);
                    in_word = true;
                }
            },
            Quote::Single => match character {
                '\'' => quote = Quote::None,
                other => word.push(other),
            },
            Quote::Double => match character {
                '"' => quote = Quote::None,
                '\\' => match characters.next()? {
                    escaped @ ('"' | '\\') => word.push(escaped),
                    other => {
                        word.push('\\');
                        word.push(other);
                    }
                },
                other => word.push(other),
            },
        }
    }

    if quote != Quote::None {
        return None;
    }
    if in_word {
        words.push(word);
    }
    match words.first() {
        Some(program) if !program.is_empty() => Some(words),
        _ => None,
    }
}

/// Builds the single command row shown for a command-mode query.
pub fn command_candidate(query: &str) -> Option<Candidate> {
    let words = parse_command(query)?;
    let (program, arguments) = words.split_first()?;
    let candidate = Candidate {
        provider_id: COMMAND_PROVIDER_ID.to_owned(),
        result: ProviderResult {
            result_id: "command".to_owned(),
            kind: ResultKind::Command,
            title: query.trim_start().strip_prefix(COMMAND_PREFIX)?.trim().to_owned(),
            subtitle: "Run command".to_owned(),
            icon: "utilities-terminal".to_owned(),
            score: 1.0,
        },
        desktop_id: None,
        activation: Activation::Spawn {
            program: program.clone(),
            arguments: arguments.to_vec(),
        },
    };
    candidate.result.validate().ok()?;
    Some(candidate)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(query: &str) -> Vec<String> {
        parse_command(query).expect("command parses")
    }

    #[test]
    fn splits_plain_words_into_argv() {
        assert_eq!(words(">ls -la /tmp"), ["ls", "-la", "/tmp"]);
        assert_eq!(words("  >   echo   hi  "), ["echo", "hi"]);
    }

    #[test]
    fn quotes_group_words_without_expansion() {
        assert_eq!(words(r#">printf "a b" 'c d'"#), ["printf", "a b", "c d"]);
        assert_eq!(words(r#">echo "$HOME" '*' ~"#), ["echo", "$HOME", "*", "~"]);
        assert_eq!(words(r#">echo "a\"b\\c" "x\y""#), ["echo", r#"a"b\c"#, r"x\y"]);
        assert_eq!(words(r#">echo a\ b"#), ["echo", "a b"]);
        assert_eq!(words(">echo ''"), ["echo", ""]);
    }

    #[test]
    fn rejects_commands_that_cannot_be_executed_as_written() {
        assert_eq!(parse_command(">"), None);
        assert_eq!(parse_command(">   "), None);
        assert_eq!(parse_command("> ''"), None);
        assert_eq!(parse_command(r#">echo "unclosed"#), None);
        assert_eq!(parse_command(">echo 'unclosed"), None);
        assert_eq!(parse_command(">echo trailing\\"), None);
        assert_eq!(parse_command(">echo \u{0}"), None);
        assert_eq!(parse_command("ls -la"), None);
    }

    #[test]
    fn detects_command_mode_after_leading_whitespace() {
        assert!(is_command_query(">ls"));
        assert!(is_command_query("   >ls"));
        assert!(!is_command_query("ls >"));
        assert!(!is_command_query("!explain"));
    }

    #[test]
    fn candidate_is_a_labelled_spawn_with_arguments() {
        let candidate = command_candidate(">  printf 'a b' c ").expect("candidate");
        assert_eq!(candidate.provider_id, COMMAND_PROVIDER_ID);
        assert_eq!(candidate.result.kind, ResultKind::Command);
        assert_eq!(candidate.result.title, "printf 'a b' c");
        assert_eq!(candidate.result.subtitle, "Run command");
        assert_eq!(candidate.desktop_id, None);
        assert_eq!(
            candidate.activation,
            Activation::Spawn {
                program: "printf".to_owned(),
                arguments: vec!["a b".to_owned(), "c".to_owned()],
            }
        );
    }

    #[test]
    fn no_candidate_for_unparseable_command_text() {
        assert!(command_candidate(">echo 'open").is_none());
        assert!(command_candidate(">").is_none());
    }
}
