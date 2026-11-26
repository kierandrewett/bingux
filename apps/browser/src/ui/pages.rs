//! The pages the browser draws itself in the content area: the new tab page and
//! the pages that stand where a page could not be shown (an address that could
//! not be loaded, a connection that cannot be trusted, a tab that crashed).
//! Each is a status page of bingux-ui with the words of `model::failure`.

use bingux_ui::prelude::*;
use bingux_ui::search;
use bingux_ui::web_view::requests::CertError;
use bingux_ui::web_view::{FailureCause, GoneReason, LoadError};
use freya::prelude::*;

use crate::address::Address;
use crate::model::failure::{self, CERT_RISK};
use crate::profile::history;

use super::ctx::Ctx;

/// How many visited pages the new tab page lists.
const RECENT: usize = 5;

/// The page of a tab that has no address.
pub fn new_tab(ctx: Ctx, key: u64) -> Element {
    NewTab { ctx, key }.into()
}

/// The new tab page. A component of its own: the pages visited last are the
/// `history` searchable's, asked off the UI thread.
#[derive(PartialEq)]
struct NewTab {
    ctx: Ctx,
    key: u64,
}

impl Component for NewTab {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        let (private, has_history) = {
            let browser = ctx.browser.read();
            (browser.options.private, browser.has_history())
        };
        let found = search::use_search_with(history::recent(RECENT), has_history);
        let page = if private {
            status_page(Icon::NetworkWirelessEncrypted, "Private browsing")
                .description("Nothing you do in this window is kept: no history, cookies or site data. Downloads you save stay on this computer.")
        } else {
            status_page(Icon::WebBrowser, "New tab")
                .description("Search or enter an address in the bar above.")
        };
        if found.hits.is_empty() {
            return page.into_element();
        }
        let mut group = Group::new("Recently visited".to_string());
        for visit in found.hits.iter().map(history::visited) {
            let name = if visit.title.is_empty() {
                Address::new(&visit.url).display()
            } else {
                visit.title
            };
            let target = visit.url.clone();
            group = group.row(
                row(name)
                    .subtitle(Address::new(&visit.url).display())
                    .leading(Icon::Clock)
                    .navigates()
                    .on_press(move |_: ()| ctx.with(|browser| browser.navigate(key, &target))),
            );
        }
        page.child(clamp(Dim::PopupNarrow, group)).into_element()
    }
}

/// A load that failed.
pub fn failed(ctx: Ctx, key: u64, error: &LoadError, url: &str) -> Element {
    let page = failure::for_load_error(error, &Address::new(url).host());
    let mut view = status_page(page.icon, page.title)
        .description(page.text)
        .child(caption(page.code));
    if page.retry {
        view = view.action("Try again", move |_: ()| {
            ctx.with(|browser| browser.reload(key, false))
        });
    }
    view.into_element()
}

/// A tab whose process went away.
pub fn crashed(ctx: Ctx, key: u64, reason: GoneReason) -> Element {
    let page = failure::for_process_gone(reason);
    status_page(page.icon, page.title)
        .description(page.text)
        .action("Reload tab", move |_: ()| {
            ctx.with(|browser| browser.reload(key, false))
        })
        .into_element()
}

/// A connection that cannot be trusted: what is wrong, a way back, and for a
/// problem that may be overridden, the details and the way on. The details are
/// a dialog, so the page stays one screen on any size and the way on is a
/// destructive response that Enter never takes.
pub fn cert_error(ctx: Ctx, key: u64, error: &CertError) -> Element {
    let host = error.host.clone();
    let shown = error.clone();
    let details = move |_: ()| {
        let mut group = Group::new(None::<String>)
            .row(property_row("Site", shown.host.clone()))
            .row(property_row("Problem", shown.code.clone()));
        if let Some(issuer) = &shown.issuer {
            group = group.row(property_row("Issued by", issuer.clone()));
        }
        if let Some(print) = &shown.fingerprint {
            group = group.row(property_row("Fingerprint", print.clone()));
        }
        let note = if shown.overridable {
            CERT_RISK
        } else {
            "This site asks to be reached only over a secure connection, so the browser does not let you go on."
        };
        let mut dialog = dialog("Certificate details", note)
            .content(group)
            .action(CANCEL_ID, "Close", ResponseKind::Default, |_: ()| {})
            .default_action(CANCEL_ID)
            .cancel_action(CANCEL_ID);
        if shown.overridable {
            dialog = dialog.response(
                "Proceed (unsafe)",
                ResponseKind::Destructive,
                move |_: ()| {
                    ctx.with(|browser| browser.cert_proceed(key));
                },
            );
        }
        ctx.overlays.show_dialog(dialog);
    };
    status_page(Icon::DialogWarning, "Your connection is not private")
        .description(format!(
            "{} Attackers might be trying to steal what you send to {host}.",
            failure::cert_problem_text(error)
        ))
        .action("Go back", move |_: ()| {
            ctx.with(|browser| browser.cert_go_back(key))
        })
        .child(action_button("Details\u{2026}", details).flat())
        .into_element()
}

/// The engine itself is gone: nothing in the window can work. When a new one
/// may be started the tabs are kept, and "Start again" opens them where they
/// were; a window that cannot go on offers only to close.
pub fn engine_failed(
    ctx: Ctx,
    cause: FailureCause,
    message: &str,
    code: u32,
    restartable: bool,
) -> Element {
    EngineFailed {
        ctx,
        cause,
        message: message.to_string(),
        code,
        restartable,
    }
    .into()
}

/// The page of a window whose engine is gone. It is a component of its own so
/// that it paints the window's background (nothing else is on screen) with the
/// tokens of the window.
#[derive(PartialEq)]
struct EngineFailed {
    ctx: Ctx,
    cause: FailureCause,
    message: String,
    code: u32,
    restartable: bool,
}

impl Component for EngineFailed {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let ctx = self.ctx;
        let window = ctx.window;
        let page =
            failure::for_engine_failure(self.cause, &self.message, self.code, self.restartable);
        let mut view = status_page(page.icon, page.title).description(page.text);
        // A status page holds one element under its action: the way out, when
        // there is a way on, and how the engine ended, when it ran.
        let mut under = Vec::new();
        if page.restart {
            view = view.action("Start again", move |_: ()| {
                ctx.with(|browser| browser.restart_engine())
            });
            under.push(
                action_button("Close window", move |_: ()| window.close())
                    .flat()
                    .into_element(),
            );
        } else {
            view = view.action("Close window", move |_: ()| window.close());
        }
        if !page.detail.is_empty() {
            under.push(caption(page.detail));
        }
        if !under.is_empty() {
            view = view.child(
                rect()
                    .tw(&tokens, &[flex_col(), items(Align::Center), gap(S2)])
                    .children(under),
            );
        }
        rect()
            .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
            .child(view)
    }
}
