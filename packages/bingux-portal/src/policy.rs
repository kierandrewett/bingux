//! Permission rules and their evaluation.
//!
//! Rules match on a capability and on structured selectors of the requester
//! identity. Any matching `deny` wins. Otherwise the last matching rule wins as
//! a whole. A rule level of `inherit` uses the store default.

use serde::{Deserialize, Serialize};

use crate::identity::Identity;
use crate::store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    ScreenCast,
    RemoteDesktop,
    InputCapture,
    Screenshot,
    Access,
}

impl Capability {
    pub fn supports_monitors(self) -> bool {
        matches!(self, Capability::ScreenCast | Capability::RemoteDesktop)
    }

    pub fn supports_devices(self) -> bool {
        self == Capability::RemoteDesktop
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    Inherit,
    Ask,
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Device {
    Keyboard,
    Pointer,
    Touchscreen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    /// A glob over the full identity string, such as `host-exe:/usr/bin/*`.
    /// `*` matches any run of characters and `?` matches one character.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    pub capabilities: Vec<Capability>,
    pub level: Level,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub monitors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<Device>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clipboard: bool,
}

impl Rule {
    pub fn matches(&self, capability: Capability, identity: &Identity) -> bool {
        if !self.capabilities.contains(&capability) {
            return false;
        }
        let mut has_selector = false;
        if let Some(app_id) = &self.app_id {
            has_selector = true;
            if identity.app_id() != Some(app_id.as_str()) {
                return false;
            }
        }
        if let Some(exe) = &self.exe {
            has_selector = true;
            if identity.exe() != Some(exe.as_str()) {
                return false;
            }
        }
        if let Some(pattern) = &self.pattern {
            has_selector = true;
            if !glob_match(pattern, &identity.key()) {
                return false;
            }
        }
        has_selector
    }
}

/// The level that applies to `capability` for the requester.
///
/// The result is always `Ask`, `Allow` or `Deny`. A missing identity gets the
/// store default, so an unknown requester is never allowed.
pub fn resolve_level(store: &Store, capability: Capability, identity: Option<&Identity>) -> Level {
    let fallback = store_fallback(store);
    let Some(identity) = identity else {
        return fallback;
    };
    let mut level = fallback;
    for rule in &store.rules {
        if !rule.matches(capability, identity) {
            continue;
        }
        level = match rule.level {
            Level::Inherit => fallback,
            explicit => explicit,
        };
        if level == Level::Deny {
            return Level::Deny;
        }
    }
    level
}

/// The level used when no rule decides. `inherit` at the top resolves to `ask`.
fn store_fallback(store: &Store) -> Level {
    match store.default {
        Level::Deny => Level::Deny,
        _ => Level::Ask,
    }
}

/// Matches `text` against a glob where `*` matches any run of characters and
/// `?` matches exactly one character.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<usize> = None;
    let mut star_text = 0;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_text = t;
            p += 1;
        } else if let Some(star_at) = star {
            p = star_at + 1;
            star_text += 1;
            t = star_text;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn app(id: &str) -> Identity {
        Identity::AppId(id.to_owned())
    }

    fn exe(path: &str) -> Identity {
        Identity::HostExe(path.to_owned())
    }

    fn rule(name: &str, level: Level) -> Rule {
        Rule {
            name: name.to_owned(),
            app_id: None,
            exe: None,
            pattern: None,
            capabilities: vec![Capability::Screenshot],
            level,
            monitors: vec![],
            devices: vec![],
            clipboard: false,
        }
    }

    fn store(default: Level, rules: Vec<Rule>) -> Store {
        Store {
            version: 1,
            default,
            rules,
        }
    }

    #[test]
    fn glob_matches_wildcards() {
        assert!(glob_match("app-id:org.*", "app-id:org.example.App"));
        assert!(glob_match("host-exe:/usr/bin/?", "host-exe:/usr/bin/x"));
        assert!(!glob_match("host-exe:/usr/bin/?", "host-exe:/usr/bin/xy"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a*b*c", "axxbyyc"));
        assert!(!glob_match("a*b*c", "axxbyy"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "exacts"));
        assert!(glob_match("*/tool", "host-exe:/usr/bin/tool"));
    }

    #[test]
    fn missing_identity_falls_back_to_default() {
        assert_eq!(
            resolve_level(&store(Level::Inherit, vec![]), Capability::Screenshot, None),
            Level::Ask
        );
        assert_eq!(
            resolve_level(&store(Level::Deny, vec![]), Capability::Screenshot, None),
            Level::Deny
        );
    }

    #[test]
    fn inherit_resolves_to_ask_and_deny_default_denies() {
        let id = app("org.example.App");
        assert_eq!(
            resolve_level(
                &store(Level::Inherit, vec![]),
                Capability::Screenshot,
                Some(&id)
            ),
            Level::Ask
        );
        assert_eq!(
            resolve_level(
                &store(Level::Ask, vec![]),
                Capability::Screenshot,
                Some(&id)
            ),
            Level::Ask
        );
        assert_eq!(
            resolve_level(
                &store(Level::Deny, vec![]),
                Capability::Screenshot,
                Some(&id)
            ),
            Level::Deny
        );
    }

    #[test]
    fn allow_rule_applies_to_matching_app() {
        let mut allow = rule("allow-app", Level::Allow);
        allow.app_id = Some("org.example.App".to_owned());
        let s = store(Level::Ask, vec![allow]);
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&app("org.example.App"))),
            Level::Allow
        );
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&app("org.other"))),
            Level::Ask
        );
    }

    #[test]
    fn any_matching_deny_wins_over_later_allow() {
        let mut deny = rule("deny", Level::Deny);
        deny.pattern = Some("app-id:org.*".to_owned());
        let mut allow = rule("allow", Level::Allow);
        allow.app_id = Some("org.example.App".to_owned());
        let s = store(Level::Ask, vec![deny, allow]);
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&app("org.example.App"))),
            Level::Deny
        );
    }

    #[test]
    fn last_matching_rule_wins_as_a_whole() {
        let mut allow = rule("allow", Level::Allow);
        allow.pattern = Some("*".to_owned());
        let mut inherit = rule("inherit", Level::Inherit);
        inherit.app_id = Some("org.example.App".to_owned());
        let s = store(Level::Deny, vec![allow.clone(), inherit.clone()]);
        // The inherit rule matches last. It resolves to the store default, which is deny.
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&app("org.example.App"))),
            Level::Deny
        );
        let s = store(Level::Ask, vec![inherit, allow]);
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&app("org.example.App"))),
            Level::Allow
        );
    }

    #[test]
    fn rules_only_apply_to_their_capability() {
        let mut deny = rule("deny-cast", Level::Deny);
        deny.capabilities = vec![Capability::ScreenCast];
        let s = store(Level::Ask, vec![deny]);
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&app("org.example.App"))),
            Level::Ask
        );
    }

    #[test]
    fn exe_selector_matches_host_identity_only() {
        let mut allow = rule("allow-tool", Level::Allow);
        allow.exe = Some("/usr/bin/tool".to_owned());
        let s = store(Level::Ask, vec![allow]);
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&exe("/usr/bin/tool"))),
            Level::Allow
        );
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&exe("/usr/bin/other"))),
            Level::Ask
        );
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&app("/usr/bin/tool"))),
            Level::Ask
        );
    }

    #[test]
    fn rule_without_selector_never_matches() {
        let s = store(Level::Ask, vec![rule("no-selector", Level::Deny)]);
        assert_eq!(
            resolve_level(&s, Capability::Screenshot, Some(&app("org.example.App"))),
            Level::Ask
        );
    }
}
