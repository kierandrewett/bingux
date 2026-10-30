//! The D-Bus objects that the desktop portal frontend calls.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zbus::fdo;
use zbus::message::Header;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{Connection, interface};

use crate::binguxctl::Ctl;
use crate::identity::{self, Identity};
use crate::policy::{Capability, Level, resolve_level};
use crate::screenshot::{self, Outcome};
use crate::store;

/// Portal response codes: success, cancelled by the user, other failure.
const RESPONSE_SUCCESS: u32 = 0;
const RESPONSE_CANCELLED: u32 = 1;
const RESPONSE_OTHER: u32 = 2;

/// State shared by every request.
pub struct Shared {
    pub ctl: Ctl,
    pub store_path: PathBuf,
    /// This backend's user ID. Requesters must run as the same user.
    pub uid: u32,
}

/// The `org.freedesktop.impl.portal.Screenshot` interface.
pub struct Screenshot {
    shared: Arc<Shared>,
}

impl Screenshot {
    pub fn new(shared: Arc<Shared>) -> Self {
        Screenshot { shared }
    }
}

fn refused() -> (u32, HashMap<String, OwnedValue>) {
    (RESPONSE_OTHER, HashMap::new())
}

fn wants_interactive(options: &HashMap<String, OwnedValue>) -> bool {
    options
        .get("interactive")
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

/// Decides whether a request may run, and whether it shows the selector.
///
/// Returns `None` when the request is refused. An `inherit` level falls back
/// to the store default, which is `ask` when no default is set.
fn decide(
    store: &store::Store,
    identity: &Identity,
    options: &HashMap<String, OwnedValue>,
) -> Option<bool> {
    match resolve_level(store, Capability::Screenshot, Some(identity)) {
        Level::Deny => None,
        Level::Ask => Some(true),
        Level::Allow => Some(wants_interactive(options)),
        Level::Inherit => Some(true),
    }
}

#[interface(name = "org.freedesktop.impl.portal.Screenshot")]
impl Screenshot {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        2
    }

    async fn screenshot(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        handle: ObjectPath<'_>,
        app_id: &str,
        _parent_window: &str,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let Some(sender) = header.sender().map(|name| name.to_string()) else {
            return refused();
        };
        let Some(identity) = identity::resolve(
            connection,
            &sender,
            app_id,
            handle.as_str(),
            self.shared.uid,
        )
        .await
        else {
            return refused();
        };
        let Some(request) = identity::parse_request_handle(handle.as_str()) else {
            return refused();
        };

        let store = store::load_effective(&self.shared.store_path);
        let Some(interactive) = decide(&store, &identity, &options) else {
            return refused();
        };

        let cancel = Arc::new(AtomicBool::new(false));
        let path: ObjectPath<'static> = handle.clone().into_owned();
        let server = connection.object_server();
        let object = RequestObject {
            owner: sender.clone(),
            cancel: cancel.clone(),
        };
        match server.at(path.clone(), object).await {
            Ok(true) => {}
            _ => return refused(),
        }

        let caller = sender.clone();
        let alive = move || {
            let caller = caller.clone();
            async move { identity::frontend_is(connection, &caller).await }
        };
        let outcome = screenshot::run(
            &self.shared.ctl,
            &request.capture_id(),
            interactive,
            &cancel,
            alive,
        )
        .await;
        let _ = server.remove::<RequestObject, _>(path).await;

        match outcome {
            Outcome::Saved(uri) => match OwnedValue::try_from(Value::from(uri)) {
                Ok(value) => {
                    let mut results = HashMap::new();
                    results.insert("uri".to_owned(), value);
                    (RESPONSE_SUCCESS, results)
                }
                Err(_) => refused(),
            },
            Outcome::Cancelled => (RESPONSE_CANCELLED, HashMap::new()),
            Outcome::Failed(message) => {
                eprintln!("xdg-desktop-portal-bingux: screenshot failed: {message}");
                refused()
            }
        }
    }
}

/// The `org.freedesktop.impl.portal.Request` object for one in-flight request.
pub struct RequestObject {
    /// The unique name of the frontend that made the request.
    owner: String,
    cancel: Arc<AtomicBool>,
}

#[interface(name = "org.freedesktop.impl.portal.Request")]
impl RequestObject {
    async fn close(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<()> {
        if header.sender().map(|name| name.as_str()) != Some(self.owner.as_str()) {
            return Err(fdo::Error::AccessDenied(
                "only the requesting frontend may close this request".to_owned(),
            ));
        }
        self.cancel.store(true, Ordering::SeqCst);
        Ok(())
    }
}
