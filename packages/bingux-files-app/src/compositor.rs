//! Where the compositor has put the windows.
//!
//! A Wayland client knows nothing about the screen: it cannot read its own
//! window's place, let alone another's, and while a compositor moves a window
//! (`xdg_toplevel.move`) it hears nothing from it. A tab dragged out of one
//! Files window onto another's strip therefore cannot be told where it is from
//! what the client is given. What can tell it is the compositor itself, when
//! it offers a way to ask. [`Locator::detect`] finds out which one there is;
//! [`Locator::frames`] asks it.
//!
//! * **sway** (and anything that speaks its IPC, `SWAYSOCK`): `GET_TREE`
//!   answers with every window's rectangle, its title and the process that
//!   owns it, and it answers while a window is being moved.
//! * **Gnoblin**: the compositor's control socket (`compositor-v1.sock`,
//!   newline-delimited JSON) answers `windows.list` with every window's
//!   `frame`, `app_id`, `title`, workspace and minimized state. Its frames are
//!   in the compositor's logical pixels, the same unit the window's own
//!   pointer events use. The reply carries no process id, so a Files window is
//!   known by its application id and title. The socket has no call for the
//!   pointer: its place follows from the dragged window's frame, as on sway.
//! * **GNOME Shell (mutter), KDE Plasma, and the rest**: nothing is offered to
//!   a client, so [`Locator::detect`] finds none, a tab can be torn off but not
//!   dropped on another window's strip by moving the window over it (see
//!   `design/tab-tearoff.md`).
//!
//! [`parse_sway`] and [`parse_gnoblin`] are pure; [`Locator::frames`] does the
//! socket I/O and blocks, so it is called on a worker thread, never on the UI's.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A frame bigger than its window's content by less than this is rounding, not
/// a frame drawn around it.
const FRAME_AT_LEAST: f32 = 1.0;

#[cfg(test)]
pub(crate) mod fake;

/// The application id Files windows carry, as sway reports it and as Gnoblin
/// reports `app_id`.
const APP_ID: &str = "bingux-files";

/// How long a compositor may take to answer before it is taken to be gone.
const ANSWER_WITHIN: Duration = Duration::from_millis(500);

/// One window, as the compositor lays it out. Positions are logical pixels in
/// the compositor's layout, the same unit the window's own pointer events use:
/// a window on a monitor with a scale of 1.5 is placed and sized in the
/// logical pixels the monitor shows, not the ones it draws.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Frame {
    /// The window's title.
    pub name: String,
    pub app_id: String,
    /// The process that owns the window, when the compositor says (sway does,
    /// Gnoblin does not).
    pub pid: Option<u32>,
    /// Where the window's own content begins (not its border or title bar).
    pub x: f32,
    pub y: f32,
    /// The size of that content.
    pub width: f32,
    pub height: f32,
    /// The workspace the window is on, when the compositor says and the window
    /// is on one only (a window on every workspace has none).
    pub workspace: Option<String>,
    /// Whether the window is minimized: it is nowhere on the screen.
    pub hidden: bool,
}

impl Frame {
    /// Whether this window is one of this application's: the one process
    /// (`pid`) when the compositor names processes, else the application id.
    pub fn is_ours(&self, pid: u32) -> bool {
        match self.pid {
            Some(owner) => owner == pid,
            None => same_app(&self.app_id, APP_ID),
        }
    }

    /// Whether a window dropped on this one could be seen to be: both are on
    /// the screen, and on the same workspace when the compositor tells.
    pub fn shows_with(&self, other: &Frame) -> bool {
        !self.hidden && !other.hidden && match (&self.workspace, &other.workspace) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
    }

