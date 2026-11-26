//! The settings page: how the address bar searches, what the browser keeps,
//! and what the host told the engine. The engine's own settings are the
//! host's: this page shows them and what the engine says about its security,
//! and does not pretend to change them. They are set where the browser is
//! started and take effect the next time it starts.

use bingux_ui::prelude::*;
use bingux_ui::search;
use bingux_ui::web_view::{
    CheckStatus, EngineInfo, EngineState, ProcessSandbox, RemoteSettings, SecurityCheck,
    SecurityReport,
};
use freya::prelude::*;

use crate::model::address::ENGINES;
use crate::model::keeping::{Keeping, Part};
use crate::profile::history;

use super::ctx::Ctx;

/// The search engine row's second line: what it is for, or why a change
/// is not kept.
pub fn settings_text(keeping: &Keeping) -> String {
    keeping
        .text(Part::Settings)
        .unwrap_or_else(|| tr!("browser-settings-search-engine-subtitle"))
}

/// The history row's second line: why history is not kept, or how many
/// pages it holds.
pub fn history_text(has_history: bool, keeping: &Keeping, pages: u64) -> String {
    if let Some(why) = keeping.text(Part::History) {
        return why;
    }
    if !has_history {
        return tr!("browser-history-not-kept");
    }
    tr!("browser-history-pages", count = pages)
}

/// What the host's choice of Remote Settings means for the person.
pub fn remote_settings_text(mode: RemoteSettings, server: Option<&str>) -> String {
    match (mode, server) {
        (RemoteSettings::Off, _) => {
            "Off: revoked certificates are caught only when the issuer answers".to_string()
        }
        (RemoteSettings::SecurityOnly, Some(server)) => {
            format!("Security data only, from {server}")
        }
        (RemoteSettings::SecurityOnly, None) => {
            "Security data only, but no mirror is set".to_string()
        }
        (RemoteSettings::MozillaDirect, _) => "Security data only, from Mozilla".to_string(),
    }
}

fn engine_state_text(info: &EngineInfo) -> String {
    match info.state {
        EngineState::Starting => "Starting".to_string(),
        EngineState::Failed => "Stopped".to_string(),
        EngineState::Ready => {
            if info.version.is_empty() {
                "Running".to_string()
            } else {
                info.version.clone()
            }
        }
    }
}

/// What the page says for the user agent: the engine's own string, or that
/// it has not given one yet.
fn user_agent_text(info: &EngineInfo) -> String {
    if info.user_agent.is_empty() {
        "Not reported yet".to_string()
    } else {
        info.user_agent.clone()
    }
}

/// The icon and the tone of a check: healthy ones are quiet, a warning is
/// amber and a failure is red, and one the engine could not tell is neither.
fn status_look(status: CheckStatus) -> (Icon, Option<Tone>) {
    match status {
        CheckStatus::Ok | CheckStatus::Info => (Icon::Check, None),
        CheckStatus::Warn => (Icon::DialogWarning, Some(Tone::Warning)),
        CheckStatus::Fail => (Icon::DialogError, Some(Tone::Danger)),
        CheckStatus::Unknown => (Icon::About, None),
    }
}

/// The sentence for the worst status in the report.
fn overall_text(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Fail => "A check failed",
        CheckStatus::Warn => "Some checks warn",
        CheckStatus::Unknown => "Some checks could not be made",
        CheckStatus::Ok | CheckStatus::Info => "Every check passes",
    }
}

fn process_group(processes: &[ProcessSandbox]) -> Group {
    let mut group = Group::new("Sandbox of each kind of process".to_string());
    for process in processes {
        let (icon, tone) = status_look(process.status);
        let line = row(process.title())
            .subtitle(process.summary())
            .leading(icon);
        group = group.row(match tone {
            Some(tone) => line.leading_tone(tone),
            None => line,
        });
    }
    group
}

fn checks_group(checks: &[SecurityCheck]) -> Group {
    let mut group = Group::new("Other checks".to_string());
    for check in checks {
        let (icon, tone) = status_look(check.status);
        // A short value sits at the end of the row; one that would not fit is its text.
        let line = row(check.label.clone()).leading(icon);
        let line = if check.value.chars().count() > 24 {
            line.subtitle(check.value.clone())
        } else {
            line.value(check.value.clone())
        };
        group = group.row(match tone {
            Some(tone) => line.leading_tone(tone),
            None => line,
        });
    }
    group
}

