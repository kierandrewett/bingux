//! A safe handle on one libmpv player.
//!
//! libmpv's client API is thread-safe: every call here may come from any
//! thread. Calls the interface makes go through the `_async` variants, which
//! queue the request and return at once; the answer, when it matters, comes
//! back as a property change on the event thread.

pub mod ffi;

use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_void};
use std::os::raw::{c_char, c_int};
use std::ptr;

/// An error code from libmpv, with its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub code: i32,
    pub message: String,
}

impl Error {
    fn of(code: c_int) -> Error {
        // SAFETY: mpv_error_string returns a static string for any code.
        let message = unsafe { CStr::from_ptr(ffi::mpv_error_string(code)) }
            .to_string_lossy()
            .into_owned();
        Error { code, message }
    }

    pub(crate) fn of_code(code: c_int) -> Error {
        Error::of(code)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for Error {}

fn check(code: c_int) -> Result<(), Error> {
    if code < 0 {
        Err(Error::of(code))
    } else {
        Ok(())
    }
}

/// A value of mpv's tree type (`MPV_FORMAT_NODE`), copied out of mpv's memory.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    None,
    String(String),
    Flag(bool),
    Int(i64),
    Double(f64),
    Array(Vec<Node>),
    Map(BTreeMap<String, Node>),
}

impl Node {
    /// Copies a node mpv owns. `raw` must be a valid node.
    unsafe fn from_raw(raw: &ffi::mpv_node) -> Node {
        // SAFETY: the union member read is the one `format` names, and the
        // pointers of a valid node are valid for the lengths it gives.
        unsafe {
            match raw.format {
                ffi::MPV_FORMAT_STRING => {
                    Node::String(CStr::from_ptr(raw.u.string).to_string_lossy().into_owned())
                }
                ffi::MPV_FORMAT_FLAG => Node::Flag(raw.u.flag != 0),
                ffi::MPV_FORMAT_INT64 => Node::Int(raw.u.int64),
                ffi::MPV_FORMAT_DOUBLE => Node::Double(raw.u.double_),
                ffi::MPV_FORMAT_NODE_ARRAY => {
                    let list = &*raw.u.list;
                    let values = (0..list.num.max(0) as usize)
                        .map(|index| Node::from_raw(&*list.values.add(index)));
                    Node::Array(values.collect())
                }
                ffi::MPV_FORMAT_NODE_MAP => {
                    let list = &*raw.u.list;
                    let entries = (0..list.num.max(0) as usize).map(|index| {
                        let key = CStr::from_ptr(*list.keys.add(index))
                            .to_string_lossy()
                            .into_owned();
                        (key, Node::from_raw(&*list.values.add(index)))
                    });
                    Node::Map(entries.collect())
                }
                _ => Node::None,
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(map) => map.get(key),
            _ => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Node::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn int(&self, key: &str) -> Option<i64> {
        match self.get(key)? {
            Node::Int(value) => Some(*value),
            Node::Double(value) => Some(*value as i64),
            _ => None,
        }
    }

    pub fn double(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            Node::Double(value) => Some(*value),
            Node::Int(value) => Some(*value as f64),
            _ => None,
        }
    }

    pub fn flag(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Node::Flag(value) => Some(*value),
            _ => None,
        }
    }

    pub fn array(&self) -> &[Node] {
        match self {
            Node::Array(values) => values,
            _ => &[],
        }
    }
}

/// A property value as an observed change carries it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Unavailable (no file, or the property has no value now).
    None,
    Flag(bool),
    Double(f64),
    Int(i64),
    String(String),
    Node(Node),
}

/// The format a property is observed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Flag,
    Double,
    Int,
    String,
    Node,
}

impl Format {
    fn raw(self) -> ffi::mpv_format {
        match self {
            Format::Flag => ffi::MPV_FORMAT_FLAG,
            Format::Double => ffi::MPV_FORMAT_DOUBLE,
            Format::Int => ffi::MPV_FORMAT_INT64,
            Format::String => ffi::MPV_FORMAT_STRING,
            Format::Node => ffi::MPV_FORMAT_NODE,
        }
    }
}

