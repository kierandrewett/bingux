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

use crate::address::SearchEngine;
use crate::history::History;
use crate::store::{Keeping, Part};

use crate::ui::Ctx;

impl Keeping {
    /// The search engine row's second line: what it is for, or why a change
    /// is not kept.
    pub fn settings_line(&self) -> String {
        self.text(Part::Settings)
            .unwrap_or_else(|| tr!("browser-settings-search-engine-subtitle"))
    }

    /// The history row's second line: why history is not kept, or how many
    /// pages it holds.
    pub fn history_line(&self, has_history: bool, pages: u64) -> String {
        if let Some(why) = self.text(Part::History) {
            return why;
        }
        if !has_history {
            return tr!("browser-history-not-kept");
        }
        tr!("browser-history-pages", count = pages)
    }
}

/// The rows of the page that come from the engine's own account of itself.
struct EngineRows;

impl EngineRows {
    /// What the host's choice of Remote Settings means for the person.
    pub fn remote_settings_text(mode: RemoteSettings, server: Option<&str>) -> String {
        match (mode, server) {
            (RemoteSettings::Off, _) => {
                tr!("browser-settings-remote-off")
            }
            (RemoteSettings::SecurityOnly, Some(server)) => {
                tr!("browser-settings-remote-mirror", server = server)
            }
            (RemoteSettings::SecurityOnly, None) => {
                tr!("browser-settings-remote-no-mirror")
            }
            (RemoteSettings::MozillaDirect, _) => tr!("browser-settings-remote-mozilla"),
        }
    }

    pub fn engine_state_text(info: &EngineInfo) -> String {
        match info.state {
            EngineState::Starting => tr!("browser-settings-engine-starting"),
            EngineState::Failed => tr!("browser-settings-engine-stopped"),
            EngineState::Ready => {
                if info.version.is_empty() {
                    tr!("browser-settings-engine-running")
                } else {
                    info.version.clone()
                }
            }
        }
    }

    /// What the page says for the user agent: the engine's own string, or that
    /// it has not given one yet.
    pub fn user_agent_text(info: &EngineInfo) -> String {
        if info.user_agent.is_empty() {
            tr!("browser-settings-not-reported")
        } else {
            info.user_agent.clone()
        }
    }

    /// The icon and the tone of a check: healthy ones are quiet, a warning is
    /// amber and a failure is red, and one the engine could not tell is neither.
    fn status_look(status: CheckStatus) -> (Icon, Option<Tone>) {
        match status {
            CheckStatus::Ok | CheckStatus::Info => (Icon::Check, None),
            CheckStatus::Warn => (Icon::TriangleExclamation, Some(Tone::Warning)),
            CheckStatus::Fail => (Icon::CircleCross, Some(Tone::Danger)),
            CheckStatus::Unknown => (Icon::CircleInfo, None),
        }
    }

    /// The sentence for the worst status in the report.
    fn overall_text(status: CheckStatus) -> String {
        match status {
            CheckStatus::Fail => tr!("browser-settings-check-failed"),
            CheckStatus::Warn => tr!("browser-settings-check-warn"),
            CheckStatus::Unknown => tr!("browser-settings-check-unknown"),
            CheckStatus::Ok | CheckStatus::Info => tr!("browser-settings-check-ok"),
        }
    }

