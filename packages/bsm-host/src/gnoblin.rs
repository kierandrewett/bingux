//! The Gnoblin side of the bridge: the session D-Bus conversation with
//! Gnoblin's Mutter services.
//!
//! * `org.gnome.Mutter.RemoteDesktop` provides the EIS connection (input) and
//!   owns the session the virtual monitors are linked to, so that EIS absolute
//!   devices get one region per virtual monitor.
//! * `org.gnome.Mutter.ScreenCast.RecordVirtual` creates a virtual monitor per
//!   bsm output; PipeWire negotiation (see `stream.rs`) sets its mode.
//! * `org.gnome.Mutter.DisplayConfig` applies scale and position once the
//!   monitor exists.
//! * `org.gnoblin.Hosted1` (Gnoblin) reports the runtime handshake.
//!
//! All of it runs on one thread with a blocking executor; results reach the
//! main loop as [`Msg`] values.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::pin::Pin;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use futures_util::stream::{self, SelectAll, Stream, StreamExt};
use futures_util::{FutureExt, select_biased};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, proxy};

use crate::msg::Msg;

pub const MUTTER_SCREENCAST: &str = "org.gnome.Mutter.ScreenCast";
pub const MUTTER_REMOTE_DESKTOP: &str = "org.gnome.Mutter.RemoteDesktop";
pub const MUTTER_DISPLAY_CONFIG: &str = "org.gnome.Mutter.DisplayConfig";
pub const HOSTED_NAME: &str = "org.gnoblin.Hosted1";

/// How long to wait for Gnoblin's services before declaring the session lost.
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);

#[proxy(
    interface = "org.gnome.Mutter.ScreenCast",
    default_service = "org.gnome.Mutter.ScreenCast",
    default_path = "/org/gnome/Mutter/ScreenCast"
)]
trait ScreenCast {
    fn create_session(&self, properties: HashMap<&str, Value<'_>>) -> zbus::Result<OwnedObjectPath>;
}

pub mod sc {
    use super::*;
    #[proxy(interface = "org.gnome.Mutter.ScreenCast.Session", default_service = "org.gnome.Mutter.ScreenCast")]
    pub trait ScreenCastSession {
        fn start(&self) -> zbus::Result<()>;
        fn stop(&self) -> zbus::Result<()>;
        fn record_virtual(&self, properties: HashMap<&str, Value<'_>>) -> zbus::Result<OwnedObjectPath>;
        #[zbus(signal)]
        fn closed(&self) -> zbus::Result<()>;
    }
}
pub use sc::{ScreenCastSessionProxy, ScreenCastSessionProxyBlocking};

#[proxy(interface = "org.gnome.Mutter.ScreenCast.Stream", default_service = "org.gnome.Mutter.ScreenCast")]
trait ScreenCastStream {
    #[zbus(signal)]
    fn pipe_wire_stream_added(&self, node_id: u32) -> zbus::Result<()>;
    #[zbus(property)]
    fn parameters(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}

#[proxy(
    interface = "org.gnome.Mutter.RemoteDesktop",
    default_service = "org.gnome.Mutter.RemoteDesktop",
    default_path = "/org/gnome/Mutter/RemoteDesktop"
)]
trait RemoteDesktop {
    fn create_session(&self) -> zbus::Result<OwnedObjectPath>;
}

pub mod rd {
    use super::*;
    #[proxy(interface = "org.gnome.Mutter.RemoteDesktop.Session", default_service = "org.gnome.Mutter.RemoteDesktop")]
    pub trait RemoteDesktopSession {
        fn start(&self) -> zbus::Result<()>;
        fn stop(&self) -> zbus::Result<()>;
        #[zbus(name = "ConnectToEIS")]
        fn connect_to_eis(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<zbus::zvariant::OwnedFd>;
        #[zbus(signal)]
        fn closed(&self) -> zbus::Result<()>;
        #[zbus(property)]
        fn session_id(&self) -> zbus::Result<String>;
    }
}
pub use rd::{RemoteDesktopSessionProxy, RemoteDesktopSessionProxyBlocking};

type DcMode = (String, i32, i32, f64, f64, Vec<f64>, HashMap<String, OwnedValue>);
type DcMonitor = ((String, String, String, String), Vec<DcMode>, HashMap<String, OwnedValue>);
type DcLogical = (i32, i32, f64, u32, bool, Vec<(String, String, String, String)>, HashMap<String, OwnedValue>);
type DcApplyMonitor = (String, String, HashMap<String, Value<'static>>);
type DcApplyLogical = (i32, i32, f64, u32, bool, Vec<DcApplyMonitor>);

#[proxy(
    interface = "org.gnome.Mutter.DisplayConfig",
    default_service = "org.gnome.Mutter.DisplayConfig",
    default_path = "/org/gnome/Mutter/DisplayConfig"
)]
trait DisplayConfig {
    fn get_current_state(
        &self,
    ) -> zbus::Result<(u32, Vec<DcMonitor>, Vec<DcLogical>, HashMap<String, OwnedValue>)>;
    fn apply_monitors_config(
        &self,
        serial: u32,
        method: u32,
        logical_monitors: Vec<DcApplyLogical>,
        properties: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.gnoblin.Hosted1",
    default_service = "org.gnoblin.Hosted1",
    default_path = "/org/gnoblin/Hosted1"
)]
trait Hosted {
    #[zbus(property)]
    fn ready(&self) -> zbus::Result<bool>;
}

/// Position and scale of one virtual monitor in Gnoblin's layout.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub output: String,
    pub x: i32,
    pub y: i32,
    pub scale_120: u32,
    pub transform: u32,
}

