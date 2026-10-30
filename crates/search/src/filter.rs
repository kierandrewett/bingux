//! Filter expressions: `category = 'Network' AND tags IN [a, b]`.
//!
//! The grammar is a subset of Meilisearch's: comparisons (`= != < <= > >=`),
//! `field a TO b`, `IN [..]`, `EXISTS`, `NOT`, `AND`, `OR` and parentheses.

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Str(String),
    Num(f64),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Filter {
    Eq(String, Lit),
    Ne(String, Lit),
    Lt(String, f64),
    Le(String, f64),
    Gt(String, f64),
    Ge(String, f64),
    /// Inclusive.
    Between(String, f64, f64),
    In(String, Vec<Lit>),
    Exists(String),
    Not(Box<Filter>),
    And(Vec<Filter>),
    Or(Vec<Filter>),
}

impl Filter {
    pub fn parse(text: &str) -> Result<Filter> {
        let tokens = lex(text)?;
        let mut p = Parser { tokens, pos: 0 };
        let f = p.or()?;
        if p.pos != p.tokens.len() {
            return Err(Error::request(format!(
                "unexpected `{}` in filter",
                p.tokens[p.pos].describe()
            )));
        }
        Ok(f)
    }

    pub fn and(self, other: Filter) -> Filter {
        match self {
            Filter::And(mut v) => {
                v.push(other);
                Filter::And(v)
            }
            f => Filter::And(vec![f, other]),
        }
    }

    /// Every field the filter names.
    pub fn fields<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Filter::Eq(f, _)
            | Filter::Ne(f, _)
            | Filter::Lt(f, _)
            | Filter::Le(f, _)
            | Filter::Gt(f, _)
            | Filter::Ge(f, _)
            | Filter::Between(f, _, _)
            | Filter::In(f, _)
            | Filter::Exists(f) => out.push(f),
            Filter::Not(inner) => inner.fields(out),
            Filter::And(v) | Filter::Or(v) => v.iter().for_each(|f| f.fields(out)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Quoted(String),
    Num(f64),
    Sym(&'static str),
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Tok::Word(w) | Tok::Quoted(w) => w.clone(),
            Tok::Num(n) => n.to_string(),
            Tok::Sym(s) => s.to_string(),
        }
    }

    fn keyword(&self, kw: &str) -> bool {
        matches!(self, Tok::Word(w) if w.eq_ignore_ascii_case(kw))
    }
}

