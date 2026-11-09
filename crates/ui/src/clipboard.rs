//! The clipboard, typed: copy a [`Payload`] (text, files, a picture, HTML,
//! any MIME type, several at once) and paste back the [`Format`] you want.
//!
//! ```ignore
//! Clipboard::general().write(Payload::new().html(html).text(plain));
//! let files = Clipboard::general().read(Format::Uris).map(|data| data.files());
//! Clipboard::primary().write_text(selected);   // middle-click paste
//! ```
//!
//! [`Clipboard::general`] is the clipboard of Copy and Paste;
//! [`Clipboard::primary`] is the selection a middle click pastes. On Wayland
//! both go through the same connection as drag and drop (see [`crate::dnd`]),
//! and Freya's text fields use it too: once the connection is up, Freya's
//! own clipboard calls land here. With no Wayland (a test, X11) the general
//! clipboard's text goes through Freya's clipboard, and everything else stays
//! in this program.
//!
//! Reads wait for the other program for at most [`READ_TIMEOUT`]; use
//! [`Clipboard::read_async`] for something large, a picture.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use crate::dnd::{Format, Payload, Received};

/// The longest a paste waits for the program that copied.
pub const READ_TIMEOUT: Duration = Duration::from_millis(1500);

/// Why a read or write failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    /// Nothing there in the asked format.
    Empty,
    /// The program that copied did not answer in time, or the data could
    /// not be read.
    Unavailable,
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ClipboardError::Empty => "the clipboard has nothing in that format",
            ClipboardError::Unavailable => "the clipboard could not be read",
        })
    }
}

impl std::error::Error for ClipboardError {}

/// A clipboard: the general one or the primary selection. `Copy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clipboard {
    primary: bool,
}

thread_local! {
    /// What was copied with no Wayland connection: the formats Freya's text
    /// clipboard cannot hold, and the whole primary selection.
    static LOCAL: RefCell<[Vec<(String, Arc<[u8]>)>; 2]> = const { RefCell::new([Vec::new(), Vec::new()]) };
}

impl Clipboard {
    /// The clipboard of Copy and Paste.
    pub const fn general() -> Self {
        Self { primary: false }
    }

    /// The primary selection: the text last selected, pasted with a middle
    /// click.
    pub const fn primary() -> Self {
        Self { primary: true }
    }

    /// Put `payload` on the clipboard, every type of it, made now (lazy parts
    /// too: the clipboard outlives the window).
    pub fn write(self, payload: Payload) {
        let data = payload.resolve();
        #[cfg(target_os = "linux")]
        if let Some(worker) = crate::dnd::wayland::worker() {
            worker.send(crate::dnd::wayland::Command::SetSelection {
                primary: self.primary,
                data,
            });
            return;
        }
        if !self.primary
            && let Some(text) = text_of(&data)
            && freya_available()
        {
            let _ = freya::clipboard::Clipboard::set(text);
        }
        LOCAL.with_borrow_mut(|local| local[usize::from(self.primary)] = data);
    }

    /// Put plain text on the clipboard.
    pub fn write_text(self, text: impl Into<String>) {
        self.write(Payload::new().text(text));
    }

    /// Empty the clipboard, if this program put what is on it there.
    pub fn clear(self) {
        #[cfg(target_os = "linux")]
        if let Some(worker) = crate::dnd::wayland::worker() {
            worker.send(crate::dnd::wayland::Command::SetSelection {
                primary: self.primary,
                data: Vec::new(),
            });
            return;
        }
        LOCAL.with_borrow_mut(|local| local[usize::from(self.primary)].clear());
    }

    /// The MIME types on the clipboard now.
    pub fn mime_types(self) -> Vec<String> {
        #[cfg(target_os = "linux")]
        if let Some(worker) = crate::dnd::wayland::worker() {
            let selection = worker
                .shared
                .selection
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            return if self.primary {
                selection.primary.clone()
            } else {
                selection.clipboard.clone()
            };
        }
        let mut types: Vec<String> = LOCAL.with_borrow(|local| {
            local[usize::from(self.primary)]
                .iter()
                .map(|(mime, _)| mime.clone())
                .collect()
        });
        if !self.primary
            && !types.iter().any(|mime| Format::Text.matches(mime))
            && freya_text().is_some()
        {
            types.push(crate::dnd::TEXT_TYPES[0].to_string());
        }
        types
    }

    /// The formats on the clipboard now: what a Paste would get.
    pub fn formats(self) -> Vec<Format> {
        Format::list(&self.mime_types())
    }

    /// Whether the clipboard has `format`: to enable a Paste.
    pub fn has(self, format: &Format) -> bool {
        format.best_mime(&self.mime_types()).is_some()
    }

    /// Read the clipboard as `format`, waiting for the program that copied
    /// for at most [`READ_TIMEOUT`]. Call it from a handler, not a render.
    pub fn read(self, format: Format) -> Result<Received, ClipboardError> {
        let mime_types = self.mime_types();
        let mime = format
            .best_mime(&mime_types)
            .ok_or(ClipboardError::Empty)?
            .to_string();
        #[cfg(target_os = "linux")]
        if let Some(worker) = crate::dnd::wayland::worker() {
            return read_wayland(worker, self.primary, mime)
                .map(|(mime, bytes)| Received::new(mime, bytes));
        }
        let local = LOCAL.with_borrow(|local| {
            local[usize::from(self.primary)]
                .iter()
                .find(|(offered, _)| *offered == mime)
                .map(|(_, bytes)| bytes.to_vec())
        });
        // Freya's text fields copy there too, so its text is the latest.
        let freya = (!self.primary && matches!(format, Format::Text))
            .then(freya_text)
            .flatten();
        match (freya, local) {
            (Some(text), _) => Ok(Received::new(mime, text.into_bytes())),
            (None, Some(bytes)) => Ok(Received::new(mime, bytes)),
            (None, None) => Err(ClipboardError::Empty),
        }
    }

