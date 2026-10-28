//! Search results from apps: what a running app exposes over its search
//! socket and what a closed app left in its snapshot. The apps, the socket and
//! snapshot formats and the merge are `bingux_search::aggregate`; this module
//! adapts them to the daemon's candidates. See `design/search.md` 8.8.
//!
//! An app is searched only when it has a `bingux.search-app` manifest in
//! `<data dir>/bingux/search-apps/`. The user's `disabledProviders` switches an
//! app off (the provider id is the app id) and `appWeights` scales its scores.

use std::path::PathBuf;

use bingux_search::aggregate::{self, Aggregator};

use crate::config::SearchConfig;
use crate::protocol::{ProviderResult, ResultKind};
use crate::providers::{Activation, Candidate};

/// The most results taken from one app.
const MAX_PER_APP: usize = 5;

pub struct AppSearch {
    aggregator: std::sync::Arc<Aggregator>,
}

impl AppSearch {
    pub fn new(config: &SearchConfig) -> Self {
        Self::with_dirs(config, &aggregate::manifest_dirs())
    }

    /// Looks for manifests in `dirs` instead of the data directories.
    pub fn with_dirs(config: &SearchConfig, dirs: &[PathBuf]) -> Self {
        Self::with_paths(config, dirs, None, None)
    }

    /// Looks for manifests in `dirs`, app sockets in `socket_dir` and
    /// snapshots under `state_root`, each defaulting to the XDG location.
    pub fn with_paths(
        config: &SearchConfig,
        dirs: &[PathBuf],
        socket_dir: Option<PathBuf>,
        state_root: Option<PathBuf>,
    ) -> Self {
        let manifests: Vec<_> = aggregate::load_manifests(dirs)
            .into_iter()
            // The daemon's provider ids are lower case words joined by `-`.
            .filter(|m| is_provider_id(&m.id))
            .collect();
        let mut aggregator = Aggregator::new(manifests);
        if let Some(dir) = socket_dir {
            aggregator = aggregator.socket_dir(dir);
        }
        if let Some(root) = state_root {
            aggregator = aggregator.state_root(root);
        }
        for (app, weight) in &config.app_weights {
            if weight.is_finite() && *weight >= 0.0 {
                aggregator = aggregator.weight(app, *weight);
            }
        }
        for app in &config.disabled_providers {
            aggregator = aggregator.disable(app);
        }
        let this = AppSearch {
            aggregator: std::sync::Arc::new(aggregator),
        };
        // Load the snapshots now, off the first keystroke.
        let warm = this.aggregator.clone();
        let _ = std::thread::Builder::new()
            .name("bingux-search-app-warm".into())
            .spawn(move || warm.warm());
        this
    }

    /// The best results of every app for `query`, at most `per_app` each.
    pub fn query(&self, query: &str, per_app: usize) -> Vec<Candidate> {
        if self.aggregator.manifests().is_empty() {
            return Vec::new();
        }
        let found = self
            .aggregator
            .search_text(query, per_app.min(MAX_PER_APP), 32);
        let mut out = Vec::new();
        for h in found.hits {
            let Some(uri) = h.hit.uri() else { continue };
            let Some(command) = self.aggregator.open_command(&h.app, uri) else {
                continue;
            };
            let title = clean(h.hit.title().unwrap_or(&h.hit.id));
            let subtitle = h
                .hit
                .joined("breadcrumb", " \u{203a} ")
                .map(|(text, _)| clean(&text))
                .unwrap_or_default();
            let icon = clean(h.hit.icon().unwrap_or_default());
            let result_id = format!(
                "app:{}:{:016x}",
                h.app,
                stable_hash(format!("{}\0{}", h.hit.index, h.hit.id).as_bytes())
            );
            let mut arguments = command;
            let program = arguments.remove(0);
            out.push(Candidate {
                provider_id: h.app.clone(),
                result: ProviderResult {
                    result_id,
                    kind: ResultKind::Action,
                    title,
                    subtitle,
                    icon,
                    score: h.weighted.clamp(0.0, 1.0),
                },
                desktop_id: None,
                activation: Activation::Spawn { program, arguments },
            });
        }
        out
    }
}

fn is_provider_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= crate::protocol::MAX_PROVIDER_ID_BYTES
        && b[0] != b'-'
        && b[b.len() - 1] != b'-'
        && !id.contains("--")
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// Display text has no control characters.
fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
