//! The session bus, reached the way every platform call reaches it: on a worker
//! thread, with its own time limit, and never twice a failure.
//!
//! D-Bus is optional. A session without a bus, without the service a call
//! names, or with a service that does not answer must cost a window a moment
//! and no more, so every call here runs in [`within`]: the caller waits at most
//! its limit, and a call that never returns is abandoned on its own thread
//! instead of freezing the window.

use std::sync::{Mutex, mpsc};
use std::time::Duration;

use zbus::blocking::Connection;

/// A call that names a service answers within this, or is taken to be absent.
/// A portal frontend that has to be started on demand needs most of it.
pub(crate) const ANSWER_WITHIN: Duration = Duration::from_millis(1500);

/// Why a call to the bus gave nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusError {
    /// There is no session bus, or the process may not use it
    /// (`BINGUX_PORTAL=0`, a test).
    NoBus,
    /// The service did not answer in time.
    TimedOut,
    /// The bus or the service refused: no such service, no such method, a
    /// rejected argument. The text is the bus's own.
    Failed(String),
}

impl std::fmt::Display for BusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoBus => f.write_str(&crate::tr!("ui-bus-missing-error")),
            Self::TimedOut => f.write_str(&crate::tr!("ui-bus-timeout-error")),
            Self::Failed(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for BusError {}

/// Whether this process talks to the session bus at all. `BINGUX_PORTAL=0`
/// says no and `BINGUX_PORTAL=1` says yes; without either, a test process
/// does not (a test that wants a bus builds its own, see `testing`) and a
/// program does when a bus address can be found.
pub(crate) fn allowed() -> bool {
    match std::env::var("BINGUX_PORTAL").as_deref() {
        Ok("0") => false,
        Ok("1") => true,
        _ => !cfg!(test) && address_exists(),
    }
}

/// Whether a session bus could be there: an address in the environment, or the
/// socket systemd and dbus-broker put in the runtime directory. A session with
/// neither costs nothing to find out.
fn address_exists() -> bool {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some_and(|value| !value.is_empty()) {
        return true;
    }
    std::env::var_os("XDG_RUNTIME_DIR")
        .is_some_and(|dir| std::path::Path::new(&dir).join("bus").exists())
}

static SESSION: Mutex<Option<Connection>> = Mutex::new(None);

/// The session bus connection, made on first use and kept.
pub(crate) fn session() -> Result<Connection, BusError> {
    if !allowed() {
        return Err(BusError::NoBus);
    }
    let mut held = SESSION.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(connection) = held.as_ref() {
        return Ok(connection.clone());
    }
    let connection = Connection::session().map_err(|error| BusError::Failed(error.to_string()))?;
    *held = Some(connection.clone());
    Ok(connection)
}

/// Forget the kept connection: the bus went away under it.
fn forget() {
    *SESSION.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
}

/// Run `work` on its own thread and wait for it at most `limit`. `None` when it
/// did not finish: the thread is left to end on its own.
pub(crate) fn within<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("bingux-platform".into())
        .spawn(move || {
            let _ = send.send(work());
        })
        .ok()?;
    receive.recv_timeout(limit).ok()
}

/// What a method call needs to say where it goes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target {
    pub destination: &'static str,
    pub path: &'static str,
    pub interface: &'static str,
}

/// Call `member` of `target` on `connection` and read the reply as `R`. Blocks
/// for as long as the service takes; callers go through [`call`].
pub(crate) fn call_on<B, R>(
    connection: &Connection,
    target: Target,
    member: &'static str,
    body: &B,
) -> Result<R, zbus::Error>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: serde::de::DeserializeOwned + zbus::zvariant::Type,
{
    let reply = connection.call_method(
        Some(target.destination),
        target.path,
        Some(target.interface),
        member,
        body,
    )?;
    Ok(reply.body().deserialize::<R>()?)
}

/// The session bus connection, within [`ANSWER_WITHIN`]: making it can wait on
/// a bus that is not answering. For callers that make several calls and give
/// each its own limit with [`within`].
pub(crate) fn connect() -> Result<Connection, BusError> {
    if !allowed() {
        return Err(BusError::NoBus);
    }
    within(ANSWER_WITHIN, session).ok_or(BusError::TimedOut)?
}

/// Run one call on `connection` within `limit`, as one step of several that
/// each get their own time. The error is the bus's own text.
pub(crate) fn step<T: Send + 'static>(
    connection: &Connection,
    limit: Duration,
    work: impl FnOnce(&Connection) -> Result<T, zbus::Error> + Send + 'static,
) -> Result<T, BusError> {
    let connection = connection.clone();
    within(limit, move || {
        work(&connection).map_err(|error| BusError::Failed(error.to_string()))
    })
    .ok_or(BusError::TimedOut)?
}

/// Run `work` with the session bus connection, within `limit`. A failure that
/// is not the service's own answer (the socket broke) drops the kept
/// connection, so the next call makes a fresh one.
pub(crate) fn on_session<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce(&Connection) -> Result<T, zbus::Error> + Send + 'static,
) -> Result<T, BusError> {
    if !allowed() {
        return Err(BusError::NoBus);
    }
    within(limit, move || {
        let connection = session()?;
        work(&connection).map_err(|error| {
            if !matches!(error, zbus::Error::MethodError(..)) {
                forget();
            }
            BusError::Failed(error.to_string())
        })
    })
    .ok_or(BusError::TimedOut)?
}

/// [`call_on`] on the session bus, within `limit`.
pub(crate) fn call<B, R>(
    limit: Duration,
    target: Target,
    member: &'static str,
    body: B,
) -> Result<R, BusError>
where
    B: serde::Serialize + zbus::zvariant::DynamicType + Send + 'static,
    R: serde::de::DeserializeOwned + zbus::zvariant::Type + Send + 'static,
{
    on_session(limit, move |connection| {
        call_on::<B, R>(connection, target, member, &body)
    })
}

/// A call whose reply is not read, on the session bus, within `limit`.
pub(crate) fn call_unit<B>(
    limit: Duration,
    target: Target,
    member: &'static str,
    body: B,
) -> Result<(), BusError>
where
    B: serde::Serialize + zbus::zvariant::DynamicType + Send + 'static,
{
    on_session(limit, move |connection| {
        connection
            .call_method(
                Some(target.destination),
                target.path,
                Some(target.interface),
                member,
                &body,
            )
            .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_that_never_returns_is_given_up_on() {
        let started = std::time::Instant::now();
        let result = within(Duration::from_millis(100), || {
            std::thread::sleep(Duration::from_secs(5));
        });
        assert!(result.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_call_that_returns_is_waited_for() {
        assert_eq!(within(Duration::from_secs(2), || 7), Some(7));
    }

    #[test]
    fn a_test_process_has_no_session_bus_unless_it_builds_one() {
        if std::env::var_os("BINGUX_PORTAL").is_none() {
            assert_eq!(session().err(), Some(BusError::NoBus));
        }
    }
}
