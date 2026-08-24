use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

const PROTOCOL_VERSION: u32 = 1;
const MAX_FILE_ROOTS: usize = 32;
const MAX_PROVIDER_MANIFESTS: usize = 64;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchConfig {
    pub protocol_version: u32,
    pub commands: SearchCommands,
    #[serde(default)]
    pub file_roots: Vec<PathBuf>,
    #[serde(default)]
    pub provider_manifest_paths: Vec<PathBuf>,
    #[serde(default)]
    pub sqlite_sources: Vec<SqliteSourceConfig>,
    #[serde(default)]
    pub weather: Option<WeatherConfig>,
    #[serde(default)]
    pub ai: Option<AiConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SqliteSourceConfig {
    pub id: String,
    pub display_name: String,
    pub database_path: PathBuf,
    pub query: String,
    #[serde(default)]
    pub activation_command: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeatherConfig {
    pub latitude: f64,
    pub longitude: f64,
    pub refresh_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key_file: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchCommands {
    pub application_launcher: Vec<String>,
    pub file_opener: Vec<String>,
    pub clipboard: Vec<String>,
}

impl SearchConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("could not read search configuration {}", path.display()))?;
        let config = serde_json::from_str::<Self>(&contents)
            .with_context(|| format!("could not parse search configuration {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.protocol_version != PROTOCOL_VERSION {
            bail!("search configuration uses unsupported protocol version");
        }

        if self.file_roots.len() > MAX_FILE_ROOTS {
            bail!("search configuration contains too many file roots");
        }

        if self.provider_manifest_paths.len() > MAX_PROVIDER_MANIFESTS {
            bail!("search configuration contains too many provider manifests");
        }

        for path in self.file_roots.iter().chain(&self.provider_manifest_paths) {
            require_absolute_path(path, "search configuration path")?;
        }

        for source in &self.sqlite_sources {
            source.validate()?;
        }

        self.commands.validate()?;

        if let Some(weather) = &self.weather {
            weather.validate()?;
        }

        if let Some(ai) = &self.ai {
            ai.validate()?;
        }

        Ok(())
    }
}

impl SqliteSourceConfig {
    fn validate(&self) -> Result<()> {
        if !is_provider_id(&self.id) {
            bail!("SQLite source id is invalid");
        }

        if self.display_name.trim().is_empty() {
            bail!("SQLite source display name is empty");
        }

        require_absolute_path(&self.database_path, "SQLite database path")?;

        let query = self.query.trim_start();
        let lower_query = query.to_ascii_lowercase();
        if !(lower_query.starts_with("select") || lower_query.starts_with("with")) {
            bail!("SQLite query must be read-only");
        }

        if query.contains(';') || !query.contains("?1") || !query.contains("?2") {
            bail!("SQLite query must contain ?1 and ?2 and only one statement");
        }

        if let Some(program) = self.activation_command.first() {
            if !Path::new(program).is_absolute() {
                bail!("SQLite activation program must be an absolute path");
            }
        }

        for argument in &self.activation_command {
            if argument.is_empty() || argument.contains('\0') {
                bail!("SQLite activation command contains an invalid argument");
            }
        }
        Ok(())
    }
}

impl WeatherConfig {
    fn validate(&self) -> Result<()> {
        if !self.latitude.is_finite() || !(-90.0..=90.0).contains(&self.latitude) {
            bail!("weather latitude is invalid");
        }

        if !self.longitude.is_finite() || !(-180.0..=180.0).contains(&self.longitude) {
            bail!("weather longitude is invalid");
        }

        if !(60..=86_400).contains(&self.refresh_seconds) {
            bail!("weather refresh interval is invalid");
        }

        Ok(())
    }
}

impl AiConfig {
    fn validate(&self) -> Result<()> {
        if !self.endpoint.starts_with("https://") {
            bail!("AI endpoint must use HTTPS");
        }

        if self.model.trim().is_empty() || self.model.len() > 256 {
            bail!("AI model is invalid");
        }

        require_absolute_path(&self.api_key_file, "AI API key path")
    }
}

impl SearchCommands {
    fn validate(&self) -> Result<()> {
        validate_command(&self.application_launcher, "application launcher")?;
        validate_command(&self.file_opener, "file opener")?;
        validate_command(&self.clipboard, "clipboard command")
    }
}

fn validate_command(command: &[String], name: &str) -> Result<()> {
    let Some(program) = command.first() else {
        bail!("{name} must contain a program path");
    };
    require_absolute_path(Path::new(program), name)?;
    if command
        .iter()
        .any(|argument| argument.is_empty() || argument.contains('\0'))
    {
        bail!("{name} contains an invalid argument");
    }
    Ok(())
}
fn require_absolute_path(path: &Path, name: &str) -> Result<()> {
    if !path.is_absolute() {
        bail!("{name} must be absolute");
    }

    Ok(())
}

fn is_provider_id(value: &str) -> bool {
    let mut previous_hyphen = false;
    let mut saw_character = false;

    for character in value.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            previous_hyphen = false;
            saw_character = true;
        } else if character == '-' && saw_character && !previous_hyphen {
            previous_hyphen = true;
        } else {
            return false;
        }
    }

    saw_character && !previous_hyphen
}

