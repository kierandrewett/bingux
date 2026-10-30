//! What an application says about itself: [`AppInfo`], the macro that fills it
//! from Cargo's package metadata, and the registry the About window reads.

use std::sync::OnceLock;

/// The Cargo metadata of the crate that calls [`app_info!`](crate::app_info),
/// read at compile time with `env!`. The macro builds it; apps do not.
#[derive(Clone, Copy, Debug)]
pub struct CargoMeta {
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub homepage: &'static str,
    pub repository: &'static str,
    pub license: &'static str,
    pub authors: &'static str,
    /// `BINGUX_BUILD_COMMIT`, when the build sets it (a `build.rs` with
    /// `cargo:rustc-env=`, or the environment of `cargo build`).
    pub commit: Option<&'static str>,
    /// `BINGUX_BUILD_DATE`, the same way.
    pub built: Option<&'static str>,
}

/// The facts of one application, supplied once at launch with
/// [`Application::about`](crate::app::Application::about). An empty field is a
/// fact the application does not give, and the About window leaves its row
/// out. Build it with [`app_info!`](crate::app_info), which reads the package
/// metadata and takes overrides:
///
/// ```ignore
/// Application::new("bingux-files")
///     .about(bingux_ui::app_info!(name = "Files", id = "bingux-files"))
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppInfo {
    /// The name people know it by: "Files".
    pub name: String,
    /// The application id: the window's app id and the desktop file's name.
    pub id: String,
    pub version: String,
    /// The icon theme name of the application's icon.
    pub icon: String,
    /// One line about what it does.
    pub description: String,
    /// Where to read about it. Opened with `xdg-open`.
    pub website: String,
    /// Where to report a problem. Opened with `xdg-open`.
    pub issues: String,
    /// The licence as an SPDX expression: "GPL-3.0-or-later".
    pub licence: String,
    /// The licence text, for the Licence page.
    pub licence_text: String,
    pub authors: Vec<String>,
    /// The git commit the binary was built from.
    pub commit: String,
    /// The day the binary was built.
    pub built: String,
    /// The licences of the libraries linked in (the output of
    /// `scripts/cargo-licenses.py`), for the Legal page. Without it the page
    /// looks for the file the package installed.
    pub third_party: String,
}