    /// [`read`](Self::read) on a background thread, for data that may be
    /// large or slow (a picture from an image editor).
    pub async fn read_async(self, format: Format) -> Result<Received, ClipboardError> {
        #[cfg(target_os = "linux")]
        if let Some(worker) = crate::dnd::wayland::worker() {
            let mime = format
                .best_mime(&self.mime_types())
                .ok_or(ClipboardError::Empty)?
                .to_string();
            let primary = self.primary;
            return crate::task::spawn_blocking(move || read_wayland(worker, primary, mime))
                .await
                .map(|(mime, bytes)| Received::new(mime, bytes));
        }
        self.read(format)
    }

    /// The clipboard as plain text, or `None`.
    pub fn read_text(self) -> Option<String> {
        self.read(Format::Text).ok()?.text()
    }
}

/// Whether Freya has a clipboard here (not in a plain unit test).
fn freya_available() -> bool {
    freya::prelude::GlobalContexts::try_get()
        .and_then(|globals| globals.try_get_context::<freya::clipboard::Clipboard>())
        .is_some()
}

/// The text on Freya's clipboard, if it has some.
fn freya_text() -> Option<String> {
    freya_available()
        .then(|| freya::clipboard::Clipboard::get().ok())
        .flatten()
        .filter(|text| !text.is_empty())
}

/// The text among copied representations, if any.
fn text_of(data: &[(String, Arc<[u8]>)]) -> Option<String> {
    let mime_types: Vec<String> = data.iter().map(|(mime, _)| mime.clone()).collect();
    let mime = Format::Text.best_mime(&mime_types)?;
    let bytes = &data.iter().find(|(offered, _)| offered == mime)?.1;
    String::from_utf8(bytes.to_vec()).ok()
}

#[cfg(target_os = "linux")]
fn read_wayland(
    worker: &crate::dnd::wayland::Worker,
    primary: bool,
    mime: String,
) -> Result<(String, Vec<u8>), ClipboardError> {
    use crate::dnd::wayland::{Command, READ_LIMIT, read_all};
    use std::sync::mpsc;

    // What this program copied comes from memory: no round trip, and no
    // wait on a pipe this same program has to fill.
    let own = {
        let selection = worker
            .shared
            .selection
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if primary {
            selection.own_primary.clone()
        } else {
            selection.own_clipboard.clone()
        }
    };
    if let Some(own) = own {
        return own
            .iter()
            .find(|(offered, _)| *offered == mime)
            .map(|(_, bytes)| (mime.clone(), bytes.to_vec()))
            .ok_or(ClipboardError::Empty);
    }
    let (reply, answer) = mpsc::channel();
    worker.send(Command::ReceiveSelection {
        primary,
        mime: mime.clone(),
        reply,
    });
    let pipe = answer
        .recv_timeout(READ_TIMEOUT)
        .map_err(|_| ClipboardError::Unavailable)?
        .map_err(|_| ClipboardError::Empty)?;
    let (done, result) = mpsc::channel();
    std::thread::Builder::new()
        .name("bingux-paste".into())
        .spawn(move || {
            let _ = done.send(read_all(pipe, READ_LIMIT));
        })
        .map_err(|_| ClipboardError::Unavailable)?;
    let bytes = result
        .recv_timeout(READ_TIMEOUT)
        .map_err(|_| ClipboardError::Unavailable)?
        .map_err(|_| ClipboardError::Unavailable)?;
    Ok((mime, bytes))
}

/// Freya's text fields read and write `freya::clipboard::Clipboard`; once
/// the Wayland connection is up, that is this clipboard's text.
#[cfg(target_os = "linux")]
struct FreyaBridge;

#[cfg(target_os = "linux")]
impl copypasta::ClipboardProvider for FreyaBridge {
    fn get_contents(&mut self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Clipboard::general()
            .read(Format::Text)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)
            .and_then(|data| {
                data.text()
                    .ok_or_else(|| Box::new(ClipboardError::Empty) as Box<_>)
            })
    }

    fn set_contents(
        &mut self,
        contents: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Clipboard::general().write_text(contents);
        Ok(())
    }
}

/// Route Freya's clipboard through this one. Called once the Wayland
/// connection is up; never before, so a test's own clipboard stays.
#[cfg(target_os = "linux")]
pub(crate) fn install_freya_bridge() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::Relaxed) {
        return;
    }
    if let Some(globals) = freya::prelude::GlobalContexts::try_get() {
        globals.insert_context(freya::clipboard::Clipboard::create(Some(Box::new(
            FreyaBridge,
        ))));
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes guarded: a copy with no Wayland losing its non-text
    //! types, and the primary selection leaking into the general clipboard.
    use super::*;

    #[test]
    fn without_wayland_every_type_stays_in_the_program() {
        Clipboard::general().write(Payload::new().bytes("application/x-test", b"abc".to_vec()));
        let data = Clipboard::general()
            .read(Format::Mime("application/x-test".into()))
            .unwrap();
        assert_eq!(data.bytes(), b"abc");
    }

    #[test]
    fn the_primary_selection_is_its_own() {
        Clipboard::primary().write(Payload::new().bytes("application/x-primary", vec![1]));
        assert!(Clipboard::primary().has(&Format::Mime("application/x-primary".into())));
        assert!(!Clipboard::general().has(&Format::Mime("application/x-primary".into())));
    }
}
