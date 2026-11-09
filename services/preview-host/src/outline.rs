//! A PDF's outline: the table of contents the author wrote.
//!
//! `poppler-rs` has no safe reader for an outline entry's action, so this
//! walks poppler's `PopplerIndexIter` through the C interface. Every pointer
//! poppler hands out is freed here, on every path.

use std::ffi::{CStr, c_char};

use bingux_preview_core::host::protocol::{MAX_OUTLINE_ENTRIES, OutlineEntry};
use glib::translate::ToGlibPtr;
use poppler::Document;
use poppler_sys as ffi;

/// The deepest level of the outline that is read.
const MAX_DEPTH: u8 = 8;

/// The outline of `document`, depth first, with the page each entry goes to.
/// Empty when the document has none.
pub fn entries(document: &Document) -> Vec<OutlineEntry> {
    let mut out = Vec::new();
    let document: *mut ffi::PopplerDocument = document.to_glib_none().0;
    // SAFETY: `document` is a live PopplerDocument for the whole call. The
    // iterator poppler returns is ours to free; a null one means no outline.
    unsafe {
        let iter = ffi::poppler_index_iter_new(document);
        if iter.is_null() {
            return out;
        }
        walk(document, iter, 0, &mut out);
        ffi::poppler_index_iter_free(iter);
    }
    out
}

/// Read the entries of one level, and each one's children, then stop.
///
/// # Safety
/// `document` and `iter` are live poppler objects; `iter` stays owned by the caller.
unsafe fn walk(
    document: *mut ffi::PopplerDocument,
    iter: *mut ffi::PopplerIndexIter,
    depth: u8,
    out: &mut Vec<OutlineEntry>,
) {
    loop {
        if out.len() >= MAX_OUTLINE_ENTRIES {
            return;
        }
        // SAFETY: both pointers are live (the caller's contract); the action
        // is ours to free once read.
        unsafe {
            let action = ffi::poppler_index_iter_get_action(iter);
            if !action.is_null() {
                if let Some((title, page)) = target(document, action) {
                    out.push(OutlineEntry { depth, page, title });
                }
                ffi::poppler_action_free(action);
            }
            if depth < MAX_DEPTH {
                let child = ffi::poppler_index_iter_get_child(iter);
                if !child.is_null() {
                    walk(document, child, depth + 1, out);
                    ffi::poppler_index_iter_free(child);
                }
            }
            if ffi::poppler_index_iter_next(iter) == 0 {
                return;
            }
        }
    }
}

/// The title and page (from 0) of an action that goes to a place in the
/// document; `None` for any other kind (a link out, a command).
///
/// # Safety
/// `action` is a live `PopplerAction`.
unsafe fn target(
    document: *mut ffi::PopplerDocument,
    action: *mut ffi::PopplerAction,
) -> Option<(String, u32)> {
    // SAFETY: the union's tag is read first, then only the member it names.
    unsafe {
        if (*action).type_ != ffi::POPPLER_ACTION_GOTO_DEST {
            return None;
        }
        let goto = &(*action).goto_dest;
        let title = text(goto.title).unwrap_or_default();
        let dest = goto.dest;
        if dest.is_null() {
            return Some((title, u32::MAX));
        }
        let page = if (*dest).type_ == ffi::POPPLER_DEST_NAMED {
            // A name the document's catalogue resolves to a place.
            match text((*dest).named_dest) {
                Some(name) => match std::ffi::CString::new(name) {
                    Ok(name) => {
                        let found = ffi::poppler_document_find_dest(document, name.as_ptr());
                        if found.is_null() {
                            u32::MAX
                        } else {
                            let page = page_of(&*found);
                            ffi::poppler_dest_free(found);
                            page
                        }
                    }
                    Err(_) => u32::MAX,
                },
                None => u32::MAX,
            }
        } else {
            page_of(&*dest)
        };
        Some((title, page))
    }
}

/// Poppler counts pages from 1; the protocol from 0, with `u32::MAX` for none.
fn page_of(dest: &ffi::PopplerDest) -> u32 {
    u32::try_from(dest.page_num)
        .ok()
        .filter(|page| *page >= 1)
        .map_or(u32::MAX, |page| page - 1)
}

/// A C string as text, or `None` for null.
///
/// # Safety
/// `pointer` is null or a NUL-terminated string.
unsafe fn text(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: not null, and NUL-terminated (the caller's contract).
    Some(
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .trim()
            .to_owned(),
    )
}
