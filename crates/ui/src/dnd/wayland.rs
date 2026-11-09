//! The Wayland side of drag and drop and the clipboard.
//!
//! winit 0.30 has no `wl_data_device` on Wayland: no drops from other
//! programs, no drags out, and Freya's clipboard is plain text through
//! `smithay-clipboard`. This module adds the protocol beside winit, on winit's
//! own connection, the way `smithay-clipboard` does: a thread of its own with
//! its own event queue on the same `wl_display`, its own `wl_seat` objects
//! (a pointer and a keyboard, only to learn the serials a drag and a
//! selection must quote) and a `wl_data_device` and a primary selection
//! device for each seat. Nothing in winit or Freya is patched.
//!
//! The thread never waits on the UI thread and the UI thread never waits on
//! it for long:
//!
//! - Drop events go to the window whose `wl_surface` they name, over an
//!   unbounded channel that wakes that window's task ([`WindowEvent`]).
//! - The UI answers (accept a type, choose actions, receive, finish) with
//!   [`Command`]s.
//! - A drag's data is produced on the UI thread when a target asks for it
//!   ([`SourceEvent::Send`]) and written to the pipe on a thread of its own,
//!   so a drop on a window of the same program cannot deadlock.
//! - Received data is read to the end on a thread of its own.
//!
//! Everything here is plumbing; `super` decides what a drag means.

use std::collections::HashMap;
use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread;
use std::time::Duration;

use futures_channel::mpsc::UnboundedSender;
use futures_channel::oneshot;
use sctk::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use sctk::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use sctk::data_device_manager::data_source::{CopyPasteSource, DataSourceHandler, DragSource};
use sctk::data_device_manager::{DataDeviceManagerState, ReadPipe, WritePipe};
use sctk::primary_selection::PrimarySelectionManagerState;
use sctk::primary_selection::device::{PrimarySelectionDevice, PrimarySelectionDeviceHandler};
use sctk::primary_selection::selection::{PrimarySelectionSource, PrimarySelectionSourceHandler};
use sctk::reexports::calloop::EventLoop;
use sctk::reexports::calloop::channel::{self, Channel, Sender};
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::reexports::client::backend::{Backend, ObjectId};
use sctk::reexports::client::globals::{GlobalList, registry_queue_init};
use sctk::reexports::client::protocol::wl_buffer::WlBuffer;
use sctk::reexports::client::protocol::wl_compositor::WlCompositor;
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::protocol::wl_data_source::WlDataSource;
use sctk::reexports::client::protocol::wl_keyboard::{self, WlKeyboard};
use sctk::reexports::client::protocol::wl_pointer::WlPointer;
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_shm;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::primary_selection::zv1::client::{
    zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
};
use sctk::registry::{ProvidesRegistryState, RegistryState};
use sctk::seat::pointer::{PointerData, PointerEvent, PointerEventKind, PointerHandler};
use sctk::seat::{Capability, SeatHandler, SeatState};
use sctk::shm::raw::RawPool;
use sctk::shm::{Shm, ShmHandler};
use sctk::{
    delegate_data_device, delegate_pointer, delegate_primary_selection, delegate_registry,
    delegate_seat, delegate_shm, registry_handlers,
};

/// The most a drop or a paste reads: a drop of a 4K screenshot is about
/// 30 MB of PNG, so this leaves room without letting a broken source fill
/// memory.
pub(crate) const READ_LIMIT: usize = 256 << 20;

/// What a window hears about a drag over it from any program, this one
/// included. Positions are surface-local logical pixels, the same space as
/// Freya's layout.
#[derive(Debug)]
pub(crate) enum WindowEvent {
    Enter {
        serial: u32,
        x: f64,
        y: f64,
        mime_types: Vec<String>,
        source_actions: DndAction,
    },
    Motion {
        x: f64,
        y: f64,
    },
    /// The compositor chose an action from the source's and ours.
    Action(DndAction),
    Leave,
    Drop {
        serial: u32,
        x: f64,
        y: f64,
        action: DndAction,
    },
}

/// What the source of a drag this program started hears.
pub(crate) enum SourceEvent {
    /// A target wants the data as `mime`: write it to `pipe` and close it.
    Send { mime: String, pipe: WritePipe },
    /// The action the drop will have.
    Action(DndAction),
    /// The person let go over a target that accepts; data transfer follows.
    Dropped,
    /// The target has the data and has finished with it.
    Finished,
    /// The drag ended with no drop, or the target refused it.
    Cancelled,
}