    fn process_group(processes: &[ProcessSandbox]) -> Group {
        let mut group = Group::new(tr!("browser-settings-sandbox-title"));
        for process in processes {
            let (icon, tone) = Self::status_look(process.status);
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
        let mut group = Group::new(tr!("browser-settings-checks-title"));
        for check in checks {
            let (icon, tone) = Self::status_look(check.status);
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
    pub fn security_groups(report: Option<&SecurityReport>) -> Vec<Group> {
        let note = tr!("browser-settings-security-description");
        let Some(report) = report else {
            return vec![
                Group::new(tr!("browser-settings-security-title"))
                    .description(note)
                    .row(
                        row(tr!("browser-settings-not-reported"))
                            .subtitle(tr!("browser-settings-not-checked-description"))
                            .leading(Icon::CircleInfo),
                    ),
            ];
        };
        let (icon, tone) = Self::status_look(report.overall);
        let overall = row(Self::overall_text(report.overall)).leading(icon);
        let mut groups = vec![
            Group::new(tr!("browser-settings-security-title"))
                .description(note)
                .row(match tone {
                    Some(tone) => overall.leading_tone(tone),
                    None => overall,
                }),
        ];
        if !report.processes.is_empty() {
            groups.push(Self::process_group(&report.processes));
        }
        if !report.checks.is_empty() {
            groups.push(Self::checks_group(&report.checks));
        }
        groups
    }
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
        let pages = search::use_search_with(History::everything(), has_history).total;
        let engines: Vec<(String, String)> = SearchEngine::ALL
            .iter()
            .enumerate()
            .map(|(at, engine)| (at.to_string(), engine.name.to_string()))
            .collect();
        let mut search = Group::new(tr!("browser-settings-address-title")).row(
            row(tr!("browser-settings-search-label"))
                .subtitle(settings_keeping.settings_line())
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
        let mut history_row = row(tr!("browser-settings-history-label"))
            .subtitle(history_keeping.history_line(has_history, pages));
        history_row = if history_keeping.can_start_over() {
            history_row.trailing(
                action_button(tr!("browser-start-over"), move |_: ()| {
                    ctx.with(|browser| browser.start_over_history());
                })
                .compact(),
            )
        } else {
            history_row.trailing(
                action_button(tr!("browser-settings-clear-button"), move |_: ()| {
                    ctx.with(|browser| browser.clear_history());
                })
                .enabled(has_history && pages > 0)
                .compact(),
            )
        };
        let mut privacy = Group::new(tr!("browser-settings-privacy-title")).row(history_row);
        if let Some(kept) = scope.text() {
            privacy = privacy.description(kept);
        }
        let privacy = privacy
            .row(
                row(tr!("browser-settings-permissions-label"))
                    .subtitle(tr!("browser-settings-permissions-kept", count = remembered))
                    .trailing(
                        action_button(tr!("browser-settings-reset-all-button"), move |_: ()| {
                            ctx.with(|browser| browser.permissions.forget_all());
                            ctx.overlays
                                .toast(tr!("browser-settings-permissions-reset-notification"));
                        })
                        .enabled(remembered > 0)
                        .compact(),
                    ),
            )
            .footer(tr!("browser-permissions-footer"));
        let settings = &info.settings;
        let mut engine = Group::new(tr!("browser-settings-engine-title"))
            .description(tr!("browser-settings-engine-description"))
            .row(property_row(
                tr!("browser-settings-engine-title"),
                EngineRows::engine_state_text(&info),
            ))
            .row(
                row(tr!("browser-settings-user-agent-label"))
                    .subtitle(EngineRows::user_agent_text(&info)),
            )
            .row(
                row(tr!("browser-settings-profile-folder-label"))
                    .path_value(settings.profile_dir.to_string_lossy().into_owned()),
            )
            .row(
                row(tr!("browser-settings-cache-folder-label"))
                    .path_value(settings.cache_dir.to_string_lossy().into_owned()),
            )
            .row(
                row(tr!("browser-settings-downloads-folder-label"))
                    .path_value(settings.download_dir.to_string_lossy().into_owned()),
            )
            .row(
                row(tr!("browser-settings-crash-label"))
                    .subtitle(tr!("browser-settings-crash-description"))
                    .path_value(settings.crash_dir.to_string_lossy().into_owned()),
            )
            .row(row(tr!("browser-settings-remote-label")).subtitle(
                EngineRows::remote_settings_text(
                    settings.remote_settings,
                    settings.remote_settings_server.as_deref(),
                ),
            ))
            .row(
                row(tr!("browser-settings-webrtc-label"))
                    .subtitle(settings.webrtc_ip_policy.clone()),
            )
            .row(property_row(
                tr!("browser-settings-processes-label"),
                settings.content_process_limit.to_string(),
            ))
            .row(property_row(
                tr!("browser-settings-languages-label"),
                settings.locales.clone(),
            ));
        engine = engine.row(
            row(tr!("browser-settings-sandbox-label")).subtitle(settings.sandbox_floor.clone()),
        );
        let security = EngineRows::security_groups(info.security.as_ref());
        let content = page_column()
            .child(heading(tr!("browser-settings-title")))
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
        use crate::store::NotSaved;
        crate::locale::register();
        let not = |why| Keeping::NotSaved(why);
        assert_eq!(
            Keeping::Saved.settings_line(),
            "Used for words typed in the address bar"
        );
        assert_eq!(
            Keeping::Saved.history_line(true, 1),
            "1 page kept in this profile"
        );
        assert_eq!(
            Keeping::Loading.history_line(true, 0),
            "0 pages kept in this profile"
        );
        assert_eq!(
            not(NotSaved::Private).history_line(false, 0),
            "Changes here last until this window closes"
        );
        assert_eq!(
            not(NotSaved::Private).settings_line(),
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
                not(why.clone()).settings_line().contains(words),
                "settings: {why:?}"
            );
            assert!(
                not(why.clone()).history_line(true, 3).contains(words),
                "history: {why:?}"
            );
        }
    }

    #[test]
    fn a_failed_check_is_red_a_warning_is_amber_and_a_healthy_one_is_quiet() {
        assert_eq!(
            EngineRows::status_look(CheckStatus::Fail),
            (Icon::CircleCross, Some(Tone::Danger))
        );
        assert_eq!(
            EngineRows::status_look(CheckStatus::Warn),
            (Icon::TriangleExclamation, Some(Tone::Warning))
        );
        assert_eq!(
            EngineRows::status_look(CheckStatus::Ok),
            (Icon::Check, None)
        );
        assert_eq!(
            EngineRows::status_look(CheckStatus::Info),
            (Icon::Check, None)
        );
        assert_eq!(
            EngineRows::overall_text(CheckStatus::Fail),
            tr!("browser-settings-check-failed")
        );
        assert_eq!(
            EngineRows::overall_text(CheckStatus::Info),
            tr!("browser-settings-check-ok")
        );
    }

    #[test]
    fn a_report_the_engine_has_not_made_is_one_row_that_says_so() {
        assert_eq!(EngineRows::security_groups(None).len(), 1);
        let report = bingux_ui::web_view::test_double::healthy_report();
        assert_eq!(
            EngineRows::security_groups(Some(&report)).len(),
            3,
            "the result, the processes and the other checks"
        );
    }

    #[test]
    fn every_remote_settings_choice_says_what_it_means() {
        assert!(
            EngineRows::remote_settings_text(RemoteSettings::Off, None)
                .contains("only when the issuer answers")
        );
        assert_eq!(
            EngineRows::remote_settings_text(
                RemoteSettings::SecurityOnly,
                Some("https://rs.example.org/v1")
            ),
            "Security data only, from https://rs.example.org/v1"
        );
        assert!(
            EngineRows::remote_settings_text(RemoteSettings::SecurityOnly, None)
                .contains("no mirror")
        );
        assert!(
            EngineRows::remote_settings_text(RemoteSettings::MozillaDirect, None)
                .contains("Mozilla")
        );
    }
}
