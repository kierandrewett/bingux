use serde::{Deserialize, Serialize};

/// Sandbox isolation level for a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxLevel {
    None,
    Minimal,
    Standard,
    Strict,
}

impl Default for SandboxLevel {
    fn default() -> Self {
        Self::Standard
    }
}

/// The `[package]` table in manifest.toml.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub version: String,
    pub arch: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub license: String,
}

fn default_scope() -> String {
    "bingux".to_string()
}

/// The `[dependencies]` table in manifest.toml.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependencies {
    #[serde(default)]
    pub runtime: Vec<String>,
    #[serde(default)]
    pub build: Vec<String>,
}

/// The `[exports]` table in manifest.toml.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exports {
    #[serde(default)]
    pub binaries: Vec<String>,
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub data: Vec<String>,
}

/// The `[sandbox]` table in manifest.toml.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default)]
    pub level: SandboxLevel,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            level: SandboxLevel::default(),
        }
    }
}

/// A complete package manifest (manifest.toml).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageInfo,
    #[serde(default)]
    pub dependencies: Dependencies,
    #[serde(default)]
    pub exports: Exports,
    #[serde(default)]
    pub sandbox: SandboxConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_toml() {
        let manifest = Manifest {
            package: PackageInfo {
                name: "firefox".to_string(),
                scope: "bingux".to_string(),
                version: "128.0.1".to_string(),
                arch: "x86_64-linux".to_string(),
                description: "Mozilla Firefox web browser".to_string(),
                license: "MPL-2.0".to_string(),
            },
            dependencies: Dependencies {
                runtime: vec!["glibc-2.39".into(), "gtk3-3.24".into()],
                build: vec!["rust-1.79".into()],
            },
            exports: Exports {
                binaries: vec!["bin/firefox".into()],
                libraries: vec!["lib/libxul.so".into()],
                data: vec!["share/applications/firefox.desktop".into()],
            },
            sandbox: SandboxConfig {
                level: SandboxLevel::Standard,
            },
        };

        let toml_str = toml::to_string_pretty(&manifest).unwrap();
        let parsed: Manifest = toml::from_str(&toml_str).unwrap();
        assert_eq!(manifest, parsed);
    }

    #[test]
    fn minimal_manifest() {
        let toml_str = r#"
[package]
name = "hello"
version = "1.0"
arch = "x86_64-linux"
"#;
        let m: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(m.package.name, "hello");
        assert_eq!(m.package.scope, "bingux");
        assert!(m.dependencies.runtime.is_empty());
        assert_eq!(m.sandbox.level, SandboxLevel::Standard);
    }
}
