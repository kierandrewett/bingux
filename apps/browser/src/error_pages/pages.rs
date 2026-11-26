//! The pages of a load that failed, of a tab that crashed and of a connection
//! that cannot be trusted: each is a status page of bingux-ui.

use bingux_ui::prelude::*;
use bingux_ui::web_view::requests::CertError;
use bingux_ui::web_view::{GoneReason, LoadError};
use freya::prelude::*;

use super::{CERT_RISK, cert_problem_text, for_load_error, for_process_gone};
use crate::address::Address;
use crate::ui::Ctx;

/// A load that failed.
pub fn failed(ctx: Ctx, key: u64, error: &LoadError, url: &str) -> Element {
    let page = for_load_error(error, &Address::new(url).host());
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
    let page = for_process_gone(reason);
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
            cert_problem_text(error)
        ))
        .action("Go back", move |_: ()| {
            ctx.with(|browser| browser.cert_go_back(key))
        })
        .child(action_button("Details\u{2026}", details).flat())
        .into_element()
}
