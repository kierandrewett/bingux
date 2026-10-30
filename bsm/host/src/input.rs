//! Input injection into Gnoblin through EIS (libei protocol, `reis` client).
//!
//! The bridge is an EI *sender*. Mutter's EIS server offers the devices after
//! the RemoteDesktop session starts: a keyboard, a relative pointer and an
//! absolute pointer whose regions are the virtual monitors. Coordinates passed
//! to this module are logical coordinates relative to the origin of a bsm
//! output; [`Ei`] adds the origin of the matching EIS region.
//!
//! Events are only sent between `start_emulating` and the device pause, each
//! batch is closed with `frame`, and every held key and button is tracked so
//! [`Ei::release_all`] can undo them when input is revoked.

use std::collections::BTreeSet;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;

use anyhow::{Result, anyhow};
use reis::ei;
use reis::event::{Connection, Device, DeviceCapability, EiEvent, EiEventConverter};
use reis::handshake::{EiHandshaker, HandshakeResp};
use reis::{PendingRequestResult, ei::keyboard::KeyState, ei::button::ButtonState};

/// A rectangle in Gnoblin's logical coordinate space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

struct Dev {
    dev: Device,
    emulating: bool,
}

pub struct Ei {
    ctx: ei::Context,
    handshaker: Option<EiHandshaker<'static>>,
    conv: Option<EiEventConverter>,
    conn: Option<Connection>,
    devices: Vec<Dev>,
    sequence: u32,
    held_keys: BTreeSet<u32>,
    held_buttons: BTreeSet<u32>,
    held_touches: BTreeSet<u32>,
}

fn now_micros() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: valid pointer to a timespec.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1000
}

impl Ei {
    pub fn new(stream: UnixStream) -> Result<Self> {
        let ctx = ei::Context::new(stream)?;
        Ok(Self {
            ctx,
            handshaker: Some(EiHandshaker::new("bsm-host", ei::handshake::ContextType::Sender)),
            conv: None,
            conn: None,
            devices: Vec::new(),
            sequence: 0,
            held_keys: BTreeSet::new(),
            held_buttons: BTreeSet::new(),
            held_touches: BTreeSet::new(),
        })
    }

