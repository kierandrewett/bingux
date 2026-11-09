//! Parses tokens into a tree.
//!
//! Precedence, loosest first: `+ -`; `* / mod of`; juxtaposition (`2π`,
//! `2(3)`, `3sin(30)`); unary `-`; `^` (right to left); the postfix `! % ° ²`.
//! So `-2^2` is `-4`, `2^3^2` is `2^9`, `1/2π` is `1/(2π)` and `2^-1` works.

use crate::error::{CalcError, Result};
use crate::lexer::{Tok, Token};
use crate::number::Num;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Const {
    Pi,
    E,
    Phi,
    Tau,
    Ans,
    Mem,
    Rand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Func {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Sinh,
    Cosh,
    Tanh,
    Asinh,
    Acosh,
    Atanh,
    Ln,
    Log,
    Log2,
    Exp,
    Sqrt,
    Cbrt,
    Root,
    Abs,
    Floor,
    Ceil,
    Round,
    Trunc,
    Sign,
    Fact,
    Gcd,
    Lcm,
    NCr,
    NPr,
    Min,
    Max,
    Mod,
}

impl Func {
    fn by_name(name: &str) -> Option<Func> {
        Some(match name {
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "asin" | "arcsin" => Func::Asin,
            "acos" | "arccos" => Func::Acos,
            "atan" | "arctan" => Func::Atan,
            "atan2" => Func::Atan2,
            "sinh" => Func::Sinh,
            "cosh" => Func::Cosh,
            "tanh" => Func::Tanh,
            "asinh" | "arsinh" => Func::Asinh,
            "acosh" | "arcosh" => Func::Acosh,
            "atanh" | "artanh" => Func::Atanh,
            "ln" => Func::Ln,
            "log" | "log10" | "lg" => Func::Log,
            "log2" | "lb" => Func::Log2,
            "exp" => Func::Exp,
            "sqrt" => Func::Sqrt,
            "cbrt" => Func::Cbrt,
            "root" => Func::Root,
            "abs" => Func::Abs,
            "floor" => Func::Floor,
            "ceil" | "ceiling" => Func::Ceil,
            "round" => Func::Round,
            "trunc" => Func::Trunc,
            "sign" | "sgn" => Func::Sign,
            "fact" | "factorial" => Func::Fact,
            "gcd" => Func::Gcd,
            "lcm" => Func::Lcm,
            "ncr" | "choose" => Func::NCr,
            "npr" | "perm" => Func::NPr,
            "min" => Func::Min,
            "max" => Func::Max,
            "mod" => Func::Mod,
            _ => return None,
        })
    }

    /// The fewest and the most arguments, `None` for no limit.
    pub(crate) fn arity(self) -> (usize, Option<usize>) {
        match self {
            Func::Atan2
            | Func::Root
            | Func::Gcd
            | Func::Lcm
            | Func::NCr
            | Func::NPr
            | Func::Mod => (2, Some(2)),
            Func::Log | Func::Round => (1, Some(2)),
            Func::Min | Func::Max => (1, None),
            _ => (1, Some(1)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ast {
    Num(Num),
    Const(Const),
    Neg(Box<Ast>),
    Bin(Op, Box<Ast>, Box<Ast>),
    Percent(Box<Ast>),
    Factorial(Box<Ast>),
    Degrees(Box<Ast>),
    Call(Func, Vec<Ast>),
}

/// The deepest nesting of brackets and operators accepted.
const MAX_DEPTH: usize = 150;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
    end: usize,
}

fn syntax(at: usize, message: impl Into<String>) -> CalcError {
    CalcError::Syntax {
        at,
        message: message.into(),
    }
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Num(_) => "a number".into(),
        Tok::Ident(name) => name.clone(),
        Tok::Plus => "+".into(),
        Tok::Minus => "-".into(),
        Tok::Star => "\u{d7}".into(),
        Tok::Slash => "\u{f7}".into(),
        Tok::Caret => "^".into(),
        Tok::Percent => "%".into(),
        Tok::Bang => "!".into(),
        Tok::Degree => "\u{b0}".into(),
        Tok::Sqrt => "\u{221a}".into(),
        Tok::Cbrt => "\u{221b}".into(),
        Tok::Super(n) => if *n == 2 { "\u{b2}" } else { "\u{b3}" }.into(),
        Tok::LParen => "(".into(),
        Tok::RParen => ")".into(),
        Tok::Sep => ";".into(),
    }
}

fn is_word_operator(name: &str) -> bool {
    matches!(name, "mod" | "of" | "to" | "in" | "as")
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_tok(&self) -> Option<&Tok> {
        self.peek().map(|t| &t.tok)
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn unexpected(&self, token: &Token) -> CalcError {
        syntax(
            token.at,
            format!("Unexpected {}. Check the expression.", describe(&token.tok)),
        )
    }

    fn descend(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(syntax(
                self.peek().map_or(self.end, |t| t.at),
                "Too many brackets are nested. Remove some of them.",
            ));
        }
        Ok(())
    }

    fn additive(&mut self) -> Result<Ast> {
        self.descend()?;
        let mut left = self.term()?;
        loop {
            let op = match self.peek_tok() {
                Some(Tok::Plus) => Op::Add,
                Some(Tok::Minus) => Op::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.term()?;
            left = Ast::Bin(op, Box::new(left), Box::new(right));
        }
        self.depth -= 1;
        Ok(left)
    }

    fn term(&mut self) -> Result<Ast> {
        let mut left = self.chain()?;
        loop {
            let op = match self.peek_tok() {
                Some(Tok::Star) => Op::Mul,
                Some(Tok::Slash) => Op::Div,
                Some(Tok::Ident(name)) if name == "mod" => Op::Mod,
                Some(Tok::Ident(name)) if name == "of" => Op::Mul,
                _ => break,
            };
            self.pos += 1;
            let right = self.chain()?;
            left = Ast::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// Whether the next token begins a factor, so it multiplies the one
    /// before it without a sign.
    fn starts_factor(&self) -> bool {
        match self.peek_tok() {
            Some(Tok::Num(_) | Tok::LParen | Tok::Sqrt | Tok::Cbrt) => true,
            Some(Tok::Ident(name)) => !is_word_operator(name),
            _ => false,
        }
    }

    fn chain(&mut self) -> Result<Ast> {
        let mut left = self.unary()?;
        while self.starts_factor() {
            if matches!(self.peek_tok(), Some(Tok::Num(_))) && ends_in_number(&left) {
                let at = self.peek().map_or(self.end, |t| t.at);
                return Err(syntax(
                    at,
                    "A number follows a number. Add an operator between them.",
                ));
            }
            let right = self.power()?;
            left = Ast::Bin(Op::Mul, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Ast> {
        match self.peek_tok() {
            Some(Tok::Minus) => {
                self.pos += 1;
                self.descend()?;
                let inner = self.unary()?;
                self.depth -= 1;
                Ok(Ast::Neg(Box::new(inner)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Ast> {
        let base = self.postfix()?;
        if matches!(self.peek_tok(), Some(Tok::Caret)) {
            self.pos += 1;
            self.descend()?;
            let exponent = self.unary()?;
            self.depth -= 1;
            return Ok(Ast::Bin(Op::Pow, Box::new(base), Box::new(exponent)));
        }
        Ok(base)
    }

    fn postfix(&mut self) -> Result<Ast> {
        let mut value = self.primary()?;
        loop {
            match self.peek_tok() {
                Some(Tok::Bang) => value = Ast::Factorial(Box::new(value)),
                Some(Tok::Percent) => value = Ast::Percent(Box::new(value)),
                Some(Tok::Degree) => value = Ast::Degrees(Box::new(value)),
                Some(Tok::Super(n)) => {
                    let n = *n;
                    value = Ast::Bin(
                        Op::Pow,
                        Box::new(value),
                        Box::new(Ast::Num(Num::from_i64(n as i64))),
                    );
                }
                _ => break,
            }
            self.pos += 1;
        }
        Ok(value)
    }

    fn primary(&mut self) -> Result<Ast> {
        let Some(token) = self.bump() else {
            return Err(CalcError::Incomplete);
        };
        match token.tok {
            Tok::Num(n) => Ok(Ast::Num(n)),
            Tok::LParen => {
                self.descend()?;
                let inner = self.additive()?;
                self.depth -= 1;
                match self.bump() {
                    Some(Token {
                        tok: Tok::RParen, ..
                    }) => Ok(inner),
                    Some(other) => Err(self.unexpected(&other)),
                    None => Err(CalcError::Incomplete),
                }
            }
            Tok::Sqrt => Ok(Ast::Call(Func::Sqrt, vec![self.unary()?])),
            Tok::Cbrt => Ok(Ast::Call(Func::Cbrt, vec![self.unary()?])),
            Tok::Ident(name) => self.named(name, token.at),
            other => {
                let token = Token {
                    tok: other,
                    at: token.at,
                    end: token.end,
                };
                Err(self.unexpected(&token))
            }
        }
    }

    fn named(&mut self, name: String, at: usize) -> Result<Ast> {
        let constant = match name.as_str() {
            "pi" => Some(Const::Pi),
            "e" => Some(Const::E),
            "phi" => Some(Const::Phi),
            "tau" => Some(Const::Tau),
            "ans" => Some(Const::Ans),
            "mem" | "mr" => Some(Const::Mem),
            "rand" | "random" => Some(Const::Rand),
            _ => None,
        };
        if let Some(constant) = constant {
            // `rand()` is allowed too.
            if constant == Const::Rand && matches!(self.peek_tok(), Some(Tok::LParen)) {
                self.pos += 1;
                match self.bump() {
                    Some(Token {
                        tok: Tok::RParen, ..
                    }) => {}
                    Some(other) => return Err(self.unexpected(&other)),
                    None => return Err(CalcError::Incomplete),
                }
            }
            return Ok(Ast::Const(constant));
        }
        let Some(func) = Func::by_name(&name) else {
            return Err(CalcError::Unknown(name));
        };
        let mut args = Vec::new();
        if matches!(self.peek_tok(), Some(Tok::LParen)) {
            self.pos += 1;
            self.descend()?;
            if matches!(self.peek_tok(), Some(Tok::RParen)) {
                self.pos += 1;
            } else {
                loop {
                    args.push(self.additive()?);
                    match self.bump() {
                        Some(Token { tok: Tok::Sep, .. }) => continue,
                        Some(Token {
                            tok: Tok::RParen, ..
                        }) => break,
                        Some(other) => return Err(self.unexpected(&other)),
                        None => return Err(CalcError::Incomplete),
                    }
                }
            }
            self.depth -= 1;
        } else {
            // `sin 30`, `ln 2`: the argument is the factors that follow.
            if self.peek().is_none() {
                return Err(CalcError::Incomplete);
            }
            self.descend()?;
            args.push(self.chain()?);
            self.depth -= 1;
        }
        let (least, most) = func.arity();
        if args.len() < least || most.is_some_and(|most| args.len() > most) {
            let wanted = match most {
                Some(most) if most == least => format!("{least}"),
                Some(most) => format!("{least} or {most}"),
                None => format!("at least {least}"),
            };
            return Err(syntax(
                at,
                format!("{name} takes {wanted} values, separated by ;"),
            ));
        }
        Ok(Ast::Call(func, args))
    }
}

fn ends_in_number(ast: &Ast) -> bool {
    matches!(ast, Ast::Num(_))
}

pub(crate) fn parse(tokens: Vec<Token>) -> Result<Ast> {
    if tokens.is_empty() {
        return Err(CalcError::Incomplete);
    }
    let end = tokens.last().map_or(0, |t| t.at + 1);
    let mut parser = Parser {
        tokens,
        pos: 0,
        depth: 0,
        end,
    };
    let ast = parser.additive()?;
    match parser.peek() {
        None => Ok(ast),
        Some(token) if token.tok == Tok::RParen => Err(syntax(
            token.at,
            "There is a closing bracket with no opening one. Remove it or add the opening one.",
        )),
        Some(token) => Err(parser.unexpected(&token.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::locale::Locale;

    fn tree(src: &str) -> Result<Ast> {
        parse(lex(src, &Locale::EN)?)
    }

    fn n(v: i64) -> Ast {
        Ast::Num(Num::from_i64(v))
    }

    fn bin(op: Op, a: Ast, b: Ast) -> Ast {
        Ast::Bin(op, Box::new(a), Box::new(b))
    }

    #[test]
    fn precedence() {
        assert_eq!(
            tree("1+2*3").unwrap(),
            bin(Op::Add, n(1), bin(Op::Mul, n(2), n(3)))
        );
        assert_eq!(
            tree("-2^2").unwrap(),
            Ast::Neg(Box::new(bin(Op::Pow, n(2), n(2))))
        );
        assert_eq!(
            tree("2^3^2").unwrap(),
            bin(Op::Pow, n(2), bin(Op::Pow, n(3), n(2)))
        );
        assert_eq!(
            tree("2^-1").unwrap(),
            bin(Op::Pow, n(2), Ast::Neg(Box::new(n(1))))
        );
    }

    #[test]
    fn implicit_multiplication_binds_tighter_than_division() {
        assert_eq!(
            tree("1/2pi").unwrap(),
            bin(Op::Div, n(1), bin(Op::Mul, n(2), Ast::Const(Const::Pi)))
        );
        assert_eq!(tree("2(3)").unwrap(), bin(Op::Mul, n(2), n(3)));
        assert_eq!(tree("(1)(2)").unwrap(), bin(Op::Mul, n(1), n(2)));
    }

    #[test]
    fn two_numbers_in_a_row_need_an_operator() {
        assert!(matches!(tree("1 000"), Err(CalcError::Syntax { .. })));
        assert!(matches!(tree("2 3"), Err(CalcError::Syntax { .. })));
    }

    #[test]
    fn postfix_operators() {
        assert_eq!(tree("5!").unwrap(), Ast::Factorial(Box::new(n(5))));
        assert_eq!(tree("50%").unwrap(), Ast::Percent(Box::new(n(50))));
        assert_eq!(tree("30°").unwrap(), Ast::Degrees(Box::new(n(30))));
        assert_eq!(tree("3²").unwrap(), bin(Op::Pow, n(3), n(2)));
    }

    #[test]
    fn functions_with_and_without_brackets() {
        assert_eq!(tree("sin(30)").unwrap(), Ast::Call(Func::Sin, vec![n(30)]));
        assert_eq!(tree("sin 30").unwrap(), Ast::Call(Func::Sin, vec![n(30)]));
        assert_eq!(
            tree("sin 30 + 1").unwrap(),
            bin(Op::Add, Ast::Call(Func::Sin, vec![n(30)]), n(1))
        );
        assert_eq!(
            tree("max(1;2;3)").unwrap(),
            Ast::Call(Func::Max, vec![n(1), n(2), n(3)])
        );
        assert_eq!(tree("√9").unwrap(), Ast::Call(Func::Sqrt, vec![n(9)]));
    }

    #[test]
    fn an_unfinished_expression_is_incomplete() {
        for src in ["5 +", "(2", "sin(", "sin", "2 *", "-", "2^"] {
            assert_eq!(tree(src), Err(CalcError::Incomplete), "{src}");
        }
    }

    #[test]
    fn mistakes_are_reported() {
        assert!(matches!(tree("2 + * 3"), Err(CalcError::Syntax { .. })));
        assert!(matches!(tree("(2))"), Err(CalcError::Syntax { .. })));
        assert!(matches!(tree("foo(2)"), Err(CalcError::Unknown(_))));
        assert!(matches!(tree("sin(1;2)"), Err(CalcError::Syntax { .. })));
        assert!(matches!(tree(")"), Err(CalcError::Syntax { .. })));
    }

    #[test]
    fn nesting_is_limited() {
        let deep = format!("{}1{}", "(".repeat(500), ")".repeat(500));
        assert!(matches!(tree(&deep), Err(CalcError::Syntax { .. })));
    }
}