fn lex(text: &str) -> Result<Vec<Tok>> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '(' | ')' | '[' | ']' | ',' => {
                out.push(Tok::Sym(match c {
                    '(' => "(",
                    ')' => ")",
                    '[' => "[",
                    ']' => "]",
                    _ => ",",
                }));
                i += 1;
            }
            '=' => {
                out.push(Tok::Sym("="));
                i += 1;
            }
            '!' if chars.get(i + 1) == Some(&'=') => {
                out.push(Tok::Sym("!="));
                i += 2;
            }
            '<' | '>' => {
                if chars.get(i + 1) == Some(&'=') {
                    out.push(Tok::Sym(if c == '<' { "<=" } else { ">=" }));
                    i += 2;
                } else {
                    out.push(Tok::Sym(if c == '<' { "<" } else { ">" }));
                    i += 1;
                }
            }
            '\'' | '"' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                let mut closed = false;
                while i < chars.len() {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        s.push(chars[i + 1]);
                        i += 2;
                    } else if chars[i] == quote {
                        closed = true;
                        i += 1;
                        break;
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                if !closed {
                    return Err(Error::request("unterminated string in filter"));
                }
                out.push(Tok::Quoted(s));
            }
            _ => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], '(' | ')' | '[' | ']' | ',' | '=' | '<' | '>' | '\'' | '"')
                    && !(chars[i] == '!' && chars.get(i + 1) == Some(&'='))
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                match word.parse::<f64>() {
                    Ok(n) if n.is_finite() => out.push(Tok::Num(n)),
                    _ => out.push(Tok::Word(word)),
                }
            }
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn or(&mut self) -> Result<Filter> {
        let mut items = vec![self.and()?];
        while self.peek().is_some_and(|t| t.keyword("OR")) {
            self.pos += 1;
            items.push(self.and()?);
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            Filter::Or(items)
        })
    }

    fn and(&mut self) -> Result<Filter> {
        let mut items = vec![self.unary()?];
        while self.peek().is_some_and(|t| t.keyword("AND")) {
            self.pos += 1;
            items.push(self.unary()?);
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            Filter::And(items)
        })
    }

    fn unary(&mut self) -> Result<Filter> {
        if self.peek().is_some_and(|t| t.keyword("NOT")) {
            self.pos += 1;
            return Ok(Filter::Not(Box::new(self.unary()?)));
        }
        if self.peek() == Some(&Tok::Sym("(")) {
            self.pos += 1;
            let f = self.or()?;
            if self.next() != Some(Tok::Sym(")")) {
                return Err(Error::request("missing `)` in filter"));
            }
            return Ok(f);
        }
        self.condition()
    }

    fn lit(&mut self) -> Result<Lit> {
        match self.next() {
            Some(Tok::Quoted(s)) => Ok(Lit::Str(s)),
            Some(Tok::Num(n)) => Ok(Lit::Num(n)),
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("true") => Ok(Lit::Bool(true)),
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("false") => Ok(Lit::Bool(false)),
            Some(Tok::Word(w)) => Ok(Lit::Str(w)),
            other => Err(Error::request(format!(
                "expected a value in filter, found {}",
                other.map_or("the end".to_string(), |t| t.describe())
            ))),
        }
    }

    fn number(&mut self) -> Result<f64> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(n),
            other => Err(Error::request(format!(
                "expected a number in filter, found {}",
                other.map_or("the end".to_string(), |t| t.describe())
            ))),
        }
    }

    fn condition(&mut self) -> Result<Filter> {
        let field = match self.next() {
            Some(Tok::Word(w)) | Some(Tok::Quoted(w)) => w,
            Some(Tok::Num(n)) => n.to_string(),
            other => {
                return Err(Error::request(format!(
                    "expected a field name in filter, found {}",
                    other.map_or("the end".to_string(), |t| t.describe())
                )));
            }
        };
        let Some(op) = self.next() else {
            return Err(Error::request(format!("filter `{field}` has no operator")));
        };
        match op {
            Tok::Sym("=") => Ok(Filter::Eq(field, self.lit()?)),
            Tok::Sym("!=") => Ok(Filter::Ne(field, self.lit()?)),
            Tok::Sym("<") => Ok(Filter::Lt(field, self.number()?)),
            Tok::Sym("<=") => Ok(Filter::Le(field, self.number()?)),
            Tok::Sym(">") => Ok(Filter::Gt(field, self.number()?)),
            Tok::Sym(">=") => Ok(Filter::Ge(field, self.number()?)),
            t if t.keyword("EXISTS") => Ok(Filter::Exists(field)),
            t if t.keyword("NOT") => {
                let next = self.next();
                if next.as_ref().is_some_and(|t| t.keyword("EXISTS")) {
                    Ok(Filter::Not(Box::new(Filter::Exists(field))))
                } else if next.as_ref().is_some_and(|t| t.keyword("IN")) {
                    Ok(Filter::Not(Box::new(self.in_list(field)?)))
                } else {
                    Err(Error::request("expected EXISTS or IN after NOT"))
                }
            }
            t if t.keyword("IN") => self.in_list(field),
            Tok::Num(a) => {
                // `field a TO b`: the lexer read `a` as the first number.
                if self.next().is_some_and(|t| t.keyword("TO")) {
                    Ok(Filter::Between(field, a, self.number()?))
                } else {
                    Err(Error::request("expected TO in a range filter"))
                }
            }
            other => Err(Error::request(format!(
                "unexpected `{}` after `{field}` in filter",
                other.describe()
            ))),
        }
    }

    fn in_list(&mut self, field: String) -> Result<Filter> {
        if self.next() != Some(Tok::Sym("[")) {
            return Err(Error::request("expected `[` after IN"));
        }
        let mut items = Vec::new();
        loop {
            if self.peek() == Some(&Tok::Sym("]")) {
                self.pos += 1;
                break;
            }
            items.push(self.lit()?);
            match self.next() {
                Some(Tok::Sym(",")) => {}
                Some(Tok::Sym("]")) => break,
                _ => return Err(Error::request("expected `,` or `]` in IN list")),
            }
        }
        Ok(Filter::In(field, items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_forms() {
        let f = Filter::parse("category = 'Network' AND (price 10 TO 20 OR tags IN [a, \"b c\"])")
            .unwrap();
        assert_eq!(
            f,
            Filter::And(vec![
                Filter::Eq("category".into(), Lit::Str("Network".into())),
                Filter::Or(vec![
                    Filter::Between("price".into(), 10.0, 20.0),
                    Filter::In("tags".into(), vec![Lit::Str("a".into()), Lit::Str("b c".into())]),
                ]),
            ])
        );
        assert!(matches!(
            Filter::parse("NOT owner EXISTS").unwrap(),
            Filter::Not(_)
        ));
        assert!(Filter::parse("price >= 3").is_ok());
    }

    #[test]
    fn rejects_garbage_with_a_reason() {
        for bad in ["category =", "a = 'x", "a IN b", "(a = 1", "a = 1 b"] {
            assert!(Filter::parse(bad).is_err(), "{bad}");
        }
    }
}
