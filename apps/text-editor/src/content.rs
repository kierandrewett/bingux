//! The editor's side of a document managed by `bingux_ui::document`: the
//! text (an [`EditorHandle`]) and how its file was written (encoding, byte
//! order mark, line endings), the notices that go with it, and the bytes of
//! the file as read for Reopen with Encoding.
//!
//! The shared manager owns the file: it opens, saves, watches and recovers
//! it. This type turns the text into bytes and bytes into text.

use std::cell::RefCell;
use std::rc::Rc;

use bingux_text::encoding::{self, FileFormat};
use bingux_text::{Editor, Rope};
use bingux_ui::document::{DocumentContent, DocumentSnapshot, LoadReason};
use bingux_ui::text_editor::EditorHandle;

use crate::document::Notice;

pub struct EditorContent {
    pub handle: EditorHandle,
    pub format: RefCell<FileFormat>,
    pub notices: RefCell<Vec<Notice>>,
    /// The bytes of the file as read, kept for Reopen with Encoding when small.
    pub raw: RefCell<Option<Rc<Vec<u8>>>>,
    /// The text belongs to the tab's own loader: the manager's load is ignored.
    text_from_tab: bool,
}

impl EditorContent {
    pub fn new(editor: Editor) -> Self {
        Self {
            handle: EditorHandle::new(editor, crate::document::signal()),
            format: RefCell::new(FileFormat::default()),
            notices: RefCell::new(Vec::new()),
            raw: RefCell::new(None),
            text_from_tab: false,
        }
    }

    /// A document over a tab's own editor, which already holds the text.
    pub fn over_tab(handle: EditorHandle, format: FileFormat) -> Self {
        Self {
            handle,
            format: RefCell::new(format),
            notices: RefCell::new(Vec::new()),
            raw: RefCell::new(None),
            text_from_tab: true,
        }
    }
}

impl DocumentContent for EditorContent {
    fn snapshot(&self) -> DocumentSnapshot {
        let rope: Rope = self.handle.peek().rope().clone();
        let format = self.format.borrow().clone();
        DocumentSnapshot::encode(move || {
            let text = rope.to_string();
            encoding::encode(&text, &format).map_err(|error| {
                crate::document::SaveError::Unmappable {
                    count: error.count,
                    first: error.first,
                }
                .message("the document", format.encoding.name())
            })
        })
    }

    fn load(&self, bytes: Vec<u8>, _reason: LoadReason) {
        if self.text_from_tab {
            return;
        }
        let decoded = encoding::decode_vec(bytes);
        *self.format.borrow_mut() = decoded.format;
        self.handle
            .update(|editor| editor.load(Rope::from_str(&decoded.text)));
    }
}
