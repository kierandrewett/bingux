//! Demangling of Rust v0 symbols (`_R…`) for the sampler's call names.
//!
//! A flame graph is read by name, and a v0 symbol such as
//! `_RNvNtCs8ZK_3std2rt10lang_start` reads as noise. This turns it into
//! `std::rt::lang_start`. It follows the v0 grammar of the Rust reference
//! (the crate `rustc-demangle` implements the same one) for what a call stack
//! needs: paths, generic arguments, closures, trait and inherent impls, and the
//! types that can appear in them. Anything it does not understand makes the
//! whole symbol fall back to its raw text, so a name is never half right.

/// Most nested paths and types followed, so a malformed back-reference cannot
/// loop.
const MAX_DEPTH: u32 = 64;
/// The longest name produced. A longer one is cut: the bar is ellipsised anyway.
const MAX_LEN: usize = 600;

/// The readable form of a v0 symbol, or `None` when it is not a well-formed
/// one.
pub fn demangle(raw: &str) -> Option<String> {
    let body = raw.strip_prefix("_R")?;
    // The compiler may append `.llvm.<hash>` or `.cold`.
    let body = body.split('.').next()?;
    let mut parser = Parser {
        s: body.as_bytes(),
        pos: 0,
        depth: 0,
        out: String::new(),
    };
    // An optional version digit string precedes the path.
    while parser.peek().is_some_and(|b| b.is_ascii_digit()) {
        parser.pos += 1;
    }
    parser.path(true)?;
    Some(parser.out)
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    depth: u32,
    out: String,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Runs `parse` with its output thrown away.
    fn silently<R>(&mut self, parse: impl FnOnce(&mut Self) -> Option<R>) -> Option<R> {
        let kept = std::mem::take(&mut self.out);
        let result = parse(self);
        self.out = kept;
        result
    }

    /// Runs `parse` one level deeper.
    fn nested<R>(&mut self, parse: impl FnOnce(&mut Self) -> Option<R>) -> Option<R> {
        if self.depth >= MAX_DEPTH || self.out.len() > MAX_LEN {
            return None;
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    /// `<base-62-number>`: digits then `_`. `_` alone is 0, `0_` is 1.
    fn base62(&mut self) -> Option<u64> {
        let mut value: u64 = 0;
        if self.eat(b'_') {
            return Some(0);
        }
        loop {
            let digit = match self.next()? {
                b'_' => return value.checked_add(1),
                b @ b'0'..=b'9' => u64::from(b - b'0'),
                b @ b'a'..=b'z' => u64::from(b - b'a') + 10,
                b @ b'A'..=b'Z' => u64::from(b - b'A') + 36,
                _ => return None,
            };
            value = value.checked_mul(62)?.checked_add(digit)?;
        }
    }

    fn decimal(&mut self) -> Option<usize> {
        let start = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        std::str::from_utf8(&self.s[start..self.pos])
            .ok()?
            .parse()
            .ok()
    }

    /// `[<disambiguator>] <undisambiguated-identifier>`: the disambiguator
    /// (0 when absent) and the name.
    fn identifier(&mut self) -> Option<(u64, String)> {
        let disambiguator = if self.eat(b's') {
            self.base62()?.checked_add(1)?
        } else {
            0
        };
        // `u` marks a Punycode name; it is shown as written.
        self.eat(b'u');
        let len = self.decimal()?;
        self.eat(b'_');
        let end = self.pos.checked_add(len)?;
        let name = std::str::from_utf8(self.s.get(self.pos..end)?).ok()?;
        self.pos = end;
        Some((disambiguator, name.to_string()))
    }

    /// Follows a `B<offset>` back-reference with `parse`.
    fn backref<R>(&mut self, parse: impl FnOnce(&mut Self) -> Option<R>) -> Option<R> {
        let from = self.pos - 1;
        let target = usize::try_from(self.base62()?).ok()?;
        if target >= from {
            return None;
        }
        let resume = self.pos;
        self.pos = target;
        let result = self.nested(parse);
        self.pos = resume;
        result
    }

    fn path(&mut self, in_value: bool) -> Option<()> {
        self.nested(|p| match p.next()? {
            b'C' => {
                let (_, name) = p.identifier()?;
                p.out.push_str(&name);
                Some(())
            }
            b'M' => {
                p.silently(Self::impl_path)?;
                p.out.push('<');
                p.ty()?;
                p.out.push('>');
                Some(())
            }
            b'X' => {
                p.silently(Self::impl_path)?;
                p.out.push('<');
                p.ty()?;
                p.out.push_str(" as ");
                p.path(false)?;
                p.out.push('>');
                Some(())
            }
            b'Y' => {
                p.out.push('<');
                p.ty()?;
                p.out.push_str(" as ");
                p.path(false)?;
                p.out.push('>');
                Some(())
            }
            b'N' => {
                let namespace = p.next()?;
                p.path(in_value)?;
                let (disambiguator, name) = p.identifier()?;
                match namespace {
                    b'C' | b'S' => {
                        let kind = if namespace == b'C' { "closure" } else { "shim" };
                        p.out.push_str("::{");
                        p.out.push_str(kind);
                        if !name.is_empty() {
                            p.out.push(':');
                            p.out.push_str(&name);
                        }
                        // The disambiguator is the index plus one; none is zero.
                        let index = disambiguator.saturating_sub(1);
                        p.out.push_str(&format!("#{index}}}"));
                    }
                    _ if !name.is_empty() => {
                        p.out.push_str("::");
                        p.out.push_str(&name);
                    }
                    _ => {}
                }
                Some(())
            }
            b'I' => {
                p.path(in_value)?;
                p.out.push_str(if in_value { "::<" } else { "<" });
                let mut first = true;
                while !p.eat(b'E') {
                    if !first {
                        p.out.push_str(", ");
                    }
                    first = false;
                    p.generic_arg()?;
                }
                p.out.push('>');
                Some(())
            }
            b'B' => p.backref(|p| p.path(in_value)),
            _ => None,
        })
    }

    /// `[<disambiguator>] <path>` of an impl: where it is, not shown.
    fn impl_path(&mut self) -> Option<()> {
        if self.eat(b's') {
            self.base62()?;
        }
        self.path(false)
    }

    fn generic_arg(&mut self) -> Option<()> {
        match self.peek()? {
            b'L' => {
                self.pos += 1;
                self.base62()?;
                self.out.push_str("'_");
                Some(())
            }
            b'K' => {
                self.pos += 1;
                self.constant()
            }
            _ => self.ty(),
        }
    }

    fn constant(&mut self) -> Option<()> {
        self.nested(|p| {
            if p.eat(b'p') {
                p.out.push('_');
                return Some(());
            }
            if p.eat(b'B') {
                return p.backref(Self::constant);
            }
            let kind = p.peek()?;
            p.silently(Self::ty)?;
            let start = p.pos;
            while p.peek()? != b'_' {
                p.pos += 1;
            }
            let digits = std::str::from_utf8(&p.s[start..p.pos]).ok()?;
            p.pos += 1;
            let value = u128::from_str_radix(if digits.is_empty() { "0" } else { digits }, 16).ok();
            match (kind, value) {
                (b'b', Some(0)) => p.out.push_str("false"),
                (b'b', Some(_)) => p.out.push_str("true"),
                (_, Some(value)) => p.out.push_str(&value.to_string()),
                _ => p.out.push('_'),
            }
            Some(())
        })
    }

    fn lifetime_opt(&mut self) -> Option<()> {
        if self.eat(b'L') {
            self.base62()?;
        }
        Some(())
    }

    fn types_until_end(&mut self) -> Option<()> {
        let mut first = true;
        while !self.eat(b'E') {
            if !first {
                self.out.push_str(", ");
            }
            first = false;
            self.ty()?;
        }
        Some(())
    }

    fn ty(&mut self) -> Option<()> {
        self.nested(|p| {
            let basic = match p.peek()? {
                b'a' => Some("i8"),
                b'b' => Some("bool"),
                b'c' => Some("char"),
                b'd' => Some("f64"),
                b'e' => Some("str"),
                b'f' => Some("f32"),
                b'h' => Some("u8"),
                b'i' => Some("isize"),
                b'j' => Some("usize"),
                b'l' => Some("i32"),
                b'm' => Some("u32"),
                b'n' => Some("i128"),
                b'o' => Some("u128"),
                b'p' => Some("_"),
                b's' => Some("i16"),
                b't' => Some("u16"),
                b'u' => Some("()"),
                b'v' => Some("..."),
                b'x' => Some("i64"),
                b'y' => Some("u64"),
                b'z' => Some("!"),
                _ => None,
            };
            if let Some(name) = basic {
                p.pos += 1;
                p.out.push_str(name);
                return Some(());
            }
            match p.peek()? {
                b'A' => {
                    p.pos += 1;
                    p.out.push('[');
                    p.ty()?;
                    p.out.push_str("; ");
                    p.constant()?;
                    p.out.push(']');
                }
                b'S' => {
                    p.pos += 1;
                    p.out.push('[');
                    p.ty()?;
                    p.out.push(']');
                }
                b'T' => {
                    p.pos += 1;
                    p.out.push('(');
                    p.types_until_end()?;
                    p.out.push(')');
                }
                b'R' | b'Q' => {
                    let mutable = p.next()? == b'Q';
                    p.lifetime_opt()?;
                    p.out.push_str(if mutable { "&mut " } else { "&" });
                    p.ty()?;
                }
                b'P' | b'O' => {
                    let mutable = p.next()? == b'O';
                    p.out.push_str(if mutable { "*mut " } else { "*const " });
                    p.ty()?;
                }
                b'F' => {
                    p.pos += 1;
                    if p.eat(b'G') {
                        p.base62()?;
                    }
                    p.eat(b'U');
                    if p.eat(b'K') {
                        if p.eat(b'C') {
                        } else {
                            p.identifier()?;
                        }
                    }
                    p.out.push_str("fn(");
                    let mut first = true;
                    while !p.eat(b'E') {
                        if !first {
                            p.out.push_str(", ");
                        }
                        first = false;
                        p.ty()?;
                    }
                    p.out.push(')');
                    if p.peek() != Some(b'u') {
                        p.out.push_str(" -> ");
                        p.ty()?;
                    } else {
                        p.pos += 1;
                    }
                }
                b'D' => {
                    p.pos += 1;
                    if p.eat(b'G') {
                        p.base62()?;
                    }
                    p.out.push_str("dyn ");
                    let mut first = true;
                    while !p.eat(b'E') {
                        if !first {
                            p.out.push_str(" + ");
                        }
                        first = false;
                        p.path(false)?;
                        // Associated type bindings: `p <name> <type>`.
                        while p.eat(b'p') {
                            let (_, name) = p.identifier()?;
                            p.out.push_str(&format!("<{name} = "));
                            p.ty()?;
                            p.out.push('>');
                        }
                    }
                    p.lifetime_opt()?;
                }
                b'B' => {
                    p.pos += 1;
                    return p.backref(Self::ty);
                }
                _ => return p.path(false),
            }
            Some(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_function_path() {
        // std::rt::lang_start
        assert_eq!(
            demangle("_RNvNtCs1234_3std2rt10lang_start").as_deref(),
            Some("std::rt::lang_start")
        );
    }

    #[test]
    fn generic_arguments_and_closures() {
        // std::rt::lang_start::<()>::{closure#0}
        assert_eq!(
            demangle("_RNCINvNtCs1234_3std2rt10lang_startuE0Cs5678_4main").as_deref(),
            Some("std::rt::lang_start::<()>::{closure#0}")
        );
    }

    #[test]
    fn trait_impls_print_their_trait() {
        // <app::Thing as core::fmt::Debug>::fmt
        assert_eq!(
            demangle("_RNvXCs1_3appNtCs1_3app5ThingNtNtCs2_4core3fmt5Debug3fmt").as_deref(),
            Some("<app::Thing as core::fmt::Debug>::fmt")
        );
    }

    #[test]
    fn anything_else_is_not_demangled() {
        assert_eq!(demangle("main"), None);
        assert_eq!(demangle("_RZ"), None);
    }
}
