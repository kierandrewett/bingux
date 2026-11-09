//! Writing numbers and expressions for the screen.

use crate::lexer::{Tok, lex};
use crate::locale::Locale;
use crate::number::Num;

/// The real minus sign, as the display writes it.
pub const MINUS: char = '\u{2212}';

/// How a result is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub locale: Locale,
    /// Significant digits for a value that does not end (a third, a root).
    pub precision: usize,
    /// Separate the digits of the integer part into groups.
    pub group: bool,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            locale: Locale::EN,
            precision: 16,
            group: true,
        }
    }
}

/// A number as shown, and whether digits were cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub text: String,
    /// The value is not exactly what the text says.
    pub rounded: bool,
}

pub fn show(n: &Num, style: &Style) -> Shown {
    let d = n.to_decimal(style.precision);
    let mut text = String::new();
    if d.negative {
        text.push(MINUS);
    }
    if d.is_scientific() {
        let (head, tail) = d.mantissa();
        text.push_str(&head);
        if !tail.is_empty() {
            text.push(style.locale.decimal);
            text.push_str(&tail);
        }
        text.push('e');
        if d.e < 0 {
            text.push(MINUS);
        }
        text.push_str(&d.e.unsigned_abs().to_string());
    } else {
        let (int, frac) = d.positional();
        if style.group {
            text.push_str(&style.locale.group_digits(&int));
        } else {
            text.push_str(&int);
        }
        if !frac.is_empty() {
            text.push(style.locale.decimal);
            text.push_str(&frac);
        }
    }
    Shown {
        text,
        rounded: d.rounded,
    }
}

/// The number as machine text: ASCII minus, a point, no group marks. It
/// reads back as the same number (to the digits kept) in any locale.
pub fn plain(n: &Num, precision: usize) -> String {
    let d = n.to_decimal(precision);
    let mut text = String::new();
    if d.negative {
        text.push('-');
    }
    if d.is_scientific() {
        let (head, tail) = d.mantissa();
        text.push_str(&head);
        if !tail.is_empty() {
            text.push('.');
            text.push_str(&tail);
        }
        text.push_str(&format!("e{}", d.e));
    } else {
        let (int, frac) = d.positional();
        text.push_str(&int);
        if !frac.is_empty() {
            text.push('.');
            text.push_str(&frac);
        }
    }
    text
}

/// The expression as the history and the echo line show it: grouped digits,
/// `×` and `÷`, spaces round binary operators, a real minus sign and `π`.
/// Text that does not lex is returned as it is.
pub fn pretty_expression(source: &str, locale: &Locale, group: bool) -> String {
    let Ok(tokens) = lex(source, locale) else {
        return source.to_string();
    };
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut previous_value = false;
    for token in &tokens {
        let span: String = chars[token.at..token.end.min(chars.len())].iter().collect();
        let is_value_end = matches!(
            token.tok,
            Tok::Num(_) | Tok::RParen | Tok::Bang | Tok::Percent | Tok::Degree | Tok::Super(_)
        ) || matches!(&token.tok, Tok::Ident(n) if matches!(n.as_str(), "pi" | "e" | "phi" | "tau" | "ans" | "mem"));
        match &token.tok {
            Tok::Num(_) => out.push_str(&regroup(&span, locale, group)),
            Tok::Plus if previous_value => out.push_str(" + "),
            Tok::Minus if previous_value => out.push_str(&format!(" {MINUS} ")),
            Tok::Minus => out.push(MINUS),
            Tok::Star => out.push_str(" \u{d7} "),
            Tok::Slash => out.push_str(" \u{f7} "),
            Tok::Sep => out.push_str("; "),
            Tok::Ident(name) => {
                let word = match name.as_str() {
                    "pi" => "\u{3c0}",
                    "tau" => "\u{3c4}",
                    "phi" => "\u{3c6}",
                    other => other,
                };
                // A word operator stands apart from its operands.
                if matches!(name.as_str(), "mod" | "of") {
                    out.push_str(&format!(" {word} "));
                } else {
                    out.push_str(word);
                }
            }
            _ => out.push_str(&span),
        }
        previous_value = is_value_end;
    }
    out.trim().to_string()
}

