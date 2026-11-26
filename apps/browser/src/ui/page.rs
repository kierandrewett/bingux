//! The content area: what the tab on show has to show. The page, or one of the
//! browser's own pages where the page cannot be. Over it, the address of the
//! link the pointer is on and the thin bar of a load that is under way.

use bingux_ui::component::ComponentName;
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::model::tab::Page;

use super::ctx::Ctx;
use super::pages;
use super::settings::SettingsPage;
use super::web::{Surface, TabPage};

/// What the area holds, copied out of the browser so no lock is held while
/// the elements are built.
enum Shown {
    Web(bingux_ui::web_view::TabId, Surface),
    NewTab,
    Settings,
    Crashed(bingux_ui::web_view::GoneReason),
    Cert(bingux_ui::web_view::requests::CertError),
    Failed(bingux_ui::web_view::LoadError, String),
    /// A page with no surface: the engine has not made one.
    Waiting,
}

#[derive(PartialEq)]
pub struct PageArea {
    pub ctx: Ctx,
    pub engine: crate::engine::Engine,
}

impl Component for PageArea {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let mut origin = ctx.chrome.page_origin;
        let (key, shown, hover, progress) = {
            let browser = ctx.browser.read();
            let key = browser.tabs.active_key();
            let Some(tab) = browser.active() else {
                return rect().into_element();
            };
            let shown = match tab.page() {
                Page::Web => tab
                    .session
                    .and_then(|session| {
                        browser
                            .engine
                            .backend()
                            .surface(session)
                            .map(|surface| Shown::Web(session, Surface(surface)))
                    })
                    .unwrap_or(Shown::Waiting),
                Page::NewTab => Shown::NewTab,
                Page::Settings => Shown::Settings,
                Page::Crashed(reason) => Shown::Crashed(reason),
                Page::CertError(error) => Shown::Cert(error.clone()),
                Page::Failed(error) => Shown::Failed(error.clone(), tab.url.clone()),
            };
            (
                key,
                shown,
                tab.hover.clone(),
                tab.load.loading.then_some(tab.load.progress),
            )
        };
        let body: Element = match shown {
            Shown::Web(session, surface) => TabPage {
                ctx,
                engine: self.engine.clone(),
                key,
                session,
                surface,
            }
            .into(),
            Shown::NewTab => pages::new_tab(ctx, key),
            Shown::Settings => SettingsPage { ctx }.into(),
            Shown::Crashed(reason) => pages::crashed(ctx, key, reason),
            Shown::Cert(error) => pages::cert_error(ctx, key, &error),
            Shown::Failed(error, url) => pages::failed(ctx, key, &error, &url),
            Shown::Waiting => status_page(Icon::WebBrowser, "Starting the engine").into_element(),
        };
        let status = hover.map(|address| {
            rect()
                .component("LinkStatus")
                .position(Position::new_absolute().bottom(S2.px()).left(S2.px()))
                .layer(Layer::Relative(1))
                .interactive(false)
                .tw(
                    &tokens,
                    &[
                        max_w(Dim::Field),
                        px(S3),
                        py(S1),
                        bg(Paint::Floating),
                        rounded(Radius::Pill),
                        border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline)),
                        shadow(Elevation::Raised),
                    ],
                )
                .a11y_role(AccessibilityRole::Status)
                .child(
                    typography::CAPTION
                        .in_ink(Ink::OnFloating)
                        .label(&tokens, address)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
        });
        let bar = progress.map(|fraction| {
            rect()
                .component("LoadProgress")
                .position(Position::new_absolute().top(0.0).left(0.0))
                .layer(Layer::Relative(1))
                .interactive(false)
                .tw(&tokens, &[w_full()])
                .child(progress_bar(f64::from(fraction.max(0.05))))
        });
        rect()
            .component("PageArea")
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_flex(),
                    flex_col(),
                    grow_children(),
                    bg(Paint::Bg),
                ],
            )
            .on_sized(move |event: Event<SizedEventData>| {
                origin.set_if_modified((event.area.min_x(), event.area.min_y()))
            })
            .child(
                rect()
                    .key(key)
                    .tw(&tokens, &[w_full(), h_flex(), flex_col(), grow_children()])
                    .child(body),
            )
            .maybe_child(bar)
            .maybe_child(status)
            .into_element()
    }
}
