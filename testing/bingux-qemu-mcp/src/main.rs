mod config;
mod error;
mod mcp;
mod qemu;
mod qmp;
mod screenshot;
mod serial;
mod tools;

use tracing::info;

#[tokio::main]
async fn main() -> error::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("bingux_qemu_mcp=info")
        .init();

    info!("bingux-qemu-mcp server starting");

    let mut server = mcp::McpServer::new();
    server.run().await?;

    Ok(())
}
