//! The window: bingux-ui's navigation split view over the page registry.

use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::pages::PageId;
use crate::system;

impl NavItem for PageId {
    fn title(self) -> &'static str {
        PageId::title(self)
    }

    fn icon(self) -> Icon {
        PageId::icon(self)
    }

    fn tile_rgb(self) -> [f32; 3] {
        PageId::tile_rgb(self)
    }

    fn keywords(self) -> &'static str {
        PageId::keywords(self)
    }
}

/// The current user's display name for the sidebar card.
fn current_user_name() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    system::run(
        "getent",
        &["passwd", &user],
        std::time::Duration::from_secs(2),
    )
    .ok()
    .and_then(|line| {
        line.split(':')
            .nth(4)
            .map(|gecos| gecos.split(',').next().unwrap_or("").trim().to_string())
    })
    .filter(|name| !name.is_empty())
    .unwrap_or(user)
}

/// Window root. `initial` is the page named on the command line.
#[derive(PartialEq)]
pub struct App {
    pub initial: PageId,
}

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let page = use_state(|| self.initial);
        let user_name = use_hook(current_user_name);
        let maximized = freya::borderless::use_maximized();
        rect()
            .expanded()
            .color(tokens.text)
            .font_family(bingux_ui::theme::FONT_FAMILY)
            .corner_radius(if maximized() {
                0.0
            } else {
                bingux_ui::theme::WINDOW_RADIUS
            })
            .overflow(Overflow::Clip)
            .child(OverlayHost::new(
                NavigationSplitView::new("Settings", PageId::ALL.to_vec(), page, page().render())
                    .profile(ProfileCard {
                        name: user_name,
                        subtitle: "Users and accounts".into(),
                        target: PageId::Users,
                    }),
            ))
    }
}
