//! Physical input routing. Exactly one principal owns the seat: the greeter, one
//! session, or nobody. Keyboard and pointer focus, and so every `wl_seat` event, go
//! only to surfaces of the owner. A change of owner takes effect at a frame boundary
//! and is refused unless the target is legitimately foreground.

use bsm_proto::scene::SceneEvent;
use bsm_proto::state::{InputOwner, SessionId};
use smithay::backend::input::{Axis, AxisSource, ButtonState, KeyState};
use smithay::desktop::{WindowSurfaceType, layer_map_for_output};
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::pointer::{AxisFrame, ButtonEvent, CursorImageStatus, MotionEvent};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::shell::wlr_layer::Layer;

use crate::state::State;

/// The surface under a point plus the global location of that surface's origin.
type Hit = (WlSurface, Point<f64, Logical>);

impl State {
    fn owner_session(&self) -> Option<&SessionId> {
        match &self.owner {
            InputOwner::Session(s) => Some(s),
            _ => None,
        }
    }

    /// May `o` receive input right now?
    fn owner_allowed(&self, o: &InputOwner) -> bool {
        match o {
            InputOwner::None | InputOwner::Greeter => true,
            InputOwner::Session(sid) => {
                let Some(rec) = self.sessions.get(sid) else { return false };
                // Only the committed, visible, unprotected foreground session may type.
                self.engine.foreground_session() == Some(&(sid.clone(), rec.generation))
            }
        }
    }

    /// `SceneCommand::SetInputOwner`: queued for the next frame boundary, refused at once
    /// when the target is not allowed (the reply then names the owner that stays).
    pub(crate) fn request_owner(&mut self, o: InputOwner) {
        if self.owner_allowed(&o) {
            self.pending_owner = Some(o);
        } else {
            tracing::warn!("input owner {o:?} refused: not the committed foreground");
            self.emit(SceneEvent::InputOwnerChanged(self.owner.clone()));
        }
    }

    /// Frame boundary: apply a queued owner change, re-checking the rules.
    pub(crate) fn apply_pending_owner(&mut self) {
        if let Some(o) = self.pending_owner.take() {
            if self.owner_allowed(&o) {
                self.set_owner_now(o);
            } else {
                self.emit(SceneEvent::InputOwnerChanged(self.owner.clone()));
            }
        }
    }

    /// Switch the owner immediately: clear both focuses first, then focus the new owner,
    /// so no event can reach two principals.
    pub(crate) fn set_owner_now(&mut self, o: InputOwner) {
        let changed = self.owner != o;
        if changed {
            tracing::info!("input owner {:?} -> {o:?}", self.owner);
        }
        self.pending_owner = None;
        let old_session = self.owner_session().cloned();
        self.owner = o;
        self.clear_focus();
        if let Some(s) = old_session {
            self.host_input_state(&s, false);
        }
        if let Some(s) = self.owner_session().cloned() {
            self.host_input_state(&s, true);
        }
        self.cursor = CursorImageStatus::default_named();
        self.refresh_focus();
        if changed {
            self.emit(SceneEvent::InputOwnerChanged(self.owner.clone()));
        }
    }

    /// A session was protected: its input route ends before anything else happens.
    pub(crate) fn revoke_input_of(&mut self, session: &SessionId) {
        if self.pending_owner == Some(InputOwner::Session(session.clone())) {
            self.pending_owner = None;
        }
        if self.owner_session() == Some(session) {
            self.set_owner_now(InputOwner::None);
        }
    }

    /// Fail closed each frame: an owner session that is no longer the committed
    /// foreground (crashed, unregistered, scene changed) loses input.
    pub(crate) fn reconcile_input(&mut self) {
        if let Some(sid) = self.owner_session().cloned() {
            if !self.owner_allowed(&InputOwner::Session(sid)) {
                self.set_owner_now(InputOwner::None);
            }
        }
    }

    fn clear_focus(&mut self) {
        let kb = self.keyboard.clone();
        let ptr = self.pointer.clone();
        let serial = SERIAL_COUNTER.next_serial();
        kb.set_focus(self, None, serial);
        ptr.motion(
            self,
            None,
            &MotionEvent { location: self.pointer_pos, serial, time: 0 },
        );
        ptr.frame(self);
    }

    // ----- hit testing -----

    fn output_at(&self, p: Point<f64, Logical>) -> Option<usize> {
        self.outputs.iter().position(|o| {
            let loc = o.output.current_location();
            let size = o
                .output
                .current_mode()
                .map(|m| {
                    m.size
                        .to_f64()
                        .to_logical(o.output.current_scale().fractional_scale())
                })
                .unwrap_or_default();
            p.x >= loc.x as f64 && p.y >= loc.y as f64 && p.x < loc.x as f64 + size.w && p.y < loc.y as f64 + size.h
        })
    }