#[cfg(test)]
mod tests {
    use super::{AiConfig, SearchCommands, SearchConfig, SqliteSourceConfig, WeatherConfig};
    use std::path::PathBuf;

    fn valid_config() -> SearchConfig {
        SearchConfig {
            protocol_version: 1,
            commands: SearchCommands {
                application_launcher: vec!["/nix/store/test/bin/gtk-launch".to_owned()],
                file_opener: vec!["/nix/store/test/bin/xdg-open".to_owned()],
                clipboard: vec!["/nix/store/test/bin/wl-copy".to_owned()],
            },
            file_roots: vec![PathBuf::from("/home/test")],
            provider_manifest_paths: vec![],
            sqlite_sources: vec![SqliteSourceConfig {
                id: "notes".to_owned(),
                display_name: "Notes".to_owned(),
                database_path: PathBuf::from("/home/test/notes.db"),
                query: "SELECT id, title, body FROM note WHERE title LIKE ?1 LIMIT ?2".to_owned(),
                activation_command: vec![
                    "/nix/store/test/bin/note-open".to_owned(),
                    "{id}".to_owned(),
                ],
            }],
            weather: Some(WeatherConfig {
                latitude: 51.5,
                longitude: -0.1,
                refresh_seconds: 900,
            }),
            ai: Some(AiConfig {
                endpoint: "https://example.test/v1/chat/completions".to_owned(),
                model: "test-model".to_owned(),
                api_key_file: PathBuf::from("/run/secrets/ai-api-key"),
            }),
        }
    }

    #[test]
    fn accepts_a_valid_profile_configuration() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn rejects_a_mutating_sqlite_query() {
        let mut config = valid_config();
        config.sqlite_sources[0].query = "DELETE FROM note WHERE title = ?1 LIMIT ?2".to_owned();

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_non_https_ai_endpoint() {
        let mut config = valid_config();
        config.ai.as_mut().expect("AI config").endpoint = "http://example.test".to_owned();

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_provider_identifiers() {
        let mut config = valid_config();
        config.sqlite_sources[0].id = "Notes".to_owned();

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_a_sqlite_activation_command_without_an_absolute_program_path() {
        let mut config = valid_config();
        config.sqlite_sources[0].activation_command =
            vec!["note-open".to_owned(), "{id}".to_owned()];

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_a_standard_command_without_an_absolute_program_path() {
        let mut config = valid_config();
        config.commands.clipboard = vec!["wl-copy".to_owned()];

        assert!(config.validate().is_err());
    }
}
