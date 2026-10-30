//! `bingux-global-shortcuts`: owns `org.gnoblin.Shell.GlobalShortcutsProvider`.
//!
//! The Gnoblin portal backend calls this provider to approve and configure
//! global shortcuts. The approval UI lives in the Bingux shell.

use std::process::ExitCode;

use bingux_portal::binguxctl::Ctl;
use bingux_portal::identity;
use bingux_portal::shortcuts::{self, PROVIDER_NAME, PROVIDER_PATH, Provider};

fn main() -> ExitCode {
    match async_io::block_on(serve()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bingux-global-shortcuts: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve() -> zbus::Result<()> {
    let provider = Provider::new(
        Ctl::from_env(),
        shortcuts::grants_path(),
        identity::current_uid(),
    );
    let _connection = zbus::connection::Builder::session()?
        .name(PROVIDER_NAME)?
        .serve_at(PROVIDER_PATH, provider)?
        .build()
        .await?;
    // The connection serves requests in the background; keep it alive.
    std::future::pending::<()>().await;
    Ok(())
}
