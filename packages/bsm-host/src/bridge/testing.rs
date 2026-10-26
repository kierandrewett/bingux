//! Test-only behaviour behind the `test-pattern` cargo feature: the bridge
//! presents generated frames instead of Gnoblin's and can feed the Bingux
//! readiness events itself, so bsm's integration tests can exercise the
//! protocol without Gnoblin. The packages never enable the feature.

use std::io::Write;

use calloop::timer::{TimeoutAction, Timer};

use super::*;
use crate::readiness::{LayerSurface, Role};

impl Bridge {
    pub(super) fn is_test(&self) -> bool {
        self.cfg.test.is_some()
    }

    pub(super) fn test_input(&mut self, v: serde_json::Value) {
        if let Some(f) = self.test.log.as_mut() {
            let _ = writeln!(f, "{v}");
            let _ = f.flush();
        }
    }

    pub(super) fn start_test_timers(&mut self) -> Result<()> {
        let Some(test) = self.cfg.test.clone() else { return Ok(()) };
        // Frames: 60 Hz in the foreground, throttled by the gate in the background.
        self.handle
            .insert_source(Timer::from_duration(Duration::from_millis(16)), |_, _, b| {
                b.test_frames();
                TimeoutAction::ToDuration(Duration::from_millis(16))
            })
            .map_err(|e| anyhow!("insert test timer: {e}"))?;
        if let Some(delay) = test.auto_ready {
            self.handle
                .insert_source(Timer::from_duration(delay), |_, _, b| {
                    b.test_auto_ready();
                    TimeoutAction::ToDuration(Duration::from_millis(100))
                })
                .map_err(|e| anyhow!("insert test ready timer: {e}"))?;
        }
        Ok(())
    }

    fn test_frames(&mut self) {
        self.test.tick += 1;
        let tick = self.test.tick;
        let names: Vec<String> = self.outputs.keys().cloned().collect();
        for name in names {
            let Some(o) = self.outputs.get_mut(&name) else { continue };
            if o.phase != Phase::Applied || !o.gate.admit(Instant::now()) {
                continue;
            }
            let Some(set) = o.gate.buffers() else { continue };
            let Some(slot) = set.claim() else { continue };
            // SAFETY: claimed above.
            let dst = unsafe { set.slot_mut(slot) };
            paint(dst, set.width, set.height, tick, &name);
            Self::commit(o, &set, slot, &[]);
        }
        let _ = self.conn.flush();
    }

    fn test_auto_ready(&mut self) {
        if self.readiness_done() {
            return;
        }
        let outputs: Vec<(String, Option<String>)> =
            self.outputs.iter().map(|(n, o)| (n.clone(), o.connector.clone())).collect();
        if outputs.is_empty() || outputs.iter().any(|(_, c)| c.is_none()) {
            return;
        }
        let g = self.generation.clone();
        let mk = |component: &str, event: &str| ReadyLine {
            generation: g.clone(),
            component: Some(component.into()),
            event: event.into(),
            output: None,
            role: None,
            mapped: None,
            required: None,
        };
        let mut lines = vec![mk("wallpaper", "launched"), mk("desktop", "launched")];
        let mut layers = Vec::new();
        for (_, connector) in &outputs {
            let connector = connector.clone().unwrap();
            for (comp, role) in [("wallpaper", Role::Wallpaper), ("desktop", Role::Bar), ("desktop", Role::Dock)] {
                lines.push(ReadyLine {
                    output: Some(connector.clone()),
                    role: Some(
                        match role {
                            Role::Wallpaper => "wallpaper",
                            Role::Bar => "bar",
                            Role::Dock => "dock",
                        }
                        .into(),
                    ),
                    mapped: Some(true),
                    ..mk(comp, "surface")
                });
                layers.push(LayerSurface { namespace: role.namespace().into(), connector: connector.clone(), mapped: true });
            }
        }
        lines.push(mk("wallpaper", "ready"));
        lines.push(mk("desktop", "ready"));
        let reports = self.readiness.set_layers(layers);
        self.send_reports(reports);
        for l in lines {
            self.on_ready_line(l);
        }
        self.test.ready_sent = true;
    }

    fn readiness_done(&self) -> bool {
        self.test.ready_sent
    }
}

/// Paint the test pattern: a dark background with a moving bright bar, a frame
/// counter in the colour of the first pixels and a border so cropping shows.
pub fn paint(dst: &mut [u8], width: u32, height: u32, tick: u64, name: &str) {
    let (w, h) = (width as usize, height as usize);
    let hue = name.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let (r0, g0, b0) = ((hue & 0x3f) as u8 + 0x10, ((hue >> 6) & 0x3f) as u8 + 0x10, ((hue >> 12) & 0x3f) as u8 + 0x10);
    let bar = (tick as usize * 4) % w.max(1);
    for y in 0..h {
        let row = &mut dst[y * w * 4..(y + 1) * w * 4];
        for x in 0..w {
            let border = x < 4 || y < 4 || x + 4 >= w || y + 4 >= h;
            let in_bar = x >= bar && x < bar + 16;
            let (r, g, b) = if border {
                (0xff, 0xff, 0xff)
            } else if in_bar {
                (0xff, 0xd0, 0x20)
            } else {
                (r0, g0.wrapping_add((y * 64 / h.max(1)) as u8), b0)
            };
            row[x * 4..x * 4 + 4].copy_from_slice(&[b, g, r, 0xff]);
        }
    }
    // The first 8 pixels carry the low bits of the tick for tests.
    for i in 0..8 {
        let bit = (tick >> i) & 1;
        let v = if bit == 1 { 0xff } else { 0x00 };
        let o = (8 + i) * 4;
        dst[(8 * w * 4) + o..(8 * w * 4) + o + 4].copy_from_slice(&[v, v, v, 0xff]);
    }
}
