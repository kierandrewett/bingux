//! Init-system-agnostic service management.
//!
//! The service backend abstracts over different init systems (systemd, dinit, s6, runit).
//! `system.toml` declares services; the active backend generates the correct config files.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A declared service from system.toml or home.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDeclaration {
    pub name: String,
    pub description: Option<String>,
    pub exec_start: String,
    pub exec_stop: Option<String>,
    pub service_type: ServiceType,
    pub restart: RestartPolicy,
    pub user: Option<String>,
    pub group: Option<String>,
    pub environment: Vec<(String, String)>,
    pub after: Vec<String>,
    pub wanted_by: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum ServiceType {
    #[default]
    Simple,
    Oneshot,
    Forking,
    Notify,
    Idle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum RestartPolicy {
    #[default]
    No,
    OnFailure,
    Always,
}

/// Generated service file ready to write to disk.
pub struct GeneratedService {
    pub path: PathBuf,
    pub content: String,
}

/// Backend trait for different init systems.
pub trait ServiceBackend {
    /// Name of this init system.
    fn name(&self) -> &str;

    /// Generate service files from declarations.
    fn generate(&self, services: &[ServiceDeclaration], target_dir: &Path) -> Vec<GeneratedService>;

    /// Generate an "enable" marker (e.g., symlink for systemd, db entry for dinit).
    fn enable_service(&self, name: &str, target_dir: &Path) -> Option<GeneratedService>;
}

/// systemd backend — generates .service unit files.
pub struct SystemdBackend;

impl ServiceBackend for SystemdBackend {
    fn name(&self) -> &str {
        "systemd"
    }

    fn generate(&self, services: &[ServiceDeclaration], target_dir: &Path) -> Vec<GeneratedService> {
        let mut results = Vec::new();

        for svc in services {
            let mut unit = String::new();
            unit.push_str("[Unit]\n");
            if let Some(ref desc) = svc.description {
                unit.push_str(&format!("Description={desc}\n"));
            }
            for after in &svc.after {
                unit.push_str(&format!("After={after}\n"));
            }
            unit.push('\n');

            unit.push_str("[Service]\n");
            unit.push_str(&format!("Type={}\n", match svc.service_type {
                ServiceType::Simple => "simple",
                ServiceType::Oneshot => "oneshot",
                ServiceType::Forking => "forking",
                ServiceType::Notify => "notify",
                ServiceType::Idle => "idle",
            }));
            unit.push_str(&format!("ExecStart={}\n", svc.exec_start));
            if let Some(ref stop) = svc.exec_stop {
                unit.push_str(&format!("ExecStop={stop}\n"));
            }
            match svc.restart {
                RestartPolicy::No => {}
                RestartPolicy::OnFailure => unit.push_str("Restart=on-failure\n"),
                RestartPolicy::Always => unit.push_str("Restart=always\n"),
            }
            if let Some(ref user) = svc.user {
                unit.push_str(&format!("User={user}\n"));
            }
            if let Some(ref group) = svc.group {
                unit.push_str(&format!("Group={group}\n"));
            }
            for (key, val) in &svc.environment {
                unit.push_str(&format!("Environment=\"{key}={val}\"\n"));
            }
            unit.push('\n');

            unit.push_str("[Install]\n");
            unit.push_str(&format!("WantedBy={}\n", svc.wanted_by));

            results.push(GeneratedService {
                path: target_dir.join(format!("{}.service", svc.name)),
                content: unit,
            });
        }

        results
    }

    fn enable_service(&self, name: &str, target_dir: &Path) -> Option<GeneratedService> {
        // systemd enables via symlinks in wants directories
        let wants_dir = target_dir.join("multi-user.target.wants");
        let link_path = wants_dir.join(format!("{name}.service"));
        let target = format!("../{name}.service");

        Some(GeneratedService {
            path: link_path,
            content: target, // For symlinks, content = target path
        })
    }
}

/// dinit backend — generates dinit service directories.
pub struct DinitBackend;

impl ServiceBackend for DinitBackend {
    fn name(&self) -> &str {
        "dinit"
    }

    fn generate(&self, services: &[ServiceDeclaration], target_dir: &Path) -> Vec<GeneratedService> {
        let mut results = Vec::new();

        for svc in services {
            let mut content = String::new();
            content.push_str(&format!("type = {}\n", match svc.service_type {
                ServiceType::Simple => "process",
                ServiceType::Oneshot => "scripted",
                ServiceType::Forking => "bgprocess",
                ServiceType::Notify => "process",
                ServiceType::Idle => "process",
            }));
            content.push_str(&format!("command = {}\n", svc.exec_start));
            if let Some(ref stop) = svc.exec_stop {
                content.push_str(&format!("stop-command = {stop}\n"));
            }
            match svc.restart {
                RestartPolicy::No => content.push_str("restart = false\n"),
                RestartPolicy::OnFailure => content.push_str("restart = true\n"),
                RestartPolicy::Always => content.push_str("restart = true\n"),
            }
            for after in &svc.after {
                content.push_str(&format!("depends-on = {after}\n"));
            }

            results.push(GeneratedService {
                path: target_dir.join(&svc.name),
                content,
            });
        }

        results
    }

    fn enable_service(&self, name: &str, target_dir: &Path) -> Option<GeneratedService> {
        // dinit uses a "boot" service that depends-on enabled services
        Some(GeneratedService {
            path: target_dir.join("boot"),
            content: format!("depends-on = {name}\n"),
        })
    }
}

/// s6/s6-rc backend — generates s6 service directories.
pub struct S6Backend;

impl ServiceBackend for S6Backend {
    fn name(&self) -> &str {
        "s6"
    }

    fn generate(&self, services: &[ServiceDeclaration], target_dir: &Path) -> Vec<GeneratedService> {
        let mut results = Vec::new();

        for svc in services {
            let svc_dir = target_dir.join(&svc.name);

            // s6 uses a `run` script and a `type` file
            let run_script = format!("#!/bin/sh\nexec {}\n", svc.exec_start);
            results.push(GeneratedService {
                path: svc_dir.join("run"),
                content: run_script,
            });

            let type_file = match svc.service_type {
                ServiceType::Oneshot => "oneshot",
                _ => "longrun",
            };
            results.push(GeneratedService {
                path: svc_dir.join("type"),
                content: type_file.to_string(),
            });

            // Dependencies via `dependencies.d/`
            for after in &svc.after {
                results.push(GeneratedService {
                    path: svc_dir.join("dependencies.d").join(after),
                    content: String::new(),
                });
            }
        }

        results
    }

    fn enable_service(&self, name: &str, target_dir: &Path) -> Option<GeneratedService> {
        Some(GeneratedService {
            path: target_dir.join("default").join("contents.d").join(name),
            content: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> ServiceDeclaration {
        ServiceDeclaration {
            name: "nginx".to_string(),
            description: Some("NGINX web server".to_string()),
            exec_start: "/usr/bin/nginx -g 'daemon off;'".to_string(),
            exec_stop: Some("/usr/bin/nginx -s quit".to_string()),
            service_type: ServiceType::Simple,
            restart: RestartPolicy::OnFailure,
            user: Some("nginx".to_string()),
            group: Some("nginx".to_string()),
            environment: vec![("LANG".to_string(), "en_GB.UTF-8".to_string())],
            after: vec!["network.target".to_string()],
            wanted_by: "multi-user.target".to_string(),
        }
    }

    #[test]
    fn systemd_generates_unit_file() {
        let backend = SystemdBackend;
        let svc = test_service();
        let dir = std::path::PathBuf::from("/tmp/test-systemd");
        let results = backend.generate(&[svc], &dir);

        assert_eq!(results.len(), 1);
        let unit = &results[0];
        assert!(unit.path.ends_with("nginx.service"));
        assert!(unit.content.contains("[Unit]"));
        assert!(unit.content.contains("Description=NGINX web server"));
        assert!(unit.content.contains("After=network.target"));
        assert!(unit.content.contains("[Service]"));
        assert!(unit.content.contains("Type=simple"));
        assert!(unit.content.contains("ExecStart=/usr/bin/nginx"));
        assert!(unit.content.contains("ExecStop=/usr/bin/nginx -s quit"));
        assert!(unit.content.contains("Restart=on-failure"));
        assert!(unit.content.contains("User=nginx"));
        assert!(unit.content.contains("Environment=\"LANG=en_GB.UTF-8\""));
        assert!(unit.content.contains("[Install]"));
        assert!(unit.content.contains("WantedBy=multi-user.target"));
    }

    #[test]
    fn dinit_generates_service() {
        let backend = DinitBackend;
        let svc = test_service();
        let dir = std::path::PathBuf::from("/tmp/test-dinit");
        let results = backend.generate(&[svc], &dir);

        assert_eq!(results.len(), 1);
        let unit = &results[0];
        assert!(unit.path.ends_with("nginx"));
        assert!(unit.content.contains("type = process"));
        assert!(unit.content.contains("command = /usr/bin/nginx"));
        assert!(unit.content.contains("stop-command = /usr/bin/nginx -s quit"));
        assert!(unit.content.contains("restart = true"));
        assert!(unit.content.contains("depends-on = network.target"));
    }

    #[test]
    fn s6_generates_run_script() {
        let backend = S6Backend;
        let svc = test_service();
        let dir = std::path::PathBuf::from("/tmp/test-s6");
        let results = backend.generate(&[svc], &dir);

        assert!(results.len() >= 2); // run + type at minimum
        let run = results.iter().find(|r| r.path.ends_with("run")).unwrap();
        assert!(run.content.contains("#!/bin/sh"));
        assert!(run.content.contains("exec /usr/bin/nginx"));

        let svc_type = results.iter().find(|r| r.path.ends_with("type")).unwrap();
        assert_eq!(svc_type.content, "longrun");
    }

    #[test]
    fn systemd_enable_creates_wants_symlink() {
        let backend = SystemdBackend;
        let dir = std::path::PathBuf::from("/tmp/test-enable");
        let result = backend.enable_service("nginx", &dir).unwrap();
        assert!(result.path.to_string_lossy().contains("multi-user.target.wants"));
        assert!(result.path.to_string_lossy().contains("nginx.service"));
    }

    #[test]
    fn backend_names() {
        assert_eq!(SystemdBackend.name(), "systemd");
        assert_eq!(DinitBackend.name(), "dinit");
        assert_eq!(S6Backend.name(), "s6");
    }
}
