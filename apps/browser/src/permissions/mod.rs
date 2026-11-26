//! What the person told this window about what sites may use. It lives as long
//! as the window: the engine keeps the permissions it is told to keep, and a
//! decision that must outlive the window is the engine's profile's.

mod banner;

use std::collections::BTreeMap;

use bingux_ui::format::List;
use bingux_ui::prelude::tr;
use bingux_ui::web_view::requests::PermissionKind;

use crate::address::Address;

pub use banner::PermissionBanner;

/// The decisions made, by site.
#[derive(Clone, Debug, Default)]
pub struct Permissions {
    /// `(origin, permission key)` to the permission and whether it is allowed.
    decisions: BTreeMap<(String, String), (PermissionKind, bool)>,
}

/// One permission kept for a site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remembered {
    /// What names it to `forget`, whatever the language shown.
    pub key: String,
    /// What a person calls it.
    pub label: String,
    pub allowed: bool,
}

impl Permissions {
    /// What a permission is called to a person.
    pub fn label(kind: &PermissionKind) -> String {
        match kind {
            PermissionKind::Geolocation => tr!("browser-permissions-geolocation-label"),
            PermissionKind::Notifications => tr!("browser-permissions-notifications-label"),
            PermissionKind::Camera => tr!("browser-permissions-camera-label"),
            PermissionKind::Microphone => tr!("browser-permissions-microphone-label"),
            PermissionKind::Screen => tr!("browser-permissions-screen-label"),
            PermissionKind::Clipboard => tr!("browser-permissions-clipboard-label"),
            PermissionKind::Midi => tr!("browser-permissions-midi-label"),
            PermissionKind::Autoplay => tr!("browser-permissions-autoplay-label"),
            PermissionKind::PersistentStorage => tr!("browser-permissions-storage-label"),
            PermissionKind::Popups => tr!("browser-permissions-popups-label"),
            PermissionKind::Other(name) => name.clone(),
        }
    }

    /// What a permission is, to read as part of a sentence: `know your location`.
    fn want(kind: &PermissionKind) -> String {
        match kind {
            PermissionKind::Geolocation => tr!("browser-permissions-geolocation-want"),
            PermissionKind::Notifications => tr!("browser-permissions-notifications-want"),
            PermissionKind::Camera => tr!("browser-permissions-camera-want"),
            PermissionKind::Microphone => tr!("browser-permissions-microphone-want"),
            PermissionKind::Screen => tr!("browser-permissions-screen-want"),
            PermissionKind::Clipboard => tr!("browser-permissions-clipboard-want"),
            PermissionKind::Midi => tr!("browser-permissions-midi-want"),
            PermissionKind::Autoplay => tr!("browser-permissions-autoplay-want"),
            PermissionKind::PersistentStorage => tr!("browser-permissions-storage-want"),
            PermissionKind::Popups => tr!("browser-permissions-popups-want"),
            PermissionKind::Other(name) => name.to_lowercase(),
        }
    }

    /// The sentence of a prompt: `example.com wants to know your location and use your camera`.
    pub fn prompt_text(origin: &str, kinds: &[PermissionKind]) -> String {
        let host = Address::new(origin).host();
        let host = if host.is_empty() {
            origin.to_string()
        } else {
            host
        };
        let wants: Vec<String> = kinds.iter().map(Self::want).collect();
        let wants = if wants.is_empty() {
            tr!("browser-permissions-prompt-nothing")
        } else {
            List::and(&wants).format()
        };
        tr!("browser-permissions-prompt", host = host, wants = wants)
    }

    /// What names `kind`, whatever the language shown.
    fn key(kind: &PermissionKind) -> String {
        match kind {
            PermissionKind::Other(name) => name.clone(),
            other => format!("{other:?}"),
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    /// What was decided for all of `kinds` at `origin`: `Some(true)` when each
    /// was allowed, `Some(false)` when any was blocked, `None` when the person
    /// has not been asked about one of them.
    pub fn decided(&self, origin: &str, kinds: &[PermissionKind]) -> Option<bool> {
        if kinds.is_empty() {
            return None;
        }
        let mut allowed = true;
        for kind in kinds {
            match self.decisions.get(&(origin.to_string(), Self::key(kind))) {
                Some((_, true)) => {}
                Some((_, false)) => return Some(false),
                None => allowed = false,
            }
        }
        // Allowed only when every one of them was.
        let all_known = kinds.iter().all(|kind| {
            self.decisions
                .contains_key(&(origin.to_string(), Self::key(kind)))
        });
        (allowed && all_known).then_some(true)
    }

    /// Keep `allow` for each of `kinds` at `origin`.
    pub fn remember(&mut self, origin: &str, kinds: &[PermissionKind], allow: bool) {
        for kind in kinds {
            self.decisions
                .insert((origin.to_string(), Self::key(kind)), (kind.clone(), allow));
        }
    }

    /// What is kept for `origin`, by name.
    pub fn for_origin(&self, origin: &str) -> Vec<Remembered> {
        let mut kept: Vec<Remembered> = self
            .decisions
            .iter()
            .filter(|((site, _), _)| site == origin)
            .map(|((_, key), (kind, allowed))| Remembered {
                key: key.clone(),
                label: Self::label(kind),
                allowed: *allowed,
            })
            .collect();
        kept.sort_by(|a, b| a.label.cmp(&b.label));
        kept
    }

    /// Forget one permission of `origin`: the next ask is a prompt again.
    pub fn forget(&mut self, origin: &str, key: &str) {
        self.decisions
            .remove(&(origin.to_string(), key.to_string()));
    }

    pub fn forget_all(&mut self) {
        self.decisions.clear();
    }

    /// Every origin that has a decision, with how many.
    pub fn origins(&self) -> Vec<(String, usize)> {
        let mut origins: Vec<(String, usize)> = Vec::new();
        for (origin, _) in self.decisions.keys() {
            match origins.last_mut() {
                Some((last, count)) if last == origin => *count += 1,
                _ => origins.push((origin.clone(), 1)),
            }
        }
        origins
    }
}

#[cfg(test)]
mod tests;