pub enum Cmd {
    AddMonitor { output: String, epoch: u64, width: u32, height: u32, refresh_mhz: u32 },
    /// The full layout of all streaming monitors.
    ApplyLayout(Vec<Layout>),
    /// Stop the sessions (removing every virtual monitor) and start fresh ones.
    Reset,
    Shutdown,
}

enum Ev {
    Cmd(Result<Cmd, async_channel::RecvError>),
    Node { output: String, epoch: u64, node: u32 },
    SessionClosed(u64),
    Name { name: String, owned: bool },
    Ready(bool),
    Action(String),
    Idle,
}

type EvStream = Pin<Box<dyn Stream<Item = Ev> + Send>>;

struct Mon {
    epoch: u64,
    path: OwnedObjectPath,
    connector: Option<String>,
    node: Option<u32>,
}

struct Sessions {
    id: u64,
    rd: RemoteDesktopSessionProxy<'static>,
    sc: ScreenCastSessionProxy<'static>,
    started: bool,
}

pub struct Handle {
    pub cmd: async_channel::Sender<Cmd>,
    pub join: std::thread::JoinHandle<()>,
}

pub fn spawn(tx: calloop::channel::Sender<Msg>) -> Result<Handle> {
    let (cmd_tx, cmd_rx) = async_channel::unbounded();
    let join = std::thread::Builder::new()
        .name("bsm-host-dbus".into())
        .spawn(move || {
            let result = async_io::block_on(run(cmd_rx, tx.clone()));
            match result {
                Ok(()) => {}
                Err(e) => {
                    log::error!("gnoblin connection: {e:#}");
                    let _ = tx.send(Msg::GnoblinLost(format!("{e:#}")));
                }
            }
        })
        .context("spawn d-bus thread")?;
    Ok(Handle { cmd: cmd_tx, join })
}

async fn name_owned(dbus: &zbus::fdo::DBusProxy<'_>, name: &str) -> bool {
    match zbus::names::BusName::try_from(name) {
        Ok(n) => dbus.name_has_owner(n).await.unwrap_or(false),
        Err(_) => false,
    }
}