/// The drag image: premultiplied ARGB8888 (Wayland's byte order, BGRA in
/// memory), `width * 4` bytes a row, at `scale` buffer pixels per logical
/// pixel. `hotspot` is where the pointer holds it, in logical pixels.
pub(crate) struct Icon {
    pub width: u32,
    pub height: u32,
    pub scale: i32,
    pub pixels: Vec<u8>,
    pub hotspot: (i32, i32),
}

pub(crate) enum Command {
    /// Route drag events over `surface` (a `wl_surface*` of this connection)
    /// to `events`.
    Register {
        surface: usize,
        events: UnboundedSender<WindowEvent>,
    },
    Unregister {
        surface: usize,
    },
    /// Answer the drag that entered with `serial`: the type we would take
    /// (`None` refuses the drop) and the actions we allow.
    Respond {
        serial: u32,
        mime: Option<String>,
        actions: DndAction,
        preferred: DndAction,
    },
    /// Read the dropped data as `mime`.
    Receive {
        serial: u32,
        mime: String,
        reply: oneshot::Sender<io::Result<Vec<u8>>>,
    },
    /// The drop with `serial` is handled: tell the source (`finish`) or, when
    /// `ok` is false, refuse it.
    Finish {
        serial: u32,
        ok: bool,
        action: Option<DndAction>,
    },
    /// Start a drag from `surface` with the button press it is held by.
    StartDrag {
        surface: usize,
        mime_types: Vec<String>,
        actions: DndAction,
        icon: Option<Icon>,
        events: UnboundedSender<SourceEvent>,
    },
    /// Cancel the drag this program started, if any.
    CancelDrag,
    /// Own the clipboard (or the primary selection) with these
    /// representations; an empty list gives it up.
    SetSelection {
        primary: bool,
        data: Vec<(String, Arc<[u8]>)>,
    },
    /// Open the current selection as `mime` and hand back the read end.
    ReceiveSelection {
        primary: bool,
        mime: String,
        reply: mpsc::Sender<io::Result<ReadPipe>>,
    },
}

/// What the UI thread can read without asking the worker.
#[derive(Default)]
pub(crate) struct Shared {
    pub selection: Mutex<Selection>,
}

#[derive(Default, Clone)]
pub(crate) struct Selection {
    /// The types of the clipboard and of the primary selection, as offered.
    pub clipboard: Vec<String>,
    pub primary: Vec<String>,
    /// What this program put there itself, while it still owns it.
    pub own_clipboard: Option<Vec<(String, Arc<[u8]>)>>,
    pub own_primary: Option<Vec<(String, Arc<[u8]>)>>,
    /// Counts selection changes, so a menu can tell it is out of date.
    pub generation: u64,
}

/// A running worker.
pub(crate) struct Worker {
    commands: Sender<Command>,
    pub shared: Arc<Shared>,
}

impl Worker {
    pub fn send(&self, command: Command) -> bool {
        self.commands.send(command).is_ok()
    }
}

static WORKER: OnceLock<Option<Worker>> = OnceLock::new();

/// The worker, if one was started.
pub(crate) fn worker() -> Option<&'static Worker> {
    WORKER.get().and_then(Option::as_ref)
}

/// Start the worker on `display` once. Later calls return the first one.
///
/// # Safety
///
/// `display` must be the `wl_display*` of winit's connection, alive for the
/// rest of the process (winit keeps it until the event loop ends, which is
/// the end of a Freya app).
pub(crate) unsafe fn start(display: *mut c_void) -> Option<&'static Worker> {
    WORKER
        .get_or_init(|| {
            // SAFETY: the caller's contract.
            let backend = unsafe { Backend::from_foreign_display(display.cast()) };
            let connection = Connection::from_backend(backend);
            let (commands, receiver) = channel::channel();
            let shared = Arc::new(Shared::default());
            let thread_shared = shared.clone();
            let (ready_tx, ready_rx) = mpsc::channel();
            let spawned = thread::Builder::new()
                .name("bingux-dnd".into())
                .spawn(move || run(connection, receiver, thread_shared, ready_tx));
            if let Err(error) = spawned {
                eprintln!("bingux-ui: drag and drop is off: could not start its thread: {error}");
                return None;
            }
            // The worker binds its globals before anything is sent to it; a
            // compositor without `wl_data_device_manager` means no worker.
            match ready_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(true) => Some(Worker { commands, shared }),
                _ => None,
            }
        })
        .as_ref()
}

