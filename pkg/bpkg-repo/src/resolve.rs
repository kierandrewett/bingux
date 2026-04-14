use std::path::PathBuf;

/// The source from which a package should be installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    /// A plain package name, resolved from the default scope.
    /// e.g. `"firefox"`
    Name(String),
    /// A scoped package name.
    /// e.g. `"@brave.brave-browser"` → scope = "brave", name = "brave-browser"
    Scoped(String, String),
    /// A local `.bgx` file path.
    /// e.g. `"./firefox-128.0.1-x86_64-linux.bgx"`
    File(PathBuf),
}

/// Parse a user-provided install input into an `InstallSource`.
///
/// Rules:
/// - If the input ends with `.bgx`, it's a file path.
/// - If the input starts with `@`, it's a scoped name: `@<scope>.<name>`.
/// - Otherwise, it's a plain package name.
pub fn parse_install_source(input: &str) -> InstallSource {
    if input.ends_with(".bgx") {
        return InstallSource::File(PathBuf::from(input));
    }

    if let Some(rest) = input.strip_prefix('@') {
        if let Some((scope, name)) = rest.split_once('.') {
            if !scope.is_empty() && !name.is_empty() {
                return InstallSource::Scoped(scope.to_string(), name.to_string());
            }
        }
    }

    InstallSource::Name(input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_name() {
        assert_eq!(
            parse_install_source("firefox"),
            InstallSource::Name("firefox".to_string())
        );
    }

    #[test]
    fn parse_scoped_name() {
        assert_eq!(
            parse_install_source("@brave.brave-browser"),
            InstallSource::Scoped("brave".to_string(), "brave-browser".to_string())
        );
    }

    #[test]
    fn parse_file_path_relative() {
        assert_eq!(
            parse_install_source("./firefox-128.0.1-x86_64-linux.bgx"),
            InstallSource::File(PathBuf::from("./firefox-128.0.1-x86_64-linux.bgx"))
        );
    }

    #[test]
    fn parse_file_path_absolute() {
        assert_eq!(
            parse_install_source("/tmp/my-package.bgx"),
            InstallSource::File(PathBuf::from("/tmp/my-package.bgx"))
        );
    }

    #[test]
    fn parse_malformed_scope_falls_back_to_name() {
        // Missing dot separator
        assert_eq!(
            parse_install_source("@bravebrowser"),
            InstallSource::Name("@bravebrowser".to_string())
        );
    }

    #[test]
    fn parse_empty_scope_falls_back_to_name() {
        assert_eq!(
            parse_install_source("@.something"),
            InstallSource::Name("@.something".to_string())
        );
    }

    #[test]
    fn parse_empty_name_falls_back_to_name() {
        assert_eq!(
            parse_install_source("@scope."),
            InstallSource::Name("@scope.".to_string())
        );
    }
}