async fn run(cmd_rx: async_channel::Receiver<Cmd>, tx: calloop::channel::Sender<Msg>) -> Result<()> {
    let conn = Connection::session()
        .await
        .context("connect to the session bus (DBUS_SESSION_BUS_ADDRESS)")?;
    let dbus = zbus::fdo::DBusProxy::new(&conn).await?;

    // Wait for Mutter's services.
    let started = Instant::now();
    loop {
        let mut all = true;
        for n in [MUTTER_SCREENCAST, MUTTER_REMOTE_DESKTOP, MUTTER_DISPLAY_CONFIG] {
            all &= name_owned(&dbus, n).await;
        }
        if all {
            break;
        }
        if started.elapsed() > STARTUP_TIMEOUT {
            return Err(anyhow!("Gnoblin's Mutter services did not appear within {STARTUP_TIMEOUT:?}"));
        }
        async_io::Timer::after(Duration::from_millis(100)).await;
    }
    log::info!("mutter services are up after {:?}", started.elapsed());

    let mut events: SelectAll<EvStream> = SelectAll::new();
    events.push(Box::pin(stream::pending::<Ev>()));
    // Name ownership of everything we depend on.
    events.push(Box::pin(dbus.receive_name_owner_changed().await?.filter_map(|s| async move {
        let args = s.args().ok()?;
        let name = args.name().to_string();
        if matches!(name.as_str(), MUTTER_SCREENCAST | MUTTER_REMOTE_DESKTOP | MUTTER_DISPLAY_CONFIG | HOSTED_NAME) {
            Some(Ev::Name { owned: args.new_owner().is_some(), name })
        } else {
            None
        }
    })));

    let hosted = HostedProxy::new(&conn).await?;
    {
        let ready = hosted.receive_ready_changed().await;
        events.push(Box::pin(ready.filter_map(|c| async move { c.get().await.ok().map(Ev::Ready) })));
        // ActionRequested is emitted by Gnoblin's Lua worker, not by the name
        // owner, so it is matched on interface and path with any sender.
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.gnoblin.Hosted1")?
            .path("/org/gnoblin/Hosted1")?
            .member("ActionRequested")?
            .build();
        let actions = zbus::MessageStream::for_match_rule(rule, &conn, None).await?;
        events.push(Box::pin(actions.filter_map(|m| async move {
            let msg = m.ok()?;
            msg.body().deserialize::<String>().ok().map(Ev::Action)
        })));
    }
    let mut hosted_seen = name_owned(&dbus, HOSTED_NAME).await;
    if hosted_seen {
        if let Ok(true) = hosted.ready().await {
            let _ = tx.send(Msg::GnoblinReady(true));
        }
    }

    let mut app = App { conn: conn.clone(), tx: tx.clone(), sessions: None, monitors: HashMap::new(), next_session: 0 };
    app.ensure_sessions(&mut events).await?;
    let _ = tx.send(Msg::GnoblinBus);

    loop {
        let ev = select_biased! {
            c = cmd_rx.recv().fuse() => Ev::Cmd(c),
            e = events.next().fuse() => e.unwrap_or(Ev::Idle),
        };
        match ev {
            Ev::Cmd(Ok(Cmd::Shutdown)) | Ev::Cmd(Err(_)) => {
                app.stop_sessions().await;
                return Ok(());
            }
            Ev::Cmd(Ok(Cmd::Reset)) => {
                app.stop_sessions().await;
                app.ensure_sessions(&mut events).await?;
            }
            Ev::Cmd(Ok(Cmd::AddMonitor { output, epoch, width, height, refresh_mhz })) => {
                if let Err(e) = app.add_monitor(&mut events, &output, epoch, width, height, refresh_mhz).await {
                    log::error!("add monitor {output}: {e:#}");
                    let _ = tx.send(Msg::MonitorFailed { output, epoch, reason: format!("{e:#}") });
                }
            }
            Ev::Cmd(Ok(Cmd::ApplyLayout(layout))) => {
                let epochs: Vec<(String, u64)> = layout
                    .iter()
                    .filter_map(|l| app.monitors.get(&l.output).map(|m| (l.output.clone(), m.epoch)))
                    .collect();
                match app.apply_layout(&layout).await {
                    Ok(connectors) => {
                        for (output, epoch) in epochs {
                            if let Some(connector) = connectors.get(&output) {
                                let _ = tx.send(Msg::MonitorApplied { output, epoch, connector: connector.clone() });
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("apply layout: {e:#}");
                        for (output, epoch) in epochs {
                            let _ = tx.send(Msg::MonitorFailed { output, epoch, reason: format!("DisplayConfig: {e:#}") });
                        }
                    }
                }
            }
            Ev::Node { output, epoch, node } => {
                if let Some(m) = app.monitors.get_mut(&output) {
                    if m.epoch == epoch {
                        m.node = Some(node);
                        let _ = tx.send(Msg::MonitorStream { output, epoch, node_id: node });
                    }
                }
            }
            Ev::SessionClosed(id) => {
                if app.sessions.as_ref().is_some_and(|s| s.id == id) {
                    return Err(anyhow!("Gnoblin closed the RemoteDesktop/ScreenCast session"));
                }
            }
            Ev::Name { name, owned } => {
                if name == HOSTED_NAME {
                    if owned {
                        hosted_seen = true;
                        if let Ok(true) = hosted.ready().await {
                            let _ = tx.send(Msg::GnoblinReady(true));
                        }
                    } else if hosted_seen {
                        return Err(anyhow!("{HOSTED_NAME} vanished: the hosted Gnoblin is gone"));
                    }
                } else if !owned {
                    return Err(anyhow!("{name} vanished: Gnoblin's Mutter is gone"));
                }
            }
            Ev::Ready(r) => {
                let _ = tx.send(Msg::GnoblinReady(r));
            }
            Ev::Action(a) => {
                let _ = tx.send(Msg::Action(a));
            }
            Ev::Idle => {
                async_io::Timer::after(Duration::from_millis(50)).await;
            }
        }
    }
}

struct App {
    conn: Connection,
    tx: calloop::channel::Sender<Msg>,
    sessions: Option<Sessions>,
    monitors: HashMap<String, Mon>,
    next_session: u64,
}

impl App {
    async fn ensure_sessions(&mut self, events: &mut SelectAll<EvStream>) -> Result<()> {
        if self.sessions.is_some() {
            return Ok(());
        }
        let rd_main = RemoteDesktopProxy::new(&self.conn).await?;
        let rd_path = rd_main.create_session().await.context("RemoteDesktop.CreateSession")?;
        let rd = RemoteDesktopSessionProxy::builder(&self.conn).path(rd_path.clone())?.build().await?;
        let session_id = rd.session_id().await.context("RemoteDesktop SessionId")?;
        let sc_main = ScreenCastProxy::new(&self.conn).await?;
        let mut props = HashMap::new();
        props.insert("remote-desktop-session-id", Value::from(session_id));
        let sc_path = sc_main.create_session(props).await.context("ScreenCast.CreateSession")?;
        let sc = ScreenCastSessionProxy::builder(&self.conn).path(sc_path)?.build().await?;
        // EIS must be requested before the session starts.
        let fd: OwnedFd = rd.connect_to_eis(HashMap::new()).await.context("ConnectToEIS")?.into();
        self.next_session += 1;
        let id = self.next_session;
        let rd_closed = rd.receive_closed().await?;
        let sc_closed = sc.receive_closed().await?;
        events.push(Box::pin(rd_closed.map(move |_| Ev::SessionClosed(id))));
        events.push(Box::pin(sc_closed.map(move |_| Ev::SessionClosed(id))));
        let _ = self.tx.send(Msg::Eis(fd));
        self.sessions = Some(Sessions { id, rd, sc, started: false });
        Ok(())
    }

    async fn stop_sessions(&mut self) {
        self.monitors.clear();
        if let Some(s) = self.sessions.take() {
            let _ = s.sc.stop().await;
            let _ = s.rd.stop().await;
        }
    }

    async fn add_monitor(
        &mut self,
        events: &mut SelectAll<EvStream>,
        output: &str,
        epoch: u64,
        width: u32,
        height: u32,
        refresh_mhz: u32,
    ) -> Result<()> {
        log::info!("record virtual monitor for {output}: {width}x{height}@{}mHz", refresh_mhz);
        let sessions = self.sessions.as_mut().ok_or_else(|| anyhow!("no session"))?;
        let mut props = HashMap::new();
        // 2 = metadata: the cursor image travels as stream metadata and bsm
        // draws it, so there is exactly one cursor on screen.
        props.insert("cursor-mode", Value::from(2u32));
        let path = sessions.sc.record_virtual(props).await.context("RecordVirtual")?;
        let stream = ScreenCastStreamProxy::builder(&self.conn).path(path.clone())?.build().await?;
        let added = stream.receive_pipe_wire_stream_added().await?;
        let out = output.to_owned();
        events.push(Box::pin(added.filter_map(move |s| {
            let out = out.clone();
            async move { s.args().ok().map(|a| Ev::Node { output: out, epoch, node: a.node_id }) }
        })));
        if let Ok(params) = stream.parameters().await {
            log::debug!("stream {output} parameters: {params:?}");
        }
        self.monitors.insert(output.to_owned(), Mon { epoch, path, connector: None, node: None });
        if !sessions.started {
            sessions.rd.start().await.context("RemoteDesktop.Start")?;
            sessions.started = true;
        }
        let _ = (width, height, refresh_mhz); // the mode is set by PipeWire negotiation
        Ok(())
    }

    /// Apply scale and position for every listed monitor and return the
    /// connector assigned to each output.
    async fn apply_layout(&mut self, layout: &[Layout]) -> Result<HashMap<String, String>> {
        let dc = DisplayConfigProxy::new(&self.conn).await?;
        let (serial, monitors, _logical, _props) = dc.get_current_state().await.context("GetCurrentState")?;

        // Connectors already claimed by an output keep it; unclaimed virtual
        // connectors go to the single monitor still waiting for one.
        let claimed: Vec<&str> = self.monitors.values().filter_map(|m| m.connector.as_deref()).collect();
        let free: Vec<String> = monitors
            .iter()
            .map(|m| m.0.0.clone())
            .filter(|c| !claimed.contains(&c.as_str()))
            .collect();
        let waiting: Vec<String> = layout
            .iter()
            .filter(|l| self.monitors.get(&l.output).is_some_and(|m| m.connector.is_none()))
            .map(|l| l.output.clone())
            .collect();
        if waiting.len() > 1 {
            return Err(anyhow!("monitors must be laid out one at a time ({waiting:?} are unassigned)"));
        }
        if let Some(w) = waiting.first() {
            let connector = free
                .first()
                .ok_or_else(|| anyhow!("Mutter has no new monitor for {w} yet"))?
                .clone();
            if let Some(m) = self.monitors.get_mut(w) {
                m.connector = Some(connector);
            }
        }

        let mut logical: Vec<DcApplyLogical> = Vec::new();
        let mut result = HashMap::new();
        for (i, l) in layout.iter().enumerate() {
            let Some(connector) = self.monitors.get(&l.output).and_then(|m| m.connector.clone()) else { continue };
            let Some(mon) = monitors.iter().find(|m| m.0.0 == connector) else {
                return Err(anyhow!("monitor {connector} for {} is not in DisplayConfig", l.output));
            };
            let mode = mon
                .1
                .iter()
                .find(|m| m.6.get("is-current").is_some_and(|v| bool::try_from(v).unwrap_or(false)))
                .or_else(|| mon.1.first())
                .ok_or_else(|| anyhow!("monitor {connector} has no modes"))?;
            let wanted = l.scale_120 as f64 / 120.0;
            let scale = nearest_scale(wanted, &mode.5, mode.4);
            if (scale - wanted).abs() > 0.001 {
                log::warn!("{}: scale {wanted} unsupported for mode {}, using {scale}", l.output, mode.0);
            }
            logical.push((l.x, l.y, scale, l.transform, i == 0, vec![(connector.clone(), mode.0.clone(), HashMap::new())]));
            result.insert(l.output.clone(), connector);
        }
        if logical.is_empty() {
            return Ok(result);
        }
        dc.apply_monitors_config(serial, 1, logical, HashMap::new())
            .await
            .context("ApplyMonitorsConfig")?;
        Ok(result)
    }
}

/// The supported scale closest to `wanted`; `wanted` itself when the list is
/// empty (Mutter then validates it).
pub fn nearest_scale(wanted: f64, supported: &[f64], preferred: f64) -> f64 {
    if supported.is_empty() {
        return if preferred > 0.0 { preferred } else { wanted };
    }
    supported
        .iter()
        .copied()
        .min_by(|a, b| (a - wanted).abs().partial_cmp(&(b - wanted).abs()).unwrap())
        .unwrap_or(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_scale_picks_closest_supported() {
        assert_eq!(nearest_scale(1.0, &[1.0, 2.0], 1.0), 1.0);
        assert_eq!(nearest_scale(1.25, &[1.0, 1.25, 1.5, 2.0], 1.0), 1.25);
        assert_eq!(nearest_scale(1.3, &[1.0, 2.0], 1.0), 1.0);
        assert_eq!(nearest_scale(1.8, &[1.0, 2.0], 1.0), 2.0);
        assert_eq!(nearest_scale(1.5, &[], 1.0), 1.0);
    }
}
