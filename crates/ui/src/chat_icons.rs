//! Every icon of the chat app's icon fork as an [`Icon`](crate::icons::Icon)
//! (`Icon::Chat(ChatIcon::House, ChatWeight::Fill)`), for choosing and trying
//! glyphs. Behind the `chat-icons` feature: the outlines of all 1513 icons in
//! three weights are 3 MB of data that an application should not carry.
//!
//! The set the apps use is `Icon`'s named variants, built by
//! `scripts/icons/build.py`, which puts each glyph on the 16 px grid (the
//! fork's own outline, moved by at most half a pixel). A fork icon drawn through this
//! module is the fork's own outline scaled by 1/16 and nothing else, so it can
//! look softer than the named icons. To adopt one, add a row for it in
//! `scripts/icons/labels.tsv`; `scripts/icons/review.py candidates Name:weight`
//! shows how it looks once fitted.
//!
//! The data is `icons/chat/geometry.json`, written by
//! `scripts/icons/import-chat.py` together with `chat_icon_list.rs` (the
//! [`ChatIcon`] enum).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

include!("chat_icon_list.rs");

/// The fork's three weights. Not every icon has all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChatWeight {
    /// 1 px strokes at 16 px.
    Regular,
    /// 1.5 px strokes at 16 px.
    Bold,
    /// Solid masses with cut-outs.
    Fill,
}

impl ChatWeight {
    pub const ALL: [ChatWeight; 3] = [Self::Regular, Self::Bold, Self::Fill];
}

#[derive(Deserialize)]
struct Outlines {
    regular: Vec<String>,
    bold: Option<Vec<String>>,
    fill: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Geometry {
    icons: BTreeMap<String, Outlines>,
}

static GEOMETRY_JSON: &str = include_str!("../icons/chat/geometry.json");

fn geometry() -> &'static Geometry {
    static GEOMETRY: OnceLock<Geometry> = OnceLock::new();
    GEOMETRY.get_or_init(|| serde_json::from_str(GEOMETRY_JSON).expect("icons/chat/geometry.json is valid"))
}

impl ChatIcon {
    fn outlines(self) -> &'static Outlines {
        geometry()
            .icons
            .get(self.name())
            .unwrap_or_else(|| panic!("icons/chat/geometry.json lacks {}", self.name()))
    }

    /// The weights this icon is drawn in.
    pub fn weights(self) -> Vec<ChatWeight> {
        ChatWeight::ALL
            .into_iter()
            .filter(|weight| self.paths(*weight).is_some())
            .collect()
    }

    fn paths(self, weight: ChatWeight) -> Option<&'static [String]> {
        let outlines = self.outlines();
        match weight {
            ChatWeight::Regular => Some(outlines.regular.as_slice()),
            ChatWeight::Bold => outlines.bold.as_deref(),
            ChatWeight::Fill => outlines.fill.as_deref(),
        }
    }

    /// The SVG source of the icon at `weight` (the fork's outline on a 16 unit
    /// box), or `None` when the fork has no such weight. Built once, then kept.
    pub fn bytes(self, weight: ChatWeight) -> Option<&'static [u8]> {
        static CACHE: OnceLock<Mutex<HashMap<(ChatIcon, ChatWeight), &'static [u8]>>> = OnceLock::new();
        let paths = self.paths(weight)?;
        let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
        Some(*cache.entry((self, weight)).or_insert_with(|| {
            let mut svg = String::from(
                "<svg height=\"16px\" viewBox=\"0 0 16 16\" width=\"16px\" xmlns=\"http://www.w3.org/2000/svg\">\n",
            );
            for d in paths {
                svg.push_str(&format!("    <path transform=\"scale(0.0625)\" d=\"{d}\"/>\n"));
            }
            svg.push_str("</svg>\n");
            Vec::leak(svg.into_bytes())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::Icon;

    #[test]
    fn every_fork_icon_is_an_icon() {
        assert_eq!(ChatIcon::ALL.len(), geometry().icons.len(), "the enum and geometry.json list the same icons");
        for chat in ChatIcon::ALL {
            let weights = chat.weights();
            assert!(weights.contains(&ChatWeight::Regular), "{} has a regular weight", chat.name());
            for weight in weights {
                let bytes = Icon::Chat(chat, weight).bytes();
                assert!(bytes.starts_with(b"<svg"), "{} {weight:?} is an svg", chat.name());
            }
        }
    }

    #[test]
    fn names_match_the_enum() {
        assert_eq!(ChatIcon::House.name(), "House");
        assert!(ChatIcon::Folder.weights().contains(&ChatWeight::Fill));
    }
}