    /// The window's own content, when the window knows how big that is
    /// (`size`, logical pixels). Gnoblin's `frame` is the window with the frame
    /// it draws around it (a titlebar, a border: `frame.extents`), which the
    /// window list does not break down, and the pointer's place is known to the
    /// window only in its own content. A frame bigger than the content by more
    /// than rounding is taken to have the same border on the left, right and
    /// bottom, and the rest of the height above: the layout of every frame
    /// Gnoblin draws (`{top, 1, 1, 1}`, `{top, 0, 0, 0}`). A frame the size of
    /// the content (a window with no frame, and all of sway's) is returned as it
    /// is, as is one that is not bigger in both ways (a window being resized).
    pub fn content(&self, size: (f32, f32)) -> Frame {
        let (extra_width, extra_height) = (self.width - size.0, self.height - size.1);
        let grown = |extra: f32| extra >= FRAME_AT_LEAST;
        if size.0 <= 0.0 || size.1 <= 0.0 || extra_width < 0.0 || extra_height < 0.0 || !(grown(extra_width) || grown(extra_height)) {
            return self.clone();
        }
        let side = extra_width / 2.0;
        let top = (extra_height - side).max(0.0);
        Frame { x: self.x + side, y: self.y + top, width: size.0, height: size.1, ..self.clone() }
    }
}

/// Application ids match with or without the `.desktop` some compositors keep.
fn same_app(reported: &str, wanted: &str) -> bool {
    reported.strip_suffix(".desktop").unwrap_or(reported) == wanted
}

/// The windows in sway's `GET_TREE` reply `tree`, in tree order. A node that is
/// not a window is only walked through.
pub(crate) fn parse_sway(tree: &str) -> Vec<Frame> {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(tree) else {
        return Vec::new();
    };
    let mut frames = Vec::new();
    walk(&root, &mut frames);
    frames
}

fn walk(node: &serde_json::Value, frames: &mut Vec<Frame>) {
    if let Some(frame) = sway_frame_of(node) {
        frames.push(frame);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(serde_json::Value::as_array) {
            for child in children {
                walk(child, frames);
            }
        }
    }
}

fn number(value: &serde_json::Value, key: &str) -> f64 {
    value.get(key).and_then(serde_json::Value::as_f64).unwrap_or(0.0)
}

fn sway_frame_of(node: &serde_json::Value) -> Option<Frame> {
    let app_id = node.get("app_id").and_then(serde_json::Value::as_str)?;
    let rect = node.get("rect")?;
    let content = node.get("window_rect")?;
    Some(Frame {
        name: node.get("name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
        app_id: app_id.to_string(),
        pid: Some(u32::try_from(node.get("pid").and_then(serde_json::Value::as_u64).unwrap_or(0)).unwrap_or(0)),
        // `rect` is the container, including its border; `window_rect` is the
        // content's place inside it.
        x: (number(rect, "x") + number(content, "x")) as f32,
        y: (number(rect, "y") + number(content, "y")) as f32,
        width: number(content, "width") as f32,
        height: number(content, "height") as f32,
        workspace: None,
        hidden: false,
    })
}

/// The windows in the `result` of Gnoblin's `windows.list` reply: an array of
/// records with a `frame` (`x`, `y`, `width`, `height`, signed integers in
/// logical pixels of the whole layout, so a monitor left of or above the
/// primary has negative ones), `app_id`, `title`, `workspace_id`, `sticky` and
/// `minimized`. A record without a frame is not placed, and is left out.
pub(crate) fn parse_gnoblin(result: &serde_json::Value) -> Vec<Frame> {
    let Some(records) = result.as_array() else {
        return Vec::new();
    };
    records
        .iter()
        .filter_map(|record| {
            let frame = record.get("frame").filter(|frame| frame.is_object())?;
            let text = |key: &str| record.get(key).and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
            let flag = |key: &str| record.get(key).and_then(serde_json::Value::as_bool).unwrap_or(false);
            let on_every_workspace = flag("sticky");
            let workspace = text("workspace_id");
            Some(Frame {
                name: text("title"),
                app_id: text("app_id"),
                pid: None,
                x: number(frame, "x") as f32,
                y: number(frame, "y") as f32,
                width: number(frame, "width") as f32,
                height: number(frame, "height") as f32,
                workspace: (!on_every_workspace && !workspace.is_empty()).then_some(workspace),
                hidden: flag("minimized"),
            })
        })
        .collect()
}

/// The type of sway's `GET_TREE` message.
const GET_TREE: u32 = 4;
const MAGIC: &[u8; 6] = b"i3-ipc";

/// The first Gnoblin API minor with `windows.list` (API 1.37: the Lua snapshot
/// reads), and the one this client speaks.
const GNOBLIN_API_MINOR: u64 = 37;

/// A reply line or greeting longer than this is not one: a list of windows is
/// kilobytes.
const MAX_LINE: u64 = 4 * 1024 * 1024;

/// The place in a window the compositor holds under the pointer while it
/// moves the window for the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Anchor {
    /// Where the pointer was when the move was asked for.
    Pointer,
    /// Where the button went down.
    Press,
}

