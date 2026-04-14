/// Parse `pkg@version` syntax for explicit version selection.
///
/// Returns (package_name, Some(version)) or (package_name, None).
///
/// # Examples
/// - `firefox@128.0.1` -> ("firefox", Some("128.0.1"))
/// - `firefox` -> ("firefox", None)
pub fn parse_version_syntax(input: &str) -> (String, Option<String>) {
    match input.split_once('@') {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            (name.to_string(), Some(version.to_string()))
        }
        _ => (input.to_string(), None),
    }
}