fn run(
    connection: Connection,
    commands: Channel<Command>,
    shared: Arc<Shared>,
    ready: mpsc::Sender<bool>,
) {
    let Ok((globals, queue)) = registry_queue_init::<State>(&connection) else {
        let _ = ready.send(false);
        return;
    };
    let Ok(mut event_loop) = EventLoop::<State>::try_new() else {
        let _ = ready.send(false);
        return;
    };
    let qh = queue.handle();
    let Some(mut state) = State::new(&globals, &qh, connection.clone(), shared) else {
        let _ = ready.send(false);
        return;
    };
    let handle = event_loop.handle();
    let inserted = handle
        .insert_source(commands, |event, _, state: &mut State| {
            if let channel::Event::Msg(command) = event {
                state.command(command);
            }
        })
        .is_ok()
        && WaylandSource::new(connection.clone(), queue)
            .insert(handle)
            .is_ok();
    let _ = ready.send(inserted);
    if !inserted {
        return;
    }
    loop {
        if event_loop.dispatch(None, &mut state).is_err() {
            break;
        }
        let _ = connection.flush();
    }
}

#[derive(Default)]
struct SeatSlot {
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    device: Option<DataDevice>,
    primary: Option<PrimarySelectionDevice>,
    /// The serial of the last button press: what a drag must quote.
    press_serial: u32,
    /// The serial of the last input of any kind: what a selection must quote.
    serial: u32,
    keyboard_focus: bool,
}

/// The drag this program started.
struct Outgoing {
    source: DragSource,
    icon: Option<(WlSurface, RawPool)>,
    events: UnboundedSender<SourceEvent>,
}

struct State {
    registry: RegistryState,
    seat_state: SeatState,
    connection: Connection,
    qh: QueueHandle<State>,
    devices: DataDeviceManagerState,
    primary_manager: Option<PrimarySelectionManagerState>,
    compositor: Option<WlCompositor>,
    shm: Option<Shm>,
    seats: HashMap<ObjectId, SeatSlot>,
    /// The seat that last had input: the one a drag or a selection uses.
    latest_seat: Option<ObjectId>,
    /// Windows by their surface's protocol id.
    windows: HashMap<u32, UnboundedSender<WindowEvent>>,
    /// The drag over one of our windows: its offer and that window.
    incoming: Option<(DragOffer, UnboundedSender<WindowEvent>)>,
    /// A drop being handled: kept until the UI finishes it.
    dropped: Option<DragOffer>,
    outgoing: Option<Outgoing>,
    clipboard_sources: Vec<(CopyPasteSource, Vec<(String, Arc<[u8]>)>)>,
    primary_sources: Vec<(PrimarySelectionSource, Vec<(String, Arc<[u8]>)>)>,
    shared: Arc<Shared>,
}

impl State {
    fn new(
        globals: &GlobalList,
        qh: &QueueHandle<State>,
        connection: Connection,
        shared: Arc<Shared>,
    ) -> Option<Self> {
        let devices = DataDeviceManagerState::bind(globals, qh).ok()?;
        let seat_state = SeatState::new(globals, qh);
        let seats = seat_state
            .seats()
            .map(|seat| (seat.id(), SeatSlot::default()))
            .collect();
        Some(Self {
            registry: RegistryState::new(globals),
            seat_state,
            connection,
            qh: qh.clone(),
            devices,
            primary_manager: PrimarySelectionManagerState::bind(globals, qh).ok(),
            // Version 4 at most: the drag image is placed by `attach`'s
            // offset, which version 5 turned into a protocol error.
            compositor: globals.bind::<WlCompositor, _, _>(qh, 1..=4, ()).ok(),
            shm: Shm::bind(globals, qh).ok(),
            seats,
            latest_seat: None,
            windows: HashMap::new(),
            incoming: None,
            dropped: None,
            outgoing: None,
            clipboard_sources: Vec::new(),
            primary_sources: Vec::new(),
            shared,
        })
    }