    fn greeter_hit(&self, p: Point<f64, Logical>) -> Option<Hit> {
        let idx = self.output_at(p)?;
        let rec = &self.outputs[idx];
        let loc = rec.output.current_location().to_f64();
        let rel = p - loc;
        let map = layer_map_for_output(&rec.output);
        for layer in [Layer::Overlay, Layer::Top, Layer::Bottom, Layer::Background] {
            let Some(l) = map.layer_under(layer, rel) else { continue };
            let geo = map.layer_geometry(l)?;
            if let Some((s, off)) = l.surface_under(rel - geo.loc.to_f64(), WindowSurfaceType::ALL) {
                return Some((s, loc + geo.loc.to_f64() + off.to_f64()));
            }
        }
        None
    }

    fn session_hit(&self, sid: &SessionId, p: Point<f64, Logical>) -> Option<Hit> {
        let rec = self.sessions.get(sid)?;
        let idx = self.output_at(p).or(if self.outputs.is_empty() { None } else { Some(0) })?;
        let o = &self.outputs[idx];
        let hs = rec.surfaces.get(&o.info.name)?;
        Some((hs.surface.clone(), o.output.current_location().to_f64()))
    }

    fn greeter_keyboard_surface(&self) -> Option<WlSurface> {
        if let Some(s) = &self.greeter_kbd_focus {
            if s.is_alive() {
                return Some(s.clone());
            }
        }
        for rec in &self.outputs {
            let map = layer_map_for_output(&rec.output);
            for layer in [Layer::Overlay, Layer::Top, Layer::Bottom, Layer::Background] {
                for l in map.layers_on(layer).rev() {
                    if l.can_receive_keyboard_focus() {
                        return Some(l.wl_surface().clone());
                    }
                }
            }
        }
        None
    }

    fn keyboard_target(&self) -> Option<WlSurface> {
        match &self.owner {
            InputOwner::None => None,
            InputOwner::Greeter => {
                self.greeter.connected?;
                self.greeter_keyboard_surface()
            }
            InputOwner::Session(sid) => {
                if !self.owner_allowed(&self.owner) {
                    return None;
                }
                self.session_hit(sid, self.pointer_pos).map(|(s, _)| s)
            }
        }
    }

    fn pointer_target(&self, p: Point<f64, Logical>) -> Option<Hit> {
        match &self.owner {
            InputOwner::None => None,
            InputOwner::Greeter => {
                self.greeter.connected?;
                self.greeter_hit(p)
            }
            InputOwner::Session(sid) => {
                if !self.owner_allowed(&self.owner) {
                    return None;
                }
                self.session_hit(sid, p)
            }
        }
    }

    /// Recompute keyboard and pointer focus for the current owner and scene.
    pub(crate) fn refresh_focus(&mut self) {
        let kb = self.keyboard.clone();
        let ptr = self.pointer.clone();
        let serial = SERIAL_COUNTER.next_serial();
        let target = self.keyboard_target();
        if kb.current_focus() != target {
            kb.set_focus(self, target, serial);
        }
        let hit = self.pointer_target(self.pointer_pos);
        ptr.motion(self, hit, &MotionEvent { location: self.pointer_pos, serial, time: 0 });
        ptr.frame(self);
    }

    // ----- device events -----

    pub(crate) fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut it = self.outputs.iter().map(|o| {
            let l = o.output.current_location();
            let s = o
                .output
                .current_mode()
                .map(|m| m.size.to_f64().to_logical(o.output.current_scale().fractional_scale()))
                .unwrap_or_default();
            (l.x as f64, l.y as f64, l.x as f64 + s.w, l.y as f64 + s.h)
        });
        let first = it.next()?;
        Some(it.fold(first, |a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))))
    }

    pub fn pointer_moved_to(&mut self, x: f64, y: f64, time: u32) {
        let (x, y) = match self.bounds() {
            Some((x0, y0, x1, y1)) => (x.clamp(x0, x1 - 1.0), y.clamp(y0, y1 - 1.0)),
            None => (x, y),
        };
        self.pointer_pos = (x, y).into();
        let ptr = self.pointer.clone();
        let hit = self.pointer_target(self.pointer_pos);
        ptr.motion(
            self,
            hit,
            &MotionEvent { location: self.pointer_pos, serial: SERIAL_COUNTER.next_serial(), time },
        );
        ptr.frame(self);
        self.needs_redraw = true;
    }

    pub fn pointer_moved_by(&mut self, dx: f64, dy: f64, time: u32) {
        let (x, y) = (self.pointer_pos.x + dx, self.pointer_pos.y + dy);
        self.pointer_moved_to(x, y, time);
    }

    pub fn pointer_button(&mut self, button: u32, pressed: bool, time: u32) {
        let ptr = self.pointer.clone();
        let serial = SERIAL_COUNTER.next_serial();
        if pressed && self.owner == InputOwner::Greeter {
            // Clicking a layer surface that accepts the keyboard moves keyboard focus to it.
            if let Some((s, _)) = self.pointer_target(self.pointer_pos) {
                let mut root = s;
                while let Some(p) = smithay::wayland::compositor::get_parent(&root) {
                    root = p;
                }
                if self.output_of_layer_surface(&root).is_some() {
                    self.greeter_kbd_focus = Some(root);
                    self.refresh_focus();
                }
            }
        }
        ptr.button(
            self,
            &ButtonEvent {
                serial,
                time,
                button,
                state: if pressed { ButtonState::Pressed } else { ButtonState::Released },
            },
        );
        ptr.frame(self);
    }

    pub fn pointer_scroll(&mut self, dx: f64, dy: f64, time: u32) {
        let ptr = self.pointer.clone();
        let mut frame = AxisFrame::new(time).source(AxisSource::Wheel);
        if dx != 0.0 {
            frame = frame.value(Axis::Horizontal, dx);
        }
        if dy != 0.0 {
            frame = frame.value(Axis::Vertical, dy);
        }
        ptr.axis(self, frame);
        ptr.frame(self);
    }

    /// `evdev_code` is the raw Linux key code; xkb keycodes are offset by 8.
    pub fn key(&mut self, evdev_code: u32, pressed: bool, time: u32) {
        let _: Option<()> =
            self.key_xkb(Keycode::new(evdev_code + 8), pressed, time, |_, _, _| FilterResult::Forward);
    }

    pub fn key_xkb<T, F>(&mut self, code: Keycode, pressed: bool, time: u32, filter: F) -> Option<T>
    where
        F: FnOnce(&mut State, &smithay::input::keyboard::ModifiersState, smithay::input::keyboard::KeysymHandle<'_>) -> FilterResult<T>,
    {
        let kb = self.keyboard.clone();
        let serial = SERIAL_COUNTER.next_serial();
        if !self.key_logged && pressed {
            self.key_logged = true;
            let focus = kb.current_focus();
            let who = self.describe_surface(focus.as_ref());
            if focus.is_some() {
                tracing::info!("first key {} delivered to {who} (input owner {:?})", code.raw(), self.owner);
            } else {
                tracing::warn!(
                    "key {} has no keyboard focus and is dropped (input owner {:?}, greeter connected: {}, \
                     greeter surface accepting the keyboard: {})",
                    code.raw(),
                    self.owner,
                    self.greeter.connected.is_some(),
                    self.greeter_keyboard_surface().is_some()
                );
            }
        }
        kb.input::<T, _>(
            self,
            code,
            if pressed { KeyState::Pressed } else { KeyState::Released },
            serial,
            time,
            filter,
        )
    }
}

