use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchEngine {
    pub id: String,
    pub name: String,
    pub shortcut: String,
    pub url: String,
    pub enabled: bool,
}

pub fn default_id() -> String {
    "duckduckgo".into()
}
pub fn defaults() -> Vec<SearchEngine> {
    vec![SearchEngine {
        id: default_id(),
        name: "DuckDuckGo".into(),
        shortcut: "ddg".into(),
        url: "https://duckduckgo.com/?q={query}".into(),
        enabled: true,
    }]
}

pub fn validate(engines: &[SearchEngine], default: &str) -> Result<()> {
    let mut ids = HashSet::new();
    let mut shortcuts = HashSet::new();
    if engines.is_empty() || engines.len() > 32 {
        bail!("configure between 1 and 32 search engines");
    }
    for engine in engines {
        let identifier = |value: &str, max| {
            !value.is_empty()
                && value.len() <= max
                && value.as_bytes()[0].is_ascii_alphanumeric()
                && value
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        };
        if !identifier(&engine.id, 64)
            || !ids.insert(&engine.id)
            || !identifier(&engine.shortcut, 24)
            || !shortcuts.insert(&engine.shortcut)
            || ["wiki", "gh", "maps", "yt"].contains(&engine.shortcut.as_str())
        {
            bail!("search engine identifiers and shortcuts must be unique");
        }
        let url = engine
            .url
            .replace("{query}", "test")
            .parse::<ureq::http::Uri>()?;
        if !matches!(url.scheme_str(), Some("https" | "http"))
            || url.host().is_none()
            || url
                .authority()
                .is_some_and(|authority| authority.as_str().contains('@'))
            || engine.url.len() > 2048
            || engine.url.matches("{query}").count() != 1
            || engine
                .url
                .split('/')
                .nth(2)
                .is_some_and(|host| host.contains("{query}"))
            || engine.url.chars().any(char::is_control)
            || engine.name.trim().is_empty()
            || engine.name.len() > 320
            || engine.name.chars().any(char::is_control)
        {
            bail!("invalid search engine name or URL template");
        }
    }
    if !engines
        .iter()
        .any(|engine| engine.id == default && engine.enabled)
    {
        bail!("default search engine must be enabled");
    }
    Ok(())
}

impl SearchEngine {
    pub fn search_url(&self, query: &str) -> String {
        let mut encoded = String::new();
        for byte in query.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                encoded.push(char::from(byte));
            } else {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
        self.url.replace("{query}", &encoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_defaults_and_encodes_untrusted_queries() {
        let engines = defaults();
        validate(&engines, "duckduckgo").unwrap();
        assert_eq!(
            engines[0].search_url("café & cats? #1"),
            "https://duckduckgo.com/?q=caf%C3%A9%20%26%20cats%3F%20%231"
        );
    }
    #[test]
    fn rejects_unsafe_templates_and_ambiguous_shortcuts() {
        for template in [
            "javascript:{query}",
            "https://user:pass@example.com/{query}",
            "https://{query}.com/",
            "https://example.com/",
            "https://example.com/{query}/{query}",
        ] {
            let mut engines = defaults();
            engines[0].url = template.into();
            assert!(validate(&engines, "duckduckgo").is_err(), "{template}");
        }
        let mut engines = defaults();
        engines[0].enabled = false;
        assert!(validate(&engines, "duckduckgo").is_err());
        engines[0].enabled = true;
        engines[0].shortcut = "wiki".into();
        assert!(validate(&engines, "duckduckgo").is_err());
    }
}