    fn seat(&self) -> Option<&SeatSlot> {
        self.latest_seat
            .as_ref()
            .and_then(|id| self.seats.get(id))
            .or_else(|| self.seats.values().find(|slot| slot.device.is_some()))
    }

    fn surface(&self, pointer: usize) -> Option<WlSurface> {
        // SAFETY: the pointer is a live `wl_surface*` of this connection: the
        // UI thread reads it from a window that is open while it asks.
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), pointer as *mut _) }.ok()?;
        WlSurface::from_id(&self.connection, id).ok()
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Register { surface, events } => {
                if let Some(surface) = self.surface(surface) {
                    self.windows.insert(surface.id().protocol_id(), events);
                }
            }
            Command::Unregister { surface } => {
                if let Some(surface) = self.surface(surface) {
                    self.windows.remove(&surface.id().protocol_id());
                }
            }
            Command::Respond {
                serial,
                mime,
                actions,
                preferred,
            } => {
                if let Some((offer, _)) = &self.incoming
                    && offer.serial == serial
                {
                    offer.accept_mime_type(serial, mime);
                    offer.set_actions(actions, preferred);
                }
            }
            Command::Receive {
                serial,
                mime,
                reply,
            } => {
                let offer = self.dropped.as_ref().filter(|offer| offer.serial == serial);
                match offer.map(|offer| offer.receive(mime)) {
                    Some(Ok(pipe)) => {
                        let _ = self.connection.flush();
                        read_off_thread(pipe, move |result| {
                            let _ = reply.send(result);
                        });
                    }
                    Some(Err(error)) => {
                        let _ = reply.send(Err(error));
                    }
                    None => {
                        let _ = reply.send(Err(io::Error::other("the drop has ended")));
                    }
                }
            }
            Command::Finish { serial, ok, action } => {
                if let Some(offer) = self.dropped.take_if(|offer| offer.serial == serial) {
                    if ok {
                        if let Some(action) = action {
                            offer.set_actions(action, action);
                        }
                        offer.finish();
                    }
                    offer.destroy();
                }
            }
            Command::StartDrag {
                surface,
                mime_types,
                actions,
                icon,
                events,
            } => self.start_drag(surface, mime_types, actions, icon, events),
            Command::CancelDrag => {
                if let Some(source) = self
                    .outgoing
                    .as_ref()
                    .map(|outgoing| outgoing.source.inner().clone())
                {
                    // Dropping the source destroys it: the compositor ends
                    // the drag.
                    self.end_outgoing(&source, SourceEvent::Cancelled);
                }
            }
            Command::SetSelection { primary, data } => self.set_selection(primary, data),
            Command::ReceiveSelection {
                primary,
                mime,
                reply,
            } => {
                let result = self.receive_selection(primary, mime);
                if result.is_ok() {
                    let _ = self.connection.flush();
                }
                let _ = reply.send(result);
            }
        }
    }

    fn start_drag(
        &mut self,
        surface: usize,
        mime_types: Vec<String>,
        actions: DndAction,
        icon: Option<Icon>,
        events: UnboundedSender<SourceEvent>,
    ) {
        let cancelled = |events: &UnboundedSender<SourceEvent>| {
            let _ = events.unbounded_send(SourceEvent::Cancelled);
        };
        let Some(origin) = self.surface(surface) else {
            return cancelled(&events);
        };
        let Some((device, serial)) = self
            .seat()
            .and_then(|slot| Some((slot.device.as_ref()?.inner().clone(), slot.press_serial)))
        else {
            return cancelled(&events);
        };
        if let Some(previous) = self.outgoing.take() {
            cancelled(&previous.events);
        }
        let source = self
            .devices
            .create_drag_and_drop_source(&self.qh, mime_types, actions);
        let icon = icon.and_then(|icon| self.icon_surface(icon));
        device.start_drag(
            Some(source.inner()),
            &origin,
            icon.as_ref().map(|(surface, _)| surface),
            serial,
        );
        self.outgoing = Some(Outgoing {
            source,
            icon,
            events,
        });
    }

    fn icon_surface(&mut self, icon: Icon) -> Option<(WlSurface, RawPool)> {
        let compositor = self.compositor.as_ref()?;
        let shm = self.shm.as_ref()?;
        if icon.width == 0 || icon.height == 0 {
            return None;
        }
        let stride = icon.width as usize * 4;
        let len = stride * icon.height as usize;
        if icon.pixels.len() < len {
            return None;
        }
        let mut pool = RawPool::new(len, shm).ok()?;
        pool.mmap()[..len].copy_from_slice(&icon.pixels[..len]);
        let buffer = pool.create_buffer(
            0,
            icon.width as i32,
            icon.height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            (),
            &self.qh,
        );
        let surface = compositor.create_surface(&self.qh, ());
        if surface.version() >= 3 {
            surface.set_buffer_scale(icon.scale.max(1));
        }
        surface.attach(Some(&buffer), -icon.hotspot.0, -icon.hotspot.1);
        surface.damage(0, 0, i32::MAX, i32::MAX);
        surface.commit();
        Some((surface, pool))
    }

    /// The drag of `source` ended with `event`: tell the UI and drop the
    /// drag image.
    fn end_outgoing(&mut self, source: &WlDataSource, event: SourceEvent) {
        if let Some(outgoing) = self
            .outgoing
            .take_if(|outgoing| outgoing.source.inner() == source)
        {
            let _ = outgoing.events.unbounded_send(event);
            if let Some((surface, _pool)) = outgoing.icon {
                surface.destroy();
            }
        }
    }

    fn set_selection(&mut self, primary: bool, data: Vec<(String, Arc<[u8]>)>) {
        let Some(slot) = self.seat() else {
            return;
        };
        let serial = slot.serial;
        let mime_types: Vec<String> = data.iter().map(|(mime, _)| mime.clone()).collect();
        let owned = (!data.is_empty()).then(|| data.clone());
        if primary {
            let (Some(manager), Some(device)) = (&self.primary_manager, slot.primary.as_ref())
            else {
                return;
            };
            if data.is_empty() {
                device.unset_selection(serial);
            } else {
                let source = manager.create_selection_source(&self.qh, mime_types);
                source.set_selection(device, serial);
                self.primary_sources.push((source, data));
            }
            self.with_selection(|selection| selection.own_primary = owned);
        } else {
            let Some(device) = slot.device.as_ref() else {
                return;
            };
            if data.is_empty() {
                device.unset_selection(serial);
            } else {
                let source = self.devices.create_copy_paste_source(&self.qh, mime_types);
                source.set_selection(device, serial);
                self.clipboard_sources.push((source, data));
            }
            self.with_selection(|selection| selection.own_clipboard = owned);
        }
    }

    fn receive_selection(&self, primary: bool, mime: String) -> io::Result<ReadPipe> {
        let slot = self
            .seat()
            .ok_or_else(|| io::Error::other("no seat has had input yet"))?;
        if primary {
            slot.primary
                .as_ref()
                .and_then(|device| device.data().selection_offer())
                .ok_or_else(|| io::Error::other("the primary selection is empty"))?
                .receive(mime)
        } else {
            slot.device
                .as_ref()
                .and_then(|device| device.data().selection_offer())
                .ok_or_else(|| io::Error::other("the clipboard is empty"))?
                .receive(mime)
                .map_err(|error| io::Error::other(error.to_string()))
        }
    }

    fn with_selection(&self, change: impl FnOnce(&mut Selection)) {
        let mut selection = self
            .shared
            .selection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        change(&mut selection);
        selection.generation += 1;
    }

    fn drag_offer(&self, device: &WlDataDevice) -> Option<DragOffer> {
        self.seats
            .values()
            .filter_map(|slot| slot.device.as_ref())
            .find(|candidate| candidate.inner() == device)
            .and_then(|device| device.data().drag_offer())
    }

    fn send_incoming(&self, event: WindowEvent) {
        if let Some((_, window)) = &self.incoming {
            let _ = window.unbounded_send(event);
        }
    }
}