/// How to ask the running compositor where its windows are.
pub(crate) enum Locator {
    /// sway's IPC socket.
    Sway(OsString),
    /// Gnoblin's control socket, and the connection kept to it.
    Gnoblin(Gnoblin),
}

impl Locator {
    /// The way the compositor this process runs under can be asked, from its
    /// environment. `None` when there is none (GNOME Shell, KDE, a bare
    /// compositor), or the environment names none.
    pub(crate) fn detect() -> Option<Locator> {
        Self::detect_in(|name| std::env::var_os(name), Path::exists)
    }

    /// [`Locator::detect`] over a given environment and file system.
    ///
    /// A session that says it is Gnoblin (`XDG_CURRENT_DESKTOP` names it, or
    /// `GNOBLIN_COMPOSITOR_SOCKET` is set) is asked on Gnoblin's socket even
    /// when `SWAYSOCK` is also set, which a nested session started from a sway
    /// terminal inherits. Otherwise `SWAYSOCK` (or `I3SOCK`) means sway, and a
    /// Gnoblin socket that is there means Gnoblin.
    fn detect_in(var: impl Fn(&str) -> Option<OsString>, exists: impl Fn(&Path) -> bool) -> Option<Locator> {
        let set = |name: &str| var(name).filter(|value| !value.is_empty());
        let socket = gnoblin_socket(&var);
        let says_gnoblin = set("GNOBLIN_COMPOSITOR_SOCKET").is_some()
            || set("XDG_CURRENT_DESKTOP")
                .is_some_and(|desktop| desktop.to_string_lossy().split(':').any(|name| name.eq_ignore_ascii_case("gnoblin")));
        if says_gnoblin {
            return Some(Locator::Gnoblin(Gnoblin::new(socket)));
        }
        if let Some(path) = set("SWAYSOCK").or_else(|| set("I3SOCK")) {
            return Some(Locator::Sway(path));
        }
        exists(&socket).then(|| Locator::Gnoblin(Gnoblin::new(socket)))
    }

    /// Where this compositor holds the pointer in a window it moves for the
    /// client (`xdg_toplevel.move`). Mutter's `xdg_toplevel_move` starts the
    /// move at `pointer->grab_x`/`grab_y` (`meta_wayland_pointer_get_grab_info`),
    /// the place the button went down, which Gnoblin (a mutter) inherits, and
    /// the window then follows the pointer with that place fixed: it jumps,
    /// when the button went down elsewhere than the pointer is now, to put that
    /// place under the pointer (seen on Gnoblin 0.1.10, `design/tab-tearoff.md`).
    /// sway's move starts at the pointer's place when it is asked for (seen on
    /// sway 1.11).
    pub(crate) fn anchor(&self) -> Anchor {
        match self {
            Locator::Sway(_) => Anchor::Pointer,
            Locator::Gnoblin(_) => Anchor::Press,
        }
    }

    /// Ask the compositor for its windows. `None` when it does not answer, or
    /// will not (Gnoblin's control socket answers an error while its Lua
    /// supervisor is not connected). Blocks; call it off the UI thread.
    pub(crate) fn frames(&mut self) -> Option<Vec<Frame>> {
        match self {
            Locator::Sway(path) => sway_frames(path),
            Locator::Gnoblin(gnoblin) => gnoblin.frames(),
        }
    }
}

/// Gnoblin's control socket: `GNOBLIN_COMPOSITOR_SOCKET`, else
/// `$XDG_RUNTIME_DIR/gnoblin/compositor-v1.sock`, else the same under
/// `/run/user/<uid>` (the path `gnoblinctl` falls back to).
fn gnoblin_socket(var: &impl Fn(&str) -> Option<OsString>) -> PathBuf {
    if let Some(path) = var("GNOBLIN_COMPOSITOR_SOCKET").filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    let runtime = var("XDG_RUNTIME_DIR")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from(format!("/run/user/{}", own_uid())), PathBuf::from);
    runtime.join("gnoblin").join("compositor-v1.sock")
}