/// What the event thread hears.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Shutdown,
    /// A file starts loading; the id is its playlist entry id.
    StartFile(i64),
    /// A file is open and playing (or paused) from its start.
    FileLoaded,
    /// A file ended: `eof` when it played to the end, `error` when it could not
    /// be played (with mpv's reason).
    EndFile {
        entry: i64,
        eof: bool,
        error: Option<String>,
    },
    /// A seek finished; the picture and the sound are at the new place.
    PlaybackRestart,
    Seek,
    VideoReconfig,
    Property {
        id: u64,
        name: String,
        value: Value,
    },
    CommandReply {
        id: u64,
        error: Option<String>,
    },
    /// A hook registered with [`Handle::add_hook`] runs; mpv waits until
    /// [`Handle::continue_hook`] is called with `id`.
    Hook {
        name: String,
        id: u64,
    },
    Other,
}

/// One libmpv player. Dropping the last [`Handle`] destroys the player; the
/// render context made on it must be freed first (the renderers hold a clone).
pub struct Handle {
    raw: *mut ffi::mpv_handle,
}

// SAFETY: libmpv's client API is thread-safe (client.h, "Thread safety").
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Handle {
    /// Makes a player with `options` set before it starts. The options are
    /// mpv's command-line options without the dashes.
    pub fn new(options: &[(&str, &str)]) -> Result<Handle, Error> {
        // SAFETY: mpv_create has no preconditions; null means out of memory.
        let raw = unsafe { ffi::mpv_create() };
        if raw.is_null() {
            return Err(Error {
                code: -1,
                message: "libmpv could not make a player".into(),
            });
        }
        let handle = Handle { raw };
        for (name, value) in options {
            handle.set_option(name, value)?;
        }
        // SAFETY: raw is a fresh handle, initialised once.
        check(unsafe { ffi::mpv_initialize(handle.raw) })?;
        Ok(handle)
    }

    pub(crate) fn raw(&self) -> *mut ffi::mpv_handle {
        self.raw
    }

    fn set_option(&self, name: &str, value: &str) -> Result<(), Error> {
        let name = cstring(name);
        let value = cstring(value);
        // SAFETY: both strings live across the call.
        check(unsafe { ffi::mpv_set_option_string(self.raw, name.as_ptr(), value.as_ptr()) })
    }

    /// Runs a command now and waits for it. Only for a worker thread (the
    /// thumbnailer); the interface uses [`Handle::command_async`].
    pub fn command(&self, args: &[&str]) -> Result<(), Error> {
        let owned: Vec<CString> = args.iter().map(|arg| cstring(arg)).collect();
        let mut pointers: Vec<*const c_char> = owned.iter().map(|arg| arg.as_ptr()).collect();
        pointers.push(ptr::null());
        // SAFETY: a null-terminated array of strings that outlive the call.
        check(unsafe { ffi::mpv_command(self.raw, pointers.as_mut_ptr()) })
    }

    /// Queues a command; the reply arrives as [`Event::CommandReply`] with `id`.
    pub fn command_async(&self, id: u64, args: &[&str]) {
        let bytes: Vec<&[u8]> = args.iter().map(|arg| arg.as_bytes()).collect();
        self.command_async_bytes(id, &bytes);
    }

    /// [`Handle::command_async`] with arguments as bytes: a path whose name is
    /// not UTF-8 reaches mpv as it is on disk.
    pub fn command_async_bytes(&self, id: u64, args: &[&[u8]]) {
        let owned: Vec<CString> = args.iter().map(|arg| cbytes(arg)).collect();
        let mut pointers: Vec<*const c_char> = owned.iter().map(|arg| arg.as_ptr()).collect();
        pointers.push(ptr::null());
        // SAFETY: mpv copies the arguments before it returns.
        let code = unsafe { ffi::mpv_command_async(self.raw, id, pointers.as_mut_ptr()) };
        if code < 0 {
            let name = args
                .first()
                .map(|arg| String::from_utf8_lossy(arg).into_owned());
            eprintln!(
                "[media] mpv command {name:?} not queued: {}",
                Error::of(code)
            );
        }
    }

    /// Sets a property from its text form without waiting.
    pub fn set_async(&self, name: &str, value: &str) {
        let name_c = cstring(name);
        let value_c = cstring(value);
        let mut pointer = value_c.as_ptr();
        // SAFETY: mpv copies the value before it returns.
        let code = unsafe {
            ffi::mpv_set_property_async(
                self.raw,
                0,
                name_c.as_ptr(),
                ffi::MPV_FORMAT_STRING,
                (&mut pointer as *mut *const c_char).cast::<c_void>(),
            )
        };
        if code < 0 {
            eprintln!("[media] mpv property {name} not set: {}", Error::of(code));
        }
    }

    /// Sets a property and waits. Only for a worker thread.
    pub fn set(&self, name: &str, value: &str) -> Result<(), Error> {
        let name_c = cstring(name);
        let value_c = cstring(value);
        let mut pointer = value_c.as_ptr();
        // SAFETY: the value outlives the call.
        check(unsafe {
            ffi::mpv_set_property(
                self.raw,
                name_c.as_ptr(),
                ffi::MPV_FORMAT_STRING,
                (&mut pointer as *mut *const c_char).cast::<c_void>(),
            )
        })
    }

    /// Reads a property as a number. Waits for the player's core: only for a
    /// worker thread.
    pub fn get_double(&self, name: &str) -> Option<f64> {
        let name = cstring(name);
        let mut value = 0.0f64;
        // SAFETY: a double out-parameter for MPV_FORMAT_DOUBLE.
        let code = unsafe {
            ffi::mpv_get_property(
                self.raw,
                name.as_ptr(),
                ffi::MPV_FORMAT_DOUBLE,
                (&mut value as *mut f64).cast(),
            )
        };
        (code >= 0).then_some(value)
    }

    /// Reads a property as a tree. Only for a worker thread.
    pub fn get_node(&self, name: &str) -> Option<Node> {
        let name = cstring(name);
        let mut raw = ffi::mpv_node {
            u: ffi::mpv_node_u { int64: 0 },
            format: ffi::MPV_FORMAT_NONE,
        };
        // SAFETY: a node out-parameter, freed with mpv_free_node_contents.
        let code = unsafe {
            ffi::mpv_get_property(
                self.raw,
                name.as_ptr(),
                ffi::MPV_FORMAT_NODE,
                (&mut raw as *mut ffi::mpv_node).cast(),
            )
        };
        if code < 0 {
            return None;
        }
        // SAFETY: mpv filled the node; it is copied and then freed once.
        unsafe {
            let node = Node::from_raw(&raw);
            ffi::mpv_free_node_contents(&mut raw);
            Some(node)
        }
    }

    /// Asks for [`Event::Property`] with `id` whenever `name` changes.
    pub fn observe(&self, id: u64, name: &str, format: Format) {
        let name = cstring(name);
        // SAFETY: mpv copies the name.
        let code = unsafe { ffi::mpv_observe_property(self.raw, id, name.as_ptr(), format.raw()) };
        if code < 0 {
            eprintln!("[media] cannot observe {name:?}: {}", Error::of(code));
        }
    }

    /// Waits for the next event, up to `timeout` seconds (negative: for ever).
    /// Only one thread may wait on a handle.
    pub fn wait_event(&self, timeout: f64) -> Event {
        // SAFETY: the returned event is valid until the next wait on this
        // handle; everything is copied out before returning.
        unsafe {
            let event = &*ffi::mpv_wait_event(self.raw, timeout);
            match event.event_id {
                ffi::MPV_EVENT_NONE => Event::Other,
                ffi::MPV_EVENT_SHUTDOWN => Event::Shutdown,
                ffi::MPV_EVENT_START_FILE => {
                    let data = &*(event.data as *const ffi::mpv_event_start_file);
                    Event::StartFile(data.playlist_entry_id)
                }
                ffi::MPV_EVENT_FILE_LOADED => Event::FileLoaded,
                ffi::MPV_EVENT_END_FILE => {
                    let data = &*(event.data as *const ffi::mpv_event_end_file);
                    let error = (data.reason == ffi::MPV_END_FILE_REASON_ERROR)
                        .then(|| Error::of(data.error).message);
                    Event::EndFile {
                        entry: data.playlist_entry_id,
                        eof: data.reason == ffi::MPV_END_FILE_REASON_EOF,
                        error,
                    }
                }
                ffi::MPV_EVENT_PLAYBACK_RESTART => Event::PlaybackRestart,
                ffi::MPV_EVENT_SEEK => Event::Seek,
                ffi::MPV_EVENT_VIDEO_RECONFIG => Event::VideoReconfig,
                ffi::MPV_EVENT_HOOK => {
                    let data = &*(event.data as *const ffi::mpv_event_hook);
                    Event::Hook {
                        name: CStr::from_ptr(data.name).to_string_lossy().into_owned(),
                        id: data.id,
                    }
                }
                ffi::MPV_EVENT_COMMAND_REPLY => Event::CommandReply {
                    id: event.reply_userdata,
                    error: (event.error < 0).then(|| Error::of(event.error).message),
                },
                ffi::MPV_EVENT_PROPERTY_CHANGE => {
                    let data = &*(event.data as *const ffi::mpv_event_property);
                    let name = CStr::from_ptr(data.name).to_string_lossy().into_owned();
                    let value = if data.data.is_null() {
                        Value::None
                    } else {
                        match data.format {
                            ffi::MPV_FORMAT_FLAG => Value::Flag(*(data.data as *const c_int) != 0),
                            ffi::MPV_FORMAT_DOUBLE => Value::Double(*(data.data as *const f64)),
                            ffi::MPV_FORMAT_INT64 => Value::Int(*(data.data as *const i64)),
                            ffi::MPV_FORMAT_STRING => {
                                let text = *(data.data as *const *const c_char);
                                Value::String(CStr::from_ptr(text).to_string_lossy().into_owned())
                            }
                            ffi::MPV_FORMAT_NODE => {
                                Value::Node(Node::from_raw(&*(data.data as *const ffi::mpv_node)))
                            }
                            _ => Value::None,
                        }
                    };
                    Event::Property {
                        id: event.reply_userdata,
                        name,
                        value,
                    }
                }
                _ => Event::Other,
            }
        }
    }

    /// Asks mpv to stop at the hook `name` (`on_load`: before a file opens,
    /// when per-file options can still be set) until it is continued.
    pub fn add_hook(&self, name: &str) {
        let name = cstring(name);
        // SAFETY: mpv copies the name.
        unsafe { ffi::mpv_hook_add(self.raw, 0, name.as_ptr(), 0) };
    }

    /// Lets mpv go on past a hook.
    pub fn continue_hook(&self, id: u64) {
        // SAFETY: an id from a hook event of this handle.
        unsafe { ffi::mpv_hook_continue(self.raw, id) };
    }

    /// Wakes a thread blocked in [`Handle::wait_event`].
    pub fn wakeup(&self) {
        // SAFETY: valid handle.
        unsafe { ffi::mpv_wakeup(self.raw) }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the last owner; nothing uses the handle after this.
        unsafe { ffi::mpv_terminate_destroy(self.raw) }
    }
}

/// A C string from Rust text; an interior NUL (never in a real path or
/// option) cuts the text there.
pub(crate) fn cstring(text: &str) -> CString {
    cbytes(text.as_bytes())
}

/// [`cstring`] for bytes (a path as it is on disk).
pub(crate) fn cbytes(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap_or_else(|error| {
        let end = error.nul_position();
        CString::new(&bytes[..end]).unwrap_or_default()
    })
}