/// Read `pipe` to its end on a thread of its own and hand over what came.
fn read_off_thread(pipe: ReadPipe, done: impl FnOnce(io::Result<Vec<u8>>) + Send + 'static) {
    let spawned = thread::Builder::new()
        .name("bingux-dnd-read".into())
        .spawn(move || done(read_all(pipe, READ_LIMIT)));
    if let Err(error) = spawned {
        eprintln!("bingux-ui: could not read dropped data: {error}");
    }
}

/// Read `reader` to its end, refusing more than `limit` bytes.
pub(crate) fn read_all(mut reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(data),
            Ok(count) => {
                if data.len() + count > limit {
                    return Err(io::Error::other("the data is too large"));
                }
                data.extend_from_slice(&chunk[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

/// Write `data` to `pipe` and close it, on a thread of its own: a reader in
/// this same program reads on another thread, and a slow reader elsewhere
/// does not hold anything up.
pub(crate) fn write_off_thread(pipe: WritePipe, data: Arc<[u8]>) {
    let spawned = thread::Builder::new()
        .name("bingux-dnd-write".into())
        .spawn(move || {
            let fd: OwnedFd = pipe.into();
            // A pipe from the compositor may arrive non-blocking.
            // SAFETY: `fd` is a valid descriptor owned here.
            unsafe {
                let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
                if flags >= 0 {
                    libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK);
                }
            }
            let mut file = std::fs::File::from(fd);
            // A reader that goes away early is its own business.
            let _ = file.write_all(&data);
        });
    if let Err(error) = spawned {
        eprintln!("bingux-ui: could not send dragged data: {error}");
    }
}

/// Keep only what a selection source has that `mime` names.
fn selection_bytes(data: &[(String, Arc<[u8]>)], mime: &str) -> Option<Arc<[u8]>> {
    data.iter()
        .find(|(offered, _)| offered == mime)
        .map(|(_, bytes)| bytes.clone())
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.entry(seat.id()).or_default();
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let slot = self.seats.entry(seat.id()).or_default();
        // Every seat that can point or type gets a data device: a drag needs
        // a pointer, a selection needs a keyboard.
        if slot.device.is_none() {
            slot.device = Some(self.devices.get_data_device(qh, &seat));
        }
        if slot.primary.is_none() {
            slot.primary = self
                .primary_manager
                .as_ref()
                .map(|manager| manager.get_selection_device(qh, &seat));
        }
        match capability {
            Capability::Pointer if slot.pointer.is_none() => {
                slot.pointer = self.seat_state.get_pointer(qh, &seat).ok();
            }
            Capability::Keyboard if slot.keyboard.is_none() => {
                slot.keyboard = Some(seat.get_keyboard(qh, seat.id()));
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let Some(slot) = self.seats.get_mut(&seat.id()) else {
            return;
        };
        match capability {
            Capability::Pointer => {
                if let Some(pointer) = slot.pointer.take()
                    && pointer.version() >= 3
                {
                    pointer.release();
                }
            }
            Capability::Keyboard => {
                if let Some(keyboard) = slot.keyboard.take()
                    && keyboard.version() >= 3
                {
                    keyboard.release();
                }
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.remove(&seat.id());
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(seat) = pointer.data::<PointerData>().map(|data| data.seat().id()) else {
            return;
        };
        let Some(slot) = self.seats.get_mut(&seat) else {
            return;
        };
        let mut moved = false;
        for event in events {
            match event.kind {
                PointerEventKind::Press { serial, .. } => {
                    slot.press_serial = serial;
                    slot.serial = serial;
                    moved = true;
                }
                PointerEventKind::Release { serial, .. } | PointerEventKind::Enter { serial } => {
                    slot.serial = serial;
                    moved = true;
                }
                _ => {}
            }
        }
        if moved {
            self.latest_seat = Some(seat);
        }
    }
}

impl Dispatch<WlKeyboard, ObjectId> for State {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        seat: &ObjectId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(slot) = state.seats.get_mut(seat) else {
            return;
        };
        match event {
            wl_keyboard::Event::Enter { serial, .. } => {
                slot.serial = serial;
                slot.keyboard_focus = true;
                state.latest_seat = Some(seat.clone());
            }
            wl_keyboard::Event::Key { serial, .. }
            | wl_keyboard::Event::Modifiers { serial, .. } => {
                slot.serial = serial;
                state.latest_seat = Some(seat.clone());
            }
            wl_keyboard::Event::Leave { .. } => slot.keyboard_focus = false,
            _ => {}
        }
    }
}

impl DataDeviceHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &WlSurface,
    ) {
        self.incoming = None;
        let Some(window) = self.windows.get(&surface.id().protocol_id()).cloned() else {
            return;
        };
        let Some(offer) = self.drag_offer(device) else {
            // A drag of this program with no source (never started here):
            // nothing to offer.
            return;
        };
        let mime_types = offer.with_mime_types(<[String]>::to_vec);
        let event = WindowEvent::Enter {
            serial: offer.serial,
            x,
            y,
            mime_types,
            source_actions: offer.source_actions,
        };
        self.incoming = Some((offer, window));
        self.send_incoming(event);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        self.send_incoming(WindowEvent::Leave);
        self.incoming = None;
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, x: f64, y: f64) {
        self.send_incoming(WindowEvent::Motion { x, y });
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let mime_types = self
            .seats
            .values()
            .filter_map(|slot| slot.device.as_ref())
            .find(|candidate| candidate.inner() == device)
            .and_then(|device| device.data().selection_offer())
            .map(|offer| offer.with_mime_types(<[String]>::to_vec))
            .unwrap_or_default();
        self.with_selection(|selection| selection.clipboard = mime_types);
    }

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some(offer) = self.drag_offer(device) else {
            return;
        };
        let event = WindowEvent::Drop {
            serial: offer.serial,
            x: offer.x,
            y: offer.y,
            action: offer.selected_action,
        };
        self.send_incoming(event);
        if let Some(previous) = self.dropped.replace(offer) {
            previous.destroy();
        }
    }
}

impl DataOfferHandler for State {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        action: DndAction,
    ) {
        self.send_incoming(WindowEvent::Action(action));
    }
}

impl DataSourceHandler for State {
    // The type a target would take: nothing here needs it, the action
    // (`action` below) is what a source acts on.
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        if let Some(outgoing) = &self.outgoing
            && outgoing.source.inner() == source
        {
            let _ = outgoing
                .events
                .unbounded_send(SourceEvent::Send { mime, pipe });
            return;
        }
        let data = self
            .clipboard_sources
            .iter()
            .find(|(candidate, _)| candidate.inner() == source)
            .and_then(|(_, data)| selection_bytes(data, &mime));
        if let Some(data) = data {
            write_off_thread(pipe, data);
        }
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.end_outgoing(source, SourceEvent::Cancelled);
        let before = self.clipboard_sources.len();
        self.clipboard_sources
            .retain(|(candidate, _)| candidate.inner() != source);
        if self.clipboard_sources.is_empty() && before > 0 {
            self.with_selection(|selection| selection.own_clipboard = None);
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if let Some(outgoing) = &self.outgoing
            && outgoing.source.inner() == source
        {
            let _ = outgoing.events.unbounded_send(SourceEvent::Dropped);
        }
    }

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.end_outgoing(source, SourceEvent::Finished);
    }

    fn action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        action: DndAction,
    ) {
        if let Some(outgoing) = &self.outgoing
            && outgoing.source.inner() == source
        {
            let _ = outgoing.events.unbounded_send(SourceEvent::Action(action));
        }
    }
}

