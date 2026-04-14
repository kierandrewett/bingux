use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{BinguxError, Result};

/// The default package scope (official Bingux repository).
pub const DEFAULT_SCOPE: &str = "bingux";

/// A repository scope for package namespacing.
///
/// Packages are namespaced: `@scope.package-name`.
/// The default scope is `bingux`, so `firefox` = `@bingux.firefox`.
///
/// Examples:
/// - `@bingux.firefox` (official)
/// - `@brave.brave-browser` (third-party)
/// - `@kieran.my-tool` (personal)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Scope(String);

impl Scope {
    /// The default (official) Bingux scope.
    pub fn default_scope() -> Self {
        Self(DEFAULT_SCOPE.to_string())
    }

    /// Create a new scope, validating the name.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if !is_valid_scope(&name) {
            return Err(BinguxError::InvalidScope(format!(
                "invalid scope: '{name}' (must be lowercase alphanumeric and hyphens)"
            )));
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is the default (bingux) scope.
    pub fn is_default(&self) -> bool {
        self.0 == DEFAULT_SCOPE
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.0)
    }
}

/// A scoped package reference: `@scope.package-name`.
///
/// When the user writes just `firefox`, it's `@bingux.firefox`.
/// When they write `@brave.brave-browser`, it's parsed explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopedName {
    pub scope: Scope,
    pub name: String,
}

impl ScopedName {
    pub fn new(scope: Scope, name: impl Into<String>) -> Self {
        Self {
            scope,
            name: name.into(),
        }
    }

    /// Create a scoped name in the default (bingux) scope.
    pub fn in_default_scope(name: impl Into<String>) -> Self {
        Self {
            scope: Scope::default_scope(),
            name: name.into(),
        }
    }
}

impl fmt::Display for ScopedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scope.is_default() {
            // Omit @bingux for brevity
            write!(f, "{}", self.name)
        } else {
            write!(f, "@{}.{}", self.scope.0, self.name)
        }
    }
}

impl FromStr for ScopedName {
    type Err = BinguxError;

    /// Parse a scoped package reference.
    ///
    /// - `@brave.brave-browser` → scope=brave, name=brave-browser
    /// - `firefox` → scope=bingux (default), name=firefox
    fn from_str(s: &str) -> Result<Self> {
        if let Some(rest) = s.strip_prefix('@') {
            // Explicit scope: @scope.name
            let dot_pos = rest.find('.').ok_or_else(|| {
                BinguxError::InvalidScope(format!(
                    "scoped name '{s}' must contain a dot: @scope.name"
                ))
            })?;

            let scope_str = &rest[..dot_pos];
            let name = &rest[dot_pos + 1..];

            if name.is_empty() {
                return Err(BinguxError::InvalidScope(format!(
                    "empty package name in '{s}'"
                )));
            }

            Ok(ScopedName {
                scope: Scope::new(scope_str)?,
                name: name.to_string(),
            })
        } else {
            // No scope prefix → default to @bingux
            if s.is_empty() {
                return Err(BinguxError::InvalidScope(
                    "empty package name".to_string(),
                ));
            }
            Ok(ScopedName::in_default_scope(s))
        }
    }
}

fn is_valid_scope(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_scope() {
        let sn: ScopedName = "firefox".parse().unwrap();
        assert_eq!(sn.scope, Scope::default_scope());
        assert_eq!(sn.name, "firefox");
        assert_eq!(sn.to_string(), "firefox");
    }

    #[test]
    fn parse_explicit_scope() {
        let sn: ScopedName = "@brave.brave-browser".parse().unwrap();
        assert_eq!(sn.scope.as_str(), "brave");
        assert_eq!(sn.name, "brave-browser");
        assert_eq!(sn.to_string(), "@brave.brave-browser");
    }

    #[test]
    fn parse_explicit_default_scope() {
        let sn: ScopedName = "@bingux.firefox".parse().unwrap();
        assert_eq!(sn.scope, Scope::default_scope());
        assert_eq!(sn.name, "firefox");
        // Display omits @bingux for brevity
        assert_eq!(sn.to_string(), "firefox");
    }

    #[test]
    fn reject_missing_dot() {
        assert!("@brave-browser".parse::<ScopedName>().is_err());
    }

    #[test]
    fn reject_empty_name() {
        assert!("@brave.".parse::<ScopedName>().is_err());
    }

    #[test]
    fn reject_empty_string() {
        assert!("".parse::<ScopedName>().is_err());
    }
}