/// A typed number with its integer digits grouped and the locale's decimal
/// mark. A number with an exponent or a base prefix is left alone.
fn regroup(span: &str, locale: &Locale, group: bool) -> String {
    let (int, frac, has_point) = match span.find(['.', ',']) {
        Some(at) => (&span[..at], &span[at + 1..], true),
        None => (span, "", false),
    };
    let plain_digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if !plain_digits(int) || !plain_digits(frac) {
        return span.to_string();
    }
    let mut out = if group {
        locale.group_digits(int)
    } else {
        int.to_string()
    };
    if has_point {
        out.push(locale.decimal);
        out.push_str(frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{Env, evaluate};

    fn calc(src: &str) -> Num {
        evaluate(src, &mut Env::seeded(1), &Locale::EN).unwrap()
    }

    fn style() -> Style {
        Style::default()
    }

    #[test]
    fn grouping_and_the_minus_sign() {
        assert_eq!(show(&calc("1234567.891"), &style()).text, "1,234,567.891");
        assert_eq!(show(&calc("-1234"), &style()).text, "\u{2212}1,234");
        let ungrouped = Style {
            group: false,
            ..style()
        };
        assert_eq!(show(&calc("1234567"), &ungrouped).text, "1234567");
    }

    #[test]
    fn locale_marks() {
        let de = Style {
            locale: Locale::from_tag("de_DE"),
            ..style()
        };
        assert_eq!(show(&calc("1234567.5"), &de).text, "1.234.567,5");
        let fr = Style {
            locale: Locale::from_tag("fr_FR"),
            ..style()
        };
        assert_eq!(show(&calc("1234.5"), &fr).text, "1\u{202f}234,5");
        let inr = Style {
            locale: Locale::from_tag("en_IN"),
            ..style()
        };
        assert_eq!(show(&calc("1234567"), &inr).text, "12,34,567");
    }

    #[test]
    fn large_and_small_values() {
        assert_eq!(
            show(&calc("2^70"), &style()).text,
            "1,180,591,620,717,411,303,424"
        );
        assert_eq!(
            show(&calc("1e30"), &style()).text,
            "1,000,000,000,000,000,000,000,000,000,000"
        );
        assert_eq!(
            show(&calc("sqrt(2)*1e-10"), &style()).text,
            "1.414213562373095e\u{2212}10"
        );
        assert_eq!(
            show(&calc("sqrt(2)*1e25"), &style()).text,
            "1.414213562373095e25"
        );
        assert_eq!(show(&calc("1e-30"), &style()).text, "1e\u{2212}30");
        assert_eq!(show(&calc("0.00001"), &style()).text, "0.00001");
    }

    #[test]
    fn rounding_is_reported() {
        assert!(show(&calc("1/3"), &style()).rounded);
        assert!(!show(&calc("1/4"), &style()).rounded);
        assert!(show(&calc("sqrt(2)"), &style()).rounded);
    }

    #[test]
    fn precision_is_adjustable() {
        let wide = Style {
            precision: 40,
            ..style()
        };
        assert_eq!(
            show(&calc("1/3"), &wide).text,
            "0.3333333333333333333333333333333333333333"
        );
        let narrow = Style {
            precision: 6,
            ..style()
        };
        assert_eq!(show(&calc("pi"), &narrow).text, "3.14159");
    }

    #[test]
    fn plain_text_reads_back() {
        for src in [
            "1234567.891",
            "-0.5",
            "1/3",
            "2^100",
            "sqrt(2)",
            "1e-30",
            "1e30",
        ] {
            let n = calc(src);
            let text = plain(&n, 16);
            let back = calc(&text);
            // Within the digits shown.
            assert_eq!(plain(&back, 16), text, "{src} -> {text}");
        }
        assert_eq!(plain(&calc("-1234.5"), 16), "-1234.5");
        assert_eq!(plain(&calc("1e-30"), 16), "1e-30");
    }

    #[test]
    fn expressions_are_made_readable() {
        let en = Locale::EN;
        assert_eq!(
            pretty_expression("1234*5678", &en, true),
            "1,234 \u{d7} 5,678"
        );
        assert_eq!(pretty_expression("2x3", &en, true), "2 \u{d7} 3");
        assert_eq!(
            pretty_expression("-5+-3", &en, true),
            "\u{2212}5 + \u{2212}3"
        );
        assert_eq!(pretty_expression("10-3", &en, true), "10 \u{2212} 3");
        assert_eq!(pretty_expression("2pi", &en, true), "2\u{3c0}");
        assert_eq!(
            pretty_expression("sin(30)/2", &en, true),
            "sin(30) \u{f7} 2"
        );
        assert_eq!(pretty_expression("7 mod 3", &en, true), "7 mod 3");
        assert_eq!(pretty_expression("1234.5e3", &en, true), "1234.5e3");
        let de = Locale::from_tag("de_DE");
        assert_eq!(pretty_expression("1234,5+1", &de, true), "1.234,5 + 1");
        assert_eq!(pretty_expression("5 +", &en, true), "5 +");
        assert_eq!(pretty_expression("5 $", &en, true), "5 $");
    }
}