/// The engine's own account of its security, as groups of the page. The
/// engine checks itself at start and when something changes; before it has
/// (`NOT_READY`) the page says so and shows no report at all, never an empty
/// one that reads as healthy.
fn security_groups(report: Option<&SecurityReport>) -> Vec<Group> {
    let note = "The engine's own check of how it is set up. It finds mistakes in that; it cannot vouch for itself if something has broken into it.";
    let Some(report) = report else {
        return vec![Group::new("Security report".to_string()).description(note).row(
            row("Not reported yet")
                .subtitle("The engine has not finished checking itself. Open this page again in a moment.")
                .leading(Icon::About),
        )];
    };
    let (icon, tone) = status_look(report.overall);
    let overall = row(overall_text(report.overall)).leading(icon);
    let mut groups = vec![
        Group::new("Security report".to_string())
            .description(note)
            .row(match tone {
                Some(tone) => overall.leading_tone(tone),
                None => overall,
            }),
    ];
    if !report.processes.is_empty() {
        groups.push(process_group(&report.processes));
    }
    if !report.checks.is_empty() {
        groups.push(checks_group(&report.checks));
    }
    groups
}

#[derive(PartialEq)]
pub struct SettingsPage {
    pub ctx: Ctx,
}

impl Component for SettingsPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let (info, selected, remembered, settings_keeping, has_history, history_keeping, scope) = {
            let browser = ctx.browser.read();
            (
                browser.engine.backend().info(),
                browser.settings.engine_index(),
                browser.permissions.origins().len(),
                browser.settings_keeping.clone(),
                browser.has_history(),
                browser.history_keeping.clone(),
                browser.history_clear_scope(),
            )
        };
        // The count is the searchable's, so it follows every visit and Clear.
        let pages = search::use_search_with(history::everything(), has_history).total;
        let engines: Vec<(String, String)> = ENGINES
            .iter()
            .enumerate()
            .map(|(at, engine)| (at.to_string(), engine.name.to_string()))
            .collect();
        let mut search = Group::new("Address bar".to_string()).row(
            row("Search engine")
                .subtitle(settings_text(&settings_keeping))
                .trailing(dropdown(
                    engines,
                    selected.to_string(),
                    move |value: String| {
                        if let Ok(at) = value.parse::<usize>() {
                            ctx.with(|browser| browser.set_search_engine(at));
                        }
                    },
                )),
        );
        if settings_keeping.can_start_over() {
            search = search.row(
                row(tr!("browser-settings-file"))
                    .subtitle(tr!("browser-settings-file-subtitle"))
                    .trailing(
                        action_button(tr!("browser-start-over"), move |_: ()| {
                            ctx.with(|browser| browser.start_over_settings());
                        })
                        .compact(),
                    ),
            );
        }
        let mut history_row =
            row("Browsing history").subtitle(history_text(has_history, &history_keeping, pages));
        history_row = if history_keeping.can_start_over() {
            history_row.trailing(
                action_button(tr!("browser-start-over"), move |_: ()| {
                    ctx.with(|browser| browser.start_over_history());
                })
                .compact(),
            )
        } else {
            history_row.trailing(
                action_button("Clear", move |_: ()| {
                    ctx.with(|browser| browser.clear_history());
                })
                .enabled(has_history && pages > 0)
                .compact(),
            )
        };
        let mut privacy = Group::new("Privacy".to_string()).row(history_row);
        if let Some(kept) = scope.text() {
            privacy = privacy.description(kept);
        }
        let privacy = privacy
            .row(
                row("Site permissions")
                    .subtitle(if remembered == 1 {
                        "1 site has choices kept".to_string()
                    } else {
                        format!("{remembered} sites have choices kept")
                    })
                    .trailing(
                        action_button("Reset all", move |_: ()| {
                            ctx.with(|browser| browser.permissions.forget_all());
                            ctx.overlays.toast("Site permissions reset");
                        })
                        .enabled(remembered > 0)
                        .compact(),
                    ),
            )
            .footer(tr!("browser-permissions-footer"));
        let settings = &info.settings;
        let mut engine = Group::new("Engine".to_string())
            .description("Chosen by whoever starts the browser, and read once at start.")
            .row(property_row("Engine", engine_state_text(&info)))
            .row(row("User agent").subtitle(user_agent_text(&info)))
            .row(
                row("Profile folder")
                    .path_value(settings.profile_dir.to_string_lossy().into_owned()),
            )
            .row(row("Cache folder").path_value(settings.cache_dir.to_string_lossy().into_owned()))
            .row(
                row("Downloads folder")
                    .path_value(settings.download_dir.to_string_lossy().into_owned()),
            )
            .row(
                row("Crash reports")
                    .subtitle(
                        "Kept in this folder, on this computer. They are never sent anywhere.",
                    )
                    .path_value(settings.crash_dir.to_string_lossy().into_owned()),
            )
            .row(row("Remote Settings").subtitle(remote_settings_text(
                settings.remote_settings,
                settings.remote_settings_server.as_deref(),
            )))
            .row(row("WebRTC addresses").subtitle(settings.webrtc_ip_policy.clone()))
            .row(property_row(
                "Content processes",
                settings.content_process_limit.to_string(),
            ))
            .row(property_row("Languages", settings.locales.clone()));
        engine = engine.row(row("Required sandbox").subtitle(settings.sandbox_floor.clone()));
        let security = security_groups(info.security.as_ref());
        let content = page_column()
            .child(heading("Settings"))
            .child(search)
            .child(privacy)
            .child(engine);
        let content = security
            .into_iter()
            .fold(content, |content, group| content.child(group));
        ScrollView::new().child(
            rect()
                .tw(
                    &tokens,
                    &[w_full(), flex_col(), items(Align::Center), p(S4)],
                )
                .child(clamp(Dim::Page, content)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_keeping_state_is_said_on_the_page() {
        use crate::model::keeping::NotSaved;
        crate::locale::register();
        let not = |why| Keeping::NotSaved(why);
        assert_eq!(
            settings_text(&Keeping::Saved),
            "Used for words typed in the address bar"
        );
        assert_eq!(
            history_text(true, &Keeping::Saved, 1),
            "1 page kept in this profile"
        );
        assert_eq!(
            history_text(true, &Keeping::Loading, 0),
            "0 pages kept in this profile"
        );
        assert_eq!(
            history_text(false, &not(NotSaved::Private), 0),
            "Changes here last until this window closes"
        );
        assert_eq!(
            settings_text(&not(NotSaved::Private)),
            "Changes here last until this window closes"
        );
        let states = [
            (NotSaved::NoProfile, "no profile"),
            (NotSaved::DiskFull, "disk is full"),
            (NotSaved::ReadOnly, "read-only"),
            (NotSaved::Busy, "Another program"),
            (
                NotSaved::TooNew {
                    found: 9,
                    supported: 1,
                },
                "newer Browser",
            ),
            (
                NotSaved::Damaged {
                    detail: String::new(),
                },
                "Start over",
            ),
            (NotSaved::Failed("no luck".to_string()), "no luck"),
        ];
        for (why, words) in states {
            assert!(
                settings_text(&not(why.clone())).contains(words),
                "settings: {why:?}"
            );
            assert!(
                history_text(true, &not(why.clone()), 3).contains(words),
                "history: {why:?}"
            );
        }
    }

    #[test]
    fn a_failed_check_is_red_a_warning_is_amber_and_a_healthy_one_is_quiet() {
        assert_eq!(
            status_look(CheckStatus::Fail),
            (Icon::DialogError, Some(Tone::Danger))
        );
        assert_eq!(
            status_look(CheckStatus::Warn),
            (Icon::DialogWarning, Some(Tone::Warning))
        );
        assert_eq!(status_look(CheckStatus::Ok), (Icon::Check, None));
        assert_eq!(status_look(CheckStatus::Info), (Icon::Check, None));
        assert_eq!(overall_text(CheckStatus::Fail), "A check failed");
        assert_eq!(overall_text(CheckStatus::Info), "Every check passes");
    }

    #[test]
    fn a_report_the_engine_has_not_made_is_one_row_that_says_so() {
        assert_eq!(security_groups(None).len(), 1);
        let report = bingux_ui::web_view::test_double::healthy_report();
        assert_eq!(
            security_groups(Some(&report)).len(),
            3,
            "the result, the processes and the other checks"
        );
    }

    #[test]
    fn every_remote_settings_choice_says_what_it_means() {
        assert!(
            remote_settings_text(RemoteSettings::Off, None)
                .contains("only when the issuer answers")
        );
        assert_eq!(
            remote_settings_text(
                RemoteSettings::SecurityOnly,
                Some("https://rs.example.org/v1")
            ),
            "Security data only, from https://rs.example.org/v1"
        );
        assert!(remote_settings_text(RemoteSettings::SecurityOnly, None).contains("no mirror"));
        assert!(remote_settings_text(RemoteSettings::MozillaDirect, None).contains("Mozilla"));
    }
}