/// VT switch requested through the keyboard (Ctrl+Alt+F1..F12), to be done by the DRM
/// backend. Intercepted before routing so that no client ever sees the combination.
pub(crate) fn vt_switch_of(
    mods: &smithay::input::keyboard::ModifiersState,
    handle: &smithay::input::keyboard::KeysymHandle<'_>,
    pressed: bool,
) -> FilterResult<i32> {
    use smithay::input::keyboard::xkb;
    if !pressed || !(mods.ctrl && mods.alt) {
        return FilterResult::Forward;
    }
    for sym in handle.modified_syms() {
        let raw = sym.raw();
        if (xkb::keysyms::KEY_XF86Switch_VT_1..=xkb::keysyms::KEY_XF86Switch_VT_12).contains(&raw) {
            return FilterResult::Intercept((raw - xkb::keysyms::KEY_XF86Switch_VT_1 + 1) as i32);
        }
    }
    FilterResult::Forward
}

impl State {
    /// Feed one libinput or winit event into the seat. Returns a requested VT number
    /// when the key combination for a VT switch was pressed.
    pub(crate) fn process_input_event<B: smithay::backend::input::InputBackend>(
        &mut self,
        event: smithay::backend::input::InputEvent<B>,
        vt_switch_enabled: bool,
    ) -> Option<i32> {
        use smithay::backend::input::{
            AbsolutePositionEvent, Event, InputEvent, KeyboardKeyEvent, PointerAxisEvent,
            PointerButtonEvent, PointerMotionEvent,
        };
        match event {
            InputEvent::Keyboard { event } => {
                let pressed = event.state() == KeyState::Pressed;
                let time = Event::time_msec(&event);
                return self.key_xkb::<i32, _>(event.key_code(), pressed, time, |_, mods, handle| {
                    if vt_switch_enabled {
                        vt_switch_of(mods, &handle, pressed)
                    } else {
                        FilterResult::Forward
                    }
                });
            }
            InputEvent::PointerMotion { event } => {
                let t = Event::time_msec(&event);
                self.pointer_moved_by(event.delta_x(), event.delta_y(), t);
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let t = Event::time_msec(&event);
                // Absolute devices span the whole layout.
                if let Some((x0, y0, x1, y1)) = self.bounds() {
                    let x = x0 + event.x_transformed((x1 - x0) as i32);
                    let y = y0 + event.y_transformed((y1 - y0) as i32);
                    self.pointer_moved_to(x, y, t);
                }
            }
            InputEvent::PointerButton { event } => {
                let t = Event::time_msec(&event);
                self.pointer_button(event.button_code(), event.state() == ButtonState::Pressed, t);
            }
            InputEvent::PointerAxis { event } => {
                let t = Event::time_msec(&event);
                let h = event.amount(Axis::Horizontal).unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0);
                let v = event.amount(Axis::Vertical).unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0);
                self.pointer_scroll(h, v, t);
            }
            _ => {}
        }
        None
    }
}
