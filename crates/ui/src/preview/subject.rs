//! What a preview shows, found off the UI thread: the file's kind, the viewer
//! that handles it and what that viewer reads from disk.
//!
//! [`resolve`] runs the registry's fallback chain: the best viewer loads the
//! source; when it cannot, the next is tried, and the reasons are kept so the
//! card can say why the better viewer did not show the file.

use std::sync::LazyLock;
use std::sync::Arc;

use bingux_preview_core::{Cancel, FileFacts, Loaded, PreviewSource, Registry, Sniffed, ViewerId};

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::standard);

/// A source, ready to be shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub source: PreviewSource,
    pub sniffed: Sniffed,
    pub facts: FileFacts,
    /// The viewer that loaded the source.
    pub viewer: ViewerId,
    pub loaded: Loaded,
    /// Why viewers that claimed the source before `viewer` gave up.
    pub notes: Vec<String>,
}

/// Sniff `source`, then load it with the first viewer that can.
/// Blocks on the disk: run it on a worker.
pub fn resolve(source: PreviewSource) -> Result<Arc<Resolved>, String> {
    resolve_with(&REGISTRY, source)
}

pub(crate) fn resolve_with(registry: &Registry, source: PreviewSource) -> Result<Arc<Resolved>, String> {
    let sniffed = Sniffed::of(&source).map_err(|error| bingux_preview_core::text::plain(&error))?;
    let facts = FileFacts::of(&source, &sniffed);
    let cancel = Cancel::new();
    let mut notes = Vec::new();
    for previewer in registry.chain(&sniffed) {
        match previewer.load(&source, &sniffed, &cancel) {
            Ok(loaded) => {
                return Ok(Arc::new(Resolved {
                    source,
                    sniffed,
                    facts,
                    viewer: previewer.id(),
                    loaded,
                    notes,
                }));
            }
            Err(error) => notes.push(error.message().to_owned()),
        }
    }
    Err("There is no preview for this file.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve_bytes(name: &str, bytes: &[u8]) -> Arc<Resolved> {
        resolve(PreviewSource::bytes(name, bytes.to_vec())).unwrap()
    }

    #[test]
    fn text_resolves_to_the_text_viewer() {
        let resolved = resolve_bytes("a.txt", b"hello\n");
        assert_eq!(resolved.viewer, ViewerId::Text);
        assert!(resolved.notes.is_empty());
        assert_eq!(resolved.facts.name, "a.txt");
        assert_eq!(resolved.facts.size, Some(6));
    }

    #[test]
    fn a_broken_archive_falls_to_the_card_and_says_why() {
        let resolved = resolve_bytes("a.zip", b"PK\x03\x04 not really");
        assert_eq!(resolved.viewer, ViewerId::Card);
        assert_eq!(resolved.notes.len(), 1);
        assert!(resolved.notes[0].contains("damaged"), "{:?}", resolved.notes);
    }

    #[test]
    fn binary_data_is_a_card_without_a_note() {
        let resolved = resolve_bytes("blob.dat", &[0, 1, 2, 3, 0, 0]);
        assert_eq!(resolved.viewer, ViewerId::Card);
        assert!(resolved.notes.is_empty());
    }

    #[test]
    fn a_missing_file_is_an_error_in_words() {
        let missing = PreviewSource::path("/var/tmp/PreviewKit/not-here.png");
        assert_eq!(resolve(missing).err(), Some("This file no longer exists.".to_owned()));
    }

    #[test]
    fn a_folder_resolves_to_the_folder_viewer() {
        let resolved = resolve(PreviewSource::path("/var/tmp")).unwrap();
        assert_eq!(resolved.viewer, ViewerId::Folder);
    }
}