impl AppInfo {
    /// The defaults for a crate: the display name and id from the package name
    /// (`bingux-files-app` is "Files" with id `bingux-files`), the version,
    /// description, homepage, licence and authors from `Cargo.toml`, the
    /// issue tracker from a GitHub repository, and the build's commit and date.
    pub fn from_cargo(meta: CargoMeta) -> Self {
        let id = meta.name.strip_suffix("-app").unwrap_or(meta.name).to_owned();
        let website = if meta.homepage.is_empty() { meta.repository } else { meta.homepage };
        let issues = if meta.repository.contains("github.com") {
            format!("{}/issues", meta.repository.trim_end_matches(".git").trim_end_matches('/'))
        } else {
            String::new()
        };
        Self {
            name: display_name(&id),
            icon: id.clone(),
            id,
            version: meta.version.to_owned(),
            description: meta.description.to_owned(),
            website: website.to_owned(),
            issues,
            licence: meta.license.to_owned(),
            licence_text: String::new(),
            authors: names_of(meta.authors),
            commit: meta.commit.unwrap_or_default().trim().to_owned(),
            built: meta.built.unwrap_or_default().trim().to_owned(),
            third_party: String::new(),
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn website(mut self, website: impl Into<String>) -> Self {
        self.website = website.into();
        self
    }

    pub fn issues(mut self, issues: impl Into<String>) -> Self {
        self.issues = issues.into();
        self
    }

    pub fn licence(mut self, spdx: impl Into<String>) -> Self {
        self.licence = spdx.into();
        self
    }

    /// The licence text: `licence_text = include_str!("../../../COPYING")`.
    pub fn licence_text(mut self, text: impl Into<String>) -> Self {
        self.licence_text = text.into();
        self
    }

    /// The authors, one name each.
    pub fn authors<S: Into<String>>(mut self, authors: impl IntoIterator<Item = S>) -> Self {
        self.authors = authors.into_iter().map(Into::into).collect();
        self
    }

    pub fn commit(mut self, commit: impl Into<String>) -> Self {
        self.commit = commit.into();
        self
    }

    pub fn built(mut self, date: impl Into<String>) -> Self {
        self.built = date.into();
        self
    }

    pub fn third_party(mut self, text: impl Into<String>) -> Self {
        self.third_party = text.into();
        self
    }

    /// Fill what the application did not say from the application the loop
    /// runs: its id, its window title and its icon. `Application::run` does
    /// this, so an app that never calls `about` still has a real About window.
    pub(crate) fn completed(mut self, app_id: &str, title: &str) -> Self {
        if self.id.is_empty() {
            self.id = app_id.to_owned();
        }
        if self.name.is_empty() {
            self.name = if title.is_empty() { display_name(&self.id) } else { title.to_owned() };
        }
        if self.icon.is_empty() {
            self.icon = self.id.clone();
        }
        self
    }

    /// The version line: "Version 0.1.0", with the commit when the build knows
    /// it ("Version 0.1.0 (444fb3b)"). Empty without a version.
    pub fn version_line(&self) -> String {
        match (self.version.is_empty(), self.commit.is_empty()) {
            (true, _) => String::new(),
            (false, true) => format!("Version {}", self.version),
            (false, false) => format!("Version {} ({})", self.version, short_commit(&self.commit)),
        }
    }
}

/// Read the package metadata of the calling crate, with overrides.
///
/// `app_info!()` takes everything from `Cargo.toml` and the build's
/// `BINGUX_BUILD_COMMIT` and `BINGUX_BUILD_DATE`. Each `key = value` after it
/// calls the builder method of that name on [`AppInfo`]:
///
/// ```ignore
/// bingux_ui::app_info!(
///     name = "Files",
///     icon = "bingux-files",
///     licence_text = include_str!("../../../COPYING"),
/// )
/// ```
#[macro_export]
macro_rules! app_info {
    ($($key:ident = $value:expr),* $(,)?) => {{
        let info = $crate::about::AppInfo::from_cargo($crate::about::CargoMeta {
            name: env!("CARGO_PKG_NAME"),
            version: env!("CARGO_PKG_VERSION"),
            description: env!("CARGO_PKG_DESCRIPTION"),
            homepage: env!("CARGO_PKG_HOMEPAGE"),
            repository: env!("CARGO_PKG_REPOSITORY"),
            license: env!("CARGO_PKG_LICENSE"),
            authors: env!("CARGO_PKG_AUTHORS"),
            commit: option_env!("BINGUX_BUILD_COMMIT"),
            built: option_env!("BINGUX_BUILD_DATE"),
        });
        $( let info = info.$key($value); )*
        info
    }};
}

/// "bingux-files" is "Files"; "bingux-power-tools" is "Power Tools".
fn display_name(id: &str) -> String {
    let id = id.strip_prefix("bingux-").unwrap_or(id);
    id.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut letters = word.chars();
            match letters.next() {
                Some(first) => first.to_uppercase().chain(letters).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The names of Cargo's `authors` string (`"A <a@x>:B"`), without addresses.
fn names_of(authors: &str) -> Vec<String> {
    authors
        .split(':')
        .map(|author| author.split('<').next().unwrap_or_default().trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect()
}

/// The first seven characters of a commit id, the way `git log --oneline`
/// shortens one.
pub(crate) fn short_commit(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

static INFO: OnceLock<AppInfo> = OnceLock::new();

/// Record the application's facts for the process. The first call wins:
/// `Application::run` makes it, once, before the window opens. Returns whether
/// this call set them.
pub(crate) fn register(info: AppInfo) -> bool {
    INFO.set(info).is_ok()
}

/// Whether an application registered its facts, which is when its windows
/// answer the About shortcut. Tests and the inspector's own window do not.
pub fn registered() -> bool {
    INFO.get().is_some()
}

/// The application's facts: what `Application::run` registered, or a bare
/// placeholder named after the running program when nothing did (a test, a
/// gallery).
pub fn info() -> AppInfo {
    INFO.get().cloned().unwrap_or_else(|| {
        let program = std::env::args()
            .next()
            .and_then(|path| path.rsplit('/').next().map(str::to_owned))
            .unwrap_or_default();
        AppInfo::default().id(program.clone()).completed(&program, "")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: CargoMeta = CargoMeta {
        name: "bingux-files-app",
        version: "0.3.1",
        description: "Browse and organise files",
        homepage: "",
        repository: "https://github.com/kierandrewett/bingux.git",
        license: "GPL-3.0-or-later",
        authors: "Kieran Drewett <k@example.org>:Someone Else",
        commit: Some("444fb3b9f2ac\n"),
        built: None,
    };

    #[test]
    fn the_defaults_come_from_the_package_metadata() {
        let info = AppInfo::from_cargo(META);
        assert_eq!(info.id, "bingux-files");
        assert_eq!(info.name, "Files");
        assert_eq!(info.icon, "bingux-files");
        assert_eq!(info.website, "https://github.com/kierandrewett/bingux.git");
        assert_eq!(info.issues, "https://github.com/kierandrewett/bingux/issues");
        assert_eq!(info.licence, "GPL-3.0-or-later");
        assert_eq!(info.authors, ["Kieran Drewett", "Someone Else"]);
        assert_eq!(info.commit, "444fb3b9f2ac");
        assert_eq!(info.built, "");
    }

    #[test]
    fn a_homepage_wins_over_the_repository_for_the_website() {
        let info = AppInfo::from_cargo(CargoMeta { homepage: "https://bingux.dev", ..META });
        assert_eq!(info.website, "https://bingux.dev");
        assert_eq!(info.issues, "https://github.com/kierandrewett/bingux/issues");
    }

    #[test]
    fn a_repository_that_is_not_github_gives_no_issue_tracker() {
        let info = AppInfo::from_cargo(CargoMeta { repository: "https://example.org/x", ..META });
        assert_eq!(info.issues, "");
    }

    #[test]
    fn display_names_drop_the_prefix_and_capitalise_each_word() {
        assert_eq!(display_name("bingux-settings"), "Settings");
        assert_eq!(display_name("bingux-power-tools"), "Power Tools");
        assert_eq!(display_name("other_app"), "Other App");
    }

    #[test]
    fn the_loop_fills_what_the_app_left_out() {
        let info = AppInfo::default().completed("bingux-taxonomy", "Taxonomy");
        assert_eq!((info.id.as_str(), info.name.as_str(), info.icon.as_str()), ("bingux-taxonomy", "Taxonomy", "bingux-taxonomy"));
        let kept = AppInfo::default().id("x").name("Mine").completed("bingux-taxonomy", "Taxonomy");
        assert_eq!((kept.id.as_str(), kept.name.as_str()), ("x", "Mine"));
        let untitled = AppInfo::default().completed("bingux-notes", "");
        assert_eq!(untitled.name, "Notes");
    }

    #[test]
    fn the_version_line_carries_the_short_commit_when_there_is_one() {
        let info = AppInfo::from_cargo(META);
        assert_eq!(info.version_line(), "Version 0.3.1 (444fb3b)");
        assert_eq!(info.clone().commit("").version_line(), "Version 0.3.1");
        assert_eq!(info.version("").version_line(), "");
    }
}
