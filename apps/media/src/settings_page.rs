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
    window.open(popup(text::SETTINGS_TITLE, SettingsBody { window }));
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
    Failed(&'static str),
}

fn error_text(error: &lastfm::Error) -> &'static str {
    match error {
        lastfm::Error::Offline => text::LASTFM_UNREACHABLE,
        lastfm::Error::BadKeys => text::LASTFM_BAD_KEYS,
        lastfm::Error::NotAllowedYet => text::LASTFM_NOT_ALLOWED,
        lastfm::Error::SignedOut | lastfm::Error::Other(_) => text::LASTFM_FAILED,
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

        let mut library = Group::new(Some(text::LIBRARY_GROUP.to_owned()));
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
                    .tooltip(text::REMOVE_FOLDER),
                ),
            );
        }
        library = library.row(row(text::MENU_ADD_FOLDER).leading(Icon::ListAdd).on_press(
            move |_| {
                add_folder(window);
            },
        ));

        let levelling_options = vec![
            ("off".to_owned(), text::LEVELLING_OFF.to_owned()),
            ("track".to_owned(), text::LEVELLING_TRACK.to_owned()),
            ("album".to_owned(), text::LEVELLING_ALBUM.to_owned()),
            ("automatic".to_owned(), text::LEVELLING_AUTO.to_owned()),
        ];
        let playback = Group::new(Some(text::PLAYBACK_GROUP.to_owned()))
            .row(row(text::LEVELLING).trailing(dropdown(
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
                switch_row(text::RESUME, current.resume, {
                    let change = change.clone();
                    move |on: bool| change(Box::new(move |settings| settings.resume = on))
                })
                .subtitle(text::RESUME_BODY),
            )
            .row(switch_row(text::SUBTITLES_SETTING, current.subtitles, {
                let change = change.clone();
                move |on: bool| {
                    if !on {
                        engine().player.select_subtitle(None);
                    }
                    change(Box::new(move |settings| settings.subtitles = on))
                }
            }));

        let saved = account.read().clone().unwrap_or_default();
        let mut lastfm_group = Group::new(Some(text::LASTFM_GROUP.to_owned()))
            .description(text::API_HELP)
            .row(switch_row(text::SCROBBLE, current.scrobble, {
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
                    lastfm_group.row(row(text::SIGNED_IN_AS).value(user.clone()).trailing(
                        action_button(text::SIGN_OUT, {
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
                    .row(entry_row(text::API_KEY, saved.api_key.clone(), {
                        let saved = saved.clone();
                        move |value: String| {
                            save_account(lastfm::Account {
                                api_key: value.trim().to_owned(),
                                ..saved.clone()
                            })
                        }
                    }))
                    .row(entry_row(text::API_SECRET, saved.secret.clone(), {
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
                        action_button(text::FINISH_SIGN_IN, move |_| {
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
                        action_button(text::SIGN_IN, move |_| {
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
                let mut sign_row = row(text::SIGN_IN.trim_end_matches('…')).trailing(button);
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