/// The user id of this process, from the owner of `/proc/self`.
fn own_uid() -> u32 {
    std::os::unix::fs::MetadataExt::uid(&std::fs::metadata("/proc/self").expect("/proc/self exists on Linux"))
}

fn sway_frames(path: &OsString) -> Option<Vec<Frame>> {
    let mut socket = UnixStream::connect(path).ok()?;
    socket.set_read_timeout(Some(ANSWER_WITHIN)).ok()?;
    socket.set_write_timeout(Some(ANSWER_WITHIN)).ok()?;
    let mut request = Vec::with_capacity(14);
    request.extend_from_slice(MAGIC);
    request.extend_from_slice(&0_u32.to_ne_bytes());
    request.extend_from_slice(&GET_TREE.to_ne_bytes());
    socket.write_all(&request).ok()?;
    let mut header = [0_u8; 14];
    socket.read_exact(&mut header).ok()?;
    if &header[..6] != MAGIC {
        return None;
    }
    let length = u32::from_ne_bytes(header[6..10].try_into().ok()?) as usize;
    // A tree of windows is kilobytes; a reply that claims far more is not one.
    if length > 16 * 1024 * 1024 {
        return None;
    }
    let mut body = vec![0_u8; length];
    socket.read_exact(&mut body).ok()?;
    Some(parse_sway(&String::from_utf8_lossy(&body)))
}

/// A connection to Gnoblin's control socket, made when first needed and kept:
/// one greeting, then a request and its reply for each question. Any failure
/// on it drops it, and the next question connects again, so a compositor that
/// restarts is found again.
pub(crate) struct Gnoblin {
    path: PathBuf,
    link: Option<Link>,
    asked: u64,
}

struct Link {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
}

/// Why a question was not answered with windows.
enum Failure {
    /// The link is no good (closed, timed out, not speaking the protocol): it
    /// is dropped.
    Broken,
    /// The compositor answered, with an error: the link is still good.
    Refused,
}

impl Gnoblin {
    pub(crate) fn new(path: PathBuf) -> Gnoblin {
        Gnoblin { path, link: None, asked: 0 }
    }

    fn frames(&mut self) -> Option<Vec<Frame>> {
        let mut link = match self.link.take() {
            Some(link) => link,
            None => Link::open(&self.path)?,
        };
        self.asked += 1;
        match link.ask(&format!("files-{}", self.asked)) {
            Ok(frames) => {
                self.link = Some(link);
                Some(frames)
            }
            Err(Failure::Refused) => {
                self.link = Some(link);
                None
            }
            Err(Failure::Broken) => None,
        }
    }
}

impl Link {
    /// Connect, and take the greeting: the server must speak API 1 at a minor
    /// that has `windows.list`, and say that it has it.
    fn open(path: &Path) -> Option<Link> {
        let writer = UnixStream::connect(path).ok()?;
        writer.set_read_timeout(Some(ANSWER_WITHIN)).ok()?;
        writer.set_write_timeout(Some(ANSWER_WITHIN)).ok()?;
        let mut reader = BufReader::new(writer.try_clone().ok()?);
        let hello = read_json_line(&mut reader)?;
        let supported = hello.get("event").and_then(serde_json::Value::as_str) == Some("hello")
            && hello.get("api_major").and_then(serde_json::Value::as_u64) == Some(1)
            && hello.get("api_minor").and_then(serde_json::Value::as_u64).is_some_and(|minor| minor >= GNOBLIN_API_MINOR)
            && hello
                .get("methods")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|methods| methods.iter().any(|method| method.as_str() == Some("windows.list")));
        supported.then_some(Link { writer, reader })
    }

    /// One `windows.list` request, and lines read until its reply. Anything
    /// else on the connection (an event, another request's reply) is passed.
    fn ask(&mut self, id: &str) -> Result<Vec<Frame>, Failure> {
        let request = serde_json::json!({
            "op": "api",
            "id": id,
            "api_version": { "major": 1, "minor": GNOBLIN_API_MINOR },
            "method": "windows.list",
            "arguments": {},
        });
        let mut line = request.to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).map_err(|_| Failure::Broken)?;
        // Nothing is subscribed, so lines that are not the reply are rare; a
        // server that sends only them is not answering.
        for _ in 0..64 {
            let record = read_json_line(&mut self.reader).ok_or(Failure::Broken)?;
            if record.get("id").and_then(serde_json::Value::as_str) != Some(id) {
                continue;
            }
            return match record.get("event").and_then(serde_json::Value::as_str) {
                Some("reply") => record.get("result").map(parse_gnoblin).ok_or(Failure::Broken),
                Some("error") => Err(Failure::Refused),
                _ => Err(Failure::Broken),
            };
        }
        Err(Failure::Broken)
    }
}

