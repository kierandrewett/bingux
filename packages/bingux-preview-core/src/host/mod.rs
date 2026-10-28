//! The helper process that decodes media and renders documents, and how a
//! viewer talks to it. Only the wire format ([`protocol`]) exists so far; the
//! helper itself is Phase 2 of `design/preview.md`.

pub mod protocol;
