//! The pages of a load that failed, of a tab that crashed and of a connection
//! that cannot be trusted: each is a status page of bingux-ui.

use bingux_ui::prelude::*;
use bingux_ui::web_view::requests::CertError;
use bingux_ui::web_view::{GoneReason, LoadError};
use freya::prelude::*;

use super::{CertSentence, ErrorPage};
use crate::address::Address;
use crate::ui::Ctx;

/// A load that failed.
#[derive(PartialEq)]
pub struct LoadFailedPage {
    pub ctx: Ctx,
    pub key: u64,
    pub error: LoadError,
    pub url: String,
}

impl Component for LoadFailedPage {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        let page = ErrorPage::for_load(&self.error, &Address::new(&self.url).host());
        let mut view = status_page(page.icon, page.title)
            .description(page.text)
            .child(caption(page.code));
        if page.retry {
            view = view.action("Try again", move |_: ()| {
                ctx.with(|browser| browser.reload(key, false))
            });
        }
        view
    }
}

/// A tab whose process went away.
#[derive(PartialEq)]
pub struct CrashedPage {
    pub ctx: Ctx,
    pub key: u64,
    pub reason: GoneReason,
}

impl Component for CrashedPage {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        let page = ErrorPage::for_crash(self.reason);
        status_page(page.icon, page.title)
            .description(page.text)
            .action("Reload tab", move |_: ()| {
                ctx.with(|browser| browser.reload(key, false))
            })
    }
}

/// A connection that cannot be trusted: what is wrong, a way back, and for a
/// problem that may be overridden, the details and the way on. The details are
/// a dialog, so the page stays one screen on any size and the way on is a
/// destructive response that Enter never takes.
#[derive(PartialEq)]
pub struct CertErrorPage {
    pub ctx: Ctx,
    pub key: u64,
    pub error: CertError,
}

impl CertErrorPage {
    /// The text that tells a person who may proceed what it risks.
    const RISK: &'static str = "Someone on the network could read or change what you send and receive. Go ahead only if you know why the certificate is wrong.";
}

impl Component for CertErrorPage {
    fn render(&self) -> impl IntoElement {
        let (ctx, key) = (self.ctx, self.key);
        let error = &self.error;
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
                CertErrorPage::RISK
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
                CertSentence(error).text()
            ))
            .action("Go back", move |_: ()| {
                ctx.with(|browser| browser.cert_go_back(key))
            })
            .child(action_button("Details\u{2026}", details).flat())
    }
}
