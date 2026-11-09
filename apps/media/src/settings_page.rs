//! Settings, a popup: the library's folders, how loudness is evened out,
//! resuming films, subtitles, and Last.fm (off until set up).

use std::path::PathBuf;

use bingux_media::{Levelling, lastfm};
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::engine::engine;
use crate::pages::add_folder;
use crate::strings as text;

/// Opens the settings popup.
pub fn open(window: Window, _overlays: Overlays) {
    window.open(popup(text::settings_title(), SettingsBody { window }));
}

#[derive(PartialEq)]
struct SettingsBody {
    window: Window,
}

/// Where signing in to Last.fm is.
#[derive(Clone, PartialEq)]
enum SignIn {
    Idle,
    Busy,
    /// The person was sent to the Last.fm page with this token.
    Waiting(String),
    Failed(String),
}

fn error_text(error: &lastfm::Error) -> String {
    match error {
        lastfm::Error::Offline => text::lastfm_unreachable(),
        lastfm::Error::BadKeys => text::lastfm_bad_keys(),
        lastfm::Error::NotAllowedYet => text::lastfm_not_allowed(),
        lastfm::Error::SignedOut | lastfm::Error::Other(_) => text::lastfm_failed(),
    }
}

impl Component for SettingsBody {
    fn render(&self) -> impl IntoElement {
        let window = self.window;
        let settings = use_state(|| engine().settings());
        let mut account = use_state(|| None::<lastfm::Account>);
        let mut sign_in = use_state(|| SignIn::Idle);
        // The saved account is a file: read it on a worker.
        use_hook(move || {
            spawn(async move {
                let saved = bingux_ui::task::spawn_blocking(lastfm::load).await;
                account.set(Some(saved.unwrap_or_default()));
            })
        });
        let change = move |edit: Box<dyn FnOnce(&mut bingux_media::settings::Settings)>| {
            let mut settings = settings;
            let mut copy = settings.peek().clone();
            edit(&mut copy);
            let stored = copy.clone();
            engine().change_settings(move |current| *current = stored);
            settings.set(copy);
        };
        let current = settings.read().clone();

        let mut library = Group::new(Some(text::library_group().to_owned()));
        for folder in current.library_folders() {
            let shown = folder.to_string_lossy().into_owned();
            let removed = folder.clone();
            let change = change.clone();
            library = library.row(
                row(folder
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or(shown.clone()))
                .subtitle(shown)
                .leading(Icon::Folder)
                .trailing(
                    icon_button(Icon::ListRemove, move |_| {
                        let removed = removed.clone();
                        change(Box::new(move |settings| {
                            if bingux_media::library::default_folders().contains(&removed) {
                                settings.removed_defaults.push(removed.clone());
                            }
                            settings
                                .folders
                                .retain(|folder: &PathBuf| folder != &removed);
                        }));
                        engine().scan();
                    })
                    .tooltip(&text::remove_folder()),
                ),
            );
        }
        library = library.row(
            row(text::menu_add_folder())
                .leading(Icon::ListAdd)
                .on_press(move |_| {
                    add_folder(window);
                }),
        );

        let levelling_options = vec![
            ("off".to_owned(), text::levelling_off()),
            ("track".to_owned(), text::levelling_track()),
            ("album".to_owned(), text::levelling_album()),
            ("automatic".to_owned(), text::levelling_auto()),
        ];
        let playback = Group::new(Some(text::playback_group().to_owned()))
            .row(row(text::levelling()).trailing(dropdown(
                levelling_options,
                current.levelling.clone(),
                {
                    let change = change.clone();
                    move |value: String| {
                        let mut probe = bingux_media::settings::Settings {
                            levelling: value.clone(),
                            ..Default::default()
                        };
                        let levelling: Levelling = probe.levelling();
                        probe.set_levelling(levelling);
                        engine().player.set_levelling(levelling);
                        change(Box::new(move |settings| settings.levelling = value));
                    }
                },
            )))
            .row(
                switch_row(text::resume(), current.resume, {
                    let change = change.clone();
                    move |on: bool| change(Box::new(move |settings| settings.resume = on))
                })
                .subtitle(text::resume_body()),
            )
            .row(switch_row(text::subtitles_setting(), current.subtitles, {
                let change = change.clone();
                move |on: bool| {
                    if !on {
                        engine().player.select_subtitle(None);
                    }
                    change(Box::new(move |settings| settings.subtitles = on))
                }
            }));

        let saved = account.read().clone().unwrap_or_default();
        let mut lastfm_group = Group::new(Some(text::lastfm_group().to_owned()))
            .description(text::api_help())
            .row(switch_row(text::scrobble(), current.scrobble, {
                let change = change.clone();
                move |on: bool| change(Box::new(move |settings| settings.scrobble = on))
            }));
        if current.scrobble {
            let save_account = move |next: lastfm::Account| {
                let mut account = account;
                account.set(Some(next.clone()));
                let _ = std::thread::Builder::new()
                    .name("media-lastfm".into())
                    .spawn(move || {
                        let _ = lastfm::save(&next);
                    });
            };
            if let Some(user) = &saved.user {
                lastfm_group =
                    lastfm_group.row(row(text::signed_in_as()).value(user.clone()).trailing(
                        action_button(text::sign_out(), {
                            let saved = saved.clone();
                            move |_| {
                                save_account(lastfm::Account {
                                    session: None,
                                    user: None,
                                    ..saved.clone()
                                })
                            }
                        }),
                    ));
            } else {
                lastfm_group = lastfm_group
                    .row(entry_row(text::api_key(), saved.api_key.clone(), {
                        let saved = saved.clone();
                        move |value: String| {
                            save_account(lastfm::Account {
                                api_key: value.trim().to_owned(),
                                ..saved.clone()
                            })
                        }
                    }))
                    .row(entry_row(text::api_secret(), saved.secret.clone(), {
                        let saved = saved.clone();
                        move |value: String| {
                            save_account(lastfm::Account {
                                secret: value.trim().to_owned(),
                                ..saved.clone()
                            })
                        }
                    }));
                let state = sign_in.read().clone();
                let button = match &state {
                    SignIn::Waiting(token) => {
                        let token = token.clone();
                        let saved = saved.clone();
                        action_button(text::finish_sign_in(), move |_| {
                            let token = token.clone();
                            let saved = saved.clone();
                            sign_in.set(SignIn::Busy);
                            spawn(async move {
                                let result = bingux_ui::task::spawn_blocking(move || {
                                    lastfm::finish_sign_in(&saved, &token)
                                })
                                .await;
                                match result {
                                    Ok(signed_in) => {
                                        save_account(signed_in);
                                        sign_in.set(SignIn::Idle);
                                    }
                                    Err(error) => sign_in.set(SignIn::Failed(error_text(&error))),
                                }
                            });
                        })
                        .suggested()
                    }
                    _ => {
                        let has_keys = saved.has_keys();
                        let saved = saved.clone();
                        action_button(text::sign_in(), move |_| {
                            let saved = saved.clone();
                            sign_in.set(SignIn::Busy);
                            spawn(async move {
                                let result = bingux_ui::task::spawn_blocking(move || {
                                    lastfm::begin_sign_in(&saved)
                                })
                                .await;
                                match result {
                                    Ok((token, page)) => {
                                        Platform::get().open_url(page);
                                        sign_in.set(SignIn::Waiting(token));
                                    }
                                    Err(error) => sign_in.set(SignIn::Failed(error_text(&error))),
                                }
                            });
                        })
                        .enabled(has_keys)
                    }
                };
                let button = button.loading(state == SignIn::Busy);
                let mut sign_row = row(text::sign_in().trim_end_matches('…')).trailing(button);
                if let SignIn::Failed(message) = state {
                    sign_row = sign_row.subtitle(message);
                }
                lastfm_group = lastfm_group.row(sign_row);
            }
        }

        popup_column()
            .child(library)
            .child(playback)
            .child(lastfm_group)
    }
}
