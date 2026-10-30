use std::process::ExitCode;
use std::sync::Arc;

use bingux_portal::binguxctl::Ctl;
use bingux_portal::identity;
use bingux_portal::portal::{Screenshot, Shared};
use bingux_portal::store;

/// The well-known name that the desktop portal frontend routes to.
const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.bingux";
/// The object path that portal backends serve.
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";

fn main() -> ExitCode {
    match async_io::block_on(serve()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xdg-desktop-portal-bingux: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve() -> zbus::Result<()> {
    let shared = Arc::new(Shared {
        ctl: Ctl::from_env(),
        store_path: store::default_path(),
        uid: identity::current_uid(),
    });
    let _connection = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, Screenshot::new(shared))?
        .build()
        .await?;
    // The connection serves requests in the background; keep it alive.
    std::future::pending::<()>().await;
    Ok(())
}
