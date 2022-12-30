#![forbid(unsafe_code)]

use color_eyre::eyre::Result;
use libthere::log::*;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};

mod executor;
mod http_server;
mod keys;

const PORT: u16 = 2345;

#[tokio::main]
#[tracing::instrument]
async fn main() -> Result<()> {
    install_logger()?;

    let _client_key = keys::get_or_create_executor_keypair().await?;

    let token = keys::get_or_create_token().await?;
    println!("* token for agent connections: {token}");
    println!("* agents can bootstrap via:");
    for iface in NetworkInterface::show()? {
        if let Some(addr) = iface.addr {
            println!(
                "  - http://{}:{PORT}/api/bootstrap?token={token}",
                addr.ip()
            );
        }
    }

    http_server::run_server(PORT).await
}