    pub fn fd(&self) -> BorrowedFd<'_> {
        self.ctx.as_fd()
    }

    /// Read and process everything pending on the socket. An error means the
    /// EIS connection is gone.
    pub fn pump(&mut self) -> Result<()> {
        self.ctx.read().map_err(|e| anyhow!("EIS connection: {e}"))?;
        while let Some(result) = self.ctx.pending_event() {
            let event = match result {
                PendingRequestResult::Request(e) => e,
                PendingRequestResult::ParseError(e) => return Err(anyhow!("EIS parse error: {e:?}")),
                PendingRequestResult::InvalidObject(id) => {
                    log::debug!("EIS event for unknown object {id}");
                    continue;
                }
            };
            if let Some(h) = self.handshaker.as_mut() {
                if let Some(resp) = h.handle_event(event).map_err(|e| anyhow!("EIS handshake: {e}"))? {
                    self.finish_handshake(resp);
                }
                continue;
            }
            if let Some(conv) = self.conv.as_mut() {
                conv.handle_event(event).map_err(|e| anyhow!("EIS event: {e}"))?;
            }
        }
        self.drain_events()?;
        if let Some(c) = &self.conn {
            let _ = c.flush();
        }
        Ok(())
    }

    fn finish_handshake(&mut self, resp: HandshakeResp) {
        self.handshaker = None;
        let conv = EiEventConverter::new(&self.ctx, resp);
        self.conn = Some(conv.connection().clone());
        self.conv = Some(conv);
        log::info!("EIS handshake complete");
    }

    fn drain_events(&mut self) -> Result<()> {
        loop {
            let Some(event) = self.conv.as_mut().and_then(|c| c.next_event()) else { return Ok(()) };
            match event {
                EiEvent::SeatAdded(e) => {
                    use DeviceCapability::*;
                    e.seat.bind_capabilities(Pointer | PointerAbsolute | Keyboard | Touch | Scroll | Button);
                }
                EiEvent::DeviceAdded(e) => {
                    log::info!(
                        "EIS device {:?} regions={} caps: kbd={} abs={} ptr={} btn={} scroll={} touch={}",
                        e.device.name(),
                        e.device.regions().len(),
                        e.device.has_capability(DeviceCapability::Keyboard),
                        e.device.has_capability(DeviceCapability::PointerAbsolute),
                        e.device.has_capability(DeviceCapability::Pointer),
                        e.device.has_capability(DeviceCapability::Button),
                        e.device.has_capability(DeviceCapability::Scroll),
                        e.device.has_capability(DeviceCapability::Touch),
                    );
                    self.devices.push(Dev { dev: e.device, emulating: false });
                }
                EiEvent::DeviceRemoved(e) => self.devices.retain(|d| d.dev != e.device),
                EiEvent::DeviceResumed(e) => {
                    let serial = self.conn.as_ref().map(|c| c.serial()).unwrap_or(0);
                    if let Some(d) = self.devices.iter_mut().find(|d| d.dev == e.device) {
                        self.sequence = self.sequence.wrapping_add(1);
                        d.dev.device().start_emulating(serial, self.sequence);
                        d.emulating = true;
                    }
                }
                EiEvent::DevicePaused(e) => {
                    if let Some(d) = self.devices.iter_mut().find(|d| d.dev == e.device) {
                        d.emulating = false;
                    }
                }
                EiEvent::Disconnected(d) => return Err(anyhow!("EIS disconnected: {d:?}")),
                _ => {}
            }
        }
    }

    /// True once a keyboard and an absolute pointer are ready to emulate.
    pub fn ready(&self) -> bool {
        self.find(DeviceCapability::Keyboard).is_some() && self.find(DeviceCapability::PointerAbsolute).is_some()
    }

    fn find(&self, cap: DeviceCapability) -> Option<&Dev> {
        self.devices.iter().find(|d| d.emulating && d.dev.has_capability(cap))
    }

    fn frame(&self, dev: &Device) {
        let serial = self.conn.as_ref().map(|c| c.serial()).unwrap_or(0);
        dev.device().frame(serial, now_micros());
        if let Some(c) = &self.conn {
            let _ = c.flush();
        }
    }

    /// The EIS region matching a logical rectangle (the monitor of an output).
    pub fn region_for(&self, want: &LogicalRect) -> Option<LogicalRect> {
        let dev = self.find(DeviceCapability::PointerAbsolute)?;
        let regions = dev.dev.regions();
        let near = |a: f64, b: f64| (a - b).abs() <= 1.5;
        let rect = |r: &reis::event::Region| LogicalRect {
            x: r.x as f64,
            y: r.y as f64,
            w: r.width as f64,
            h: r.height as f64,
        };
        regions
            .iter()
            .map(rect)
            .find(|r| near(r.x, want.x) && near(r.y, want.y) && near(r.w, want.w) && near(r.h, want.h))
            .or_else(|| if regions.len() == 1 { regions.first().map(rect) } else { None })
    }

    pub fn key(&mut self, code: u32, pressed: bool) {
        let Some(d) = self.find(DeviceCapability::Keyboard) else { return };
        let Some(kb) = d.dev.interface::<ei::Keyboard>() else { return };
        let dev = d.dev.clone();
        kb.key(code, if pressed { KeyState::Press } else { KeyState::Released });
        self.frame(&dev);
        if pressed {
            self.held_keys.insert(code);
        } else {
            self.held_keys.remove(&code);
        }
    }

    /// Absolute pointer motion to `(lx, ly)` relative to the origin of `region`.
    pub fn pointer_abs(&mut self, region: &LogicalRect, lx: f64, ly: f64) {
        let Some(d) = self.find(DeviceCapability::PointerAbsolute) else { return };
        let Some(p) = d.dev.interface::<ei::PointerAbsolute>() else { return };
        let dev = d.dev.clone();
        let x = region.x + lx.clamp(0.0, (region.w - 1.0).max(0.0));
        let y = region.y + ly.clamp(0.0, (region.h - 1.0).max(0.0));
        p.motion_absolute(x as f32, y as f32);
        self.frame(&dev);
    }

    pub fn button(&mut self, code: u32, pressed: bool) {
        let Some(d) = self.find(DeviceCapability::Button) else { return };
        let Some(b) = d.dev.interface::<ei::Button>() else { return };
        let dev = d.dev.clone();
        b.button(code, if pressed { ButtonState::Press } else { ButtonState::Released });
        self.frame(&dev);
        if pressed {
            self.held_buttons.insert(code);
        } else {
            self.held_buttons.remove(&code);
        }
    }

    /// Smooth scroll in logical pixels.
    pub fn scroll(&mut self, dx: f32, dy: f32) {
        let Some(d) = self.find(DeviceCapability::Scroll) else { return };
        let Some(s) = d.dev.interface::<ei::Scroll>() else { return };
        let dev = d.dev.clone();
        s.scroll(dx, dy);
        self.frame(&dev);
    }

    /// Discrete scroll in 1/120 steps.
    pub fn scroll_discrete(&mut self, dx: i32, dy: i32) {
        let Some(d) = self.find(DeviceCapability::Scroll) else { return };
        let Some(s) = d.dev.interface::<ei::Scroll>() else { return };
        let dev = d.dev.clone();
        s.scroll_discrete(dx, dy);
        self.frame(&dev);
    }

    pub fn scroll_stop(&mut self, x: bool, y: bool) {
        let Some(d) = self.find(DeviceCapability::Scroll) else { return };
        let Some(s) = d.dev.interface::<ei::Scroll>() else { return };
        let dev = d.dev.clone();
        s.scroll_stop(x as u32, y as u32, 0);
        self.frame(&dev);
    }

    pub fn touch_down(&mut self, id: u32, region: &LogicalRect, lx: f64, ly: f64) {
        let Some(d) = self.find(DeviceCapability::Touch) else { return };
        let Some(t) = d.dev.interface::<ei::Touchscreen>() else { return };
        let dev = d.dev.clone();
        t.down(id, (region.x + lx) as f32, (region.y + ly) as f32);
        self.frame(&dev);
        self.held_touches.insert(id);
    }

    pub fn touch_motion(&mut self, id: u32, region: &LogicalRect, lx: f64, ly: f64) {
        let Some(d) = self.find(DeviceCapability::Touch) else { return };
        let Some(t) = d.dev.interface::<ei::Touchscreen>() else { return };
        let dev = d.dev.clone();
        t.motion(id, (region.x + lx) as f32, (region.y + ly) as f32);
        self.frame(&dev);
    }

    pub fn touch_up(&mut self, id: u32) {
        let Some(d) = self.find(DeviceCapability::Touch) else { return };
        let Some(t) = d.dev.interface::<ei::Touchscreen>() else { return };
        let dev = d.dev.clone();
        t.up(id);
        self.frame(&dev);
        self.held_touches.remove(&id);
    }

    /// Release everything that is held. Called whenever input is revoked so a
    /// key pressed in the foreground never stays down in the background.
    pub fn release_all(&mut self) {
        for k in std::mem::take(&mut self.held_keys) {
            if let Some(d) = self.find(DeviceCapability::Keyboard) {
                if let Some(kb) = d.dev.interface::<ei::Keyboard>() {
                    let dev = d.dev.clone();
                    kb.key(k, KeyState::Released);
                    self.frame(&dev);
                }
            }
        }
        for b in std::mem::take(&mut self.held_buttons) {
            if let Some(d) = self.find(DeviceCapability::Button) {
                if let Some(btn) = d.dev.interface::<ei::Button>() {
                    let dev = d.dev.clone();
                    btn.button(b, ButtonState::Released);
                    self.frame(&dev);
                }
            }
        }
        for t in std::mem::take(&mut self.held_touches) {
            if let Some(d) = self.find(DeviceCapability::Touch) {
                if let Some(ts) = d.dev.interface::<ei::Touchscreen>() {
                    let dev = d.dev.clone();
                    ts.up(t);
                    self.frame(&dev);
                }
            }
        }
    }

    pub fn held(&self) -> (usize, usize, usize) {
        (self.held_keys.len(), self.held_buttons.len(), self.held_touches.len())
    }
}