impl PrimarySelectionDeviceHandler for State {
    fn selection(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &ZwpPrimarySelectionDeviceV1,
    ) {
        let mime_types = self
            .seats
            .values()
            .filter_map(|slot| slot.primary.as_ref())
            .find(|candidate| candidate.inner() == device)
            .and_then(|device| device.data().selection_offer())
            .map(|offer| offer.with_mime_types(<[String]>::to_vec))
            .unwrap_or_default();
        self.with_selection(|selection| selection.primary = mime_types);
    }
}

impl PrimarySelectionSourceHandler for State {
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &ZwpPrimarySelectionSourceV1,
        mime: String,
        pipe: WritePipe,
    ) {
        let data = self
            .primary_sources
            .iter()
            .find(|(candidate, _)| candidate.inner() == source)
            .and_then(|(_, data)| selection_bytes(data, &mime));
        if let Some(data) = data {
            write_off_thread(pipe, data);
        }
    }

    fn cancelled(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &ZwpPrimarySelectionSourceV1,
    ) {
        let before = self.primary_sources.len();
        self.primary_sources
            .retain(|(candidate, _)| candidate.inner() != source);
        if self.primary_sources.is_empty() && before > 0 {
            self.with_selection(|selection| selection.own_primary = None);
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        self.shm
            .as_mut()
            .expect("wl_shm events arrive only for a bound wl_shm")
    }
}

impl ProvidesRegistryState for State {
    registry_handlers![SeatState];

    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
}

impl Dispatch<WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: <WlCompositor as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSurface,
        _: <WlSurface as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_seat!(State);
delegate_pointer!(State);
delegate_data_device!(State);
delegate_primary_selection!(State);
delegate_shm!(State);
delegate_registry!(State);
