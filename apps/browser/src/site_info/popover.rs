//! What is known about the page's connection and what the site may do, as a
//! card that hangs from the connection button.

use bingux_ui::prelude::*;
use freya::prelude::*;

use super::{Summary, Tone};
use crate::address::Address;
use crate::tabs::TabKind;
use crate::ui::Ctx;

/// The states of the page info that belong to the window, not to a tab.
#[derive(Clone, Copy, PartialEq)]
pub struct PopoverState {
    open: State<bool>,
    /// Where the connection button is, in the window.
    pub(super) anchor: State<Area>,
}

impl PopoverState {
    /// The states, as a hook: call it on every render, in the same place.
    pub fn use_new() -> Self {
        Self {
            open: use_state(|| false),
            anchor: use_state(Area::default),
        }
    }

    /// Show the page info.
    pub fn open(self) {
        let mut open = self.open;
        open.set(true);
    }
}

impl Tone {
    /// The tone a status row takes.
    fn style(self) -> Option<bingux_ui::style::Tone> {
        match self {
            Self::Neutral | Self::Good => None,
            Self::Warning => Some(bingux_ui::style::Tone::Warning),
            Self::Danger => Some(bingux_ui::style::Tone::Danger),
        }
    }
}

/// The page info, open or not.
pub struct SiteInfoPopover;

impl SiteInfoPopover {
    pub fn element(ctx: Ctx) -> Element {
        let state = ctx.site_info;
        let mut open = state.open;
        let anchor = *state.anchor.read();
        popover(open(), &anchor, page_info(ctx))
            .width_px(ctx.popover_width())
            .on_dismiss(move |_: ()| open.set(false))
            .into()
    }
}

/// The connection of the page, and what the site may do.
fn page_info(ctx: Ctx) -> Element {
    let (summary, origin, remembered, key, internal) = {
        let browser = ctx.browser.read();
        let Some(tab) = browser.active() else {
            return rect().into_element();
        };
        let summary = Summary::of(&tab.security, &tab.url);
        let origin = Address::new(&tab.url).origin();
        (
            summary,
            origin.clone(),
            browser.permissions.for_origin(&origin),
            tab.key,
            tab.kind == TabKind::Settings || tab.is_blank(),
        )
    };
    let mut status_row = row(summary.headline.clone())
        .subtitle(summary.detail.clone())
        .leading(summary.icon);
    if let Some(tone) = summary.tone.style() {
        status_row = status_row.leading_tone(tone);
    }
    let mut column = rect()
        .tw_layout(&[flex_col(), w_full(), gap(S3)])
        .child(Group::new(None::<String>).row(status_row));
    if !internal && origin.contains("://") {
        let site = Group::new(tr!("browser-site-info-site-title"))
            .row(property_row(
                tr!("browser-site-info-host-label"),
                Address::new(&origin).host(),
            ))
            .row(property_row(tr!("browser-site-info-address-label"), {
                let browser = ctx.browser.read();
                browser
                    .tabs
                    .get(key)
                    .map(|tab| Address::new(&tab.url).display())
                    .unwrap_or_default()
            }));
        column = column.child(site);
    }
    if !remembered.is_empty() {
        let mut group = Group::new(tr!("browser-site-info-permissions-title"))
            .footer(tr!("browser-site-info-permissions-footer"));
        for kept in remembered {
            let site = origin.clone();
            let forget = kept.key.clone();
            group = group.row(
                row(kept.label)
                    .value(if kept.allowed {
                        tr!("browser-site-info-allowed")
                    } else {
                        tr!("browser-site-info-blocked")
                    })
                    .trailing(
                        action_button(tr!("browser-site-info-reset-button"), move |_: ()| {
                            ctx.with(|browser| browser.permissions.forget(&site, &forget));
                        })
                        .compact()
                        .flat(),
                    ),
            );
        }
        column = column.child(group);
    }
    column.into_element()
}
