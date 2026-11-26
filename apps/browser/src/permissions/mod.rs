//! What the person told this window about what sites may use. It lives as long
//! as the window: the engine keeps the permissions it is told to keep, and a
//! decision that must outlive the window is the engine's profile's.

mod banner;

use std::collections::BTreeMap;

use bingux_ui::web_view::requests::PermissionKind;

use crate::address::Address;

pub use banner::PermissionBanner;

/// What a permission is called to a person.
pub fn label(kind: &PermissionKind) -> String {
    match kind {
        PermissionKind::Geolocation => "Know your location",
        PermissionKind::Notifications => "Show notifications",
        PermissionKind::Camera => "Use your camera",
        PermissionKind::Microphone => "Use your microphone",
        PermissionKind::Screen => "Share your screen",
        PermissionKind::Clipboard => "See what you copied",
        PermissionKind::Midi => "Use your MIDI devices",
        PermissionKind::Autoplay => "Play sound on its own",
        PermissionKind::PersistentStorage => "Keep data when space is low",
        PermissionKind::Popups => "Open pop-up windows",
        PermissionKind::Other(name) => return name.clone(),
    }
    .to_string()
}

/// The sentence of a prompt: `example.com wants to know your location and use your camera`.
pub fn prompt_text(origin: &str, kinds: &[PermissionKind]) -> String {
    let host = Address::new(origin).host();
    let host = if host.is_empty() {
        origin.to_string()
    } else {
        host
    };
    let mut wants: Vec<String> = kinds.iter().map(label).collect();
    let last = wants.pop();
    let list = match (wants.is_empty(), last) {
        (true, Some(only)) => only.to_lowercase(),
        (false, Some(last)) => format!(
            "{} and {}",
            wants.join(", ").to_lowercase(),
            last.to_lowercase()
        ),
        (_, None) => "do something".to_string(),
    };
    format!("{host} wants to {list}")
}

/// The decisions made, by site.
#[derive(Clone, Debug, Default)]
pub struct Permissions {
    /// `(origin, permission name)` to allowed or blocked.
    decisions: BTreeMap<(String, String), bool>,
}

impl Permissions {
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
            match self.decisions.get(&(origin.to_string(), label(kind))) {
                Some(true) => {}
                Some(false) => return Some(false),
                None => allowed = false,
            }
        }
        // Allowed only when every one of them was.
        let all_known = kinds.iter().all(|kind| {
            self.decisions
                .contains_key(&(origin.to_string(), label(kind)))
        });
        (allowed && all_known).then_some(true)
    }

    /// Keep `allow` for each of `kinds` at `origin`.
    pub fn remember(&mut self, origin: &str, kinds: &[PermissionKind], allow: bool) {
        for kind in kinds {
            self.decisions
                .insert((origin.to_string(), label(kind)), allow);
        }
    }

    /// What is kept for `origin`: the permission's name and whether it is allowed.
    pub fn for_origin(&self, origin: &str) -> Vec<(String, bool)> {
        self.decisions
            .iter()
            .filter(|((site, _), _)| site == origin)
            .map(|((_, name), allowed)| (name.clone(), *allowed))
            .collect()
    }

    /// Forget one permission of `origin`: the next ask is a prompt again.
    pub fn forget(&mut self, origin: &str, name: &str) {
        self.decisions
            .remove(&(origin.to_string(), name.to_string()));
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
