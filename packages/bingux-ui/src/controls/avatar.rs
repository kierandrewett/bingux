//! An account's avatar, drawn the way the Bingux shell draws the signed-in
//! user: the picture AccountsService keeps for the account, clipped to a
//! circle, or the default person glyph when there is no picture.

use std::path::PathBuf;

use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::use_tokens;
use crate::task::use_resource;

/// An avatar of `size` for the account with login name `user`.
pub fn avatar(user: impl Into<String>, size: Dim) -> Avatar {
    Avatar {
        user: user.into(),
        size,
    }
}

/// The login name of the signed-in account, for its avatar.
pub fn current_user() -> String {
    std::env::var("USER").unwrap_or_default()
}

/// Where AccountsService keeps the picture of `user`. The Bingux shell reads
/// the same file for the signed-in account. It stats the disk, so it runs on
/// the task pool through [`use_resource`], never in `render`.
fn account_picture(user: String) -> Result<Option<PathBuf>, String> {
    if user.is_empty() || user.contains('/') {
        return Ok(None);
    }
    let path = PathBuf::from("/var/lib/AccountsService/icons").join(&user);
    Ok(path.is_file().then_some(path))
}

/// The default glyph's size inside an avatar of `size`.
fn glyph_size(size: Dim) -> IconSize {
    match size {
        Dim::Box7 | Dim::Box11 => IconSize::Row,
        Dim::Box24 => IconSize::Hero,
        _ => IconSize::Control,
    }
}

#[derive(PartialEq)]
pub struct Avatar {
    user: String,
    size: Dim,
}

impl Component for Avatar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let frame = rect().tw(
            &tokens,
            &[
                w(self.size),
                h(self.size),
                rounded(Radius::Pill),
                bg(Paint::Control),
                center(),
                clip(),
            ],
        );
        let picture = use_resource(self.user.clone(), account_picture);
        let path = picture.with(|state| state.value().cloned().flatten());
        match path {
            Some(path) => {
                let px = self.size.px();
                frame.child(
                    ImageViewer::new(path)
                        .width(Size::px(px))
                        .height(Size::px(px)),
                )
            }
            None => frame.child(icons::icon(
                Icon::AvatarDefault,
                glyph_size(self.size),
                Paint::Muted.color(&tokens),
            )),
        }
    }
}