/// One line of the control socket's newline-delimited JSON. `None` at the end
/// of the stream, on a timeout, on a line too long to be one, and on a line
/// that is not JSON.
fn read_json_line(reader: &mut BufReader<UnixStream>) -> Option<serde_json::Value> {
    let mut line = Vec::new();
    reader.by_ref().take(MAX_LINE).read_until(b'\n', &mut line).ok()?;
    if line.last() != Some(&b'\n') {
        return None;
    }
    serde_json::from_slice(&line).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::Ordering;

    use super::fake::{Fake, windows_list};
    use super::*;

    /// The shape of sway's reply: an output holding a workspace with a tiled
    /// window, and floating windows beside it.
    const TREE: &str = r#"{
      "type": "root", "name": "root", "rect": {"x":0,"y":0,"width":1400,"height":900},
      "nodes": [{
        "type": "output", "name": "HEADLESS-1", "rect": {"x":0,"y":0,"width":1400,"height":900},
        "nodes": [{
          "type": "workspace", "name": "1", "rect": {"x":0,"y":0,"width":1400,"height":900},
          "nodes": [{
            "type": "con", "name": "Home – Files", "app_id": "bingux-files", "pid": 4242,
            "rect": {"x":10,"y":20,"width":600,"height":400},
            "window_rect": {"x":0,"y":0,"width":600,"height":400},
            "nodes": [], "floating_nodes": []
          }],
          "floating_nodes": [{
            "type": "floating_con", "name": "Documents – Files (2)", "app_id": "bingux-files", "pid": 4242,
            "rect": {"x":700,"y":100,"width":602,"height":422},
            "window_rect": {"x":1,"y":21,"width":600,"height":400},
            "nodes": [], "floating_nodes": []
          }]
        }]
      }]
    }"#;

    #[test]
    fn every_window_in_the_tree_is_found_with_its_content_rectangle() {
        let frames = parse_sway(TREE);
        assert_eq!(frames.len(), 2, "only nodes with an app id are windows: {frames:?}");
        assert_eq!(
            frames[0],
            Frame {
                name: "Home – Files".into(),
                app_id: "bingux-files".into(),
                pid: Some(4242),
                x: 10.0,
                y: 20.0,
                width: 600.0,
                height: 400.0,
                workspace: None,
                hidden: false,
            }
        );
        // The content of a window with a border begins inside the container.
        assert_eq!((frames[1].x, frames[1].y), (701.0, 121.0));
        assert_eq!((frames[1].width, frames[1].height), (600.0, 400.0));
        assert_eq!(frames[1].name, "Documents – Files (2)");
    }

    #[test]
    fn a_reply_that_is_not_a_tree_has_no_windows() {
        assert!(parse_sway("").is_empty());
        assert!(parse_sway("not json").is_empty());
        assert!(parse_sway("{}").is_empty());
        assert!(parse_sway(r#"{"nodes": [{"app_id": "x"}]}"#).is_empty(), "a window with no rectangle is not placed");
    }

    #[test]
    fn gnoblins_window_list_is_read_into_frames_with_signed_logical_coordinates() {
        let frames = parse_gnoblin(&windows_list());
        assert_eq!(frames.len(), 4, "a record without a frame is not placed: {frames:?}");
        assert_eq!(
            frames[0],
            Frame {
                name: "Home – Files".into(),
                app_id: "bingux-files".into(),
                pid: None,
                x: 100.0,
                y: 60.0,
                width: 600.0,
                height: 400.0,
                workspace: Some("workspace-1".into()),
                hidden: false,
            }
        );
        assert_eq!((frames[1].x, frames[1].y), (-1280.0, -120.0), "a monitor left of the first is negative");
        assert!(frames[2].hidden, "minimized");
        assert_eq!(frames[3].workspace, None, "a sticky window is on every workspace");
    }

    #[test]
    fn what_gnoblin_does_not_say_is_not_invented() {
        assert!(parse_gnoblin(&serde_json::json!({})).is_empty(), "not an array");
        assert!(parse_gnoblin(&serde_json::json!([1, "x", null, {"frame": 3}])).is_empty());
        let bare = parse_gnoblin(&serde_json::json!([{"frame": {}}]));
        assert_eq!(bare.len(), 1);
        assert_eq!((bare[0].name.as_str(), bare[0].app_id.as_str(), bare[0].workspace.clone()), ("", "", None));
    }

    #[test]
    fn each_compositor_holds_the_pointer_where_it_begins_the_move() {
        assert_eq!(Locator::Sway("/s/sway.sock".into()).anchor(), Anchor::Pointer);
        assert_eq!(Locator::Gnoblin(Gnoblin::new("/g.sock".into())).anchor(), Anchor::Press);
    }

    #[test]
    fn a_window_is_ours_by_process_where_the_compositor_names_it_and_by_application_id_where_not() {
        let frames = parse_gnoblin(&windows_list());
        assert!(frames[0].is_ours(1), "Gnoblin names no process: the application id decides");
        assert!(!frames[2].is_ours(1), "another application's window");
        let with_suffix = Frame { app_id: "bingux-files.desktop".into(), ..frames[0].clone() };
        assert!(with_suffix.is_ours(1), "the .desktop some compositors keep is not part of the id");
        let sway = &parse_sway(TREE)[0];
        assert!(sway.is_ours(4242) && !sway.is_ours(7));
    }

    #[test]
    fn windows_are_shown_together_when_both_are_on_the_screen_and_the_same_workspace() {
        let frames = parse_gnoblin(&windows_list());
        assert!(frames[0].shows_with(&frames[1]));
        assert!(!frames[0].shows_with(&frames[2]), "a minimized window is nowhere");
        assert!(frames[0].shows_with(&frames[3]) && frames[3].shows_with(&frames[0]), "sticky is on every workspace");
        let other = Frame { workspace: Some("workspace-2".into()), ..frames[1].clone() };
        assert!(!frames[0].shows_with(&other), "another workspace");
    }

    fn framed(x: f32, y: f32, width: f32, height: f32) -> Frame {
        Frame { x, y, width, height, ..parse_gnoblin(&windows_list())[0].clone() }
    }

    #[test]
    fn a_window_with_no_frame_drawn_around_it_is_its_own_content() {
        let frame = framed(100.0, 60.0, 600.0, 400.0);
        assert_eq!(frame.content((600.0, 400.0)), frame, "the size of the content: sway's, or Gnoblin's with no frame");
        assert_eq!(frame.content((599.6, 399.7)), frame, "rounding to whole pixels is not a frame");
        assert_eq!(frame.content((0.0, 0.0)), frame, "a window that does not know its size");
        assert_eq!(frame.content((640.0, 420.0)), frame, "a frame smaller than the content is a window being resized");
    }

    #[test]
    fn gnoblins_titlebar_and_border_are_taken_off_the_frame_to_find_the_content() {
        // The built-in native frame, extents {32, 1, 1, 1}: a window of 600 x
        // 400 is 602 x 433 with it, and its content begins 1 across, 32 down.
        let content = framed(100.0, 60.0, 602.0, 433.0).content((600.0, 400.0));
        assert_eq!((content.x, content.y, content.width, content.height), (101.0, 92.0, 600.0, 400.0));
        // Bingux's frame, {36, 0, 0, 0}.
        let content = framed(-1280.0, -120.0, 600.0, 436.0).content((600.0, 400.0));
        assert_eq!((content.x, content.y, content.width, content.height), (-1280.0, -84.0, 600.0, 400.0));
        // Everything else about the window is kept.
        assert_eq!(content.name, "Home – Files");
        assert_eq!(content.workspace.as_deref(), Some("workspace-1"));
    }

    // ---- Finding the compositor ----

    fn env(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<&str, &str> = pairs.iter().copied().collect();
        move |name| map.get(name).map(OsString::from)
    }

    fn found(pairs: &[(&'static str, &'static str)], socket_exists: bool) -> Option<String> {
        Locator::detect_in(env(pairs), |_| socket_exists).map(|locator| match locator {
            Locator::Sway(path) => format!("sway {}", path.to_string_lossy()),
            Locator::Gnoblin(gnoblin) => format!("gnoblin {}", gnoblin.path.display()),
        })
    }

    #[test]
    fn the_compositor_is_found_from_the_environment() {
        let run = ("XDG_RUNTIME_DIR", "/run/user/1000");
        assert_eq!(found(&[("SWAYSOCK", "/s/sway.sock")], false).as_deref(), Some("sway /s/sway.sock"));
        assert_eq!(found(&[("I3SOCK", "/s/i3.sock")], false).as_deref(), Some("sway /s/i3.sock"));
        assert_eq!(
            found(&[run, ("XDG_CURRENT_DESKTOP", "Gnoblin")], false).as_deref(),
            Some("gnoblin /run/user/1000/gnoblin/compositor-v1.sock"),
            "a Gnoblin session is asked even before its socket shows"
        );
        assert_eq!(
            found(&[run, ("XDG_CURRENT_DESKTOP", "GNOME:Gnoblin")], false).as_deref(),
            Some("gnoblin /run/user/1000/gnoblin/compositor-v1.sock")
        );
        assert_eq!(
            found(&[("GNOBLIN_COMPOSITOR_SOCKET", "/tmp/private.sock")], false).as_deref(),
            Some("gnoblin /tmp/private.sock"),
            "the private socket is used on both ends"
        );
        assert_eq!(
            found(&[run], true).as_deref(),
            Some("gnoblin /run/user/1000/gnoblin/compositor-v1.sock"),
            "no desktop name, but the socket is there"
        );
    }

    #[test]
    fn a_nested_gnoblin_started_from_a_sway_terminal_is_asked_not_sway() {
        let nested = [("SWAYSOCK", "/s/sway.sock"), ("XDG_CURRENT_DESKTOP", "Gnoblin"), ("XDG_RUNTIME_DIR", "/r")];
        assert_eq!(found(&nested, true).as_deref(), Some("gnoblin /r/gnoblin/compositor-v1.sock"));
        let private = [("SWAYSOCK", "/s/sway.sock"), ("GNOBLIN_COMPOSITOR_SOCKET", "/t/g.sock")];
        assert_eq!(found(&private, false).as_deref(), Some("gnoblin /t/g.sock"));
        // Under sway, a Gnoblin socket left by another session is not asked.
        assert_eq!(found(&[("SWAYSOCK", "/s/sway.sock"), ("XDG_RUNTIME_DIR", "/r")], true).as_deref(), Some("sway /s/sway.sock"));
    }

    #[test]
    fn a_compositor_that_offers_nothing_has_no_locator() {
        assert!(found(&[], false).is_none());
        assert!(found(&[("XDG_CURRENT_DESKTOP", "GNOME"), ("XDG_RUNTIME_DIR", "/r")], false).is_none());
        assert!(found(&[("XDG_CURRENT_DESKTOP", "KDE"), ("XDG_RUNTIME_DIR", "/r")], false).is_none());
        assert!(found(&[("SWAYSOCK", ""), ("GNOBLIN_COMPOSITOR_SOCKET", "")], false).is_none(), "empty is unset");
    }

    #[test]
    fn without_a_runtime_dir_the_socket_is_where_gnoblinctl_looks() {
        let uid = own_uid();
        assert_eq!(
            found(&[("XDG_CURRENT_DESKTOP", "Gnoblin")], false),
            Some(format!("gnoblin /run/user/{uid}/gnoblin/compositor-v1.sock"))
        );
    }

    #[test]
    fn gnoblin_is_asked_for_windows_list_in_its_wire_format_and_the_answer_is_parsed() {
        let fake = Fake::start(windows_list());
        let mut locator = fake.locator();
        let frames = locator.frames().expect("the server answers");
        assert_eq!(frames, parse_gnoblin(&windows_list()));
        let requests = fake.requests.lock().expect("requests");
        assert_eq!(
            requests[0],
            serde_json::json!({
                "op": "api", "id": "files-1", "api_version": {"major": 1, "minor": 37},
                "method": "windows.list", "arguments": {},
            }),
            "what gnoblin-control-requests.c accepts for a snapshot collection read (API 1.37)"
        );
    }

    #[test]
    fn one_connection_serves_every_poll() {
        let fake = Fake::start(windows_list());
        let mut locator = fake.locator();
        for _ in 0..5 {
            assert_eq!(locator.frames().map(|frames| frames.len()), Some(4));
        }
        assert_eq!(fake.accepted.load(Ordering::Relaxed), 1, "one greeting, five questions");
        let ids: Vec<String> =
            fake.requests.lock().expect("requests").iter().map(|request| request["id"].as_str().unwrap_or_default().to_string()).collect();
        assert_eq!(ids, ["files-1", "files-2", "files-3", "files-4", "files-5"], "each question has its own id");
    }

    #[test]
    fn lines_that_are_not_the_reply_are_passed() {
        let fake = Fake::start(windows_list());
        *fake.chatty.lock().expect("chatty") = true;
        let mut locator = fake.locator();
        assert_eq!(locator.frames().map(|frames| frames.len()), Some(4));
        assert_eq!(locator.frames().map(|frames| frames.len()), Some(4));
    }

    #[test]
    fn an_error_reply_is_no_answer_but_the_connection_stays_and_answers_again() {
        // Gnoblin answers "Lua supervisor is not connected" until its Lua
        // worker is up, and "Lua worker is restarting" while it restarts.
        let fake = Fake::start(windows_list());
        *fake.refuse.lock().expect("refuse") = Some("Lua supervisor is not connected");
        let mut locator = fake.locator();
        assert!(locator.frames().is_none());
        assert!(locator.frames().is_none());
        *fake.refuse.lock().expect("refuse") = None;
        assert_eq!(locator.frames().map(|frames| frames.len()), Some(4));
        assert_eq!(fake.accepted.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn a_connection_the_server_closes_is_made_again() {
        let fake = Fake::start(windows_list());
        fake.close_after.store(1, Ordering::Relaxed);
        let mut locator = fake.locator();
        assert!(locator.frames().is_some(), "the first reply arrives before the close");
        // The next question finds the link closed: no answer, and a new link.
        assert!(locator.frames().is_none());
        assert!(locator.frames().is_some());
        assert_eq!(fake.accepted.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_socket_nobody_listens_on_has_no_answer() {
        let mut locator = Locator::Gnoblin(Gnoblin::new(PathBuf::from("/nonexistent/gnoblin/compositor-v1.sock")));
        assert!(locator.frames().is_none());
        assert!(locator.frames().is_none());
    }

    #[test]
    fn a_server_too_old_or_without_windows_list_is_not_asked() {
        let old = Fake::start(windows_list());
        old.hello.lock().expect("hello").0 = 36;
        assert!(old.locator().frames().is_none(), "windows.list is API 1.37");
        assert!(old.requests.lock().expect("requests").is_empty(), "no request was sent");

        let without = Fake::start(windows_list());
        without.hello.lock().expect("hello").1 = vec!["ping"];
        assert!(without.locator().frames().is_none());
        assert!(without.requests.lock().expect("requests").is_empty());
    }

    #[test]
    fn a_server_that_does_not_speak_the_protocol_is_no_answer() {
        let path = std::env::temp_dir().join(format!("bingux-files-fake-junk-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let _ = stream.write_all(b"this is not json\n");
            }
        });
        let mut locator = Locator::Gnoblin(Gnoblin::new(path.clone()));
        assert!(locator.frames().is_none());
        let _ = std::fs::remove_file(&path);
    }
}
