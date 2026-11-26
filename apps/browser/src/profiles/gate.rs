//! The window a start shows instead of the browser when it cannot open a
//! profile (another window has it, the list of profiles is damaged, the first
//! start's move is waiting), or when it must ask which profile to open. Every
//! way forward starts the browser again, so the browser itself only ever
//! opens a profile it holds.

use bingux_ui::prelude::*;
use bingux_ui::task::spawn_blocking;
use freya::prelude::*;

use crate::profiles::open::ProfileState;
use crate::profiles::places::Places;
use crate::profiles::registry::Profiles;

/// What the window shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Gate {
    Problem(ProfileState),
    /// The profiles to choose from: id, name.
    Choose(Vec<(String, String)>),
}

#[derive(PartialEq)]
pub struct GateWindow {
    pub gate: Gate,
}

impl GateWindow {
    /// Starts the browser again, in place of this process.
    fn again(profile: Option<String>) {
        spawn(async move {
            spawn_blocking(move || crate::app::Launch::relaunch(profile.as_deref())).await;
        });
    }
}

impl Component for GateWindow {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let body: Element = match &self.gate {
            Gate::Problem(state) => {
                let mut page =
                    status_page(Icon::DialogWarning, state.title()).description(state.text());
                if state.can_try_again() {
                    page = page.action(tr!("browser-profile-try-again-button"), move |_: ()| {
                        Self::again(None)
                    });
                } else if matches!(state, ProfileState::RegistryDamaged { .. }) {
                    page = page.action(tr!("browser-profile-start-over-button"), move |_: ()| {
                        spawn(async move {
                            spawn_blocking(|| {
                                // A list that cannot be read is kept beside the new one.
                                let _ = Profiles::new(Places::from_environment()).start_over();
                                crate::app::Launch::relaunch(None);
                            })
                            .await;
                        });
                    });
                }
                page.into_element()
            }
            Gate::Choose(profiles) => {
                let mut group = Group::new(tr!("browser-profile-choose-title"));
                for (id, name) in profiles {
                    let id = id.clone();
                    group = group.row(
                        row(name.clone())
                            .leading(Icon::AvatarDefault)
                            .navigates()
                            .on_press(move |_: ()| Self::again(Some(id.clone()))),
                    );
                }
                status_page(Icon::AvatarDefault, tr!("browser-profile-which-title"))
                    .description(tr!("browser-profile-which-description"))
                    .child(clamp(Dim::PopupNarrow, group))
                    .into_element()
            }
        };
        let header = HeaderBar::new(tr!("browser-title")).trailing(WindowControls);
        rect()
            .tw(
                &tokens,
                &[w_full(), h_full(), ink(Paint::Text), bg(Paint::Bg)],
            )
            .font_family(bingux_ui::theme::font_family())
            .child(Screen::new(header).content(body))
    }
}
